#pragma once
#include "LineBuffer.h"
#include "RpcError.h"

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QStringList>
#include <functional>

class QProcess;
class QTimer;

// Spawns axiomd and speaks JSON-RPC 2.0 (one message per line) over its
// stdio. The only code in the shell that knows JSON-RPC. Never blocks the
// GUI thread, except for the bounded waits in shutdown().
class DaemonClient : public QObject
{
    Q_OBJECT
    Q_PROPERTY(State state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString errorText READ errorText NOTIFY errorTextChanged)
    Q_PROPERTY(QString serverVersion READ serverVersion NOTIFY serverVersionChanged)

public:
    enum State { Starting, Ready, Failed, Stopped };
    Q_ENUM(State)

    using Callback = std::function<void(const QJsonObject &result, const RpcError &error)>;

    static constexpr int DefaultTimeoutMs = 30000;
    static constexpr int InitializeTimeoutMs = 10000;

    explicit DaemonClient(QObject *parent = nullptr);
    ~DaemonClient() override;

    State state() const { return m_state; }
    QString errorText() const { return m_errorText; }
    QString serverVersion() const { return m_serverVersion; }
    qint64 processId() const;

    // Spawns the daemon and performs `initialize`; `state` becomes Ready or Failed.
    void start(const QString &program, const QStringList &args = {});
    // Stops the current daemon (if any) and starts a fresh one with the same arguments.
    void restart();
    // processes.md lifecycle: `shutdown`, wait up to 3 s for the answer, close
    // stdin, wait up to 2 s, then kill. Blocks the caller for at most ~5 s.
    void shutdown();

    // Sends a request. The callback runs exactly once, on the GUI thread:
    // with the result, with the daemon's error, with kind `timeout` (after
    // which `$/cancel` is sent), or with kind `internal` ("Backend stopped").
    // Returns the request id.
    int call(const QString &method, const QJsonObject &params, Callback callback,
             int timeoutMs = DefaultTimeoutMs);

signals:
    void notification(const QString &method, const QJsonObject &params);
    void stateChanged();
    void errorTextChanged();
    void serverVersionChanged();

private:
    friend struct DaemonClientTestAccess;

    struct Pending
    {
        Callback callback;
        QTimer *timer = nullptr;
    };

    void launch();
    void sendInitialize();
    void failStart(const QString &text);
    void processGone(const QString &text);
    void detachProcess(bool graceful);
    void handleOutput(const QByteArray &data);
    void handleLine(const QByteArray &line);
    void writeMessage(const QJsonObject &message);
    void addPending(int id, Callback callback, int timeoutMs);
    bool takePending(int id, Pending *out);
    void onTimeout(int id);
    void failAll(const QString &kind, const QString &message);
    void setState(State s);
    void setErrorText(const QString &t);
    void setServerVersion(const QString &v);

    QString m_program;
    QStringList m_args;
    QProcess *m_proc = nullptr;
    State m_state = Stopped;
    QString m_errorText;
    QString m_serverVersion;
    bool m_stopping = false;
    int m_nextId = 0;
    QHash<int, Pending> m_pending;
    LineBuffer m_out;
    LineBuffer m_err;
    QStringList m_stderrTail;
};
