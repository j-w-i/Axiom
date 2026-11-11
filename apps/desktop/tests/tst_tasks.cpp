#include "daemon/DaemonClient.h"
#include "models/TaskController.h"
#include "models/TaskOutputModel.h"
#include "models/WorkspaceController.h"

#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QJsonArray>
#include <QJsonObject>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>
#include "TestSupport.h"

// TaskController / TaskOutputModel against the real daemon. Commands go through the
// cross-platform `emit` helper (tests/helpers/emit) so the same tests run on every OS.
class TstTasks : public QObject
{
    Q_OBJECT
    QString m_daemon;

    template<typename F>
    bool waitFor(F done, int ms = 10000)
    {
        QElapsedTimer t;
        t.start();
        while (!done() && t.elapsed() < ms)
            QTest::qWait(5);
        return done();
    }

    struct Env
    {
        DaemonClient client;
        WorkspaceController ws;
        TaskController tasks;
        QTemporaryDir data;
        QTemporaryDir dir;
        Env(const QString &daemon) : ws(&client), tasks(&client, &ws)
        {
            QFile f(dir.path() + "/README.md");
            if (f.open(QIODevice::WriteOnly))
                f.write("hi");
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
        TaskOutputModel *out() { return tasks.outputModel(); }
        bool runAndWait(const QString &cmd, int ms = 15000)
        {
            QSignalSpy done(&tasks, &TaskController::finished);
            tasks.run(cmd);
            return done.count() > 0 || done.wait(ms);
        }
    };

    static QStringList rows(TaskOutputModel *m, int role = TaskOutputModel::TextRole)
    {
        QStringList l;
        for (int i = 0; i < m->rowCount(); ++i)
            l << m->data(m->index(i), role).toString();
        return l;
    }

private slots:
    void initTestCase()
    {
        m_daemon = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        QVERIFY2(QFileInfo(m_daemon).isExecutable(), qPrintable("axiomd not built: " + m_daemon));
        QVERIFY2(QFileInfo(TestSupport::emitPath()).isExecutable(), qPrintable("emit helper not built: " + TestSupport::emitPath()));
    }

    // ---- the model on its own ----------------------------------------------------

    void modelSplitsLinesKeepsPartialsAndTagsStreams()
    {
        TaskOutputModel m;
        m.append(0, "stdout", "hel");
        m.append(1, "stderr", "err1\nerr2");
        m.append(2, "stdout", "lo\nworld\n");
        m.append(3, "stderr", "!\n");
        m.flushNow();
        QCOMPARE(rows(&m), (QStringList{ "err1", "hello", "world", "err2!" }));
        QCOMPARE(rows(&m, TaskOutputModel::StreamRole), (QStringList{ "stderr", "stdout", "stdout", "stderr" }));
    }

    void modelAppliesChunksInSeqOrderAndIgnoresDuplicates()
    {
        TaskOutputModel m;
        m.append(2, "stdout", "c\n");
        m.append(1, "stdout", "b\n");
        QCOMPARE(m.rowCount(), 0); // waiting for seq 0
        m.append(0, "stdout", "a\n");
        m.append(0, "stdout", "again\n"); // duplicate
        m.flushNow();
        QCOMPARE(rows(&m), (QStringList{ "a", "b", "c" }));
    }

    void modelFlushesTrailingPartialLineOnFinish()
    {
        TaskOutputModel m;
        m.append(0, "stdout", "no newline");
        m.flushNow();
        QCOMPARE(m.rowCount(), 0);
        m.finish();
        QCOMPARE(rows(&m), QStringList{ "no newline" });
    }

    void ansiSequencesAreStripped()
    {
        QCOMPARE(TaskOutputModel::stripAnsi("\x1b[31mred\x1b[0m"), QStringLiteral("red"));
        QCOMPARE(TaskOutputModel::stripAnsi("\x1b[1;38;5;196mbold\x1b[m ok"), QStringLiteral("bold ok"));
        QCOMPARE(TaskOutputModel::stripAnsi("\x1b[2K\x1b[1Gline"), QStringLiteral("line"));
        QCOMPARE(TaskOutputModel::stripAnsi("\x1b]0;title\x07text"), QStringLiteral("text"));
        QCOMPARE(TaskOutputModel::stripAnsi("a\x07\x08z"), QStringLiteral("az"));
        QCOMPARE(TaskOutputModel::stripAnsi("tab\there"), QStringLiteral("tab\there"));
        // An escape sequence split across two chunks is not a concern for CSI-complete chunks,
        // but a plain colour code inside a longer text must disappear.
        TaskOutputModel m;
        m.append(0, "stdout", "\x1b[32mgreen\x1b[0m and \x1b[1mbold\x1b[0m\n");
        m.flushNow();
        QCOMPARE(rows(&m), QStringList{ "green and bold" });
    }

