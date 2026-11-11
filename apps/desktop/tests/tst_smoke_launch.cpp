#include "Shell.h"
#include "editor/EditorSetup.h"

#include <QGuiApplication>
#include <QImage>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTemporaryDir>
#include <QtQml>
#include "TestSupport.h"
#include <QtTest>

// The QML modules' static plugins are linked into this test.
Q_IMPORT_QML_PLUGIN(Axiom_UiPlugin)
Q_IMPORT_QML_PLUGIN(Axiom_AppPlugin)

class TstSmokeLaunch : public QObject
{
    Q_OBJECT

    QTemporaryDir m_data;
    QString m_shots;

    template<typename F>
    bool waitFor(F done, int ms = 15000)
    {
        QElapsedTimer t;
        t.start();
        while (!done() && t.elapsed() < ms)
            QTest::qWait(10);
        return done();
    }

    static QQuickItem *findByName(QQuickItem *root, const QString &name)
    {
        if (root->objectName() == name)
            return root;
        for (QQuickItem *child : root->childItems())
            if (QQuickItem *r = findByName(child, name))
                return r;
        return nullptr;
    }

    // Visual-tree search (QObject parents do not follow the visual hierarchy).
    static QQuickItem *findByProperty(QQuickItem *root, const char *prop, const QString &value)
    {
        if (root->property(prop).toString() == value)
            return root;
        for (QQuickItem *child : root->childItems())
            if (QQuickItem *r = findByProperty(child, prop, value))
                return r;
        return nullptr;
    }

    QString statusText(QQuickWindow *w)
    {
        QQuickItem *item = findByName(w->contentItem(), QStringLiteral("statusText"));
        return item ? item->property("text").toString() : QString();
    }

private slots:
    void initTestCase()
    {
        QVERIFY(m_data.isValid());
        m_shots = qEnvironmentVariable("AXIOM_SCREENSHOT_DIR", QDir::tempPath() + "/axiom-shots");
        QDir().mkpath(m_shots);
        QGuiApplication::setApplicationVersion(QStringLiteral(AXIOM_VERSION));
    }

    void launchesConnectsAndShowsStatus()
    {
        const QString daemon = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        QVERIFY2(QFileInfo(daemon).isExecutable(), qPrintable("axiomd not built: " + daemon));

        ShellOptions opts;
        opts.daemonPath = daemon;
        opts.dataDir = m_data.path();
        Shell shell(opts);
        QVERIFY(shell.load());
        QQuickWindow *w = shell.window();
        QVERIFY(w);
        QCOMPARE(w->minimumWidth(), 720);
        QCOMPARE(w->minimumHeight(), 480);

        QVERIFY(waitFor([&] { return shell.backend()->ready(); }));
        QCOMPARE(shell.backend()->serverVersion(), QStringLiteral("0.1.0"));
        QVERIFY(waitFor([&] { return statusText(w) == QStringLiteral("Connected — axiomd 0.1.0"); }));

        // Light and dark both render; the images are attached for review.
        QObject *theme = shell.engine()->singletonInstance<QObject *>("Axiom.Ui", "Theme");
        QVERIFY(theme);
        for (bool dark : { false, true }) {
            theme->setProperty("dark", dark);
            QTest::qWait(200);
            const QImage img = w->grabWindow();
            QVERIFY(!img.isNull());
            QVERIFY(img.width() > 0 && img.height() > 0);
            const QString path = m_shots + (dark ? "/dark.png" : "/light.png");
            QVERIFY(img.save(path));
            // The two themes must actually differ at the content area.
            const QColor c = img.pixelColor(img.width() / 2, img.height() / 3);
            QCOMPARE(c.lightness() < 128, dark);
            qInfo() << "screenshot" << path;
        }
    }

