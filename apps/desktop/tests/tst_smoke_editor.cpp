#include "Shell.h"
#include "editor/EditorSetup.h"

#include <QDir>
#include <QFile>
#include <QGuiApplication>
#include <QImage>
#include <QKeyEvent>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtQml>
#include <QtTest>
#include "TestSupport.h"

Q_IMPORT_QML_PLUGIN(Axiom_UiPlugin)
Q_IMPORT_QML_PLUGIN(Axiom_AppPlugin)

// The real thing, offscreen: real daemon, the real editor bundle in Qt WebEngine,
// real key events. Every wait is bounded and a timeout is a FAILURE, never a skip.
class TstSmokeEditor : public QObject
{
    Q_OBJECT

    QTemporaryDir m_data;
    QTemporaryDir m_ws;
    QString m_assets;

    template<typename F>
    bool waitFor(F done, int ms = 20000)
    {
        QElapsedTimer t;
        t.start();
        while (!done() && t.elapsed() < ms)
            QTest::qWait(20);
        return done();
    }

    static QByteArray readFile(const QString &path)
    {
        QFile f(path);
        return f.open(QIODevice::ReadOnly) ? f.readAll() : QByteArray();
    }
    static void writeFile(const QString &path, const QByteArray &data)
    {
        QDir().mkpath(QFileInfo(path).absolutePath());
        QFile f(path);
        QVERIFY(f.open(QIODevice::WriteOnly | QIODevice::Truncate));
        f.write(data);
    }

    // The platform's "command" modifier as Monaco's CtrlCmd sees it: Qt maps ControlModifier to
    // the Command key on macOS and to Ctrl elsewhere, which is the one Monaco binds Save to.
    static constexpr Qt::KeyboardModifier SaveModifier = Qt::ControlModifier;

    // Delivers to the item with active focus, as E1 does: WebEngine forwards input from its focus proxy.
    void key(QQuickWindow *w, int k, Qt::KeyboardModifiers mods, const QString &text)
    {
        QObject *target = w->activeFocusItem() ? static_cast<QObject *>(w->activeFocusItem()) : w;
        QKeyEvent press(QEvent::KeyPress, k, mods, text);
        QKeyEvent release(QEvent::KeyRelease, k, mods, text);
        QCoreApplication::sendEvent(target, &press);
        QCoreApplication::sendEvent(target, &release);
    }

    QQuickItem *find(QQuickItem *root, const QString &name)
    {
        if (root->objectName() == name)
            return root;
        for (QQuickItem *c : root->childItems())
            if (QQuickItem *r = find(c, name))
                return r;
        return nullptr;
    }

    struct Session
    {
        QTemporaryDir data; // fresh per session: no restored tabs from an earlier one
        std::unique_ptr<Shell> shell;
        QQuickWindow *w = nullptr;
    };

    std::unique_ptr<Session> start()
    {
        auto s = std::make_unique<Session>();
        ShellOptions opts;
        opts.daemonPath = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        opts.dataDir = s->data.path();
        opts.editorAssets = m_assets;
        s->shell = std::make_unique<Shell>(opts);
        if (!s->shell->load())
            return nullptr;
        s->w = s->shell->window();
        s->w->show();
        s->w->requestActivate();
        if (!QTest::qWaitForWindowActive(s->w, 5000))
            qWarning("window did not become active (offscreen)");
        return s;
    }

private slots:
    void initTestCase()
    {
        const QString daemon = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        QVERIFY2(QFileInfo(daemon).isExecutable(), qPrintable("axiomd not built: " + daemon));
        m_assets = locateEditorAssets(QString());
        QVERIFY2(!m_assets.isEmpty(), "editor bundle not found: run `pnpm run build` in editor/");
        QVERIFY(m_ws.isValid() && m_data.isValid());
        writeFile(m_ws.path() + "/src/main.rs", "fn main() {}\n");
        writeFile(m_ws.path() + "/src/lib.rs", "pub fn f() {}\n");
        writeFile(m_ws.path() + "/notes.txt", "notes\n");
    }

