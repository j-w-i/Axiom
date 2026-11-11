#pragma once
#include "editor/EditorBridge.h"

#include <QAbstractListModel>
#include <QPointer>
#include <QSet>
#include <QStringList>
#include <QTimer>
#include <functional>

class DaemonClient;
class WorkspaceController;

// The documents open in the editor surface and the flows around them
// (docs/architecture/data-flow.md §Open, edit and save): open, save with
// conflict handling, external changes, close/quit prompts, tab persistence and
// recovery after a renderer crash. QML sees it as `documents`.
//
// The editor surface owns unsaved text; the daemon owns the file on disk; this
// object owns which documents are open and each one's saved `version`. It
// never parses or transforms document text.
class OpenDocumentsModel : public QAbstractListModel, public EditorBridge::Listener
{
    Q_OBJECT
    Q_PROPERTY(int count READ count NOTIFY countChanged)
    Q_PROPERTY(QString activePath READ activePath NOTIFY activePathChanged)
    Q_PROPERTY(QString cursorText READ cursorText NOTIFY cursorTextChanged)
    Q_PROPERTY(bool editorReady READ editorReady NOTIFY editorReadyChanged)
    Q_PROPERTY(bool rendererCrashed READ rendererCrashed NOTIFY rendererCrashedChanged)
    Q_PROPERTY(bool hasDirty READ hasDirty NOTIFY dirtyStateChanged)
    Q_PROPERTY(QString bannerText READ bannerText NOTIFY bannerTextChanged)
    Q_PROPERTY(bool conflictActive READ conflictActive NOTIFY conflictChanged)
    Q_PROPERTY(QString conflictName READ conflictName NOTIFY conflictChanged)
    Q_PROPERTY(bool promptActive READ promptActive NOTIFY promptChanged)
    Q_PROPERTY(QString promptKind READ promptKind NOTIFY promptChanged)
    Q_PROPERTY(QStringList promptNames READ promptNames NOTIFY promptChanged)
    Q_PROPERTY(QObject *bridge READ bridge CONSTANT)

public:
    enum Roles { PathRole = Qt::UserRole + 1, TitleRole, DirtyRole, SavedVersionRole, ChangedOnDiskRole, ActiveRole };

    OpenDocumentsModel(DaemonClient *client, WorkspaceController *workspace, QObject *parent = nullptr);

    // QAbstractListModel
    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    int count() const { return static_cast<int>(m_docs.size()); }
    QString activePath() const { return m_active; }
    QString cursorText() const { return m_cursorText; }
    bool editorReady() const { return m_ready; }
    bool rendererCrashed() const { return m_crashed; }
    bool hasDirty() const;
    QString bannerText() const { return m_banner; }
    bool conflictActive() const { return !m_conflicts.isEmpty(); }
    QString conflictName() const;
    bool promptActive() const { return m_prompt.active; }
    QString promptKind() const { return m_prompt.kind; }
    QStringList promptNames() const;
    QObject *bridge() { return &m_bridge; }
    EditorBridge *editorBridge() { return &m_bridge; }

    // ---- user actions
    Q_INVOKABLE void open(const QString &path);
    Q_INVOKABLE void activate(const QString &path);
    Q_INVOKABLE void close(const QString &path);
    Q_INVOKABLE void save(const QString &path);
    Q_INVOKABLE void saveAll();
    Q_INVOKABLE void cycle(int delta);
    Q_INVOKABLE void runCommand(const QString &command); // undo|redo|find|replace|selectAll|format
    Q_INVOKABLE void revealPosition(const QString &path, int line, int column);
    Q_INVOKABLE void setAppearance(bool dark, int fontSize, const QString &fontFamily, bool wordWrap);
    Q_INVOKABLE void dismissBanner();

    // Answers to the dialogs. Idempotent: only the first answer counts.
    Q_INVOKABLE void resolveConflict(const QString &action); // overwrite | reload | cancel
    Q_INVOKABLE void resolvePrompt(const QString &action);   // save | discard | cancel