    void failedBackendShowsBannerAndRestartRecovers()
    {
        const QString daemon = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        ShellOptions opts;
        opts.daemonPath = daemon;
        opts.dataDir = m_data.path();
        Shell shell(opts);
        QVERIFY(shell.load());
        QQuickWindow *w = shell.window();
        QVERIFY(waitFor([&] { return shell.backend()->ready(); }));

        TestSupport::killProcess(shell.client()->processId());
        QVERIFY(waitFor([&] { return shell.backend()->failed(); }));
        QVERIFY(waitFor([&] { return statusText(w) == QStringLiteral("Backend stopped"); }));
        QVERIFY(!shell.backend()->errorText().isEmpty());

        // The banner is a visible AxBanner carrying the error text.
        QQuickItem *banner = findByProperty(w->contentItem(), "actionText", QStringLiteral("Restart backend"));
        QVERIFY(banner);
        QVERIFY(banner->isVisible());
        QCOMPARE(banner->property("text").toString(), shell.backend()->errorText());
        w->grabWindow().save(m_shots + "/backend-failed.png");

        // A real mouse click on the button, delivered through Qt's input path.
        QQuickItem *button = findByProperty(w->contentItem(), "text", QStringLiteral("Restart backend"));
        QVERIFY(button);
        QVERIFY(button->isVisible() && button->isEnabled());
        const QPoint centre = button->mapToScene(QPointF(button->width() / 2, button->height() / 2)).toPoint();
        QTest::mouseClick(w, Qt::LeftButton, Qt::NoModifier, centre);
        QVERIFY(waitFor([&] { return shell.backend()->ready(); }));
        QVERIFY(!banner->isVisible());
        QVERIFY(waitFor([&] { return statusText(w) == QStringLiteral("Connected — axiomd 0.1.0"); }));
    }

    // The whole explorer path through QML: open a folder, the tree shows it, keys drive it.
    void explorerShowsTreeAndKeyboardNavigationWorks()
    {
        const QString daemon = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        QTemporaryDir ws;
        QVERIFY(ws.isValid());
        QDir(ws.path()).mkpath("src");
        QDir(ws.path()).mkpath("docs");
        for (const char *f : { "src/main.rs", "src/lib.rs", "README.md" }) {
            QFile file(QString(ws.path()) + "/" + f);
            QVERIFY(file.open(QIODevice::WriteOnly));
            file.write("x");
        }
        ShellOptions opts;
        opts.daemonPath = daemon;
        opts.dataDir = m_data.path();
        Shell shell(opts);
        QVERIFY(shell.load());
        QQuickWindow *w = shell.window();
        QVERIFY(waitFor([&] { return shell.backend()->ready(); }));
        w->show();

        // Welcome view first: Open Folder button present, no recent placeholder text.
        QVERIFY(findByName(w->contentItem(), "openFolderButton"));
        QVERIFY(findByName(w->contentItem(), "openFolderButton")->isVisible());

        QSignalSpy opened(shell.workspace(), &WorkspaceController::opened);
        shell.workspace()->openFolder(ws.path());
        QVERIFY(opened.wait(8000));
        QVERIFY(waitFor([&] { return w->title() == QFileInfo(ws.path()).fileName() + QStringLiteral(" — Axiom"); }));
        QVERIFY(!findByName(w->contentItem(), "openFolderButton")->isVisible());

        QQuickItem *tree = findByName(w->contentItem(), "explorerTree");
        QVERIFY(tree);
        QVERIFY(waitFor([&] { return tree->property("rows").toInt() == 3; })); // docs, src, README.md
        tree->forceActiveFocus();
        QVERIFY(tree->hasActiveFocus());

        auto currentRow = [&] { return tree->property("currentRow").toInt(); };
        auto rows = [&] { return tree->property("rows").toInt(); };

        // Down selects the first row; Down again moves on.
        QTest::keyClick(w, Qt::Key_Down);
        QTRY_COMPARE(currentRow(), 0);
        QTest::keyClick(w, Qt::Key_Down);
        QTRY_COMPARE(currentRow(), 1); // src
        // Right expands src: its two files appear.
        QTest::keyClick(w, Qt::Key_Right);
        QTRY_COMPARE(rows(), 5);
        // Right again moves into the first child, Left returns to the parent, Left collapses.
        QTest::keyClick(w, Qt::Key_Right);
        QTRY_COMPARE(currentRow(), 2);
        QTest::keyClick(w, Qt::Key_Left);
        QTRY_COMPARE(currentRow(), 1);
        QTest::keyClick(w, Qt::Key_Left);
        QTRY_COMPARE(rows(), 3);
        // End / Home.
        QTest::keyClick(w, Qt::Key_End);
        QTRY_COMPARE(currentRow(), 2);
        QTest::keyClick(w, Qt::Key_Home);
        QTRY_COMPARE(currentRow(), 0);
        // Enter on a directory toggles it (src is row 1).
        QTest::keyClick(w, Qt::Key_Down);
        QTRY_COMPARE(currentRow(), 1);
        QTest::keyClick(w, Qt::Key_Return);
        QTRY_VERIFY(shell.workspace()->treeModel()->expandedPaths().contains("src"));
        QTRY_COMPARE(rows(), 5);
        QTest::keyClick(w, Qt::Key_Return);
        QTRY_VERIFY(!shell.workspace()->treeModel()->expandedPaths().contains("src"));
        QTRY_COMPARE(rows(), 3);

        // Enter on a file activates it (the signal is consumed by the editor in M1-SHELL-003).
        QSignalSpy activated(findByName(w->contentItem(), "explorerView"), SIGNAL(fileActivated(QString)));
        QTest::keyClick(w, Qt::Key_End);
        QTest::keyClick(w, Qt::Key_Return);
        QTRY_COMPARE(activated.count(), 1);
        QCOMPARE(activated.first().first().toString(), QStringLiteral("README.md"));

        w->grabWindow().save(m_shots + "/explorer.png");
    }

