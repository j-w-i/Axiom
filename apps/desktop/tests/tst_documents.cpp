#include "daemon/DaemonClient.h"
#include "models/OpenDocumentsModel.h"
#include "models/WorkspaceController.h"

#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonObject>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>

// OpenDocumentsModel against the real daemon, with a fake editor page. The
// fake records every shell -> page signal and speaks the page -> shell slots,
// so the whole bridge contract (editor-bridge.md) is exercised without Qt WebEngine.

struct FakePage
{
    struct Msg
    {
        QString name;
        QJsonObject p;
    };
    QList<Msg> log;
    EditorBridge *b;

    explicit FakePage(EditorBridge *bridge) : b(bridge)
    {
        auto rec = [this](const char *n) {
            return [this, n](const QJsonObject &p) { log.append({ QString::fromLatin1(n), p }); };
        };
        QObject::connect(b, &EditorBridge::configure, b, rec("configure"));
        QObject::connect(b, &EditorBridge::openDocument, b, rec("openDocument"));
        QObject::connect(b, &EditorBridge::activateDocument, b, rec("activateDocument"));
        QObject::connect(b, &EditorBridge::closeDocument, b, rec("closeDocument"));
        QObject::connect(b, &EditorBridge::requestSave, b, rec("requestSave"));
        QObject::connect(b, &EditorBridge::documentSaved, b, rec("documentSaved"));
        QObject::connect(b, &EditorBridge::saveFailed, b, rec("saveFailed"));
        QObject::connect(b, &EditorBridge::revealPosition, b, rec("revealPosition"));
        QObject::connect(b, &EditorBridge::runCommand, b, rec("runCommand"));
    }
    int count(const QString &name) const
    {
        int n = 0;
        for (const Msg &m : log)
            n += m.name == name;
        return n;
    }
    QJsonObject last(const QString &name) const
    {
        for (int i = log.size() - 1; i >= 0; --i)
            if (log.at(i).name == name)
                return log.at(i).p;
        return {};
    }
    QStringList names() const
    {
        QStringList n;
        for (const Msg &m : log)
            n << m.name;
        return n;
    }
    void ready(int version = 1) { b->ready({ { "bridgeVersion", version }, { "monacoVersion", "0.57.0" } }); }
    void dirty(const QString &path, bool d) { b->dirtyChanged({ { "path", path }, { "dirty", d } }); }
    void saveRequested(const QString &path, const QString &text) { b->saveRequested({ { "path", path }, { "text", text } }); }
    void snapshot(const QString &path, const QString &text) { b->textSnapshot({ { "path", path }, { "text", text } }); }
};

static void writeFile(const QString &path, const QByteArray &data)
{
    QDir().mkpath(QFileInfo(path).absolutePath());
    QFile f(path);
    QVERIFY(f.open(QIODevice::WriteOnly | QIODevice::Truncate));
    f.write(data);
}
static QByteArray readFile(const QString &path)
{
    QFile f(path);
    return f.open(QIODevice::ReadOnly) ? f.readAll() : QByteArray();
}

class TstDocuments : public QObject
{
    Q_OBJECT
    QString m_daemon;

    template<typename F>
    bool waitFor(F done, int ms = 5000)
    {
        QElapsedTimer t;
        t.start();
        while (!done() && t.elapsed() < ms)
            QTest::qWait(10);
        return done();
    }

    struct Env
    {
        DaemonClient client;
        WorkspaceController ws;
        OpenDocumentsModel docs;
        QTemporaryDir data;
        QTemporaryDir dir;
        FakePage page;
        Env(const QString &daemon)
            : ws(&client), docs(&client, &ws), page(docs.editorBridge())
        {
            writeFile(dir.path() + "/src/main.rs", "fn main() {}\n");
            writeFile(dir.path() + "/src/lib.rs", "pub fn f() {}\n");
            writeFile(dir.path() + "/a.txt", "one\n");
            writeFile(dir.path() + "/crlf.txt", "a\r\nb\r\n");
            writeFile(dir.path() + "/bad.bin", QByteArray("\xff\xfe\x00z", 4));
            client.start(daemon, { "--data-dir", data.path() });
        }
        bool openWorkspace()
        {
            QElapsedTimer t;
            t.start();
            while (client.state() != DaemonClient::Ready && t.elapsed() < 10000)
                QTest::qWait(10);
            QSignalSpy opened(&ws, &WorkspaceController::opened);
            ws.openFolder(dir.path());
            return opened.wait(8000);
        }
        QString path(const QString &rel) const { return dir.path() + "/" + rel; }
    };

