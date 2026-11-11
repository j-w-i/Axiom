#include "OpenDocumentsModel.h"
#include "WorkspaceController.h"
#include "daemon/DaemonClient.h"

#include <QDesktopServices>
#include <QFileInfo>
#include <QJsonArray>
#include <QUrl>

namespace {
constexpr int TabSaveDebounceMs = 500;
const char OpenPathsKey[] = "editor.openPaths";
const char ActivePathKey[] = "editor.activePath";

QJsonObject pathObj(const QString &path)
{
    return { { "path", path } };
}
} // namespace

OpenDocumentsModel::OpenDocumentsModel(DaemonClient *client, WorkspaceController *workspace,
                                       QObject *parent)
    : QAbstractListModel(parent), m_client(client), m_workspace(workspace), m_bridge(this)
{
    m_bridge.setListener(this);
    m_tabSaveTimer.setSingleShot(true);
    m_tabSaveTimer.setInterval(TabSaveDebounceMs);
    connect(&m_tabSaveTimer, &QTimer::timeout, this, &OpenDocumentsModel::saveTabState);

    connect(client, &DaemonClient::notification, this, &OpenDocumentsModel::onNotification);

    // A workspace opening (or another one replacing it) starts from a clean slate;
    // when its tree is ready, saved tabs come back.
    connect(workspace, &WorkspaceController::closed, this, [this] {
        ++m_generation;
        m_tabSaveTimer.stop();
        clearAll(true);
    });
    connect(workspace, &WorkspaceController::openChanged, this, [this] {
        if (!m_workspace->isOpen() && !m_docs.isEmpty()) {
            ++m_generation;
            clearAll(true);
        }
    });
    connect(workspace, &WorkspaceController::opened, this, [this] { restoreTabs(); });
}

// ---- model -------------------------------------------------------------------

int OpenDocumentsModel::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : static_cast<int>(m_docs.size());
}

QVariant OpenDocumentsModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() >= m_docs.size())
        return {};
    const Doc &d = m_docs.at(index.row());
    switch (role) {
    case Qt::DisplayRole:
    case TitleRole: return d.title;
    case PathRole: return d.path;
    case DirtyRole: return d.dirty;
    case SavedVersionRole: return d.savedVersion;
    case ChangedOnDiskRole: return d.changedOnDisk;
    case ActiveRole: return d.path == m_active;
    default: return {};
    }
}

QHash<int, QByteArray> OpenDocumentsModel::roleNames() const
{
    return { { PathRole, "path" },
             { TitleRole, "title" },
             { DirtyRole, "dirty" },
             { SavedVersionRole, "savedVersion" },
             { ChangedOnDiskRole, "changedOnDisk" },
             { ActiveRole, "active" } };
}

int OpenDocumentsModel::indexOf(const QString &path) const
{
    for (int i = 0; i < m_docs.size(); ++i)
        if (m_docs.at(i).path == path)
            return i;
    return -1;
}

QString OpenDocumentsModel::titleOf(const QString &path)
{
    return path.mid(path.lastIndexOf(QLatin1Char('/')) + 1);
}

QString OpenDocumentsModel::wsId() const
{
    return m_workspace ? m_workspace->id() : QString();
}

void OpenDocumentsModel::rowChanged(int row, const QList<int> &roles)
{
    const QModelIndex idx = index(row);
    emit dataChanged(idx, idx, roles);
}

bool OpenDocumentsModel::hasDirty() const
{
    for (const Doc &d : m_docs)
        if (d.dirty)
            return true;
    return false;
}

QStringList OpenDocumentsModel::dirtyPaths() const
{
    QStringList out;
    for (const Doc &d : m_docs)
        if (d.dirty)
            out << d.path;
    return out;
}

void OpenDocumentsModel::emitDirtyState()
{
    emit dirtyStateChanged();
}

QString OpenDocumentsModel::conflictName() const
{
    return m_conflicts.isEmpty() ? QString() : titleOf(m_conflicts.first().path);
}

QStringList OpenDocumentsModel::promptNames() const
{
    QStringList names;
    for (const QString &p : m_prompt.paths)
        names << titleOf(p);
    return names;
}

