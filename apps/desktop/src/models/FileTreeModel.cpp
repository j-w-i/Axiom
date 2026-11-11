#include "FileTreeModel.h"
#include "daemon/DaemonClient.h"

#include <QJsonArray>
#include <QJsonObject>
#include <algorithm>

int FileTreeModel::Node::row() const
{
    if (!parent)
        return 0;
    for (size_t i = 0; i < parent->children.size(); ++i)
        if (parent->children[i].get() == this)
            return static_cast<int>(i);
    return 0;
}

FileTreeModel::FileTreeModel(DaemonClient *client, QObject *parent)
    : QAbstractItemModel(parent), m_client(client)
{
    m_root.kind = QStringLiteral("directory");
    m_root.expanded = true;
}

FileTreeModel::~FileTreeModel() = default;

QString FileTreeModel::parentPath(const QString &path)
{
    const qsizetype slash = path.lastIndexOf(QLatin1Char('/'));
    return slash < 0 ? QString() : path.left(slash);
}

FileTreeModel::Node *FileTreeModel::nodeFor(const QModelIndex &index) const
{
    return index.isValid() ? static_cast<Node *>(index.internalPointer())
                           : const_cast<Node *>(&m_root);
}

QModelIndex FileTreeModel::indexFor(const Node *node) const
{
    if (!node || node == &m_root)
        return {};
    return createIndex(node->row(), 0, const_cast<Node *>(node));
}

FileTreeModel::Node *FileTreeModel::find(const QString &path) const
{
    Node *n = const_cast<Node *>(&m_root);
    if (path.isEmpty())
        return n;
    for (const QString &part : path.split(QLatin1Char('/'))) {
        Node *next = nullptr;
        for (const auto &c : n->children)
            if (c->name == part && c->kind != QLatin1String("error")) {
                next = c.get();
                break;
            }
        if (!next)
            return nullptr;
        n = next;
    }
    return n;
}

// ---- workspace binding -------------------------------------------------------

void FileTreeModel::setWorkspace(const QString &workspaceId)
{
    beginResetModel();
    ++m_epoch;
    m_workspaceId = workspaceId;
    m_root.children.clear();
    m_root.loaded = false;
    m_root.loading = false;
    m_root.refetch = false;
    m_restore.clear();
    endResetModel();
    emit rootLoadedChanged();
}

void FileTreeModel::setExpandedToRestore(const QStringList &paths)
{
    m_restore = QSet<QString>(paths.begin(), paths.end());
}

void FileTreeModel::loadRoot()
{
    if (!m_workspaceId.isEmpty())
        fetch(&m_root);
}

bool FileTreeModel::rootLoaded() const
{
    return m_root.loaded;
}

// ---- QAbstractItemModel ------------------------------------------------------

QModelIndex FileTreeModel::index(int row, int column, const QModelIndex &parent) const
{
    if (column != 0 || row < 0)
        return {};
    const Node *p = nodeFor(parent);
    if (static_cast<size_t>(row) >= p->children.size())
        return {};
    return createIndex(row, 0, p->children[static_cast<size_t>(row)].get());
}

QModelIndex FileTreeModel::parent(const QModelIndex &child) const
{
    if (!child.isValid())
        return {};
    const Node *n = nodeFor(child);
    return indexFor(n->parent);
}

int FileTreeModel::rowCount(const QModelIndex &parent) const
{
    if (parent.isValid() && parent.column() != 0)
        return 0;
    return static_cast<int>(nodeFor(parent)->children.size());
}

int FileTreeModel::columnCount(const QModelIndex &) const
{
    return 1;
}

QVariant FileTreeModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid())
        return {};
    const Node *n = nodeFor(index);
    switch (role) {
    case Qt::DisplayRole:
    case NameRole: return n->name;
    case PathRole: return n->path;
    case KindRole: return n->kind;
    case IgnoredRole: return n->ignored;
    case ExpandedRole: return n->expanded;
    case LoadingRole: return n->loading;
    default: return {};
    }
}

QHash<int, QByteArray> FileTreeModel::roleNames() const
{
    return {
        { NameRole, "name" },       { PathRole, "path" },       { KindRole, "kind" },
        { IgnoredRole, "ignored" }, { ExpandedRole, "expanded" }, { LoadingRole, "loading" },
    };
}

bool FileTreeModel::hasChildren(const QModelIndex &parent) const
{
    const Node *n = nodeFor(parent);
    if (n == &m_root)
        return !n->children.empty();
    // Directories always offer expansion; their content is unknown until listed.
    return n->kind == QLatin1String("directory");
}