    // The output panel through QML: type a command, Enter runs it, rows appear, Stop works.
    void outputPanelRunsCommandsAndStops()
    {
        QVERIFY2(QFileInfo(TestSupport::emitPath()).isExecutable(), "emit helper not built");
        QTemporaryDir ws;
        QVERIFY(ws.isValid());
        ShellOptions opts;
        opts.daemonPath = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        opts.dataDir = m_data.path();
        Shell shell(opts);
        QVERIFY(shell.load());
        QQuickWindow *w = shell.window();
        QVERIFY(waitFor([&] { return shell.backend()->ready(); }));
        w->show();

        // Before a workspace is open the command field is disabled and says why.
        QQuickItem *field = findByName(w->contentItem(), "commandField");
        QVERIFY(field);
        QVERIFY(!field->isEnabled());

        QSignalSpy opened(shell.workspace(), &WorkspaceController::opened);
        shell.workspace()->openFolder(ws.path());
        QVERIFY(opened.wait(8000));
        QVERIFY(waitFor([&] { return field->isEnabled(); }));

        // Type into the real text field and press Enter.
        auto *editor = field->property("contentItem").value<QQuickItem *>();
        QVERIFY(editor);
        editor->forceActiveFocus();
        QVERIFY(waitFor([&] { return editor->hasActiveFocus(); }));
        // The command contains characters that need modifiers or are not plain keys, so it is typed
        // as text events (what an input method or keyboard layout would deliver).
        const QString command = TestSupport::emitCommand({ "out", "hi", "err", "oops", "exit", "2" });
        for (QChar c : command)
            QTest::sendKeyEvent(QTest::Click, w, Qt::Key_unknown, c, Qt::NoModifier);
        QTRY_COMPARE(field->property("editText").toString(), command);
        QSignalSpy finished(shell.tasks(), &TaskController::finished);
        QTest::keyClick(w, Qt::Key_Return);
        QVERIFY(finished.wait(10000));

        QQuickItem *list = findByName(w->contentItem(), "outputList");
        QVERIFY(list);
        QTRY_COMPARE(list->property("count").toInt(), 3);
        auto *out = shell.tasks()->outputModel();
        QCOMPARE(out->textAt(2).contains("Failed with exit code 2"), true);
        QCOMPARE(shell.tasks()->history(), QStringList{ command });
        QQuickItem *status = findByName(w->contentItem(), "taskStatus");
        QVERIFY(status && status->isVisible());
        QVERIFY(status->property("text").toString().contains("failed (2)"));
        w->grabWindow().save(m_shots + "/output-failed.png");

        // Run via the button, then stop it via the Stop button.
        QQuickItem *run = findByName(w->contentItem(), "runButton");
        QQuickItem *stop = findByName(w->contentItem(), "stopButton");
        QVERIFY(run && stop);
        QVERIFY(!stop->isEnabled());
        field->setProperty("editText", TestSupport::emitCommand({ "sleep", "30000" }));
        QTRY_VERIFY(run->isEnabled());
        auto center = [](QQuickItem *i) { return i->mapToScene(QPointF(i->width() / 2, i->height() / 2)).toPoint(); };
        QTest::mouseClick(w, Qt::LeftButton, Qt::NoModifier, center(run));
        QTRY_VERIFY(shell.tasks()->running());
        QTRY_VERIFY(stop->isEnabled());
        QVERIFY(!run->isEnabled()); // one task at a time
        QSignalSpy finished2(shell.tasks(), &TaskController::finished);
        QTest::mouseClick(w, Qt::LeftButton, Qt::NoModifier, center(stop));
        QVERIFY(finished2.wait(5000));
        QVERIFY(!shell.tasks()->running());
        QCOMPARE(finished2.first().first().toString(), QStringLiteral("cancelled"));
        QVERIFY(status->property("text").toString().contains("cancelled"));

        // Clear empties the list.
        QQuickItem *clear = findByName(w->contentItem(), "clearButton");
        QVERIFY(clear);
        QTest::mouseClick(w, Qt::LeftButton, Qt::NoModifier, center(clear));
        QTRY_COMPARE(list->property("count").toInt(), 0);
    }