QString OpenDocumentsModel::savedVersionOf(const QString &path) const
{
    const int i = indexOf(path);
    return i < 0 ? QString() : m_docs.at(i).savedVersion;
}
bool OpenDocumentsModel::isDirty(const QString &path) const
{
    const int i = indexOf(path);
    return i >= 0 && m_docs.at(i).dirty;
}
bool OpenDocumentsModel::isChangedOnDisk(const QString &path) const
{
    const int i = indexOf(path);
    return i >= 0 && m_docs.at(i).changedOnDisk;
}

void OpenDocumentsModel::setBanner(const QString &text)
{
    if (m_banner == text)
        return;
    m_banner = text;
    emit bannerTextChanged();
}
void OpenDocumentsModel::showBanner(const QString &text) { setBanner(text); }
void OpenDocumentsModel::dismissBanner() { setBanner({}); }

void OpenDocumentsModel::setReady(bool r)
{
    if (m_ready == r)
        return;
    m_ready = r;
    emit editorReadyChanged();
}

// ---- open / activate / close ---------------------------------------------------

void OpenDocumentsModel::open(const QString &path)
{
    if (indexOf(path) >= 0) {
        activate(path);
        return;
    }
    openInternal(path, false, {});
}

void OpenDocumentsModel::openInternal(const QString &path, bool silent, std::function<void()> done)
{
    if (!m_client || !m_workspace || !m_workspace->isOpen() || m_opening.contains(path)) {
        if (done)
            done();
        return;
    }
    m_opening.insert(path);
    const quint64 gen = m_generation;
    m_client->call(QStringLiteral("fs/readFile"), { { "workspaceId", wsId() }, { "path", path } },
        [this, path, gen, silent, done = std::move(done)](const QJsonObject &r, const RpcError &err) {
            m_opening.remove(path);
            if (gen != m_generation) {
                if (done)
                    done();
                return;
            }
            if (!err.ok()) {
                if (!silent) {
                    const QString reason = err.kind == QLatin1String("unsupported_encoding")
                        ? tr("it is not a UTF-8 text file")
                        : err.kind == QLatin1String("too_large") ? tr("it is too large (over 16 MiB)")
                        : err.message;
                    setBanner(tr("Cannot open %1: %2").arg(titleOf(path), reason));
                }
            } else {
                addDocument(path, r.value(QLatin1String("text")).toString(),
                            r.value(QLatin1String("version")).toString());
            }
            if (done)
                done();
        });
}

void OpenDocumentsModel::addDocument(const QString &path, const QString &text, const QString &version)
{
    if (indexOf(path) >= 0)
        return;
    Doc d;
    d.path = path;
    d.title = titleOf(path);
    d.savedVersion = version;
    beginInsertRows({}, static_cast<int>(m_docs.size()), static_cast<int>(m_docs.size()));
    m_docs.append(d);
    endInsertRows();
    emit countChanged();
    setBanner({});
    sendOpen(d, text, false);
    setActive(path);
    emit documentOpened(path);
    scheduleTabSave();
}

void OpenDocumentsModel::sendOpen(const Doc &d, const QString &text, bool dirty)
{
    if (!m_ready)
        return; // the handshake replays every open document
    emit m_bridge.openDocument({ { "path", d.path },
                                 { "text", text },
                                 { "version", d.savedVersion },
                                 { "activate", d.path == m_active || m_active.isEmpty() },
                                 { "dirty", dirty } });
}

void OpenDocumentsModel::setActive(const QString &path)
{
    if (m_active == path)
        return;
    const QString old = m_active;
    m_active = path;
    emit activePathChanged();
    for (const QString &p : { old, path }) {
        const int i = indexOf(p);
        if (i >= 0)
            rowChanged(i, { ActiveRole });
    }
    m_cursorText.clear();
    emit cursorTextChanged();
    if (m_ready && !path.isEmpty())
        emit m_bridge.activateDocument(pathObj(path));
    scheduleTabSave();
}

void OpenDocumentsModel::activate(const QString &path)
{
    if (indexOf(path) >= 0)
        setActive(path);
}

void OpenDocumentsModel::cycle(int delta)
{
    if (m_docs.size() < 2)
        return;
    int i = indexOf(m_active);
    if (i < 0)
        i = 0;
    const int n = static_cast<int>(m_docs.size());
    setActive(m_docs.at(((i + delta) % n + n) % n).path);
}