bool FileTreeModel::canFetchMore(const QModelIndex &parent) const
{
    if (!parent.isValid())
        return false;
    const Node *n = nodeFor(parent);
    return n->kind == QLatin1String("directory") && !n->loaded && !n->loading
        && !m_workspaceId.isEmpty();
}

void FileTreeModel::fetchMore(const QModelIndex &parent)
{
    if (canFetchMore(parent))
        fetch(nodeFor(parent));
}

// ---- user interaction --------------------------------------------------------

QString FileTreeModel::pathAt(const QModelIndex &index) const
{
    return index.isValid() ? nodeFor(index)->path : QString();
}

bool FileTreeModel::isDirectory(const QModelIndex &index) const
{
    return index.isValid() && nodeFor(index)->kind == QLatin1String("directory");
}

bool FileTreeModel::isFile(const QModelIndex &index) const
{
    if (!index.isValid())
        return false;
    const QString k = nodeFor(index)->kind;
    return k == QLatin1String("file") || k == QLatin1String("symlink");
}

void FileTreeModel::setExpanded(const QModelIndex &index, bool expanded)
{
    if (!index.isValid())
        return;
    Node *n = nodeFor(index);
    if (n->kind != QLatin1String("directory") || n->expanded == expanded)
        return;
    n->expanded = expanded;
    emit dataChanged(index, index, { ExpandedRole });
    // Also re-list when already loaded: it may have changed while collapsed.
    if (expanded)
        fetch(n);
    emit expandedPathsChanged();
}

void FileTreeModel::collectExpanded(const Node *node, QStringList *out) const
{
    for (const auto &c : node->children) {
        if (c->kind != QLatin1String("directory"))
            continue;
        if (c->expanded)
            out->append(c->path);
        collectExpanded(c.get(), out);
    }
}

QStringList FileTreeModel::expandedPaths() const
{
    QStringList out;
    collectExpanded(&m_root, &out);
    out.sort();
    return out;
}

// ---- listing -----------------------------------------------------------------

void FileTreeModel::fetch(Node *node, bool queueIfBusy)
{
    if (!m_client || m_workspaceId.isEmpty())
        return;
    if (node->loading) {
        // A change arrived mid-listing: that answer may already be stale.
        if (queueIfBusy)
            node->refetch = true;
        return;
    }
    node->loading = true;
    // Only the first load is shown as "loading"; refreshes are silent.
    if (node != &m_root && !node->loaded) {
        const QModelIndex idx = indexFor(node);
        emit dataChanged(idx, idx, { LoadingRole });
    }
    const QString path = node->path;
    const quint64 epoch = m_epoch;
    QPointer<FileTreeModel> self(this);
    m_client->call(QStringLiteral("fs/list"),
                   { { "workspaceId", m_workspaceId }, { "path", path } },
                   [self, path, epoch](const QJsonObject &result, const RpcError &err) {
                       if (self)
                           self->listed(path, epoch, result, err.ok() ? QString() : err.message);
                   });
}

void FileTreeModel::listed(const QString &path, quint64 epoch, const QJsonObject &result,
                           const QString &error)
{
    if (epoch != m_epoch)
        return; // answer for a workspace that is no longer shown
    Node *dir = find(path);
    if (!dir)
        return; // the directory disappeared while it was being listed
    dir->loading = false;
    const bool wasLoaded = dir->loaded;

    if (!error.isEmpty()) {
        if (dir == &m_root) {
            emit rootListFailed(error);
        } else {
            const QModelIndex idx = indexFor(dir);
            emit dataChanged(idx, idx, { LoadingRole });
            setError(dir, error);
        }
        return;
    }

    QList<Entry> entries;
    for (const QJsonValue &v : result.value(QLatin1String("entries")).toArray()) {
        const QJsonObject o = v.toObject();
        entries.append({ o.value(QLatin1String("name")).toString(),
                         o.value(QLatin1String("kind")).toString(),
                         o.value(QLatin1String("ignored")).toBool() });
    }
    clearErrorRows(dir);
    dir->loaded = true;
    if (dir == &m_root) {
        applyListing(dir, entries);
        if (!wasLoaded)
            emit rootLoadedChanged();
    } else {
        applyListing(dir, entries);
        const QModelIndex idx = indexFor(dir);
        emit dataChanged(idx, idx, { LoadingRole });
    }
    if (dir->refetch) {
        dir->refetch = false;
        fetch(dir, true);
    }
}

