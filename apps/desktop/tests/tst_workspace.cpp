#include "daemon/DaemonClient.h"
#include "models/FileTreeModel.h"
#include "models/WorkspaceController.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QJsonArray>
#include <QJsonObject>
#include <QtTest>
#include "TestSupport.h"

// Real daemon, a temp copy of tests/fixtures/basic. The controller and model
// under test never touch the filesystem; only the test does, to cause changes.

static void copyDir(const QString &from, const QString &to)
{
    QDir().mkpath(to);
    for (const QFileInfo &fi : QDir(from).entryInfoList(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden)) {
        const QString dest = to + "/" + fi.fileName();
        if (fi.isDir())
            copyDir(fi.absoluteFilePath(), dest);
        else
            QFile::copy(fi.absoluteFilePath(), dest);
    }
}

class TstWorkspace : public QObject
{
    Q_OBJECT

    QString m_daemon;
    QTemporaryDir m_data;
    QString m_fixtures;

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
        WorkspaceController ctl;
        QTemporaryDir dir;
        Env(const QString &daemon, const QString &data, const QString &fixtures)
            : ctl(&client)
        {
            client.start(daemon, { "--data-dir", data });
            copyDir(fixtures, dir.path());
            QDir(dir.path()).mkpath(".git");
            QDir(dir.path()).mkpath("target");
            QFile f(dir.path() + "/build.log");
            if (f.open(QIODevice::WriteOnly))
                f.write("log");
        }
        FileTreeModel *tree() { return ctl.treeModel(); }
    };

    // Canonical path in the same form the daemon reports it (native separators;
    // QDir::canonicalPath() uses '/' even on Windows).
    static QString canon(const QString &path)
    {
        return QDir::toNativeSeparators(QFileInfo(path).canonicalFilePath());
    }

    static QStringList names(FileTreeModel *m, const QModelIndex &parent = {})
    {
        QStringList out;
        for (int i = 0; i < m->rowCount(parent); ++i)
            out << m->data(m->index(i, 0, parent), FileTreeModel::NameRole).toString();
        return out;
    }
    static QModelIndex child(FileTreeModel *m, const QString &name, const QModelIndex &parent = {})
    {
        for (int i = 0; i < m->rowCount(parent); ++i)
            if (m->data(m->index(i, 0, parent), FileTreeModel::NameRole).toString() == name)
                return m->index(i, 0, parent);
        return {};
    }

    bool openAndWait(Env &e)
    {
        if (!waitFor([&] { return e.client.state() == DaemonClient::Ready; }, 10000))
            return false;
        QSignalSpy opened(&e.ctl, &WorkspaceController::opened);
        e.ctl.openFolder(e.dir.path());
        return opened.wait(8000);
    }