void OpenDocumentsModel::close(const QString &path)
{
    const int i = indexOf(path);
    if (i < 0)
        return;
    if (m_docs.at(i).dirty) {
        guarded(QStringLiteral("close"), { path }, [this, path] { closeNow(path); });
        return;
    }
    closeNow(path);
}

void OpenDocumentsModel::closeNow(const QString &path)
{
    const int i = indexOf(path);
    if (i < 0)
        return;
    const bool wasActive = path == m_active;
    beginRemoveRows({}, i, i);
    m_docs.removeAt(i);
    endRemoveRows();
    m_awaitingSave.remove(path);
    for (int c = static_cast<int>(m_conflicts.size()) - 1; c >= 0; --c)
        if (m_conflicts.at(c).path == path)
            m_conflicts.removeAt(c);
    emit conflictChanged();
    emit countChanged();
    if (m_ready)
        emit m_bridge.closeDocument(pathObj(path));
    if (wasActive) {
        m_active.clear();
        const QString next = m_docs.isEmpty() ? QString() : m_docs.at(qMin(i, int(m_docs.size()) - 1)).path;
        emit activePathChanged();
        m_cursorText.clear();
        emit cursorTextChanged();
        if (!next.isEmpty())
            setActive(next);
    }
    emitDirtyState();
    scheduleTabSave();
}

void OpenDocumentsModel::clearAll(bool notifyPage)
{
    if (notifyPage && m_ready)
        for (const Doc &d : std::as_const(m_docs))
            emit m_bridge.closeDocument(pathObj(d.path));
    beginResetModel();
    m_docs.clear();
    endResetModel();
    m_awaitingSave.clear();
    m_conflicts.clear();
    m_opening.clear();
    m_prompt = {};
    m_active.clear();
    m_cursorText.clear();
    emit countChanged();
    emit activePathChanged();
    emit cursorTextChanged();
    emit conflictChanged();
    emit promptChanged();
    emitDirtyState();
}

// ---- editor commands -----------------------------------------------------------

void OpenDocumentsModel::runCommand(const QString &command)
{
    if (m_ready && !m_active.isEmpty())
        emit m_bridge.runCommand({ { "command", command } });
}

void OpenDocumentsModel::revealPosition(const QString &path, int line, int column)
{
    if (m_ready && indexOf(path) >= 0)
        emit m_bridge.revealPosition({ { "path", path }, { "line", line }, { "column", column } });
}

void OpenDocumentsModel::setAppearance(bool dark, int fontSize, const QString &fontFamily, bool wordWrap)
{
    m_dark = dark;
    m_fontSize = fontSize;
    m_fontFamily = fontFamily;
    m_wordWrap = wordWrap;
    if (m_ready)
        emit m_bridge.configure({ { "theme", dark ? "dark" : "light" },
                                  { "fontSize", fontSize },
                                  { "fontFamily", fontFamily },
                                  { "wordWrap", wordWrap } });
}

// ---- bridge: page -> shell -----------------------------------------------------

void OpenDocumentsModel::bridgeReady(int bridgeVersion, const QString &monacoVersion)
{
    if (bridgeVersion != EditorBridge::Version) {
        setBanner(tr("The editor speaks bridge version %1 but this build needs %2.")
                      .arg(bridgeVersion).arg(EditorBridge::Version));
        return; // never emit a signal to a page we do not understand
    }
    setReady(true);
    if (m_crashed) {
        m_crashed = false;
        emit rendererCrashedChanged();
    }
    emit monacoVersionKnown(monacoVersion);
    // The handshake: configure, then every open document, then the active one.
    emit m_bridge.configure({ { "theme", m_dark ? "dark" : "light" },
                              { "fontSize", m_fontSize },
                              { "fontFamily", m_fontFamily },
                              { "wordWrap", m_wordWrap } });
    for (const Doc &d : std::as_const(m_docs)) {
        // After a crash: unsaved text comes back from the last snapshot, marked dirty.
        const bool fromSnapshot = d.hasSnapshot;
        Doc copy = d;
        sendOpen(copy, fromSnapshot ? d.snapshot : QString(), fromSnapshot);
        if (!fromSnapshot) {
            // Documents opened before the page was ready have no text yet: re-read them.
            reloadFromDisk(d.path);
        }
    }
    if (!m_active.isEmpty())
        emit m_bridge.activateDocument(pathObj(m_active));
}