    bool openDoc(Env &e, const QString &rel)
    {
        QSignalSpy s(&e.docs, &OpenDocumentsModel::documentOpened);
        e.docs.open(rel);
        return s.count() > 0 || s.wait(5000);
    }

private slots:
    void initTestCase()
    {
        m_daemon = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        QVERIFY2(QFileInfo(m_daemon).isExecutable(), qPrintable("axiomd not built: " + m_daemon));
    }

    // ---- handshake and opening

    void handshakeSendsConfigureThenOpenDocumentsThenActivate()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        QVERIFY(openDoc(e, "src/main.rs"));
        QVERIFY(openDoc(e, "a.txt"));
        QCOMPARE(e.page.count("openDocument"), 0); // nothing before `ready`
        QCOMPARE(e.docs.count(), 2);

        e.page.ready();
        QVERIFY(e.docs.editorReady());
        QVERIFY(waitFor([&] { return e.page.count("openDocument") >= 2; }));
        const QStringList n = e.page.names();
        QCOMPARE(n.first(), QStringLiteral("configure"));
        QCOMPARE(n.last(), QStringLiteral("activateDocument"));
        QCOMPARE(e.page.last("configure").value("theme").toString(), QStringLiteral("light"));
        QCOMPARE(e.page.last("activateDocument").value("path").toString(), QStringLiteral("a.txt"));
    }

    void wrongBridgeVersionIsRefusedAndNothingIsEmitted()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready(2);
        QVERIFY(!e.docs.editorReady());
        QVERIFY(!e.docs.bannerText().isEmpty());
        QVERIFY(openDoc(e, "a.txt"));
        QCOMPARE(e.page.log.size(), 0);
    }

    void openSendsTextVersionAndActivate()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "src/main.rs"));
        QCOMPARE(e.page.count("openDocument"), 1);
        const QJsonObject o = e.page.last("openDocument");
        QCOMPARE(o.value("path").toString(), QStringLiteral("src/main.rs"));
        QCOMPARE(o.value("text").toString(), QStringLiteral("fn main() {}\n"));
        QCOMPARE(o.value("version").toString().size(), 64);
        QCOMPARE(o.value("activate").toBool(), true);
        QCOMPARE(o.value("dirty").toBool(), false);
        QCOMPARE(e.docs.activePath(), QStringLiteral("src/main.rs"));
        QCOMPARE(e.docs.savedVersionOf("src/main.rs"), o.value("version").toString());
    }

    void crlfSurvivesTheRoundTrip()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "crlf.txt"));
        QCOMPARE(e.page.last("openDocument").value("text").toString(), QStringLiteral("a\r\nb\r\n"));
        e.page.saveRequested("crlf.txt", "a\r\nb\r\nc\r\n");
        QVERIFY(waitFor([&] { return e.page.count("documentSaved") == 1; }));
        QCOMPARE(readFile(e.path("crlf.txt")), QByteArray("a\r\nb\r\nc\r\n"));
    }

    void openingAnOpenFileOnlyActivates()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "src/main.rs"));
        QVERIFY(openDoc(e, "a.txt"));
        e.page.log.clear();
        e.docs.open("src/main.rs");
        QCOMPARE(e.docs.count(), 2);
        QCOMPARE(e.page.count("openDocument"), 0);
        QCOMPARE(e.page.names(), QStringList{ "activateDocument" });
        QCOMPARE(e.docs.activePath(), QStringLiteral("src/main.rs"));
    }

    void unsupportedEncodingAndMissingFilesShowABanner()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        e.docs.open("bad.bin");
        QVERIFY(waitFor([&] { return !e.docs.bannerText().isEmpty(); }));
        QVERIFY2(e.docs.bannerText().startsWith("Cannot open bad.bin:"), qPrintable(e.docs.bannerText()));
        QVERIFY(e.docs.bannerText().contains("UTF-8"));
        QCOMPARE(e.docs.count(), 0);
        e.docs.dismissBanner();
        QVERIFY(e.docs.bannerText().isEmpty());

        e.docs.open("nope.txt");
        QVERIFY(waitFor([&] { return !e.docs.bannerText().isEmpty(); }));
        QVERIFY(e.docs.bannerText().startsWith("Cannot open nope.txt:"));
        QCOMPARE(e.page.count("openDocument"), 0);
    }

    void tooLargeFilesShowABanner()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        QFile f(e.path("big.txt"));
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.resize(17 * 1024 * 1024);
        f.close();
        e.docs.open("big.txt");
        QVERIFY(waitFor([&] { return !e.docs.bannerText().isEmpty(); }));
        QVERIFY(e.docs.bannerText().contains("too large"));
    }

    // ---- dirty tracking, cursor, snapshots

    void dirtyAndCursorAreForwardedToTheModel()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        QSignalSpy dirtyState(&e.docs, &OpenDocumentsModel::dirtyStateChanged);
        e.page.dirty("a.txt", true);
        QVERIFY(e.docs.isDirty("a.txt"));
        QVERIFY(e.docs.hasDirty());
        QCOMPARE(dirtyState.count(), 1);
        e.page.dirty("a.txt", true); // no change: no signal
        QCOMPARE(dirtyState.count(), 1);
        e.page.dirty("nope", true);  // unknown document: ignored
        e.page.b->dirtyChanged({});

        e.page.b->cursorChanged({ { "path", "a.txt" }, { "line", 3 }, { "column", 7 }, { "selectionLength", 0 } });
        QCOMPARE(e.docs.cursorText(), QStringLiteral("Ln 3, Col 7"));
        e.page.b->cursorChanged({ { "path", "a.txt" }, { "line", 1 }, { "column", 2 }, { "selectionLength", 4 } });
        QCOMPARE(e.docs.cursorText(), QStringLiteral("Ln 1, Col 2 (4 selected)"));
        e.page.b->cursorChanged({ { "path", "other" }, { "line", 9 }, { "column", 9 }, { "selectionLength", 0 } });
        QCOMPARE(e.docs.cursorText(), QStringLiteral("Ln 1, Col 2 (4 selected)")); // not the active document
    }

    // ---- saving

    void saveWritesWithExpectedVersionAndAnswersExactlyOnce()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        const QString v0 = e.docs.savedVersionOf("a.txt");
        e.page.dirty("a.txt", true);
        e.page.saveRequested("a.txt", "two\n");
        QVERIFY(waitFor([&] { return e.page.count("documentSaved") == 1; }));
        QCOMPARE(e.page.count("saveFailed"), 0);
        QCOMPARE(readFile(e.path("a.txt")), QByteArray("two\n"));
        QVERIFY(e.docs.savedVersionOf("a.txt") != v0);
        QCOMPARE(e.page.last("documentSaved").value("version").toString(), e.docs.savedVersionOf("a.txt"));
        QTest::qWait(500); // the watcher reports our own write: it must not cause a reload or a second answer
        QCOMPARE(e.page.count("documentSaved"), 1);
        QCOMPARE(e.page.count("openDocument"), 1);
        // A second save uses the new version and succeeds.
        e.page.saveRequested("a.txt", "three\n");
        QVERIFY(waitFor([&] { return e.page.count("documentSaved") == 2; }));
        QCOMPARE(readFile(e.path("a.txt")), QByteArray("three\n"));
    }

    void saveRequestedForAnUnknownPathIsIgnored()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        e.page.saveRequested("never/opened.txt", "x");
        QTest::qWait(300);
        QCOMPARE(e.page.count("documentSaved") + e.page.count("saveFailed"), 0);
        QVERIFY(!QFileInfo::exists(e.path("never/opened.txt")));
    }

    void conflictOverwriteRetriesAndAnswersSaved()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        writeFile(e.path("a.txt"), "changed elsewhere\n");
        e.page.dirty("a.txt", true);
        e.page.saveRequested("a.txt", "mine\n");
        QVERIFY(waitFor([&] { return e.docs.conflictActive(); }));
        QCOMPARE(e.docs.conflictName(), QStringLiteral("a.txt"));
        QCOMPARE(e.page.count("documentSaved") + e.page.count("saveFailed"), 0); // unanswered while the dialog is open
        QCOMPARE(readFile(e.path("a.txt")), QByteArray("changed elsewhere\n"));

        e.docs.resolveConflict("overwrite");
        QVERIFY(waitFor([&] { return e.page.count("documentSaved") == 1; }));
        QCOMPARE(readFile(e.path("a.txt")), QByteArray("mine\n"));
        QVERIFY(!e.docs.conflictActive());
        e.docs.resolveConflict("overwrite"); // second answer: ignored
        QTest::qWait(200);
        QCOMPARE(e.page.count("documentSaved"), 1);
    }

    void conflictReloadShowsTheDiskVersionAsClean()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        writeFile(e.path("a.txt"), "theirs\n");
        e.page.dirty("a.txt", true);
        e.page.saveRequested("a.txt", "mine\n");
        QVERIFY(waitFor([&] { return e.docs.conflictActive(); }));
        const int opens = e.page.count("openDocument");
        e.docs.resolveConflict("reload");
        QVERIFY(waitFor([&] { return e.page.count("openDocument") == opens + 1; }));
        const QJsonObject o = e.page.last("openDocument");
        QCOMPARE(o.value("text").toString(), QStringLiteral("theirs\n"));
        QCOMPARE(o.value("dirty").toBool(), false);
        QCOMPARE(e.page.count("saveFailed"), 1); // the page's save request is over
        QVERIFY(!e.docs.isDirty("a.txt"));
        QCOMPARE(readFile(e.path("a.txt")), QByteArray("theirs\n"));
    }

    void conflictCancelAnswersSaveFailedAndKeepsDirty()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        writeFile(e.path("a.txt"), "theirs\n");
        e.page.dirty("a.txt", true);
        e.page.saveRequested("a.txt", "mine\n");
        QVERIFY(waitFor([&] { return e.docs.conflictActive(); }));
        e.docs.resolveConflict("cancel");
        QCOMPARE(e.page.count("saveFailed"), 1);
        QCOMPARE(e.page.count("documentSaved"), 0);
        QVERIFY(e.docs.isDirty("a.txt"));
        QCOMPARE(readFile(e.path("a.txt")), QByteArray("theirs\n"));
    }

    void otherSaveErrorsShowABannerAndAnswerSaveFailed()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "src/main.rs"));
        // Remove the parent directory so the write fails with not_found.
        QVERIFY(QFile::remove(e.path("src/main.rs")));
        QVERIFY(QFile::remove(e.path("src/lib.rs")));
        QVERIFY(QDir(e.path("src")).removeRecursively());
        e.page.dirty("src/main.rs", true);
        e.page.saveRequested("src/main.rs", "x");
        QVERIFY(waitFor([&] { return e.page.count("saveFailed") == 1; }));
        QVERIFY(e.docs.bannerText().startsWith("Could not save main.rs:"));
        QVERIFY(!e.docs.conflictActive());
    }

    void menuSaveAsksThePageWhichAnswersWithSaveRequested()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        e.docs.save("a.txt");
        QCOMPARE(e.page.last("requestSave").value("path").toString(), QStringLiteral("a.txt"));
        e.page.dirty("a.txt", true);
        e.page.dirty("src/x", true);
        e.page.log.clear();
        e.docs.saveAll();
        QCOMPARE(e.page.count("requestSave"), 1); // only dirty documents
    }

    // ---- external changes

    void cleanDocumentReloadsWhenTheFileChangesOnDisk()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        const int opens = e.page.count("openDocument");
        writeFile(e.path("a.txt"), "edited outside\n");
        QVERIFY2(waitFor([&] { return e.page.count("openDocument") == opens + 1; }, 4000), "no reload within 4 s");
        const QJsonObject o = e.page.last("openDocument");
        QCOMPARE(o.value("text").toString(), QStringLiteral("edited outside\n"));
        QCOMPARE(o.value("dirty").toBool(), false);
        QVERIFY(!e.docs.isChangedOnDisk("a.txt"));
    }

    void dirtyDocumentIsMarkedChangedOnDiskNotReloaded()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        e.page.dirty("a.txt", true);
        const int opens = e.page.count("openDocument");
        writeFile(e.path("a.txt"), "edited outside\n");
        QVERIFY(waitFor([&] { return e.docs.isChangedOnDisk("a.txt"); }, 4000));
        QCOMPARE(e.page.count("openDocument"), opens); // the user's text is never replaced
        // And the next save hits the conflict flow.
        e.page.saveRequested("a.txt", "mine\n");
        QVERIFY(waitFor([&] { return e.docs.conflictActive(); }));
        QCOMPARE(e.page.count("documentSaved"), 0);
    }

    // ---- close / tabs

    void closingACleanDocumentDisposesAndActivatesNeighbour()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        QVERIFY(openDoc(e, "src/main.rs"));
        QVERIFY(openDoc(e, "src/lib.rs"));
        QCOMPARE(e.docs.activePath(), QStringLiteral("src/lib.rs"));
        e.page.log.clear();
        e.docs.close("src/lib.rs");
        QCOMPARE(e.docs.count(), 2);
        QCOMPARE(e.page.last("closeDocument").value("path").toString(), QStringLiteral("src/lib.rs"));
        QCOMPARE(e.docs.activePath(), QStringLiteral("src/main.rs"));
        e.docs.close("src/main.rs");
        e.docs.close("a.txt");
        QCOMPARE(e.docs.count(), 0);
        QVERIFY(e.docs.activePath().isEmpty());
    }

    void cycleWrapsAround()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        QVERIFY(openDoc(e, "src/main.rs"));
        QVERIFY(openDoc(e, "src/lib.rs"));
        e.docs.cycle(1);
        QCOMPARE(e.docs.activePath(), QStringLiteral("a.txt"));
        e.docs.cycle(-1);
        QCOMPARE(e.docs.activePath(), QStringLiteral("src/lib.rs"));
        e.docs.activate("src/main.rs");
        QCOMPARE(e.page.last("activateDocument").value("path").toString(), QStringLiteral("src/main.rs"));
    }

    void closingADirtyDocumentPromptsAndEachChoiceDoesWhatItSays()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        e.page.dirty("a.txt", true);

        e.docs.close("a.txt");
        QVERIFY(e.docs.promptActive());
        QCOMPARE(e.docs.promptKind(), QStringLiteral("close"));
        QCOMPARE(e.docs.promptNames(), QStringList{ "a.txt" });
        QCOMPARE(e.docs.count(), 1);

        e.docs.resolvePrompt("cancel");
        QVERIFY(!e.docs.promptActive());
        QCOMPARE(e.docs.count(), 1);
        QVERIFY(e.docs.isDirty("a.txt"));

        e.docs.close("a.txt");
        e.docs.resolvePrompt("discard");
        QCOMPARE(e.docs.count(), 0);
        QCOMPARE(readFile(e.path("a.txt")), QByteArray("one\n")); // untouched

        // Save: asks the page, then closes once the save completed.
        QVERIFY(openDoc(e, "a.txt"));
        e.page.dirty("a.txt", true);
        e.docs.close("a.txt");
        e.docs.resolvePrompt("save");
        QVERIFY(waitFor([&] { return e.page.count("requestSave") >= 1; }));
        QCOMPARE(e.docs.count(), 1); // not closed yet: waiting for the save
        e.page.saveRequested("a.txt", "saved on close\n");
        QVERIFY(waitFor([&] { return e.docs.count() == 0; }));
        QCOMPARE(readFile(e.path("a.txt")), QByteArray("saved on close\n"));
    }

    void saveInThePromptThenCancelledConflictKeepsTheDocument()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        e.page.dirty("a.txt", true);
        e.docs.close("a.txt");
        e.docs.resolvePrompt("save");
        writeFile(e.path("a.txt"), "theirs\n");
        e.page.saveRequested("a.txt", "mine\n");
        QVERIFY(waitFor([&] { return e.docs.conflictActive(); }));
        e.docs.resolveConflict("cancel");
        QTest::qWait(200);
        QCOMPARE(e.docs.count(), 1); // a cancelled save must not close the tab
        QVERIFY(e.docs.isDirty("a.txt"));
    }

    void quitAndFolderChangesListAllDirtyDocuments()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        QVERIFY(openDoc(e, "src/main.rs"));
        QVERIFY(openDoc(e, "src/lib.rs"));
        e.page.dirty("a.txt", true);
        e.page.dirty("src/lib.rs", true);

        QSignalSpy quit(&e.docs, &OpenDocumentsModel::quitConfirmed);
        e.docs.requestQuit();
        QVERIFY(e.docs.promptActive());
        QCOMPARE(e.docs.promptKind(), QStringLiteral("quit"));
        QCOMPARE(e.docs.promptNames(), (QStringList{ "a.txt", "lib.rs" }));
        e.docs.resolvePrompt("cancel");
        QCOMPARE(quit.count(), 0);
        QVERIFY(e.ws.isOpen());

        e.docs.requestCloseFolder();
        QCOMPARE(e.docs.promptKind(), QStringLiteral("folder"));
        e.docs.resolvePrompt("cancel");
        QVERIFY(e.ws.isOpen());
        QCOMPARE(e.docs.count(), 3);

        e.docs.requestQuit();
        e.docs.resolvePrompt("discard");
        QCOMPARE(quit.count(), 1);
    }

    void cleanQuitAndFolderCloseDoNotPrompt()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        QSignalSpy quit(&e.docs, &OpenDocumentsModel::quitConfirmed);
        e.docs.requestQuit();
        QCOMPARE(quit.count(), 1);
        QVERIFY(!e.docs.promptActive());
        e.docs.requestCloseFolder();
        QVERIFY(!e.docs.promptActive());
        QVERIFY(!e.ws.isOpen());
        QCOMPARE(e.docs.count(), 0);
        QVERIFY(e.page.count("closeDocument") >= 1);
    }

    // ---- commands

    void runCommandAndRevealGoToThePage()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        e.docs.runCommand("undo"); // no document: nothing to undo
        QCOMPARE(e.page.count("runCommand"), 0);
        QVERIFY(openDoc(e, "a.txt"));
        e.docs.runCommand("find");
        QCOMPARE(e.page.last("runCommand").value("command").toString(), QStringLiteral("find"));
        e.docs.revealPosition("a.txt", 4, 2);
        QCOMPARE(e.page.last("revealPosition").value("line").toInt(), 4);
        QCOMPARE(e.page.last("revealPosition").value("column").toInt(), 2);
        e.docs.setAppearance(true, 16, "Fira Code", true);
        const QJsonObject c = e.page.last("configure");
        QCOMPARE(c.value("theme").toString(), QStringLiteral("dark"));
        QCOMPARE(c.value("fontSize").toInt(), 16);
        QCOMPARE(c.value("fontFamily").toString(), QStringLiteral("Fira Code"));
        QCOMPARE(c.value("wordWrap").toBool(), true);
    }

    // ---- tab persistence

    void tabsAndActiveTabAreRestoredSkippingMissingFiles()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        QVERIFY(openDoc(e, "src/main.rs"));
        QVERIFY(openDoc(e, "src/lib.rs"));
        e.docs.activate("src/main.rs");
        QTest::qWait(900); // debounced save (500 ms)

        QSignalSpy closed(&e.ws, &WorkspaceController::closed);
        e.ws.close();
        QCOMPARE(closed.count(), 1);
        QCOMPARE(e.docs.count(), 0);
        QTest::qWait(300);
        QVERIFY(QFile::remove(e.path("a.txt"))); // gone: must be skipped without a banner

        QSignalSpy opened(&e.docs, &OpenDocumentsModel::documentOpened);
        QSignalSpy wsOpened(&e.ws, &WorkspaceController::opened);
        e.ws.openFolder(e.dir.path());
        QVERIFY(wsOpened.wait(8000));
        QVERIFY(waitFor([&] { return e.docs.count() == 2 && e.docs.activePath() == "src/main.rs"; }));
        QVERIFY(e.docs.bannerText().isEmpty());
        QCOMPARE(e.docs.data(e.docs.index(0), OpenDocumentsModel::PathRole).toString(), QStringLiteral("src/main.rs"));
        QCOMPARE(e.docs.data(e.docs.index(1), OpenDocumentsModel::PathRole).toString(), QStringLiteral("src/lib.rs"));
    }

    // ---- renderer crash recovery (model level)

    void unsavedTextComesBackFromTheLatestSnapshotAfterACrash()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        QVERIFY(openDoc(e, "src/main.rs"));
        e.page.dirty("a.txt", true);
        e.page.snapshot("a.txt", "typed but unsaved");
        e.page.snapshot("a.txt", "typed but unsaved, latest");
        // a clean document sends no snapshots

        e.docs.editorCrashed();
        QVERIFY(e.docs.rendererCrashed());
        QVERIFY(!e.docs.editorReady());
        e.page.log.clear();

        e.page.ready(); // the reloaded page performs the handshake again
        QVERIFY(!e.docs.rendererCrashed());
        QVERIFY(waitFor([&] { return e.page.count("openDocument") >= 2; }));
        QJsonObject a, m;
        for (const auto &msg : e.page.log)
            if (msg.name == "openDocument")
                (msg.p.value("path").toString() == "a.txt" ? a : m) = msg.p;
        QCOMPARE(a.value("text").toString(), QStringLiteral("typed but unsaved, latest"));
        QCOMPARE(a.value("dirty").toBool(), true);
        QCOMPARE(m.value("dirty").toBool(), false); // the clean one is re-read from disk
        QVERIFY(e.docs.isDirty("a.txt"));
        QCOMPARE(readFile(e.path("a.txt")), QByteArray("one\n")); // disk untouched
        // The dirty document can now be saved and the unsaved edit becomes the file.
        e.page.saveRequested("a.txt", "typed but unsaved, latest");
        QVERIFY(waitFor([&] { return e.page.count("documentSaved") == 1; }));
    }

    void switchingWorkspacesClosesDocumentsAndIgnoresLateAnswers()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.page.ready();
        QVERIFY(openDoc(e, "a.txt"));
        QTemporaryDir other;
        writeFile(other.path() + "/z.txt", "zed");
        QSignalSpy wsOpened(&e.ws, &WorkspaceController::opened);
        e.ws.openFolder(other.path());
        QVERIFY(wsOpened.wait(8000));
        QCOMPARE(e.docs.count(), 0);
        QVERIFY(e.page.count("closeDocument") >= 1);
        QVERIFY(openDoc(e, "z.txt"));
        QCOMPARE(e.page.last("openDocument").value("text").toString(), QStringLiteral("zed"));
    }

    void linksOnlyOpenForHttpSchemes()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        QTest::ignoreMessage(QtWarningMsg, QRegularExpression("refused to open a link with scheme file"));
        e.docs.bridgeOpenLink("file:///etc/passwd");
        QTest::ignoreMessage(QtWarningMsg, QRegularExpression("refused to open a link with scheme javascript"));
        e.docs.bridgeOpenLink("javascript:alert(1)");
    }

    void pageErrorsAreLoggedNotFatal()
    {
        Env e(m_daemon);
        QTest::ignoreMessage(QtWarningMsg, QRegularExpression("\\[editor\\] page error: boom"));
        e.page.b->reportError({ { "message", "boom" }, { "detail", "stack" } });
    }
};

QTEST_GUILESS_MAIN(TstDocuments)
#include "tst_documents.moc"
