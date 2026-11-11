#include "Backend.h"

#include <QCoreApplication>

Backend::Backend(DaemonClient *client, QObject *parent) : QObject(parent), m_client(client)
{
    connect(client, &DaemonClient::stateChanged, this, &Backend::stateChanged);
    connect(client, &DaemonClient::errorTextChanged, this, &Backend::errorTextChanged);
    connect(client, &DaemonClient::serverVersionChanged, this, &Backend::serverVersionChanged);
}

QString Backend::appVersion() const
{
    return QCoreApplication::applicationVersion();
}

void Backend::restart()
{
    m_client->restart();
}
