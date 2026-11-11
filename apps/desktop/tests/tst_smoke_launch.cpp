#include "Shell.h"

#include <QGuiApplication>
#include <QImage>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTemporaryDir>
#include <QtQml>
#include <csignal>
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

        ::kill(static_cast<pid_t>(shell.client()->processId()), SIGKILL);
        QVERIFY(waitFor([&] { return shell.backend()->failed(); }));
        QVERIFY(waitFor([&] { return statusText(w) == QStringLiteral("Backend stopped"); }));
        QVERIFY(!shell.backend()->errorText().isEmpty());

        // The banner is a visible AxBanner carrying the error text.
        QQuickItem *banner = findByProperty(w->contentItem(), "actionText", QStringLiteral("Restart backend"));
        QVERIFY(banner);
        QVERIFY(banner->isVisible());
        QCOMPARE(banner->property("text").toString(), shell.backend()->errorText());
        w->grabWindow().save(m_shots + "/backend-failed.png");

        shell.backend()->restart();
        QVERIFY(waitFor([&] { return shell.backend()->ready(); }));
        QVERIFY(!banner->isVisible());
        QVERIFY(waitFor([&] { return statusText(w) == QStringLiteral("Connected — axiomd 0.1.0"); }));
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

QTEST_MAIN(TstSmokeLaunch)
#include "tst_smoke_launch.moc"
