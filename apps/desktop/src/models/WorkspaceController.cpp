#include "WorkspaceController.h"
#include "daemon/DaemonClient.h"

#include <QJsonArray>
#include <QJsonObject>

namespace {
constexpr int StateSaveDebounceMs = 500;
const char ExpandedKey[] = "explorer.expanded";
} // namespace

WorkspaceController::WorkspaceController(DaemonClient *client, QObject *parent)
    : QObject(parent), m_client(client), m_tree(client, this), m_recent(this)
{
    m_saveTimer.setSingleShot(true);
    m_saveTimer.setInterval(StateSaveDebounceMs);
    connect(&m_saveTimer, &QTimer::timeout, this, [this] { flushState(); });

    connect(client, &DaemonClient::notification, this, &WorkspaceController::onNotification);
    connect(&m_tree, &FileTreeModel::expandedPathsChanged, this, [this] {
        if (!m_restoring)
            scheduleStateSave();
    });
    connect(&m_tree, &FileTreeModel::rootListFailed, this, [this](const QString &message) {
        emit errorOccurred(tr("Could not list the folder: %1").arg(message));
    });
    // A new daemon process knows no workspaces: drop ours and show what the backend says.
    connect(client, &DaemonClient::stateChanged, this, [this] {
        if (m_client->state() == DaemonClient::Ready) {
            refreshRecent();
        } else {
            m_pendingClose = 0;
            m_deferredOpen.clear();
            if (isOpen() || m_opening)
                clearOpenState();
        }
    });
}

void WorkspaceController::setOpening(bool o)
{
    if (m_opening == o)
        return;
    m_opening = o;
    emit openingChanged();
}

void WorkspaceController::clearOpenState()
{
    m_saveTimer.stop();
    ++m_generation;
    m_id.clear();
    m_root.clear();
    m_name.clear();
    m_tree.setWorkspace({});
    setOpening(false);
    emit openChanged();
    emit closed();
}

void WorkspaceController::openFolderUrl(const QUrl &url)
{
    openFolder(url.isLocalFile() ? url.toLocalFile() : url.toString());
}

void WorkspaceController::openFolder(const QString &path)
{
    if (!m_client || path.isEmpty())
        return;
    const quint64 gen = ++m_generation;
    setOpening(true);
    // A close still in flight would otherwise reach the daemon after this open and kill
    // it (opening a folder is idempotent per root and would return the closing id).
    if (m_pendingClose > 0) {
        m_deferredOpen = path;
        return;
    }

    auto openNew = [this, path, gen] {
        if (gen != m_generation)
            return;
        m_client->call(QStringLiteral("workspace/open"), { { "path", path } },
            [this, gen](const QJsonObject &r, const RpcError &err) {
                if (gen != m_generation)
                    return;
                if (!err.ok()) {
                    setOpening(false);
                    emit errorOccurred(err.kind == QLatin1String("not_found")
                                           ? tr("The folder does not exist.")
                                           : tr("Could not open the folder: %1").arg(err.message));
                    return;
                }
                const QString id = r.value(QLatin1String("workspaceId")).toString();
                const QString root = r.value(QLatin1String("root")).toString();
                const QString name = r.value(QLatin1String("name")).toString();

                m_id = id;
                m_root = root;
                m_name = name;
                m_tree.setWorkspace(id);
                m_restoring = true;
                emit openChanged();

                // fs/watch, then the saved explorer state, then the root listing.
                m_client->call(QStringLiteral("fs/watch"), { { "workspaceId", id }, { "enabled", true } },
                    [this, gen](const QJsonObject &, const RpcError &) {
                        if (gen != m_generation)
                            return;
                        m_client->call(QStringLiteral("workspace/getState"),
                            { { "workspaceId", m_id }, { "keys", QJsonArray{ QString::fromLatin1(ExpandedKey) } } },
                            [this, gen](const QJsonObject &s, const RpcError &) {
                                if (gen != m_generation)
                                    return;
                                QStringList expanded;
                                const QJsonArray arr = s.value(QLatin1String("values")).toObject()
                                                           .value(QString::fromLatin1(ExpandedKey)).toArray();
                                for (const QJsonValue &v : arr)
                                    if (v.isString())
                                        expanded.append(v.toString());
                                m_tree.setExpandedToRestore(expanded);
                                // `opened` means the first listing is in (or failed).
                                auto finish = [this, gen] {
                                    if (gen != m_generation || !m_opening)
                                        return;
                                    m_restoring = false;
                                    setOpening(false);
                                    emit opened();
                                    refreshRecent();
                                };
                                auto *once = new QObject(this);
                                connect(&m_tree, &FileTreeModel::rootLoadedChanged, once,
                                        [once, finish] { finish(); once->deleteLater(); });
                                connect(&m_tree, &FileTreeModel::rootListFailed, once,
                                        [once, finish] { finish(); once->deleteLater(); });
                                m_tree.loadRoot();
                            });
                    });
            });
    };

    if (isOpen()) {
        // Switching workspaces: save and close the current one first.
        const QString oldId = m_id;
        flushState([this, oldId, openNew] {
            m_client->call(QStringLiteral("workspace/close"), { { "workspaceId", oldId } },
                           [openNew](const QJsonObject &, const RpcError &) { openNew(); });
        });
        // Show the switch immediately.
        m_id.clear();
        m_tree.setWorkspace({});
        emit openChanged();
    } else {
        openNew();
    }
}

