#pragma once
#include <QAbstractItemModel>
#include <QPointer>
#include <QSet>
#include <QStringList>
#include <memory>
#include <vector>

class DaemonClient;

// The workspace's file tree. Directories are listed lazily with `fs/list` on
// first expansion and kept in sync by re-listing expanded directories when
// the daemon reports changes (minimal row insert/remove diffs, so expansion
// and selection survive). The shell never touches the filesystem: every
// listing comes from the daemon.
class FileTreeModel : public QAbstractItemModel
{
    Q_OBJECT
    Q_PROPERTY(bool rootLoaded READ rootLoaded NOTIFY rootLoadedChanged)

public:
    enum Roles {
        NameRole = Qt::UserRole + 1,
        PathRole,     // workspace-relative, '/'-separated
        KindRole,     // "file" | "directory" | "symlink" | "error"
        IgnoredRole,
        ExpandedRole,
        LoadingRole,
    };

    explicit FileTreeModel(DaemonClient *client, QObject *parent = nullptr);
    ~FileTreeModel() override;

    // Binds the model to a workspace (empty = none) and clears it.
    void setWorkspace(const QString &workspaceId);
    // Directories to expand as soon as they appear (restoring saved state).
    void setExpandedToRestore(const QStringList &paths);
    // Lists the workspace root.
    void loadRoot();
    bool rootLoaded() const;

    // Paths of expanded directories, sorted.
    Q_INVOKABLE QStringList expandedPaths() const;
    // Records that the user expanded/collapsed `index` and fetches children when needed.
    Q_INVOKABLE void setExpanded(const QModelIndex &index, bool expanded);
    Q_INVOKABLE QString pathAt(const QModelIndex &index) const;
    Q_INVOKABLE bool isDirectory(const QModelIndex &index) const;
    // A file or symlink (not a directory or an inline error row).
    Q_INVOKABLE bool isFile(const QModelIndex &index) const;

    // `paths` are workspace-relative changed paths from fs/didChange; "" = re-list everything shown.
    void applyChanges(const QStringList &paths);

    // QAbstractItemModel
    QModelIndex index(int row, int column, const QModelIndex &parent = {}) const override;
    QModelIndex parent(const QModelIndex &child) const override;
    int rowCount(const QModelIndex &parent = {}) const override;
    int columnCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    bool hasChildren(const QModelIndex &parent = {}) const override;
    bool canFetchMore(const QModelIndex &parent) const override;
    void fetchMore(const QModelIndex &parent) override;

signals:
    void rootLoadedChanged();
    // A restored directory has appeared: the view should expand it.
    void expandRequested(const QModelIndex &index);
    // User-driven expansion changed (not emitted while restoring).
    void expandedPathsChanged();
    // The root listing failed (child listings show an inline error row instead).
    void rootListFailed(const QString &message);

private:
    struct Node
    {
        Node *parent = nullptr;
        QString name;
        QString path;
        QString kind;
        bool ignored = false;
        bool expanded = false;
        bool loaded = false;
        bool loading = false;
        bool refetch = false; // a change arrived while listing: list again afterwards
        std::vector<std::unique_ptr<Node>> children;

        int row() const;
    };
    struct Entry
    {
        QString name;
        QString kind;
        bool ignored = false;
    };

    Node *nodeFor(const QModelIndex &index) const;
    QModelIndex indexFor(const Node *node) const;
    Node *find(const QString &path) const;
    void fetch(Node *node, bool queueIfBusy = false);
    void listed(const QString &path, quint64 epoch, const QJsonObject &result, const QString &error);
    void applyListing(Node *dir, const QList<Entry> &entries);
    void setError(Node *dir, const QString &message);
    void clearErrorRows(Node *dir);
    void collectExpanded(const Node *node, QStringList *out) const;
    void refreshExpanded(Node *node, const QSet<QString> &parents, bool all);
    static QString parentPath(const QString &path);

    QPointer<DaemonClient> m_client;
    QString m_workspaceId;
    quint64 m_epoch = 0; // answers for an earlier workspace are ignored
    Node m_root;
    QSet<QString> m_restore;
};