void OpenDocumentsModel::bridgeSaveRequested(const QString &path, const QString &text)
{
    const int i = indexOf(path);
    if (i < 0)
        return; // the shell does not consider it open: ignored (contract §Failure modes)
    writeFile(path, text, false);
}

void OpenDocumentsModel::bridgeDirtyChanged(const QString &path, bool dirty)
{
    const int i = indexOf(path);
    if (i < 0 || m_docs.at(i).dirty == dirty)
        return;
    m_docs[i].dirty = dirty;
    if (!dirty) {
        m_docs[i].hasSnapshot = false;
        m_docs[i].snapshot.clear();
    }
    rowChanged(i, { DirtyRole });
    emitDirtyState();
}

void OpenDocumentsModel::bridgeCursorChanged(const QString &path, int line, int column, int selectionLength)
{
    if (path != m_active)
        return;
    m_cursorText = selectionLength > 0
        ? tr("Ln %1, Col %2 (%3 selected)").arg(line).arg(column).arg(selectionLength)
        : tr("Ln %1, Col %2").arg(line).arg(column);
    emit cursorTextChanged();
}

void OpenDocumentsModel::bridgeTextSnapshot(const QString &path, const QString &text)
{
    const int i = indexOf(path);
    if (i < 0)
        return;
    m_docs[i].snapshot = text; // memory only, to restore after a renderer crash
    m_docs[i].hasSnapshot = true;
}

void OpenDocumentsModel::bridgeOpenLink(const QString &url)
{
    const QUrl u(url);
    const QString scheme = u.scheme().toLower();
    if (scheme == QLatin1String("http") || scheme == QLatin1String("https"))
        QDesktopServices::openUrl(u);
    else
        qWarning().noquote() << "[editor] refused to open a link with scheme" << scheme;
}

void OpenDocumentsModel::bridgeReportError(const QString &message, const QString &detail)
{
    qWarning().noquote() << "[editor] page error:" << message << detail;
}

// ---- saving --------------------------------------------------------------------

void OpenDocumentsModel::writeFile(const QString &path, const QString &text, bool overwrite)
{
    const int i = indexOf(path);
    if (i < 0 || !m_client)
        return;
    QJsonObject params{ { "workspaceId", wsId() }, { "path", path }, { "text", text } };
    if (overwrite)
        params.insert(QStringLiteral("overwrite"), true);
    else if (!m_docs.at(i).savedVersion.isEmpty())
        params.insert(QStringLiteral("expectedVersion"), m_docs.at(i).savedVersion);
    const quint64 gen = m_generation;
    m_client->call(QStringLiteral("fs/writeFile"), params,
        [this, path, text, gen](const QJsonObject &r, const RpcError &err) {
            if (gen != m_generation) {
                // The workspace was replaced: nothing to answer on.
                return;
            }
            if (err.ok()) {
                saveSucceeded(path, r.value(QLatin1String("version")).toString());
            } else if (err.kind == QLatin1String("conflict")) {
                m_conflicts.append({ path, text });
                emit conflictChanged();
            } else {
                setBanner(tr("Could not save %1: %2").arg(titleOf(path), err.message));
                saveFailed(path);
            }
        });
}

void OpenDocumentsModel::saveSucceeded(const QString &path, const QString &version)
{
    const int i = indexOf(path);
    if (i >= 0) {
        m_docs[i].savedVersion = version;
        m_docs[i].changedOnDisk = false;
        rowChanged(i, { SavedVersionRole, ChangedOnDiskRole });
        if (m_ready)
            emit m_bridge.documentSaved({ { "path", path }, { "version", version } });
    }
    m_awaitingSave.remove(path);
    if (m_awaitingSave.isEmpty() && m_afterSave) {
        auto f = std::move(m_afterSave);
        m_afterSave = {};
        f();
    }
}

void OpenDocumentsModel::saveFailed(const QString &path)
{
    if (m_ready && indexOf(path) >= 0)
        emit m_bridge.saveFailed(pathObj(path));
    // A failed or cancelled save aborts whatever was waiting for it.
    if (m_awaitingSave.contains(path)) {
        m_awaitingSave.clear();
        m_afterSave = {};
    }
}