    void carriageReturnProgressKeepsTheLastSegment()
    {
        TaskOutputModel m;
        m.append(0, "stdout", "10%\r50%\r100%\n");
        m.flushNow();
        QCOMPARE(rows(&m), QStringList{ "100%" });
    }

    void modelCapsAtTenThousandLinesWithATruncationRow()
    {
        TaskOutputModel m;
        QString chunk;
        for (int i = 0; i < 30000; ++i)
            chunk += QStringLiteral("line%1\n").arg(i);
        QSignalSpy truncated(&m, &TaskOutputModel::truncatedChanged);
        m.append(0, "stdout", chunk);
        m.flushNow();
        QVERIFY(m.truncated());
        QCOMPARE(truncated.count(), 1);
        QCOMPARE(m.rowCount(), TaskOutputModel::MaxLines + 1); // + the truncation row
        QVERIFY(m.textAt(0).startsWith("… earlier output truncated"));
        QCOMPARE(m.data(m.index(0), TaskOutputModel::KindRole).toString(), QStringLiteral("notice"));
        QCOMPARE(m.textAt(1), QStringLiteral("line20000")); // the oldest 20 000 are gone
        QCOMPARE(m.textAt(m.rowCount() - 1), QStringLiteral("line29999"));
        QCOMPARE(m.droppedLines(), 20000);
    }

    void resetClearsEverything()
    {
        TaskOutputModel m;
        m.append(0, "stdout", "x\n");
        m.flushNow();
        m.reset();
        QCOMPARE(m.rowCount(), 0);
        m.append(0, "stdout", "fresh\n"); // seq counting restarts
        m.flushNow();
        QCOMPARE(rows(&m), QStringList{ "fresh" });
    }

    // ---- against the daemon ------------------------------------------------------

    void stdoutStderrAndFailedStatus()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        QVERIFY(e.tasks.canRun());
        QVERIFY(e.runAndWait(TestSupport::emitCommand({ "out", "out", "err", "err", "exit", "3" })));
        QVERIFY(!e.tasks.running());
        auto *m = e.out();
        QCOMPARE(m->rowCount(), 3);
        QCOMPARE(m->textAt(0).trimmed().isEmpty(), false);
        QStringList streams = rows(m, TaskOutputModel::StreamRole);
        QVERIFY(streams.contains("stdout") && streams.contains("stderr"));
        for (int i = 0; i < 2; ++i) {
            if (m->textAt(i) == "out")
                QCOMPARE(streams.at(i), QStringLiteral("stdout"));
            else
                QCOMPARE(streams.at(i), QStringLiteral("stderr"));
        }
        QVERIFY(rows(m).contains("out") && rows(m).contains("err"));
        QVERIFY2(m->textAt(2).contains("Failed with exit code 3"), qPrintable(m->textAt(2)));
        QCOMPARE(m->data(m->index(2), TaskOutputModel::KindRole).toString(), QStringLiteral("statusFailed"));
        const QVariantMap st = e.tasks.lastStatus();
        QCOMPARE(st.value("exitCode").toInt(), 3);
        QCOMPARE(st.value("outcome").toString(), QStringLiteral("failed"));
        QVERIFY(st.value("durationMs").toLongLong() >= 0);
        QVERIFY(e.tasks.statusText().contains("failed (3)"));
    }

