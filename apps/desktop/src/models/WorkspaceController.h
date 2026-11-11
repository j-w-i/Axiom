#pragma once
#include "FileTreeModel.h"
#include "RecentModel.h"

#include <QObject>
#include <QTimer>
#include <QUrl>

class DaemonClient;

// Opens and closes workspaces and owns what belongs to the open one: the
// file tree, the recent list and persisted explorer state. QML sees this as
// `workspace`; it never sees JSON-RPC.
class WorkspaceController : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool open READ isOpen NOTIFY openChanged)
    Q_PROPERTY(bool opening READ opening NOTIFY openingChanged)
    Q_PROPERTY(QString name READ name NOTIFY openChanged)
    Q_PROPERTY(QString root READ root NOTIFY openChanged)
    Q_PROPERTY(QString id READ id NOTIFY openChanged)
    Q_PROPERTY(QObject *recent READ recent CONSTANT)
    Q_PROPERTY(QObject *tree READ tree CONSTANT)

public:
    explicit WorkspaceController(DaemonClient *client, QObject *parent = nullptr);

    bool isOpen() const { return !m_id.isEmpty(); }
    bool opening() const { return m_opening; }
    QString name() const { return m_name; }
    QString root() const { return m_root; }
    QString id() const { return m_id; }
    RecentModel *recentModel() { return &m_recent; }
    FileTreeModel *treeModel() { return &m_tree; }
    QObject *recent() { return &m_recent; }
    QObject *tree() { return &m_tree; }

    // Opens a directory: workspace/open → fs/watch → state → root listing.
    Q_INVOKABLE void openFolder(const QString &path);
    // For FolderDialog results (file: URLs).
    Q_INVOKABLE void openFolderUrl(const QUrl &url);
    Q_INVOKABLE void close();
    Q_INVOKABLE void refreshRecent();
    // Opens the last workspace when `workspace.reopenLast` is true and `workspace.lastRoot` is set.
    Q_INVOKABLE void reopenLast();
    // Writes explorer state now instead of after the debounce.
    void flushState(std::function<void()> then = {});

signals:
    void openChanged();
    void openingChanged();
    void errorOccurred(const QString &message);
    // The tree finished its first listing for the workspace just opened.
    void opened();
    void closed();

private:
    void onNotification(const QString &method, const QJsonObject &params);
    void scheduleStateSave();
    void setOpening(bool o);
    void clearOpenState();

    QPointer<DaemonClient> m_client;
    FileTreeModel m_tree;
    RecentModel m_recent;
    QString m_id;
    QString m_root;
    QString m_name;
    bool m_opening = false;
    bool m_restoring = false;
    quint64 m_generation = 0; // invalidates answers for an open that was superseded
    QTimer m_saveTimer;
};
