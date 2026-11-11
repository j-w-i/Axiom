#include "TaskController.h"
#include "WorkspaceController.h"
#include "daemon/DaemonClient.h"

#include <QJsonArray>

namespace {
const char HistoryKey[] = "run.history";
}

TaskController::TaskController(DaemonClient *client, WorkspaceController *workspace, QObject *parent)
    : QObject(parent), m_client(client), m_workspace(workspace), m_output(this)
{
    connect(client, &DaemonClient::notification, this, &TaskController::onNotification);
    // History is per workspace: load when one opens, forget when it closes. A task of a
    // workspace that went away was cancelled by the daemon, so its state is dropped.
    connect(workspace, &WorkspaceController::opened, this, [this] { loadHistory(); });
    connect(workspace, &WorkspaceController::closed, this, [this] {
        resetState();
        m_history.clear();
        emit historyChanged();
    });
    connect(client, &DaemonClient::stateChanged, this, [this] {
        if (m_client->state() != DaemonClient::Ready && (running()))
            resetState();
    });
}

bool TaskController::canRun() const
{
    return !running() && m_workspace && m_workspace->isOpen() && m_client
        && m_client->state() == DaemonClient::Ready;
}

QString TaskController::formatDuration(qint64 ms)
{
    if (ms < 1000)
        return QStringLiteral("%1 ms").arg(ms);
    const double s = static_cast<double>(ms) / 1000.0;
    if (s < 60)
        return QStringLiteral("%1 s").arg(s, 0, 'f', 1);
    const qint64 m = ms / 60000;
    return QStringLiteral("%1 min %2 s").arg(m).arg((ms % 60000) / 1000);
}

void TaskController::setStatusText(const QString &t)
{
    if (m_statusText == t)
        return;
    m_statusText = t;
    emit statusTextChanged();
}

void TaskController::resetState()
{
    ++m_generation;
    const bool was = running();
    m_taskId.clear();
    m_label.clear();
    m_starting = false;
    m_stopRequested = false;
    m_output.finish();
    if (!m_lastStatus.isEmpty()) {
        m_lastStatus.clear();
        emit lastStatusChanged();
    }
    setStatusText({});
    m_output.reset();
    if (was)
        emit runningChanged();
}

// ---- history -------------------------------------------------------------------

void TaskController::pushHistory(const QString &cmd)
{
    m_history.removeAll(cmd);
    m_history.prepend(cmd);
    while (m_history.size() > MaxHistory)
        m_history.removeLast();
    emit historyChanged();
    saveHistory();
}

void TaskController::saveHistory()
{
    if (!m_client || !m_workspace || !m_workspace->isOpen())
        return;
    QJsonArray arr;
    for (const QString &h : std::as_const(m_history))
        arr.append(h);
    m_client->call(QStringLiteral("workspace/setState"),
                   { { "workspaceId", m_workspace->id() },
                     { "values", QJsonObject{ { QString::fromLatin1(HistoryKey), arr } } } },
                   [](const QJsonObject &, const RpcError &) {});
}

void TaskController::loadHistory()
{
    if (!m_client || !m_workspace->isOpen())
        return;
    const quint64 gen = m_generation;
    m_client->call(QStringLiteral("workspace/getState"),
                   { { "workspaceId", m_workspace->id() }, { "keys", QJsonArray{ QString::fromLatin1(HistoryKey) } } },
                   [this, gen](const QJsonObject &r, const RpcError &err) {
                       if (!err.ok() || gen != m_generation)
                           return;
                       QStringList loaded;
                       for (const QJsonValue &v : r.value(QLatin1String("values")).toObject()
                                                      .value(QString::fromLatin1(HistoryKey)).toArray())
                           if (v.isString() && !loaded.contains(v.toString()))
                               loaded << v.toString();
                       while (loaded.size() > MaxHistory)
                           loaded.removeLast();
                       // Anything run before the saved list arrived stays on top.
                       for (const QString &h : std::as_const(m_history))
                           loaded.removeAll(h);
                       m_history += loaded;
                       emit historyChanged();
                   });
}

// ---- running -------------------------------------------------------------------

void TaskController::run(const QString &commandLine)
{
    const QString cmd = commandLine.trimmed();
    if (cmd.isEmpty() || !canRun())
        return;
    start({ { "shell", commandLine } }, cmd, true); // verbatim, including surrounding spaces
}

void TaskController::rerunLast()
{
    if (!m_history.isEmpty())
        run(m_history.first());
}

void TaskController::runArgvForTest(const QStringList &argv)
{
    if (!canRun())
        return;
    QJsonArray a;
    for (const QString &s : argv)
        a.append(s);
    start({ { "argv", a } }, argv.join(QLatin1Char(' ')), false);
}