void WorkspaceController::close()
{
    if (!isOpen() || !m_client)
        return;
    const QString id = m_id;
    const QString expandedKeyId = id;
    ++m_pendingClose;
    flushState([this, expandedKeyId] {
        if (!m_client) {
            m_pendingClose = 0;
            return;
        }
        m_client->call(QStringLiteral("workspace/close"), { { "workspaceId", expandedKeyId } },
                       [this](const QJsonObject &, const RpcError &) {
                           if (m_pendingClose > 0)
                               --m_pendingClose;
                           if (m_pendingClose == 0 && !m_deferredOpen.isEmpty()) {
                               const QString path = std::exchange(m_deferredOpen, QString());
                               openFolder(path);
                           }
                       });
    });
    clearOpenState();
}

void WorkspaceController::flushState(std::function<void()> then)
{
    m_saveTimer.stop();
    if (!isOpen() || !m_client || m_tree.rootLoaded() == false) {
        if (then)
            then();
        return;
    }
    QJsonArray arr;
    for (const QString &p : m_tree.expandedPaths())
        arr.append(p);
    m_client->call(QStringLiteral("workspace/setState"),
                   { { "workspaceId", m_id },
                     { "values", QJsonObject{ { QString::fromLatin1(ExpandedKey), arr } } } },
                   [then = std::move(then)](const QJsonObject &, const RpcError &) {
                       if (then)
                           then();
                   });
}

void WorkspaceController::scheduleStateSave()
{
    if (isOpen())
        m_saveTimer.start();
}

void WorkspaceController::refreshRecent()
{
    if (!m_client)
        return;
    m_client->call(QStringLiteral("workspace/recent"), {},
        [this](const QJsonObject &r, const RpcError &err) {
            if (!err.ok())
                return;
            QList<RecentModel::Item> items;
            for (const QJsonValue &v : r.value(QLatin1String("workspaces")).toArray()) {
                const QJsonObject o = v.toObject();
                items.append({ o.value(QLatin1String("root")).toString(),
                               o.value(QLatin1String("name")).toString() });
            }
            m_recent.setItems(items);
        });
}

void WorkspaceController::reopenLast()
{
    if (!m_client)
        return;
    m_client->call(QStringLiteral("settings/get"),
        { { "keys", QJsonArray{ "workspace.reopenLast", "workspace.lastRoot" } } },
        [this](const QJsonObject &r, const RpcError &err) {
            if (!err.ok() || isOpen() || m_opening)
                return;
            const QJsonObject v = r.value(QLatin1String("values")).toObject();
            const QString last = v.value(QLatin1String("workspace.lastRoot")).toString();
            if (v.value(QLatin1String("workspace.reopenLast")).toBool(true) && !last.isEmpty())
                openFolder(last);
        });
}

void WorkspaceController::onNotification(const QString &method, const QJsonObject &params)
{
    if (method != QLatin1String("fs/didChange"))
        return;
    if (params.value(QLatin1String("workspaceId")).toString() != m_id)
        return;
    QStringList paths;
    for (const QJsonValue &v : params.value(QLatin1String("changes")).toArray())
        paths.append(v.toObject().value(QLatin1String("path")).toString());
    m_tree.applyChanges(paths);
}
