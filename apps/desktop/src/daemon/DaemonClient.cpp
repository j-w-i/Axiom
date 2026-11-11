#include "DaemonClient.h"

#include <QCoreApplication>
#include <QElapsedTimer>
#include <QJsonDocument>
#include <QJsonParseError>
#include <QProcess>
#include <QTimer>
#include <memory>

namespace {
constexpr int ShutdownAnswerWaitMs = 3000;
constexpr int ShutdownExitWaitMs = 2000;
constexpr int StderrTailLines = 5;
} // namespace

DaemonClient::DaemonClient(QObject *parent) : QObject(parent) {}

DaemonClient::~DaemonClient()
{
    // Nobody is left to receive answers: the objects that issued requests may
    // already be gone (members are destroyed in reverse order of declaration).
    // Drop pending callbacks silently before the bounded wait in shutdown().
    for (auto it = m_pending.begin(); it != m_pending.end(); ++it)
        delete it->timer;
    m_pending.clear();
    if (m_proc)
        shutdown();
}

qint64 DaemonClient::processId() const
{
    return m_proc ? m_proc->processId() : 0;
}

void DaemonClient::setState(State s)
{
    if (m_state == s)
        return;
    m_state = s;
    emit stateChanged();
}

void DaemonClient::setErrorText(const QString &t)
{
    if (m_errorText == t)
        return;
    m_errorText = t;
    emit errorTextChanged();
}

void DaemonClient::setServerVersion(const QString &v)
{
    if (m_serverVersion == v)
        return;
    m_serverVersion = v;
    emit serverVersionChanged();
}

void DaemonClient::start(const QString &program, const QStringList &args)
{
    m_program = program;
    m_args = args;
    launch();
}

void DaemonClient::restart()
{
    // Old requests can never be answered by a new process.
    detachProcess(true);
    failAll(QStringLiteral("internal"), tr("Backend stopped"));
    launch();
}

void DaemonClient::launch()
{
    m_stopping = false;
    m_out.clear();
    m_err.clear();
    m_stderrTail.clear();
    setErrorText({});
    setServerVersion({});
    setState(Starting);

    if (m_program.isEmpty()) {
        setErrorText(tr("The backend program (axiomd) was not found. "
                        "Pass --daemon <path>, set AXIOMD, or install axiomd next to axiom."));
        setState(Failed);
        return;
    }

    auto *p = new QProcess(this);
    m_proc = p;
    p->setProgram(m_program);
    p->setArguments(m_args);
    p->setProcessChannelMode(QProcess::SeparateChannels);

    // Signals of a process that was replaced (restart, failed start) are ignored:
    // everything below checks `p == m_proc`.
    connect(p, &QProcess::readyReadStandardOutput, this, [this, p] {
        const QByteArray data = p->readAllStandardOutput();
        if (p == m_proc)
            handleOutput(data);
    });
    connect(p, &QProcess::readyReadStandardError, this, [this, p] {
        const QByteArray data = p->readAllStandardError();
        if (p != m_proc)
            return;
        for (const QByteArray &line : m_err.append(data)) {
            const QString text = QString::fromUtf8(line);
            qInfo().noquote() << "[axiomd]" << text;
            m_stderrTail.append(text);
            while (m_stderrTail.size() > StderrTailLines)
                m_stderrTail.removeFirst();
        }
    });
    connect(p, &QProcess::started, this, [this, p] {
        if (p == m_proc)
            sendInitialize();
    });
    connect(p, &QProcess::errorOccurred, this, [this, p](QProcess::ProcessError e) {
        if (p != m_proc || e != QProcess::FailedToStart)
            return;
        processGone(tr("Could not start the backend (%1): %2").arg(m_program, p->errorString()));
    });
    connect(p, &QProcess::finished, this, [this, p](int code, QProcess::ExitStatus st) {
        if (p != m_proc)
            return;
        QString text = st == QProcess::CrashExit
                ? tr("The backend crashed.")
                : tr("The backend exited unexpectedly (exit code %1).").arg(code);
        if (!m_stderrTail.isEmpty())
            text += QLatin1Char(' ') + tr("Last output: %1").arg(m_stderrTail.last());
        processGone(text);
    });
    p->start();
}

void DaemonClient::sendInitialize()
{
    const QJsonObject params{
        { "protocolVersion", "0.1" },
        { "client",
          QJsonObject{ { "name", "axiom-desktop" },
                       { "version", QCoreApplication::applicationVersion() } } },
    };
    call(QStringLiteral("initialize"), params,
         [this](const QJsonObject &result, const RpcError &err) {
             if (m_state != Starting)
                 return; // the process went away; that path already reported
             if (!err.ok()) {
                 failStart(err.kind == QLatin1String("timeout")
                               ? tr("The backend did not answer 'initialize' within 10 seconds.")
                               : tr("The backend refused to initialize: %1").arg(err.message));
                 return;
             }
             setServerVersion(result.value(QLatin1String("server")).toObject()
                                  .value(QLatin1String("version")).toString());
             setState(Ready);
         },
         InitializeTimeoutMs);
}

void DaemonClient::failStart(const QString &text)
{
    detachProcess(false);
    setErrorText(text);
    setState(Failed);
    failAll(QStringLiteral("internal"), tr("Backend stopped"));
}

void DaemonClient::processGone(const QString &text)
{
    if (m_proc) {
        QProcess *p = m_proc;
        m_proc = nullptr;
        p->deleteLater();
    }
    if (m_stopping) {
        setState(Stopped);
    } else {
        setErrorText(text);
        setState(Failed);
    }
    failAll(QStringLiteral("internal"), tr("Backend stopped"));
}