    void successStatusRow()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        QVERIFY(e.runAndWait(TestSupport::emitCommand({ "out", "fine" })));
        QCOMPARE(rows(e.out()).first(), QStringLiteral("fine"));
        QVERIFY2(rows(e.out()).last().startsWith("✔ Succeeded in"), qPrintable(rows(e.out()).last()));
        QCOMPARE(e.tasks.lastStatus().value("exitCode").toInt(), 0);
    }

    void missingCommandIsReportedByTheShellWithExit127()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        QVERIFY(e.runAndWait(QStringLiteral("definitely-not-a-command-xyz")));
        QVERIFY2(rows(e.out()).last().contains("exit code"), qPrintable(rows(e.out()).join('|')));
        // 127 on POSIX shells, 1 on cmd.exe: both say "command not found".
        const int code = e.tasks.lastStatus().value("exitCode").toInt();
        QVERIFY2(code == 127 || code == 1, qPrintable(QString::number(code)));
        // The shell's complaint is on stderr.
        const QStringList streams = rows(e.out(), TaskOutputModel::StreamRole);
        QVERIFY(streams.contains("stderr"));
    }

    void spawnFailedOfAnArgvShowsADangerRow()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.tasks.runArgvForTest({ "definitely-not-a-program-xyz" });
        auto *m = e.out();
        QVERIFY(waitFor([&] { return m->rowCount() == 1; }));
        QCOMPARE(m->data(m->index(0), TaskOutputModel::KindRole).toString(), QStringLiteral("error"));
        QVERIFY2(m->textAt(0).contains("definitely-not-a-program-xyz"), qPrintable(m->textAt(0)));
        QVERIFY(waitFor([&] { return !e.tasks.running(); }));
        QVERIFY(e.tasks.canRun());
    }

    void stopCancelsALongCommandWithin5Seconds()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        QSignalSpy done(&e.tasks, &TaskController::finished);
        e.tasks.run(TestSupport::emitCommand({ "sleep", "30000" }));
        QVERIFY(waitFor([&] { return e.tasks.running(); }));
        QVERIFY(!e.tasks.canRun()); // one task at a time
        QElapsedTimer t;
        t.start();
        e.tasks.stop();
        QVERIFY(done.count() > 0 || done.wait(5000));
        QVERIFY2(t.elapsed() < 5000, qPrintable(QString::number(t.elapsed())));
        QVERIFY(!e.tasks.running());
        QCOMPARE(done.first().first().toString(), QStringLiteral("cancelled"));
        QVERIFY2(rows(e.out()).last().contains("Cancelled"), qPrintable(rows(e.out()).last()));
        QVERIFY(e.tasks.statusText().contains("cancelled"));
    }

    void runIsRefusedWhileRunningAndWithoutAWorkspace()
    {
        Env e(m_daemon);
        QTest::qWait(300);
        e.tasks.run("echo nope"); // no workspace yet
        QVERIFY(!e.tasks.running());
        QVERIFY(e.tasks.history().isEmpty());
        QVERIFY(e.openWorkspace());
        const QString sleeper = TestSupport::emitCommand({ "sleep", "30000" });
        e.tasks.run(sleeper);
        QVERIFY(e.tasks.running());
        const int rowsBefore = e.out()->rowCount();
        e.tasks.run("echo second"); // refused
        QCOMPARE(e.tasks.history(), QStringList{ sleeper });
        QCOMPARE(e.out()->rowCount(), rowsBefore);
        QSignalSpy done(&e.tasks, &TaskController::finished);
        e.tasks.stop();
        QVERIFY(done.wait(5000));
    }

    void ansiIsStrippedEndToEnd()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        QVERIFY(e.runAndWait(TestSupport::emitCommand({ "raw-out", "\\e[31mred\\e[0m\\n" })));
        QCOMPARE(rows(e.out()).first(), QStringLiteral("red"));
    }

    void commandLineIsPassedVerbatimToTheShell()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        // Shell features work because the line goes to the platform shell untouched:
        // a pipe-free two-command line with `&&` is valid in both sh and cmd.
        const QString first = TestSupport::emitCommand({ "out", "one" });
        const QString second = TestSupport::emitCommand({ "out", "two  spaced" });
        QVERIFY(e.runAndWait(first + " && " + second));
        QCOMPARE(rows(e.out()).mid(0, 2), (QStringList{ "one", "two  spaced" }));
        // The working directory is the workspace root.
        QVERIFY(e.runAndWait(TestSupport::emitCommand({ "cwd" })));
        QCOMPARE(QDir::cleanPath(rows(e.out()).first()), QDir::cleanPath(QDir(e.dir.path()).canonicalPath()));
    }

    void historyIsMostRecentFirstDedupedAndCapped()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        auto say = [](const QString &w) { return TestSupport::emitCommand({ "out", w }); };
        QVERIFY(e.runAndWait(say("one")));
        QVERIFY(e.runAndWait(say("two")));
        QVERIFY(e.runAndWait(say("one"))); // moves to the front, no duplicate
        QCOMPARE(e.tasks.history(), (QStringList{ say("one"), say("two") }));
        QCOMPARE(e.tasks.lastCommand(), say("one"));
        for (int i = 0; i < 25; ++i)
            QVERIFY(e.runAndWait(say(QStringLiteral("n%1").arg(i))));
        QCOMPARE(e.tasks.history().size(), TaskController::MaxHistory);
        QCOMPARE(e.tasks.history().first(), say("n24"));
        QVERIFY(!e.tasks.history().contains(say("one")));
    }

    void rerunLastRunsTheMostRecentCommand()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        QVERIFY(e.runAndWait(TestSupport::emitCommand({ "out", "again" })));
        QSignalSpy done(&e.tasks, &TaskController::finished);
        e.tasks.rerunLast();
        QVERIFY(done.wait(8000));
        QCOMPARE(rows(e.out()).first(), QStringLiteral("again"));
        QCOMPARE(e.tasks.history().size(), 1);
    }

    void historyIsPersistedAcrossWorkspaceReopen()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        const QString kept = TestSupport::emitCommand({ "out", "kept" });
        const QString alsoKept = TestSupport::emitCommand({ "out", "also-kept" });
        QVERIFY(e.runAndWait(kept));
        QVERIFY(e.runAndWait(alsoKept));
        QTest::qWait(300); // setState requests reach the daemon
        QSignalSpy closed(&e.ws, &WorkspaceController::closed);
        e.ws.close();
        QCOMPARE(closed.count(), 1);
        QVERIFY(e.tasks.history().isEmpty());
        QTest::qWait(300);
        QSignalSpy opened(&e.ws, &WorkspaceController::opened);
        e.ws.openFolder(e.dir.path());
        QVERIFY(opened.wait(8000));
        QVERIFY(waitFor([&] { return e.tasks.history().size() == 2; }));
        QCOMPARE(e.tasks.history(), (QStringList{ alsoKept, kept }));
    }

    void clearEmptiesTheOutputButNotTheHistory()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        const QString cmd = TestSupport::emitCommand({ "out", "x" });
        QVERIFY(e.runAndWait(cmd));
        QVERIFY(e.out()->rowCount() > 0);
        e.tasks.clear();
        QCOMPARE(e.out()->rowCount(), 0);
        QVERIFY(!e.tasks.hasLastStatus());
        QCOMPARE(e.tasks.history(), QStringList{ cmd });
    }

    void closingTheWorkspaceCancelsARunningTask()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());
        e.tasks.run(TestSupport::emitCommand({ "sleep", "30000" }));
        QVERIFY(waitFor([&] { return e.tasks.running(); }));
        QTest::qWait(300);
        e.ws.close();
        QVERIFY(!e.tasks.running()); // state dropped at once; the daemon cancels the process
        QCOMPARE(e.out()->rowCount(), 0);
        // The daemon really killed it: a new workspace can run tasks, none are left running.
        QSignalSpy opened(&e.ws, &WorkspaceController::opened);
        e.ws.openFolder(e.dir.path());
        QVERIFY(opened.wait(8000));
        QVERIFY(e.runAndWait(TestSupport::emitCommand({ "out", "alive" })));
    }

    // ---- large output ------------------------------------------------------------

    void fiftyThousandLinesAreCappedWithoutStallingTheEventLoop()
    {
        Env e(m_daemon);
        QVERIFY(e.openWorkspace());

        // Measure the longest gap between 5 ms timer ticks while the output streams in.
        qint64 longest = 0;
        QElapsedTimer sinceTick;
        sinceTick.start();
        QTimer ticker;
        ticker.setInterval(5);
        connect(&ticker, &QTimer::timeout, this, [&] {
            longest = qMax(longest, sinceTick.elapsed());
            sinceTick.restart();
        });
        ticker.start();

        // A cheap row-count observer stands in for a view: it forces the model's signals to be delivered.
        qint64 insertBatches = 0;
        connect(e.out(), &QAbstractItemModel::rowsInserted, this, [&] { ++insertBatches; });

        QSignalSpy done(&e.tasks, &TaskController::finished);
        QElapsedTimer total;
        total.start();
        e.tasks.run(TestSupport::emitCommand({ "seq", "1", "50000" }));
        QVERIFY(done.count() > 0 || done.wait(60000));
        e.out()->flushNow();
        const qint64 wall = total.elapsed();
        ticker.stop();

        auto *m = e.out();
        QVERIFY(m->truncated());
        QCOMPARE(m->data(m->index(0), TaskOutputModel::KindRole).toString(), QStringLiteral("notice"));
        // 10 000 kept lines + the notice row + the status row.
        QVERIFY2(m->rowCount() >= TaskOutputModel::MaxLines && m->rowCount() <= TaskOutputModel::MaxLines + 2,
                 qPrintable(QString::number(m->rowCount())));
        QCOMPARE(m->textAt(m->rowCount() - 2), QStringLiteral("50000")); // the newest line survived
        QVERIFY(m->textAt(m->rowCount() - 1).contains("Succeeded"));
        QCOMPARE(m->droppedLines(), 40001); // 50 000 lines + the status row, minus the 10 000 kept

        qInfo().noquote() << QStringLiteral("MEASURED longest event-loop gap during 50 000 lines: %1 ms (total %2 ms, %3 insert batches)")
                                 .arg(longest).arg(wall).arg(insertBatches);
        QVERIFY2(longest <= 200, qPrintable(QStringLiteral("event loop stalled for %1 ms").arg(longest)));
    }
};

QTEST_GUILESS_MAIN(TstTasks)
#include "tst_tasks.moc"