void OpenDocumentsModel::save(const QString &path)
{
    if (indexOf(path) >= 0 && m_ready)
        emit m_bridge.requestSave(pathObj(path));
}

void OpenDocumentsModel::saveAll()
{
    for (const Doc &d : std::as_const(m_docs))
        if (d.dirty)
            save(d.path);
}

void OpenDocumentsModel::resolveConflict(const QString &action)
{
    if (m_conflicts.isEmpty())
        return;
    const Conflict c = m_conflicts.takeFirst();
    emit conflictChanged();
    if (action == QLatin1String("overwrite")) {
        writeFile(c.path, c.text, true);
    } else if (action == QLatin1String("reload")) {
        // Take the disk's version; the page must still hear that its save is over.
        saveFailed(c.path);
        reloadFromDisk(c.path);
    } else {
        saveFailed(c.path);
    }
}

void OpenDocumentsModel::reloadFromDisk(const QString &path)
{
    if (!m_client || !m_workspace || !m_workspace->isOpen())
        return;
    const quint64 gen = m_generation;
    m_client->call(QStringLiteral("fs/readFile"), { { "workspaceId", wsId() }, { "path", path } },
        [this, path, gen](const QJsonObject &r, const RpcError &err) {
            const int i = indexOf(path);
            if (gen != m_generation || i < 0 || !err.ok())
                return;
            m_docs[i].savedVersion = r.value(QLatin1String("version")).toString();
            m_docs[i].changedOnDisk = false;
            m_docs[i].dirty = false;
            m_docs[i].hasSnapshot = false;
            rowChanged(i, { SavedVersionRole, ChangedOnDiskRole, DirtyRole });
            emitDirtyState();
            if (m_ready)
                emit m_bridge.openDocument({ { "path", path },
                                             { "text", r.value(QLatin1String("text")).toString() },
                                             { "version", m_docs[i].savedVersion },
                                             { "activate", path == m_active },
                                             { "dirty", false } });
        });
}

// ---- external changes ----------------------------------------------------------

void OpenDocumentsModel::onNotification(const QString &method, const QJsonObject &params)
{
    if (method != QLatin1String("fs/didChange") || params.value(QLatin1String("workspaceId")).toString() != wsId())
        return;
    bool everything = false;
    QSet<QString> changed;
    for (const QJsonValue &v : params.value(QLatin1String("changes")).toArray()) {
        const QString p = v.toObject().value(QLatin1String("path")).toString();
        if (p.isEmpty())
            everything = true;
        else
            changed.insert(p);
    }
    for (const Doc &d : QList<Doc>(m_docs))
        if (everything || changed.contains(d.path))
            recheck(d.path);
}

// An open file may have changed on disk. The version is the arbiter: our own
// saves and unchanged content cause no action.
void OpenDocumentsModel::recheck(const QString &path)
{
    if (!m_client || !m_workspace->isOpen())
        return;
    const quint64 gen = m_generation;
    m_client->call(QStringLiteral("fs/readFile"), { { "workspaceId", wsId() }, { "path", path } },
        [this, path, gen](const QJsonObject &r, const RpcError &err) {
            const int i = indexOf(path);
            if (gen != m_generation || i < 0)
                return;
            Doc &d = m_docs[i];
            if (!err.ok()) {
                // Deleted or unreadable: a dirty document keeps the user's text and warns.
                if (d.dirty && !d.changedOnDisk) {
                    d.changedOnDisk = true;
                    rowChanged(i, { ChangedOnDiskRole });
                }
                return;
            }
            const QString version = r.value(QLatin1String("version")).toString();
            if (version == d.savedVersion)
                return; // our own write, or no real change
            if (!d.dirty) {
                reloadFromDisk(path); // clean: take the new content
            } else if (!d.changedOnDisk) {
                d.changedOnDisk = true;
                rowChanged(i, { ChangedOnDiskRole });
            }
        });
}

// ---- unsaved-work prompts ------------------------------------------------------

void OpenDocumentsModel::guarded(const QString &kind, const QStringList &dirty, std::function<void()> proceed)
{
    if (dirty.isEmpty()) {
        proceed();
        return;
    }
    startPrompt(kind, dirty, std::move(proceed));
}