    // Settings through QML: the dialog opens, a change flips Theme.dark live, About shows details.
    void settingsApplyLiveAndAboutShowsDetails()
    {
        ShellOptions opts;
        opts.daemonPath = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        QTemporaryDir data;
        opts.dataDir = data.path();
        Shell shell(opts);
        QVERIFY(shell.load());
        QQuickWindow *w = shell.window();
        QVERIFY(waitFor([&] { return shell.settings()->loaded(); }));
        QObject *theme = shell.engine()->singletonInstance<QObject *>("Axiom.Ui", "Theme");
        QVERIFY(theme);
        w->show();

        // Light: the theme setting rules, not the OS.
        // Wait for the daemon's confirmation of each value: requests are handled concurrently,
        // so two writes in flight at once may be applied in either order.
        shell.settings()->setTheme("light");
        QTRY_COMPARE(shell.settings()->theme(), QStringLiteral("light"));
        QTRY_COMPARE(theme->property("dark").toBool(), false);
        QCOMPARE(w->color().lightness() > 128, true);
        shell.settings()->setTheme("dark");
        QTRY_COMPARE(shell.settings()->theme(), QStringLiteral("dark"));
        QTRY_COMPARE(theme->property("dark").toBool(), true);
        QTRY_VERIFY(w->color().lightness() < 128);

        // Open the dialog through its QML object and change the font size with the real control.
        QObject *dialog = w->findChild<QObject *>("settingsDialog");
        QVERIFY(dialog);
        QMetaObject::invokeMethod(dialog, "open");
        QTRY_COMPARE(dialog->property("visible").toBool(), true);
        QQuickItem *spin = nullptr;
        QTRY_VERIFY((spin = dialog->findChild<QQuickItem *>("fontSizeBox")) != nullptr);
        spin->setProperty("value", 21);
        QMetaObject::invokeMethod(spin, "valueModified");
        QTRY_COMPARE(shell.settings()->editorFontSize(), 21);
        // An invalid size typed in the box is refused and the box returns to the stored value.
        QSignalSpy err(shell.settings(), &SettingsStore::errorOccurred);
        shell.settings()->setEditorFontSize(99);
        QVERIFY(err.wait(5000));
        QCOMPARE(shell.settings()->editorFontSize(), 21);
        QTRY_COMPARE(spin->property("value").toInt(), 21);
        QTRY_VERIFY(dialog->findChild<QQuickItem *>("themeBox") != nullptr);

        for (bool dark : { false, true }) {
            shell.settings()->setTheme(dark ? "dark" : "light");
            QTRY_COMPARE(shell.settings()->theme(), dark ? QStringLiteral("dark") : QStringLiteral("light"));
            QTRY_COMPARE(theme->property("dark").toBool(), dark);
            QTest::qWait(250);
            const QImage img = w->grabWindow();
            QVERIFY(img.save(m_shots + (dark ? "/settings-dark.png" : "/settings-light.png")));
        }
        QMetaObject::invokeMethod(dialog, "close");

        // About: versions and the data directory as passed on the command line.
        QObject *about = w->findChild<QObject *>("aboutDialog");
        QVERIFY(about);
        QMetaObject::invokeMethod(about, "open");
        QTRY_COMPARE(about->property("visible").toBool(), true);
        auto text = [&](const char *name) {
            QObject *o = about->findChild<QObject *>(name);
            return o ? o->property("text").toString() : QString();
        };
        QTRY_VERIFY(text("aboutBackend").startsWith("axiomd 0.1.0"));
        QCOMPARE(text("aboutProtocol"), QStringLiteral("0.1"));
        QCOMPARE(text("aboutQt"), QString::fromLatin1(qVersion()));
        QCOMPARE(text("aboutDataDir"), data.path());
        w->grabWindow().save(m_shots + "/about.png");
        QMetaObject::invokeMethod(about, "close");
    }