    void editTypeSaveAndConflict()
    {
        auto s = start();
        QVERIFY(s);
        Shell &shell = *s->shell;
        QQuickWindow *w = s->w;
        OpenDocumentsModel *docs = shell.documents();
        QVERIFY(waitFor([&] { return shell.backend()->ready(); }, 15000));

        QSignalSpy wsOpened(shell.workspace(), &WorkspaceController::opened);
        shell.workspace()->openFolder(m_ws.path());
        QVERIFY(wsOpened.wait(10000));

        // ---- open: the page loads from axiom://editor/, performs the handshake, shows the file
        docs->open("src/main.rs");
        QVERIFY2(waitFor([&] { return docs->editorReady(); }), "editor page never sent `ready` (bridge handshake)");
        QCOMPARE(docs->activePath(), QStringLiteral("src/main.rs"));
        QVERIFY(!docs->isDirty("src/main.rs"));

        QQuickItem *view = find(w->contentItem(), "editorView");
        QVERIFY(view);
        QVERIFY(waitFor([&] { return view->isVisible(); }));
        view->forceActiveFocus();
        QVERIFY(waitFor([&] { return w->activeFocusItem() != nullptr; }));
        QTest::qWait(1000); // Monaco finishes creating the model and takes focus

        // ---- type `//K` with real key events
        key(w, Qt::Key_Slash, Qt::NoModifier, "/");
        key(w, Qt::Key_Slash, Qt::NoModifier, "/");
        key(w, Qt::Key_K, Qt::ShiftModifier, "K");
        QVERIFY2(waitFor([&] { return docs->isDirty("src/main.rs"); }), "typing did not reach Monaco (not dirty)");

        // ---- Ctrl+S, handled by Monaco's own keybinding
        key(w, Qt::Key_S, Qt::ControlModifier, QString());
        const bool savedByKey = waitFor([&] { return readFile(m_ws.path() + "/src/main.rs").contains("//K"); }, 8000);
        if (!savedByKey)
            qWarning().noquote() << "Ctrl+S diagnostics: dirty =" << docs->isDirty("src/main.rs")
                                 << "editorReady =" << docs->editorReady()
                                 << "cursor =" << docs->cursorText()
                                 << "file =" << readFile(m_ws.path() + "/src/main.rs").left(80);
        QVERIFY2(savedByKey, "Ctrl+S did not write the file (the keybinding did not reach Monaco)");
        QVERIFY2(waitFor([&] { return !docs->isDirty("src/main.rs"); }), "document stayed dirty after the save");
        QVERIFY(readFile(m_ws.path() + "/src/main.rs").contains("fn main()"));
        // The cursor position reached the status text.
        QVERIFY(waitFor([&] { return docs->cursorText().startsWith("Ln "); }));

        // ---- external change while clean: the model reloads it
        const QString v1 = docs->savedVersionOf("src/main.rs");
        writeFile(m_ws.path() + "/src/main.rs", "// rewritten outside\nfn main() {}\n");
        QVERIFY2(waitFor([&] { return docs->savedVersionOf("src/main.rs") != v1; }, 6000), "clean document did not reload");
        QVERIFY(!docs->isDirty("src/main.rs"));

        // ---- dirty + external change + save: the conflict dialog state is raised
        key(w, Qt::Key_X, Qt::NoModifier, "x");
        QVERIFY(waitFor([&] { return docs->isDirty("src/main.rs"); }));
        writeFile(m_ws.path() + "/src/main.rs", "// changed again, elsewhere\n");
        QVERIFY(waitFor([&] { return docs->isChangedOnDisk("src/main.rs"); }, 6000));
        key(w, Qt::Key_S, Qt::ControlModifier, QString());
        QVERIFY2(waitFor([&] { return docs->conflictActive(); }), "no conflict was raised");
        QCOMPARE(docs->conflictName(), QStringLiteral("main.rs"));
        w->grabWindow().save(QDir::tempPath() + "/axiom-shots/editor-conflict.png");
        // Overwrite resolves it: the file becomes the editor's text.
        docs->resolveConflict("overwrite");
        QVERIFY(waitFor([&] { return !docs->isDirty("src/main.rs") && !docs->conflictActive(); }));
        QVERIFY(readFile(m_ws.path() + "/src/main.rs").contains("x"));
        QVERIFY(!readFile(m_ws.path() + "/src/main.rs").contains("changed again"));

        // ---- second tab, switching, closing
        docs->open("notes.txt");
        QVERIFY(waitFor([&] { return docs->count() == 2 && docs->activePath() == "notes.txt"; }));
        docs->cycle(1);
        QCOMPARE(docs->activePath(), QStringLiteral("src/main.rs"));
        docs->close("notes.txt");
        QCOMPARE(docs->count(), 1);

        QDir().mkpath(QDir::tempPath() + "/axiom-shots");
        w->grabWindow().save(QDir::tempPath() + "/axiom-shots/editor.png");

    }

