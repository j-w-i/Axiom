#include "daemon/DaemonClient.h"

#include <QCoreApplication>
#include <QFileInfo>
#include <QJsonArray>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>
#include "TestSupport.h"

class TstDaemonClient : public QObject
{
    Q_OBJECT

    QString m_daemon;
    QTemporaryDir m_data;

    // Runs the event loop until `done` or the timeout; never waits blindly.
    template<typename F>
    bool waitFor(F done, int ms = 10000)
    {
        QElapsedTimer t;
        t.start();
        while (!done() && t.elapsed() < ms)
            QTest::qWait(10);
        return done();
    }

    DaemonClient *newClient()
    {
        auto *c = new DaemonClient(this);
        c->start(m_daemon, { QStringLiteral("--data-dir"), m_data.path() });
        return c;
    }

private slots:
    void initTestCase()
    {
        m_daemon = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        // Fail closed: a missing daemon is an error, never a silent skip.
        QVERIFY2(!m_daemon.isEmpty(), "AXIOMD_UNDER_TEST is not set");
        QVERIFY2(QFileInfo(m_daemon).isExecutable(),
                 qPrintable("axiomd not built: " + m_daemon + " (run `cargo build -p axiomd`)"));
        QVERIFY(m_data.isValid());
    }

    void initializeSucceeds()
    {
        DaemonClient c;
        QCOMPARE(c.state(), DaemonClient::Stopped);
        c.start(m_daemon, { QStringLiteral("--data-dir"), m_data.path() });
        QCOMPARE(c.state(), DaemonClient::Starting);
        QVERIFY(waitFor([&] { return c.state() == DaemonClient::Ready; }));
        QCOMPARE(c.serverVersion(), QStringLiteral("0.1.0"));
        QVERIFY(c.errorText().isEmpty());
        c.shutdown();
        QCOMPARE(c.state(), DaemonClient::Stopped);
    }