private slots:
    void initTestCase()
    {
        m_daemon = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        QVERIFY2(QFileInfo(m_daemon).isExecutable(), qPrintable("axiomd not built: " + m_daemon));
        m_fixtures = QDir::cleanPath(QStringLiteral(AXIOM_FIXTURES_DIR) + "/basic");
        QVERIFY2(QDir(m_fixtures).exists(), qPrintable(m_fixtures));
        QVERIFY(m_data.isValid());
    }

    void listsRootEntriesInProtocolOrder()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        QVERIFY(e.ctl.isOpen());
        QCOMPARE(e.ctl.id(), QStringLiteral("w1"));
        QCOMPARE(e.ctl.name(), QFileInfo(e.dir.path()).fileName());
        // directories first, then files, case-insensitive: exactly the daemon's order.
        QCOMPARE(names(e.tree()), (QStringList{ ".git", "data", "notes", "src", "target", ".gitignore", "build.log", "README.md" }));
        QVERIFY(e.tree()->rootLoaded());
    }

    void ignoredFlagAndKindsAreExposed()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        auto *t = e.tree();
        auto ignored = [&](const QString &n) { return t->data(child(t, n), FileTreeModel::IgnoredRole).toBool(); };
        QVERIFY(ignored(".git"));
        QVERIFY(ignored("target"));
        QVERIFY(ignored("build.log"));
        QVERIFY(!ignored("src"));
        QVERIFY(!ignored("README.md"));
        QCOMPARE(t->data(child(t, "src"), FileTreeModel::KindRole).toString(), QStringLiteral("directory"));
        QCOMPARE(t->data(child(t, "README.md"), FileTreeModel::KindRole).toString(), QStringLiteral("file"));
        QCOMPARE(t->data(child(t, "src"), FileTreeModel::PathRole).toString(), QStringLiteral("src"));
        QVERIFY(t->hasChildren(child(t, "src")));
        QVERIFY(!t->hasChildren(child(t, "README.md")));
    }

    void expandingListsChildrenLazily()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        auto *t = e.tree();
        const QModelIndex src = child(t, "src");
        QCOMPARE(t->rowCount(src), 0); // not fetched yet
        QVERIFY(t->canFetchMore(src));
        t->setExpanded(src, true);
        QVERIFY(waitFor([&] { return t->rowCount(src) == 2; }));
        QCOMPARE(names(t, src), (QStringList{ "lib.rs", "main.rs" }));
        QCOMPARE(t->data(child(t, "main.rs", src), FileTreeModel::PathRole).toString(), QStringLiteral("src/main.rs"));
        QVERIFY(!t->canFetchMore(src));
        QCOMPARE(t->expandedPaths(), QStringList{ "src" });
    }

    void newFileAppearsWithoutCollapsingOtherDirectories()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        auto *t = e.tree();
        t->setExpanded(child(t, "src"), true);
        t->setExpanded(child(t, "notes"), true);
        QVERIFY(waitFor([&] { return t->rowCount(child(t, "src")) == 2 && t->rowCount(child(t, "notes")) == 1; }));

        QFile f(e.dir.path() + "/src/new.rs");
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.write("fn x() {}");
        f.close();

        QVERIFY2(waitFor([&] { return t->rowCount(child(t, "src")) == 3; }, 3000), "new file did not appear within 3 s");
        QCOMPARE(names(t, child(t, "src")), (QStringList{ "lib.rs", "main.rs", "new.rs" }));
        // Other expanded directories are untouched.
        QCOMPARE(names(t, child(t, "notes")), QStringList{ "crlf.txt" });
        QCOMPARE(t->expandedPaths(), (QStringList{ "notes", "src" }));

        // Deleting removes the row again.
        QVERIFY(QFile::remove(e.dir.path() + "/src/new.rs"));
        QVERIFY2(waitFor([&] { return t->rowCount(child(t, "src")) == 2; }, 3000), "deleted file did not disappear");
        QCOMPARE(names(t, child(t, "src")), (QStringList{ "lib.rs", "main.rs" }));
        QCOMPARE(t->expandedPaths(), (QStringList{ "notes", "src" }));
    }

    void changesUseMinimalRowSignals()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        auto *t = e.tree();
        t->setExpanded(child(t, "src"), true);
        QVERIFY(waitFor([&] { return t->rowCount(child(t, "src")) == 2; }));
        QSignalSpy reset(t, &QAbstractItemModel::modelReset);
        QSignalSpy inserted(t, &QAbstractItemModel::rowsInserted);
        QSignalSpy removed(t, &QAbstractItemModel::rowsRemoved);
        QFile f(e.dir.path() + "/src/zz.rs");
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.close();
        QVERIFY(waitFor([&] { return inserted.count() == 1; }, 3000));
        QCOMPARE(reset.count(), 0);
        QCOMPARE(removed.count(), 0);
        QCOMPARE(inserted.first().at(1).toInt(), 2); // appended after lib.rs, main.rs
    }

    void newTopLevelFileAndRename()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        auto *t = e.tree();
        QVERIFY(QFile::rename(e.dir.path() + "/README.md", e.dir.path() + "/ABOUT.md"));
        QVERIFY(waitFor([&] { return child(t, "ABOUT.md").isValid() && !child(t, "README.md").isValid(); }, 3000));
    }

    void collapsedDirectoriesAreRelistedWhenExpandedAgain()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        auto *t = e.tree();
        const QModelIndex src = child(t, "src");
        t->setExpanded(src, true);
        QVERIFY(waitFor([&] { return t->rowCount(src) == 2; }));
        t->setExpanded(src, false);
        QFile f(e.dir.path() + "/src/while-collapsed.rs");
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.close();
        QTest::qWait(400);
        t->setExpanded(src, true);
        QVERIFY(waitFor([&] { return t->rowCount(src) == 3; }));
    }

    void expandedStateIsRestoredAfterCloseAndReopen()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        auto *t = e.tree();
        t->setExpanded(child(t, "src"), true);
        t->setExpanded(child(t, "notes"), true);
        QVERIFY(waitFor([&] { return t->rowCount(child(t, "src")) == 2; }));

        // Close flushes the explorer state (debounced saves are cut short).
        QSignalSpy closed(&e.ctl, &WorkspaceController::closed);
        e.ctl.close();
        QCOMPARE(closed.count(), 1);
        QVERIFY(!e.ctl.isOpen());
        QCOMPARE(t->rowCount(), 0);
        QTest::qWait(300); // let the setState/close requests reach the daemon

        QSignalSpy expand(t, &FileTreeModel::expandRequested);
        QSignalSpy opened(&e.ctl, &WorkspaceController::opened);
        e.ctl.openFolder(e.dir.path());
        QVERIFY(opened.wait(8000));
        // The expanded directories come back expanded, with their children loaded.
        QVERIFY(waitFor([&] { return expand.count() == 2; }));
        QVERIFY(waitFor([&] { return t->rowCount(child(t, "src")) == 2 && t->rowCount(child(t, "notes")) == 1; }));
        QCOMPARE(t->expandedPaths(), (QStringList{ "notes", "src" }));
    }

    void expandedStateSurvivesDebounceWithoutClose()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        auto *t = e.tree();
        t->setExpanded(child(t, "src"), true);
        QTest::qWait(900); // > 500 ms debounce: saved without any close

        DaemonClient second;
        second.start(m_daemon, { "--data-dir", m_data.path() });
        QVERIFY(waitFor([&] { return second.state() == DaemonClient::Ready; }, 10000));
        QJsonObject state;
        bool done = false;
        QString id;
        second.call("workspace/open", { { "path", e.dir.path() } }, [&](const QJsonObject &r, const RpcError &) {
            id = r.value("workspaceId").toString();
            second.call("workspace/getState", { { "workspaceId", id }, { "keys", QJsonArray{ "explorer.expanded" } } },
                        [&](const QJsonObject &s, const RpcError &) {
                            state = s;
                            done = true;
                        });
        });
        QVERIFY(waitFor([&] { return done; }));
        QCOMPARE(state.value("values").toObject().value("explorer.expanded").toArray(), QJsonArray{ "src" });
    }

    void openingAMissingPathEmitsError()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(waitFor([&] { return e.client.state() == DaemonClient::Ready; }, 10000));
        QSignalSpy err(&e.ctl, &WorkspaceController::errorOccurred);
        e.ctl.openFolder(e.dir.path() + "/definitely/not/here");
        QVERIFY(err.wait(5000));
        QVERIFY(!err.first().first().toString().isEmpty());
        QVERIFY(!e.ctl.isOpen());
        QVERIFY(!e.ctl.opening());
    }

    void openingAFileEmitsError()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(waitFor([&] { return e.client.state() == DaemonClient::Ready; }, 10000));
        QSignalSpy err(&e.ctl, &WorkspaceController::errorOccurred);
        e.ctl.openFolder(e.dir.path() + "/README.md");
        QVERIFY(err.wait(5000));
        QVERIFY(!e.ctl.isOpen());
    }

    void unreadableDirectoryShowsInlineErrorRow()
    {
#ifdef Q_OS_UNIX
        if (TestSupport::runningAsRoot())
            QSKIP("root can read anything");
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        auto *t = e.tree();
        QVERIFY(QFile::setPermissions(e.dir.path() + "/notes", QFileDevice::Permissions()));
        const QModelIndex notes = child(t, "notes");
        t->setExpanded(notes, true);
        QVERIFY(waitFor([&] { return t->rowCount(notes) == 1; }));
        const QModelIndex row = t->index(0, 0, notes);
        QCOMPARE(t->data(row, FileTreeModel::KindRole).toString(), QStringLiteral("error"));
        QVERIFY(!t->data(row, FileTreeModel::NameRole).toString().isEmpty());
        QVERIFY(!t->hasChildren(row));
        QFile::setPermissions(e.dir.path() + "/notes", QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
#else
        QSKIP("permissions test is unix-only");
#endif
    }

    void recentListIsPopulatedAndNewestFirst()
    {
        QTemporaryDir data;
        Env e(m_daemon, data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        QTemporaryDir other;
        QSignalSpy opened(&e.ctl, &WorkspaceController::opened);
        e.ctl.openFolder(other.path());
        QVERIFY(opened.wait(8000)); // switching closes the first workspace
        QCOMPARE(e.ctl.root(), canon(other.path()));
        e.ctl.refreshRecent();
        auto *r = e.ctl.recentModel();
        QVERIFY(waitFor([&] { return r->rowCount() == 2; }));
        QCOMPARE(QDir::toNativeSeparators(r->data(r->index(0), RecentModel::RootRole).toString()), canon(other.path()));
        QCOMPARE(QDir::toNativeSeparators(r->data(r->index(1), RecentModel::RootRole).toString()), canon(e.dir.path()));
    }

    void reopenLastOpensTheLastWorkspaceWhenEnabled()
    {
        QTemporaryDir data;
        Env first(m_daemon, data.path(), m_fixtures); // keeps its directory alive
        QVERIFY(openAndWait(first));
        first.client.shutdown();
        // New daemon, same data dir: lastRoot is remembered.
        DaemonClient client;
        WorkspaceController ctl(&client);
        client.start(m_daemon, { "--data-dir", data.path() });
        QVERIFY(waitFor([&] { return client.state() == DaemonClient::Ready; }, 10000));
        QSignalSpy opened(&ctl, &WorkspaceController::opened);
        ctl.reopenLast();
        QVERIFY(opened.wait(8000));
        QVERIFY(ctl.isOpen());
        QVERIFY(ctl.treeModel()->rowCount() > 0);

        // And it does nothing when the setting is off.
        ctl.close();
        QSignalSpy opened2(&ctl, &WorkspaceController::opened);
        bool set = false;
        client.call("settings/set", { { "values", QJsonObject{ { "workspace.reopenLast", false } } } },
                    [&](const QJsonObject &, const RpcError &) { set = true; });
        QVERIFY(waitFor([&] { return set; }));
        ctl.reopenLast();
        QVERIFY(!opened2.wait(800));
        QVERIFY(!ctl.isOpen());
    }

    void daemonRestartDropsTheWorkspace()
    {
        Env e(m_daemon, m_data.path(), m_fixtures);
        QVERIFY(openAndWait(e));
        TestSupport::killProcess(e.client.processId());
        QVERIFY(waitFor([&] { return !e.ctl.isOpen(); }));
        QCOMPARE(e.tree()->rowCount(), 0);
        e.client.restart();
        QVERIFY(waitFor([&] { return e.client.state() == DaemonClient::Ready; }, 10000));
        QSignalSpy opened(&e.ctl, &WorkspaceController::opened);
        e.ctl.openFolder(e.dir.path());
        QVERIFY(opened.wait(8000));
    }
};

QTEST_GUILESS_MAIN(TstWorkspace)
#include "tst_workspace.moc"