    // Operations that can lose unsaved work go through the prompt.
    Q_INVOKABLE void requestOpenFolder(const QString &path);
    Q_INVOKABLE void requestOpenFolderUrl(const QUrl &url);
    Q_INVOKABLE void requestCloseFolder();
    Q_INVOKABLE void requestQuit();

    // From the QML WebEngineView.
    Q_INVOKABLE void editorUnavailable(); // renderer crashed, or the page is reloading
    Q_INVOKABLE void editorCrashed();
    Q_INVOKABLE void showBanner(const QString &text);

    // Test and diagnostics access.
    QString savedVersionOf(const QString &path) const;
    bool isDirty(const QString &path) const;
    bool isChangedOnDisk(const QString &path) const;

    // EditorBridge::Listener
    void bridgeReady(int bridgeVersion, const QString &monacoVersion) override;
    void bridgeSaveRequested(const QString &path, const QString &text) override;
    void bridgeDirtyChanged(const QString &path, bool dirty) override;
    void bridgeCursorChanged(const QString &path, int line, int column, int selectionLength) override;
    void bridgeTextSnapshot(const QString &path, const QString &text) override;
    void bridgeOpenLink(const QString &url) override;
    void bridgeReportError(const QString &message, const QString &detail) override;

signals:
    void countChanged();
    void activePathChanged();
    void cursorTextChanged();
    void editorReadyChanged();
    void rendererCrashedChanged();
    void dirtyStateChanged();
    void bannerTextChanged();
    void conflictChanged();
    void promptChanged();
    void documentOpened(const QString &path);
    void quitConfirmed();
    void monacoVersionKnown(const QString &version);

private:
    struct Doc
    {
        QString path;
        QString title;
        bool dirty = false;
        QString savedVersion;
        bool changedOnDisk = false;
        QString snapshot;
        bool hasSnapshot = false;
    };
    struct Conflict
    {
        QString path;
        QString text;
    };
    struct Prompt
    {
        bool active = false;
        QString kind; // close | folder | quit
        QStringList paths;
        std::function<void()> proceed;
    };

    int indexOf(const QString &path) const;
    QString wsId() const;
    static QString titleOf(const QString &path);
    void setBanner(const QString &text);

    void openInternal(const QString &path, bool silent, std::function<void()> done);
    void addDocument(const QString &path, const QString &text, const QString &version);
    void sendOpen(const Doc &d, const QString &text, bool dirty);
    void closeNow(const QString &path);
    void clearAll(bool notifyPage);
    void setActive(const QString &path);
    void setReady(bool r);
    void emitDirtyState();
    void rowChanged(int row, const QList<int> &roles);

    void writeFile(const QString &path, const QString &text, bool overwrite);
    void saveSucceeded(const QString &path, const QString &version);
    void saveFailed(const QString &path);
    void reloadFromDisk(const QString &path);
    void recheck(const QString &path);
    void onNotification(const QString &method, const QJsonObject &params);

    void guarded(const QString &kind, const QStringList &dirty, std::function<void()> proceed);
    void startPrompt(const QString &kind, const QStringList &dirty, std::function<void()> proceed);
    void afterSaved(const QStringList &paths, std::function<void()> proceed);

    void scheduleTabSave();
    void saveTabState();
    void restoreTabs();
    QStringList dirtyPaths() const;

    QPointer<DaemonClient> m_client;
    QPointer<WorkspaceController> m_workspace;
    EditorBridge m_bridge;

    QList<Doc> m_docs;
    QString m_active;
    QString m_cursorText;
    QString m_banner;
    bool m_ready = false;
    bool m_crashed = false;
    bool m_restoring = false;
    QSet<QString> m_opening;
    QList<Conflict> m_conflicts;
    Prompt m_prompt;
    QSet<QString> m_awaitingSave;
    std::function<void()> m_afterSave;

    bool m_dark = false;
    int m_fontSize = 13;
    QString m_fontFamily;
    bool m_wordWrap = false;

    QTimer m_tabSaveTimer;
    quint64 m_generation = 0; // invalidates answers from an earlier workspace
};
