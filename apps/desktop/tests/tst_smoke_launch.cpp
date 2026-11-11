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
