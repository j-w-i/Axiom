#pragma once
#include "daemon/DaemonClient.h"

#include <QObject>

// The only object QML sees of the daemon connection: presentation state and
// one action. No JSON-RPC reaches QML.
class Backend : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int state READ state NOTIFY stateChanged)
    Q_PROPERTY(bool starting READ starting NOTIFY stateChanged)
    Q_PROPERTY(bool ready READ ready NOTIFY stateChanged)
    Q_PROPERTY(bool failed READ failed NOTIFY stateChanged)
    Q_PROPERTY(QString errorText READ errorText NOTIFY errorTextChanged)
    Q_PROPERTY(QString serverVersion READ serverVersion NOTIFY serverVersionChanged)
    Q_PROPERTY(QString appVersion READ appVersion CONSTANT)

public:
    explicit Backend(DaemonClient *client, QObject *parent = nullptr);

    int state() const { return m_client->state(); }
    bool starting() const { return m_client->state() == DaemonClient::Starting; }
    bool ready() const { return m_client->state() == DaemonClient::Ready; }
    bool failed() const { return m_client->state() == DaemonClient::Failed; }
    QString errorText() const { return m_client->errorText(); }
    QString serverVersion() const { return m_client->serverVersion(); }
    QString appVersion() const;

    Q_INVOKABLE void restart();

signals:
    void stateChanged();
    void errorTextChanged();
    void serverVersionChanged();

private:
    DaemonClient *m_client;
};
