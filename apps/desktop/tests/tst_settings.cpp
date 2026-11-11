#include "daemon/DaemonClient.h"
#include "models/SettingsStore.h"

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>

// SettingsStore against the real daemon: the daemon is the single source of truth.
class TstSettings : public QObject
{
    Q_OBJECT
    QString m_daemon;

    template<typename F>
    bool waitFor(F done, int ms = 5000)
    {
        QElapsedTimer t;
        t.start();
        while (!done() && t.elapsed() < ms)
            QTest::qWait(5);
        return done();
    }

private slots:
    void initTestCase()
    {
        m_daemon = qEnvironmentVariable("AXIOMD_UNDER_TEST");
        QVERIFY2(QFileInfo(m_daemon).isExecutable(), qPrintable("axiomd not built: " + m_daemon));
    }

    void loadsDefaultsWhenTheDaemonBecomesReady()
    {
        QTemporaryDir data;
        DaemonClient client;
        SettingsStore s(&client);
        QVERIFY(!s.loaded());
        client.start(m_daemon, { "--data-dir", data.path() });
        QVERIFY(waitFor([&] { return s.loaded(); }, 10000));
        QCOMPARE(s.theme(), QStringLiteral("system"));
        QCOMPARE(s.editorFontSize(), 13);
        QCOMPARE(s.editorFontFamily(), QString());
        QCOMPARE(s.editorWordWrap(), false);
        QCOMPARE(s.reopenLast(), true);
    }

    void changesApplyOnlyWhenTheDaemonNotifies()
    {
        QTemporaryDir data;
        DaemonClient client;
        SettingsStore s(&client);
        client.start(m_daemon, { "--data-dir", data.path() });
        QVERIFY(waitFor([&] { return s.loaded(); }, 10000));

        QSignalSpy theme(&s, &SettingsStore::themeChanged);
        QSignalSpy any(&s, &SettingsStore::changed);
        s.setTheme("dark");
        // Not optimistic: nothing changed yet, synchronously.
        QCOMPARE(s.theme(), QStringLiteral("system"));
        QCOMPARE(theme.count(), 0);
        QVERIFY(waitFor([&] { return s.theme() == "dark"; }));
        QCOMPARE(theme.count(), 1);
        QVERIFY(any.count() >= 1);

        QSignalSpy size(&s, &SettingsStore::editorFontSizeChanged);
        s.setEditorFontSize(20);
        s.setEditorFontFamily("Fira Code");
        s.setEditorWordWrap(true);
        s.setReopenLast(false);
        QVERIFY(waitFor([&] { return s.editorFontSize() == 20 && s.editorFontFamily() == "Fira Code" && s.editorWordWrap() && !s.reopenLast(); }));
        QCOMPARE(size.count(), 1);
        // Setting the same value again is not a change.
        QSignalSpy theme2(&s, &SettingsStore::themeChanged);
        s.setTheme("dark");
        QTest::qWait(300);
        QCOMPARE(theme2.count(), 0);
    }

    void outOfRangeFontSizeIsRejectedAndThePropertyKeepsItsValue()
    {
        QTemporaryDir data;
        DaemonClient client;
        SettingsStore s(&client);
        client.start(m_daemon, { "--data-dir", data.path() });
        QVERIFY(waitFor([&] { return s.loaded(); }, 10000));
        s.setEditorFontSize(22);
        QVERIFY(waitFor([&] { return s.editorFontSize() == 22; }));

        QSignalSpy err(&s, &SettingsStore::errorOccurred);
        QSignalSpy size(&s, &SettingsStore::editorFontSizeChanged);
        s.setEditorFontSize(7);   // below 8
        QVERIFY(err.wait(5000));
        QVERIFY2(err.first().first().toString().contains("editor.fontSize"), qPrintable(err.first().first().toString()));
        QCOMPARE(s.editorFontSize(), 22);
        err.clear();
        s.setEditorFontSize(41);  // above 40
        QVERIFY(err.wait(5000));
        QCOMPARE(s.editorFontSize(), 22);
        // The notification of a bound control resetting is emitted so it shows the real value again.
        QVERIFY(size.count() >= 2);

        s.setTheme("blue");
        QVERIFY(err.wait(5000));
        QCOMPARE(s.theme(), QStringLiteral("system"));
        // Valid values still work afterwards.
        s.setEditorFontSize(40);
        QVERIFY(waitFor([&] { return s.editorFontSize() == 40; }));
        s.setEditorFontSize(8);
        QVERIFY(waitFor([&] { return s.editorFontSize() == 8; }));
    }

    void valuesPersistAcrossAnApplicationRestart()
    {
        QTemporaryDir data;
        {
            DaemonClient client;
            SettingsStore s(&client);
            client.start(m_daemon, { "--data-dir", data.path() });
            QVERIFY(waitFor([&] { return s.loaded(); }, 10000));
            s.setTheme("light");
            s.setEditorFontSize(17);
            s.setEditorFontFamily("Menlo");
            s.setEditorWordWrap(true);
            s.setReopenLast(false);
            QVERIFY(waitFor([&] { return s.theme() == "light" && s.editorFontSize() == 17 && s.editorFontFamily() == "Menlo" && s.editorWordWrap() && !s.reopenLast(); }));
            client.shutdown();
        }
        DaemonClient client2;
        SettingsStore s2(&client2);
        client2.start(m_daemon, { "--data-dir", data.path() });
        QVERIFY(waitFor([&] { return s2.loaded(); }, 10000));
        QCOMPARE(s2.theme(), QStringLiteral("light"));
        QCOMPARE(s2.editorFontSize(), 17);
        QCOMPARE(s2.editorFontFamily(), QStringLiteral("Menlo"));
        QCOMPARE(s2.editorWordWrap(), true);
        QCOMPARE(s2.reopenLast(), false);
    }

    void reloadsAfterTheDaemonIsRestarted()
    {
        QTemporaryDir data;
        DaemonClient client;
        SettingsStore s(&client);
        client.start(m_daemon, { "--data-dir", data.path() });
        QVERIFY(waitFor([&] { return s.loaded(); }, 10000));
        s.setEditorFontSize(25);
        QVERIFY(waitFor([&] { return s.editorFontSize() == 25; }));
        client.restart();
        QVERIFY(waitFor([&] { return client.state() == DaemonClient::Ready; }, 10000));
        QTest::qWait(300);
        QCOMPARE(s.editorFontSize(), 25);
    }

    void unknownKeysInANotificationAreIgnored()
    {
        QTemporaryDir data;
        DaemonClient client;
        SettingsStore s(&client);
        client.start(m_daemon, { "--data-dir", data.path() });
        QVERIFY(waitFor([&] { return s.loaded(); }, 10000));
        QSignalSpy any(&s, &SettingsStore::changed);
        bool done = false;
        client.call("settings/set", { { "values", QJsonObject{ { "x.custom", 1 }, { "editor.fontSize", "oops" } } } },
                    [&](const QJsonObject &, const RpcError &) { done = true; });
        QVERIFY(waitFor([&] { return done; }));
        QTest::qWait(200);
        QCOMPARE(any.count(), 0);
        QCOMPARE(s.editorFontSize(), 13);
    }
};

QTEST_GUILESS_MAIN(TstSettings)
#include "tst_settings.moc"
