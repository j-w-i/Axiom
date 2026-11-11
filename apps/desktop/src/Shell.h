#pragma once
#include "Backend.h"
#include "daemon/DaemonClient.h"
#include "models/OpenDocumentsModel.h"
#include "models/TaskController.h"
#include "models/WorkspaceController.h"

#include <QQmlApplicationEngine>
#include <QString>

class QQuickWindow;

struct ShellOptions
{
    QString editorAssets; // from --editor-assets; empty = look it up
    QString daemonPath; // from --daemon; empty = look it up
    QString dataDir;    // forwarded to the daemon as --data-dir
};

// Finds axiomd: explicit path, $AXIOMD, a sibling of the executable, then PATH.
// Returns an empty string when nothing is found.
QString locateDaemon(const QString &explicitPath);

// First available monospace family from the design system's list.
QString resolveMonoFamily();

// Owns the daemon connection, the QML engine and the main window.
class Shell : public QObject
{
    Q_OBJECT
public:
    explicit Shell(const ShellOptions &options, QObject *parent = nullptr);
    ~Shell() override;

    // Creates the window and starts the daemon. False if the QML did not load.
    bool load();

    DaemonClient *client() { return &m_client; }
    Backend *backend() { return &m_backend; }
    WorkspaceController *workspace() { return &m_workspace; }
    OpenDocumentsModel *documents() { return &m_documents; }
    TaskController *tasks() { return &m_tasks; }
    QString editorAssetDir() const { return m_editorAssets; }
    QQmlApplicationEngine *engine() { return &m_engine; }
    QQuickWindow *window() const { return m_window; }

private:
    void restoreGeometry();
    void saveGeometry();

    ShellOptions m_options;
    DaemonClient m_client;
    Backend m_backend;
    WorkspaceController m_workspace;
    OpenDocumentsModel m_documents;
    TaskController m_tasks;
    QString m_editorAssets;
    bool m_reopenDone = false;
    QQmlApplicationEngine m_engine; // last: destroyed first, before what QML uses
    QQuickWindow *m_window = nullptr;
};
