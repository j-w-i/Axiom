#include "Shell.h"

#include "editor/EditorSetup.h"

#include <QCoreApplication>
#include <QEventLoop>
#include <QTimer>
#include <QDir>
#include <QFileInfo>
#include <QFontDatabase>
#include <QGuiApplication>
#include <QQmlContext>
#include <QQuickWindow>
#include <QScreen>
#include <QSettings>
#include <QStandardPaths>
#include <QtQuickControls2/QQuickStyle>

QString locateDaemon(const QString &explicitPath)
{
    if (!explicitPath.isEmpty())
        return explicitPath;
    const QString fromEnv = qEnvironmentVariable("AXIOMD");
    if (!fromEnv.isEmpty())
        return fromEnv;
#ifdef Q_OS_WIN
    const QString name = QStringLiteral("axiomd.exe");
#else
    const QString name = QStringLiteral("axiomd");
#endif
    const QFileInfo sibling(QDir(QCoreApplication::applicationDirPath()).filePath(name));
    if (sibling.isFile() && sibling.isExecutable())
        return sibling.absoluteFilePath();
    return QStandardPaths::findExecutable(QStringLiteral("axiomd"));
}

QString resolveMonoFamily()
{
    static const QStringList candidates{ QStringLiteral("JetBrains Mono"),
                                         QStringLiteral("Cascadia Code"),
                                         QStringLiteral("SF Mono"),
                                         QStringLiteral("Menlo"),
                                         QStringLiteral("Consolas"),
                                         QStringLiteral("DejaVu Sans Mono") };
    const QStringList families = QFontDatabase::families();
    for (const QString &c : candidates)
        if (families.contains(c))
            return c;
    return QStringLiteral("monospace");
}

Shell::Shell(const ShellOptions &options, QObject *parent)
    : QObject(parent), m_options(options), m_client(this), m_backend(&m_client, this), m_workspace(&m_client, this),
      m_documents(&m_client, &m_workspace, this)
{
    // The design system is built on the Basic style.
    QQuickStyle::setStyle(QStringLiteral("Basic"));
}

Shell::~Shell()
{
    saveGeometry();
    // Persist explorer and tab state, waiting (bounded) for the daemon to confirm.
    if (m_client.state() == DaemonClient::Ready && m_workspace.isOpen()) {
        QEventLoop loop;
        QTimer::singleShot(1500, &loop, &QEventLoop::quit);
        m_workspace.flushState([&loop] { loop.quit(); });
        loop.exec();
    }
    m_client.shutdown();
}

bool Shell::load()
{
    m_engine.rootContext()->setContextProperty(QStringLiteral("backend"), &m_backend);
    m_engine.rootContext()->setContextProperty(QStringLiteral("workspace"), &m_workspace);
    m_engine.rootContext()->setContextProperty(QStringLiteral("documents"), &m_documents);
    m_editorAssets = locateEditorAssets(m_options.editorAssets);
    m_engine.rootContext()->setContextProperty(QStringLiteral("editorAvailable"), !m_editorAssets.isEmpty());
    if (!m_editorAssets.isEmpty())
        installEditorSchemeHandler(m_editorAssets, this);
    else
        qWarning("[editor] no editor assets found (use --editor-assets <dir> or build editor/dist)");
    // Once per launch: reopen the last workspace when the setting says so.
    connect(&m_client, &DaemonClient::stateChanged, this, [this] {
        if (m_client.state() == DaemonClient::Ready && !m_reopenDone) {
            m_reopenDone = true;
            m_workspace.reopenLast();
        }
    });
    QObject::connect(&m_engine, &QQmlEngine::quit, qApp, &QCoreApplication::quit);
    m_engine.loadFromModule("Axiom.App", "Main");
    if (m_engine.rootObjects().isEmpty())
        return false;
    m_window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
    if (!m_window)
        return false;

    if (QObject *theme = m_engine.singletonInstance<QObject *>("Axiom.Ui", "Theme"))
        theme->setProperty("monoFamily", resolveMonoFamily());

    restoreGeometry();
    connect(m_window, &QQuickWindow::closing, this, &Shell::saveGeometry);
    connect(qApp, &QCoreApplication::aboutToQuit, this, &Shell::saveGeometry);

    QStringList args;
    if (!m_options.dataDir.isEmpty())
        args << QStringLiteral("--data-dir") << m_options.dataDir;
    m_client.start(locateDaemon(m_options.daemonPath), args);
    return true;
}

static QSettings windowSettings()
{
    return QSettings(QStringLiteral("axiom"), QStringLiteral("Axiom"));
}

void Shell::restoreGeometry()
{
    if (!m_window)
        return;
    QSettings s = windowSettings();
    const QRect rect = s.value(QStringLiteral("window/geometry")).toRect();
    if (rect.isValid() && rect.width() >= m_window->minimumWidth()
        && rect.height() >= m_window->minimumHeight()) {
        // Never restore onto a screen that is gone.
        bool visible = false;
        for (QScreen *screen : QGuiApplication::screens())
            if (screen->availableGeometry().intersects(rect))
                visible = true;
        if (visible)
            m_window->setGeometry(rect);
    }
    if (s.value(QStringLiteral("window/maximized")).toBool())
        m_window->showMaximized();
}

void Shell::saveGeometry()
{
    if (!m_window || !m_window->isVisible())
        return;
    QSettings s = windowSettings();
    const bool maximized = m_window->visibility() == QWindow::Maximized;
    s.setValue(QStringLiteral("window/maximized"), maximized);
    if (!maximized && m_window->visibility() != QWindow::FullScreen)
        s.setValue(QStringLiteral("window/geometry"), m_window->geometry());
}