void OpenDocumentsModel::startPrompt(const QString &kind, const QStringList &dirty, std::function<void()> proceed)
{
    if (m_prompt.active)
        return; // one question at a time
    m_prompt = { true, kind, dirty, std::move(proceed) };
    emit promptChanged();
}

void OpenDocumentsModel::afterSaved(const QStringList &paths, std::function<void()> proceed)
{
    m_awaitingSave = QSet<QString>(paths.begin(), paths.end());
    m_afterSave = std::move(proceed);
    for (const QString &p : paths)
        save(p);
    if (!m_ready) { // cannot save without the page: do not lose the request silently
        m_awaitingSave.clear();
        m_afterSave = {};
    }
}

void OpenDocumentsModel::resolvePrompt(const QString &action)
{
    if (!m_prompt.active)
        return;
    Prompt p = std::move(m_prompt);
    m_prompt = {};
    emit promptChanged();
    if (action == QLatin1String("save"))
        afterSaved(p.paths, std::move(p.proceed));
    else if (action == QLatin1String("discard"))
        p.proceed();
    // cancel: do nothing
}

void OpenDocumentsModel::requestOpenFolder(const QString &path)
{
    guarded(QStringLiteral("folder"), dirtyPaths(), [this, path] { m_workspace->openFolder(path); });
}
void OpenDocumentsModel::requestOpenFolderUrl(const QUrl &url)
{
    requestOpenFolder(url.isLocalFile() ? url.toLocalFile() : url.toString());
}
void OpenDocumentsModel::requestCloseFolder()
{
    guarded(QStringLiteral("folder"), dirtyPaths(), [this] { m_workspace->close(); });
}
void OpenDocumentsModel::requestQuit()
{
    guarded(QStringLiteral("quit"), dirtyPaths(), [this] { emit quitConfirmed(); });
}

// ---- renderer lifecycle --------------------------------------------------------

void OpenDocumentsModel::editorUnavailable()
{
    setReady(false);
}

void OpenDocumentsModel::editorCrashed()
{
    setReady(false);
    if (!m_crashed) {
        m_crashed = true;
        emit rendererCrashedChanged();
    }
}

// ---- tab persistence -----------------------------------------------------------

void OpenDocumentsModel::scheduleTabSave()
{
    if (!m_restoring && m_workspace && m_workspace->isOpen())
        m_tabSaveTimer.start();
}

void OpenDocumentsModel::saveTabState()
{
    if (!m_client || !m_workspace->isOpen())
        return;
    QJsonArray paths;
    for (const Doc &d : std::as_const(m_docs))
        paths.append(d.path);
    m_client->call(QStringLiteral("workspace/setState"),
        { { "workspaceId", wsId() },
          { "values", QJsonObject{ { QString::fromLatin1(OpenPathsKey), paths },
                                   { QString::fromLatin1(ActivePathKey), m_active.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(m_active) } } } },
        [](const QJsonObject &, const RpcError &) {});
}

void OpenDocumentsModel::restoreTabs()
{
    if (!m_client || !m_workspace->isOpen())
        return;
    const quint64 gen = m_generation;
    m_client->call(QStringLiteral("workspace/getState"),
        { { "workspaceId", wsId() },
          { "keys", QJsonArray{ QString::fromLatin1(OpenPathsKey), QString::fromLatin1(ActivePathKey) } } },
        [this, gen](const QJsonObject &s, const RpcError &err) {
            if (gen != m_generation || !err.ok())
                return;
            const QJsonObject values = s.value(QLatin1String("values")).toObject();
            QStringList paths;
            for (const QJsonValue &v : values.value(QString::fromLatin1(OpenPathsKey)).toArray())
                if (v.isString())
                    paths << v.toString();
            const QString active = values.value(QString::fromLatin1(ActivePathKey)).toString();
            if (paths.isEmpty())
                return;
            m_restoring = true;
            // Sequentially, so tabs keep their order; files that are gone are skipped silently.
            auto step = std::make_shared<std::function<void(int)>>();
            *step = [this, paths, active, gen, step](int i) {
                if (gen != m_generation)
                    return;
                if (i >= paths.size()) {
                    m_restoring = false;
                    if (!active.isEmpty() && indexOf(active) >= 0)
                        setActive(active);
                    return;
                }
                openInternal(paths.at(i), true, [step, i] { (*step)(i + 1); });
            };
            (*step)(0);
        });
}
