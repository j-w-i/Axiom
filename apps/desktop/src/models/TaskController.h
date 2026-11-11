#pragma once
#include "TaskOutputModel.h"

#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QStringList>
#include <QTimer>
#include <QVariantMap>

class DaemonClient;
class WorkspaceController;

// Runs one command at a time in the open workspace and shows its output.
// QML sees it as `tasks`. The command line is passed verbatim to the daemon
// (`command: { shell: ... }`); the shell never parses or quotes it.
class TaskController : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool running READ running NOTIFY runningChanged)
    Q_PROPERTY(QString currentLabel READ currentLabel NOTIFY runningChanged)
    Q_PROPERTY(QVariantMap lastStatus READ lastStatus NOTIFY lastStatusChanged)
    Q_PROPERTY(bool hasLastStatus READ hasLastStatus NOTIFY lastStatusChanged)
    Q_PROPERTY(QString statusText READ statusText NOTIFY statusTextChanged)
    Q_PROPERTY(QStringList history READ history NOTIFY historyChanged)
    Q_PROPERTY(QString lastCommand READ lastCommand NOTIFY historyChanged)
    Q_PROPERTY(QObject *output READ output CONSTANT)
    Q_PROPERTY(bool canRun READ canRun NOTIFY runningChanged)

public:
    static constexpr int MaxHistory = 20;

    TaskController(DaemonClient *client, WorkspaceController *workspace, QObject *parent = nullptr);

    bool running() const { return !m_taskId.isEmpty() || m_starting; }
    QString currentLabel() const { return m_label; }
    QVariantMap lastStatus() const { return m_lastStatus; }
    bool hasLastStatus() const { return !m_lastStatus.isEmpty(); }
    QString statusText() const { return m_statusText; }
    QStringList history() const { return m_history; }
    QString lastCommand() const { return m_history.isEmpty() ? QString() : m_history.first(); }
    QObject *output() { return &m_output; }
    TaskOutputModel *outputModel() { return &m_output; }
    bool canRun() const;

    Q_INVOKABLE void run(const QString &commandLine);
    Q_INVOKABLE void rerunLast();
    Q_INVOKABLE void stop();
    Q_INVOKABLE void clear();

    // For tests: starts a task with an explicit argv (no shell), to exercise `spawn_failed`.
    void runArgvForTest(const QStringList &argv);

signals:
    void runningChanged();
    void lastStatusChanged();
    void statusTextChanged();
    void historyChanged();
    void finished(const QString &outcome);

private:
    void start(const QJsonObject &command, const QString &label, bool recordHistory);
    void onNotification(const QString &method, const QJsonObject &params);
    void onExited(const QJsonObject &params);
    void setStatusText(const QString &t);
    void pushHistory(const QString &cmd);
    void saveHistory();
    void loadHistory();
    void resetState();
    static QString formatDuration(qint64 ms);

    QPointer<DaemonClient> m_client;
    QPointer<WorkspaceController> m_workspace;
    TaskOutputModel m_output;
    QString m_taskId;
    QString m_label;
    bool m_starting = false;
    bool m_stopRequested = false;
    QVariantMap m_lastStatus;
    QString m_statusText;
    QStringList m_history;
    quint64 m_generation = 0;
    // Output that arrives before the `task/start` response is processed cannot happen
    // (the daemon sends the response first), but exits for another task are ignored.
};