void FileTreeModel::applyListing(Node *dir, const QList<Entry> &entries)
{
    const QModelIndex parentIdx = indexFor(dir);

    QHash<QString, QString> wanted; // name -> kind
    for (const Entry &e : entries)
        wanted.insert(e.name, e.kind);

    // 1. Remove rows that are gone or changed kind (backwards keeps rows valid).
    for (int i = static_cast<int>(dir->children.size()) - 1; i >= 0; --i) {
        const Node *c = dir->children[static_cast<size_t>(i)].get();
        const auto it = wanted.constFind(c->name);
        if (it == wanted.constEnd() || *it != c->kind) {
            beginRemoveRows(parentIdx, i, i);
            dir->children.erase(dir->children.begin() + i);
            endRemoveRows();
        }
    }

    // 2. What is left is a subsequence of `entries` in the same order: walk
    //    the new list, updating rows that match and inserting the rest.
    QList<QModelIndex> toExpand;
    for (int i = 0; i < entries.size(); ++i) {
        const Entry &e = entries.at(i);
        if (static_cast<size_t>(i) < dir->children.size()
            && dir->children[static_cast<size_t>(i)]->name == e.name) {
            Node *c = dir->children[static_cast<size_t>(i)].get();
            if (c->ignored != e.ignored) {
                c->ignored = e.ignored;
                const QModelIndex idx = createIndex(i, 0, c);
                emit dataChanged(idx, idx, { IgnoredRole });
            }
            continue;
        }
        auto node = std::make_unique<Node>();
        node->parent = dir;
        node->name = e.name;
        node->path = dir->path.isEmpty() ? e.name : dir->path + QLatin1Char('/') + e.name;
        node->kind = e.kind;
        node->ignored = e.ignored;
        const bool restore = node->kind == QLatin1String("directory") && m_restore.remove(node->path);
        node->expanded = restore;
        Node *raw = node.get();
        beginInsertRows(parentIdx, i, i);
        dir->children.insert(dir->children.begin() + i, std::move(node));
        endInsertRows();
        if (restore)
            toExpand.append(createIndex(i, 0, raw));
    }

    for (const QModelIndex &idx : std::as_const(toExpand)) {
        // Rows may have shifted after later inserts; re-derive from the node.
        const QModelIndex fresh = indexFor(nodeFor(idx));
        emit expandRequested(fresh);
        fetch(nodeFor(fresh));
    }
}

void FileTreeModel::clearErrorRows(Node *dir)
{
    const QModelIndex parentIdx = indexFor(dir);
    for (int i = static_cast<int>(dir->children.size()) - 1; i >= 0; --i) {
        if (dir->children[static_cast<size_t>(i)]->kind == QLatin1String("error")) {
            beginRemoveRows(parentIdx, i, i);
            dir->children.erase(dir->children.begin() + i);
            endRemoveRows();
        }
    }
}

void FileTreeModel::setError(Node *dir, const QString &message)
{
    const QModelIndex parentIdx = indexFor(dir);
    if (!dir->children.empty()) {
        beginRemoveRows(parentIdx, 0, static_cast<int>(dir->children.size()) - 1);
        dir->children.clear();
        endRemoveRows();
    }
    auto err = std::make_unique<Node>();
    err->parent = dir;
    err->name = message;
    err->path = dir->path;
    err->kind = QStringLiteral("error");
    beginInsertRows(parentIdx, 0, 0);
    dir->children.push_back(std::move(err));
    endInsertRows();
}

// ---- change notifications ----------------------------------------------------

void FileTreeModel::applyChanges(const QStringList &paths)
{
    if (m_workspaceId.isEmpty() || !m_root.loaded)
        return;
    bool all = false;
    QSet<QString> parents;
    for (const QString &p : paths) {
        if (p.isEmpty())
            all = true;
        else
            parents.insert(parentPath(p));
    }
    refreshExpanded(&m_root, parents, all);
}

void FileTreeModel::refreshExpanded(Node *node, const QSet<QString> &parents, bool all)
{
    // The root is always "expanded"; other directories only when the user expanded them
    // and their content is known.
    const bool shown = node == &m_root || (node->expanded && node->loaded);
    if (!shown)
        return;
    if (all || parents.contains(node->path))
        fetch(node, true);
    // `fetch` is asynchronous, so the children list is stable here.
    for (const auto &c : node->children)
        if (c->kind == QLatin1String("directory"))
            refreshExpanded(c.get(), parents, all);
}