    void unsavedTextSurvivesARendererCrash()
    {
        writeFile(m_ws.path() + "/src/lib.rs", "pub fn f() {}\n");
        auto s = start();
        QVERIFY(s);
        Shell &shell = *s->shell;
        QQuickWindow *w = s->w;
        OpenDocumentsModel *docs = shell.documents();
        QVERIFY(waitFor([&] { return shell.backend()->ready(); }, 15000));
        QSignalSpy wsOpened(shell.workspace(), &WorkspaceController::opened);
        shell.workspace()->openFolder(m_ws.path());
        QVERIFY(wsOpened.wait(10000));

        docs->open("src/lib.rs");
        QVERIFY(waitFor([&] { return docs->editorReady(); }));
        QQuickItem *view = find(w->contentItem(), "editorView");
        QVERIFY(view);
        QVERIFY(waitFor([&] { return view->isVisible(); }));
        view->forceActiveFocus();
        QVERIFY(waitFor([&] { return w->activeFocusItem() != nullptr; }));
        QTest::qWait(1000);
        key(w, Qt::Key_Z, Qt::NoModifier, "z");
        key(w, Qt::Key_Z, Qt::NoModifier, "z");
        QVERIFY(waitFor([&] { return docs->isDirty("src/lib.rs"); }));
        // Snapshots are throttled to 1 s; give the page time to send one after the last key.
        QTest::qWait(1800);

        // Kill the real renderer process: a child of this one started with --type=renderer.
        const QList<qint64> renderers = TestSupport::descendantsMatching("--type=renderer");
        QVERIFY2(!renderers.isEmpty(), qPrintable("no Chromium renderer process found to kill; descendants seen:\n" + TestSupport::describeDescendants()));
        for (qint64 pid : renderers)
            TestSupport::killProcess(pid);
        QVERIFY2(waitFor([&] { return docs->rendererCrashed(); }, 15000),
                 "renderProcessTerminated was not reported after killing the renderer");
        QVERIFY(docs->isDirty("src/lib.rs"));

        // Reload: the handshake runs again and the typed text returns, dirty.
        QMetaObject::invokeMethod(view, "reload");
        QVERIFY2(waitFor([&] { return docs->editorReady() && !docs->rendererCrashed(); }, 20000), "editor did not come back after reload");
        QVERIFY(docs->isDirty("src/lib.rs"));
        QTest::qWait(1500); // the page applies the reopened document
        view->forceActiveFocus();
        QTest::qWait(500);
        key(w, Qt::Key_S, Qt::ControlModifier, QString());
        const bool saved = waitFor([&] { return readFile(m_ws.path() + "/src/lib.rs").contains("zz"); }, 8000);
        if (!saved)
            qWarning() << "file after Ctrl+S:" << readFile(m_ws.path() + "/src/lib.rs") << "dirty:" << docs->isDirty("src/lib.rs");
        QVERIFY2(saved, "the typed text was not restored after the renderer crash");
    }
};

// WebEngine needs the scheme registered and initialised before the application exists.
int main(int argc, char *argv[])
{
    initEditorWebEngine();
    QGuiApplication app(argc, argv);
    TstSmokeEditor tc;
    return QTest::qExec(&tc, argc, argv);
}
#include "tst_smoke_editor.moc"