void TaskController::start(const QJsonObject &command, const QString &label, bool recordHistory)
{
    m_output.reset();
    if (!m_lastStatus.isEmpty()) {
        m_lastStatus.clear();
        emit lastStatusChanged();
    }
    m_label = label;
    m_starting = true;
    m_stopRequested = false;
    setStatusText(tr("Running: %1").arg(label));
    emit runningChanged();
    if (recordHistory)
        pushHistory(label);

    const quint64 gen = m_generation;
    m_client->call(QStringLiteral("task/start"),
                   { { "workspaceId", m_workspace->id() }, { "command", command }, { "label", label } },
                   [this, gen, label](const QJsonObject &r, const RpcError &err) {
                       if (gen != m_generation)
                           return;
                       m_starting = false;
                       if (!err.ok()) {
                           const QString text = err.kind == QLatin1String("spawn_failed")
                               ? tr("Could not start %1: %2")
                                     .arg(err.details.value(QLatin1String("program")).toString(label),
                                          err.details.value(QLatin1String("osError")).toString(err.message))
                               : tr("Could not run the command: %1").arg(err.message);
                           m_output.appendError(text);
                           m_label.clear();
                           setStatusText(tr("Failed to start"));
                           emit runningChanged();
                           return;
                       }
                       m_taskId = r.value(QLatin1String("taskId")).toString();
                       emit runningChanged();
                       if (m_stopRequested)
                           stop();
                   });
}

void TaskController::stop()
{
    if (!running())
        return;
    m_stopRequested = true;
    if (m_taskId.isEmpty())
        return; // still starting: cancelled as soon as the id is known
    m_client->call(QStringLiteral("task/cancel"), { { "taskId", m_taskId } },
                   [](const QJsonObject &, const RpcError &) {});
}

void TaskController::clear()
{
    m_output.reset();
    if (!running()) {
        if (!m_lastStatus.isEmpty()) {
            m_lastStatus.clear();
            emit lastStatusChanged();
        }
        setStatusText({});
    }
}

// ---- notifications -------------------------------------------------------------

void TaskController::onNotification(const QString &method, const QJsonObject &params)
{
    if (params.value(QLatin1String("taskId")).toString() != m_taskId || m_taskId.isEmpty())
        return;
    if (method == QLatin1String("task/output")) {
        m_output.append(static_cast<quint64>(params.value(QLatin1String("seq")).toDouble()),
                        params.value(QLatin1String("stream")).toString(),
                        params.value(QLatin1String("data")).toString());
    } else if (method == QLatin1String("task/exited")) {
        onExited(params);
    }
}

void TaskController::onExited(const QJsonObject &params)
{
    const QString outcome = params.value(QLatin1String("outcome")).toString();
    const qint64 duration = static_cast<qint64>(params.value(QLatin1String("durationMs")).toDouble());
    const QJsonValue code = params.value(QLatin1String("exitCode"));
    const QJsonValue signal = params.value(QLatin1String("signal"));

    QString text;
    bool ok = false;
    if (outcome == QLatin1String("succeeded")) {
        text = tr("✔ Succeeded in %1").arg(formatDuration(duration));
        ok = true;
    } else if (outcome == QLatin1String("cancelled")) {
        text = tr("■ Cancelled");
    } else if (code.isDouble()) {
        text = tr("✖ Failed with exit code %1 in %2").arg(code.toInt()).arg(formatDuration(duration));
    } else if (signal.isDouble()) {
        text = tr("✖ Failed (signal %1) in %2").arg(signal.toInt()).arg(formatDuration(duration));
    } else {
        text = tr("✖ Failed in %1").arg(formatDuration(duration));
    }

    QVariantMap status;
    status.insert(QStringLiteral("outcome"), outcome);
    status.insert(QStringLiteral("exitCode"), code.isDouble() ? QVariant(code.toInt()) : QVariant());
    status.insert(QStringLiteral("durationMs"), duration);
    m_lastStatus = status;

    m_output.appendStatus(text, ok);
    m_taskId.clear();
    m_stopRequested = false;
    const QString label = m_label;
    m_label.clear();
    setStatusText(outcome == QLatin1String("succeeded") ? tr("%1: succeeded").arg(label)
                  : outcome == QLatin1String("cancelled") ? tr("%1: cancelled").arg(label)
                  : code.isDouble() ? tr("%1: failed (%2)").arg(label).arg(code.toInt())
                                    : tr("%1: failed").arg(label));
    emit lastStatusChanged();
    emit runningChanged();
    emit finished(outcome);
}