    void aboutShowsDefaultLocationWithoutDataDir()
    {
        ShellOptions opts;
        opts.daemonPath = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        opts.dataDir = m_data.path(); // the daemon needs one in tests; the label is what we check below
        Shell shell(opts);
        QVERIFY(shell.load());
        QCOMPARE(shell.engine()->rootContext()->contextProperty("dataDirText").toString(), m_data.path());
        // No --data-dir: the daemon would use the default location. Tests never touch the user's
        // profile, so redirect that default to a temp dir through the environment.
        QTemporaryDir isolated;
        qputenv("AXIOM_DATA_DIR", isolated.path().toUtf8());
        ShellOptions none;
        none.daemonPath = opts.daemonPath;
        Shell shell2(none);
        QVERIFY(shell2.load());
        QCOMPARE(shell2.engine()->rootContext()->contextProperty("dataDirText").toString(), QStringLiteral("default location"));
        qunsetenv("AXIOM_DATA_DIR");
    }

    void missingDaemonShowsFailureNotACrash()
    {
        ShellOptions opts;
        opts.daemonPath = QStringLiteral("/no/such/axiomd");
        Shell shell(opts);
        QVERIFY(shell.load());
        QVERIFY(waitFor([&] { return shell.backend()->failed(); }));
        QVERIFY(waitFor([&] { return statusText(shell.window()) == QStringLiteral("Backend stopped"); }));
    }
};

// WebEngine needs the scheme registered and initialised before the application exists.
int main(int argc, char *argv[])
{
    initEditorWebEngine();
    QGuiApplication app(argc, argv);
    TstSmokeLaunch tc;
    return QTest::qExec(&tc, argc, argv);
}
#include "tst_smoke_launch.moc"