    void unknownMethodIsMethodNotFound()
    {
        auto *c = newClient();
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));
        RpcError got;
        bool done = false;
        c->call(QStringLiteral("no/such"), {}, [&](const QJsonObject &, const RpcError &e) {
            got = e;
            done = true;
        });
        QVERIFY(waitFor([&] { return done; }));
        QVERIFY(!got.ok());
        QCOMPARE(got.kind, QStringLiteral("method_not_found"));
        QCOMPARE(got.code, -32601);
        delete c;
    }

    void resultsAndIdsAreCorrelated()
    {
        auto *c = newClient();
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));
        QJsonObject r1, r2;
        int done = 0;
        const int id1 = c->call(QStringLiteral("settings/get"),
                                { { "keys", QJsonArray{ "editor.fontSize" } } },
                                [&](const QJsonObject &r, const RpcError &e) { QVERIFY(e.ok()); r1 = r; ++done; });
        const int id2 = c->call(QStringLiteral("settings/get"),
                                { { "keys", QJsonArray{ "editor.wordWrap" } } },
                                [&](const QJsonObject &r, const RpcError &e) { QVERIFY(e.ok()); r2 = r; ++done; });
        QCOMPARE(id2, id1 + 1);
        QVERIFY(waitFor([&] { return done == 2; }));
        QCOMPARE(r1.value("values").toObject().value("editor.fontSize").toInt(), 13);
        QCOMPARE(r2.value("values").toObject().value("editor.wordWrap").toBool(), false);
        delete c;
    }

    void notificationsAreEmitted()
    {
        auto *c = newClient();
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));
        QSignalSpy spy(c, &DaemonClient::notification);
        bool done = false;
        c->call(QStringLiteral("settings/set"), { { "values", QJsonObject{ { "editor.fontSize", 20 } } } },
                [&](const QJsonObject &, const RpcError &e) { QVERIFY(e.ok()); done = true; });
        QVERIFY(waitFor([&] { return done && spy.count() >= 1; }));
        QCOMPARE(spy.first().at(0).toString(), QStringLiteral("settings/didChange"));
        QCOMPARE(spy.first().at(1).toJsonObject().value("values").toObject().value("editor.fontSize").toInt(), 20);
        delete c;
    }

    void killingTheDaemonFailsAndFiresPendingCallbacks()
    {
        auto *c = newClient();
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));
        QVERIFY(c->processId() > 0);

        // A request that cannot finish before the process dies: a task that sleeps.
        RpcError got;
        bool done = false;
        c->call(QStringLiteral("workspace/open"), { { "path", m_data.path() } },
                [&](const QJsonObject &, const RpcError &) {});
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));

        QSignalSpy states(c, &DaemonClient::stateChanged);
        // Pending at the moment of death: queue it, then kill before the answer is read.
        c->call(QStringLiteral("settings/get"), {}, [&](const QJsonObject &, const RpcError &e) {
            got = e;
            done = true;
        });
        TestSupport::killProcess(c->processId());

        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Failed; }));
        QVERIFY(!c->errorText().isEmpty());
        QVERIFY(waitFor([&] { return done; }));
        // Either the daemon answered just before dying, or the client reported it stopped.
        if (!got.ok()) {
            QCOMPARE(got.kind, QStringLiteral("internal"));
            QCOMPARE(got.message, QStringLiteral("Backend stopped"));
        }
        delete c;
    }

    void pendingCallbacksFireWithBackendStoppedOnDeath()
    {
        auto *c = newClient();
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));
        // A workspace and a long task give a request guaranteed to be in flight: none
        // exists in protocol 0.1, so use an unanswerable call: write while the process
        // is stopped by SIGSTOP, then kill it.
        const qint64 pid = c->processId();
        if (TestSupport::canSuspend())
            TestSupport::suspendProcess(pid);
        RpcError got;
        bool done = false;
        c->call(QStringLiteral("settings/get"), {}, [&](const QJsonObject &, const RpcError &e) {
            got = e;
            done = true;
        });
        // Where a process can be frozen the call stays pending until it is killed; elsewhere it
        // may be answered before the kill, which is a valid outcome of the same race.
        if (TestSupport::canSuspend()) {
            QTest::qWait(100);
            QVERIFY(!done);
        }
        TestSupport::killProcess(pid);
        QVERIFY(waitFor([&] { return done; }));
        if (TestSupport::canSuspend()) {
            QVERIFY(!got.ok());
            QCOMPARE(got.kind, QStringLiteral("internal"));
            QCOMPARE(got.message, QStringLiteral("Backend stopped"));
        }
        QCOMPARE(c->state(), DaemonClient::Failed);
        delete c;
    }

    void restartRecovers()
    {
        auto *c = newClient();
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));
        const qint64 first = c->processId();
        TestSupport::killProcess(first);
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Failed; }));

        c->restart();
        QCOMPARE(c->state(), DaemonClient::Starting);
        QVERIFY(c->errorText().isEmpty());
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));
        QVERIFY(c->processId() > 0);
        QVERIFY(c->processId() != first);

        bool ok = false;
        c->call(QStringLiteral("settings/get"), {}, [&](const QJsonObject &, const RpcError &e) { ok = e.ok(); });
        QVERIFY(waitFor([&] { return ok; }));
        delete c;
    }

    void restartWhileRunningReplacesTheProcess()
    {
        auto *c = newClient();
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));
        const qint64 first = c->processId();
        c->restart();
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready && c->processId() != first; }));
        delete c;
    }

    void missingProgramFailsWithAnExplanation()
    {
        DaemonClient c;
        c.start(QString(), {});
        QCOMPARE(c.state(), DaemonClient::Failed);
        QVERIFY(c.errorText().contains("axiomd"));

        DaemonClient d;
        d.start(QStringLiteral("/definitely/not/a/program"), {});
        QVERIFY(waitFor([&] { return d.state() == DaemonClient::Failed; }));
        QVERIFY(d.errorText().contains("Could not start"));
    }

    void callWithoutProcessReportsBackendStopped()
    {
        DaemonClient c;
        RpcError got;
        bool done = false;
        c.call(QStringLiteral("settings/get"), {}, [&](const QJsonObject &, const RpcError &e) {
            got = e;
            done = true;
        });
        QVERIFY(waitFor([&] { return done; }, 2000));
        QCOMPARE(got.kind, QStringLiteral("internal"));
    }

    void callTimesOutAndSendsCancel()
    {
        auto *c = newClient();
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));
        const qint64 pid = c->processId();
        if (!TestSupport::canSuspend())
            QSKIP("freezing a process (SIGSTOP) is not available on this platform");
        TestSupport::suspendProcess(pid); // the daemon cannot answer
        RpcError got;
        bool done = false;
        QElapsedTimer t;
        t.start();
        c->call(QStringLiteral("settings/get"), {}, [&](const QJsonObject &, const RpcError &e) {
            got = e;
            done = true;
        }, 300);
        QVERIFY(waitFor([&] { return done; }, 3000));
        QVERIFY(t.elapsed() >= 290);
        QCOMPARE(got.kind, QStringLiteral("timeout"));
        QVERIFY(c->state() == DaemonClient::Ready); // a timeout does not fail the backend
        TestSupport::resumeProcess(pid);

        // The late answer to the timed-out request is ignored and the connection works on.
        bool ok = false;
        c->call(QStringLiteral("settings/get"), {}, [&](const QJsonObject &, const RpcError &e) { ok = e.ok(); });
        QVERIFY(waitFor([&] { return ok; }));
        delete c;
    }

    void shutdownIsOrderlyAndStopsTheProcess()
    {
        auto *c = newClient();
        QVERIFY(waitFor([&] { return c->state() == DaemonClient::Ready; }));
        const qint64 pid = c->processId();
        QElapsedTimer t;
        t.start();
        c->shutdown();
        QVERIFY(t.elapsed() < 2000);
        QCOMPARE(c->state(), DaemonClient::Stopped);
        QVERIFY(c->errorText().isEmpty()); // an orderly stop is not an error
        QVERIFY(!TestSupport::processAlive(pid)); // gone
        delete c;
    }

    void stderrLinesAreForwardedWithPrefix()
    {
        DaemonClient c;
        QTest::ignoreMessage(QtInfoMsg, QRegularExpression("^\\[axiomd\\] .*"));
        // `--log-level error` prints nothing normally; force output with an invalid flag.
        c.start(m_daemon, { QStringLiteral("--no-such-flag") });
        QVERIFY(waitFor([&] { return c.state() == DaemonClient::Failed; }));
        QVERIFY(c.errorText().contains("exited unexpectedly"));
    }
};

QTEST_GUILESS_MAIN(TstDaemonClient)
#include "tst_daemonclient.moc"