void DaemonClient::detachProcess(bool graceful)
{
    QProcess *p = m_proc;
    m_proc = nullptr;
    if (!p)
        return;
    if (p->state() == QProcess::NotRunning) {
        p->deleteLater();
        return;
    }
    connect(p, &QProcess::finished, p, &QObject::deleteLater);
    if (graceful) {
        p->closeWriteChannel(); // EOF on stdin is a shutdown for the daemon
        QTimer::singleShot(ShutdownExitWaitMs, p, [p] {
            if (p->state() != QProcess::NotRunning)
                p->kill();
        });
    } else {
        p->kill();
    }
}

void DaemonClient::shutdown()
{
    QProcess *p = m_proc;
    if (!p) {
        setState(Stopped);
        failAll(QStringLiteral("internal"), tr("Backend stopped"));
        return;
    }
    m_stopping = true;
    if (p->state() == QProcess::Running && m_state == Ready) {
        auto answered = std::make_shared<bool>(false);
        const int id = call(QStringLiteral("shutdown"), {},
                            [answered](const QJsonObject &, const RpcError &) { *answered = true; },
                            ShutdownAnswerWaitMs);
        QElapsedTimer t;
        t.start();
        while (!*answered && t.elapsed() < ShutdownAnswerWaitMs && p->state() == QProcess::Running)
            p->waitForReadyRead(50);
        if (!*answered) {
            Pending dropped;
            if (takePending(id, &dropped)) {
                delete dropped.timer;
            }
        }
    }
    if (p->state() != QProcess::NotRunning) {
        p->closeWriteChannel();
        if (!p->waitForFinished(ShutdownExitWaitMs)) {
            p->kill();
            p->waitForFinished(1000);
        }
    }
    // `finished` normally already ran processGone(); make the end state certain.
    if (m_proc == p) {
        m_proc = nullptr;
        p->deleteLater();
    }
    setState(Stopped);
    failAll(QStringLiteral("internal"), tr("Backend stopped"));
}

int DaemonClient::call(const QString &method, const QJsonObject &params, Callback callback,
                       int timeoutMs)
{
    const int id = ++m_nextId;
    if (!m_proc || m_proc->state() != QProcess::Running) {
        QTimer::singleShot(0, this, [callback = std::move(callback)] {
            callback({}, RpcError::make(QStringLiteral("internal"), tr("Backend stopped")));
        });
        return id;
    }
    addPending(id, std::move(callback), timeoutMs);
    writeMessage(QJsonObject{ { "jsonrpc", "2.0" },
                              { "id", id },
                              { "method", method },
                              { "params", params } });
    return id;
}

void DaemonClient::writeMessage(const QJsonObject &message)
{
    if (!m_proc)
        return;
    m_proc->write(QJsonDocument(message).toJson(QJsonDocument::Compact) + '\n');
}

void DaemonClient::addPending(int id, Callback callback, int timeoutMs)
{
    auto *timer = new QTimer(this);
    timer->setSingleShot(true);
    timer->setInterval(timeoutMs);
    connect(timer, &QTimer::timeout, this, [this, id] { onTimeout(id); });
    timer->start();
    m_pending.insert(id, Pending{ std::move(callback), timer });
}

bool DaemonClient::takePending(int id, Pending *out)
{
    auto it = m_pending.find(id);
    if (it == m_pending.end())
        return false;
    *out = std::move(it.value());
    m_pending.erase(it);
    if (out->timer)
        out->timer->stop();
    return true;
}

void DaemonClient::onTimeout(int id)
{
    Pending p;
    if (!takePending(id, &p))
        return;
    writeMessage(QJsonObject{ { "jsonrpc", "2.0" },
                              { "method", "$/cancel" },
                              { "params", QJsonObject{ { "id", id } } } });
    p.timer->deleteLater();
    p.callback({}, RpcError::make(QStringLiteral("timeout"),
                                  tr("The backend did not respond in time.")));
}

void DaemonClient::failAll(const QString &kind, const QString &message)
{
    QHash<int, Pending> all;
    all.swap(m_pending);
    for (auto it = all.begin(); it != all.end(); ++it) {
        if (it->timer) {
            it->timer->stop();
            it->timer->deleteLater();
        }
    }
    for (auto it = all.begin(); it != all.end(); ++it)
        it->callback({}, RpcError::make(kind, message));
}

void DaemonClient::handleOutput(const QByteArray &data)
{
    for (const QByteArray &line : m_out.append(data))
        handleLine(line);
}

void DaemonClient::handleLine(const QByteArray &line)
{
    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(line, &parseError);
    if (parseError.error != QJsonParseError::NoError || !doc.isObject()) {
        qWarning().noquote() << "[axiomd] ignoring a malformed protocol line:" << parseError.errorString();
        return;
    }
    const QJsonObject obj = doc.object();
    if (obj.contains(QLatin1String("method"))) {
        if (!obj.contains(QLatin1String("id")))
            emit notification(obj.value(QLatin1String("method")).toString(),
                              obj.value(QLatin1String("params")).toObject());
        return; // the daemon sends no requests in protocol 0.1
    }
    const QJsonValue idValue = obj.value(QLatin1String("id"));
    if (!idValue.isDouble()) {
        qWarning().noquote() << "[axiomd] response without an id:"
                             << obj.value(QLatin1String("error")).toObject()
                                    .value(QLatin1String("message")).toString();
        return;
    }
    Pending p;
    if (!takePending(idValue.toInt(), &p))
        return; // late answer to a request that already timed out
    p.timer->deleteLater();
    if (obj.contains(QLatin1String("error")))
        p.callback({}, RpcError::fromJson(obj.value(QLatin1String("error")).toObject()));
    else
        p.callback(obj.value(QLatin1String("result")).toObject(), RpcError());
}
