#include "SettingsStore.h"
#include "daemon/DaemonClient.h"

#include <QJsonArray>

SettingsStore::SettingsStore(DaemonClient *client, QObject *parent) : QObject(parent), m_client(client)
{
    connect(client, &DaemonClient::notification, this, &SettingsStore::onNotification);
    connect(client, &DaemonClient::stateChanged, this, [this] {
        if (m_client->state() == DaemonClient::Ready)
            load();
    });
}

void SettingsStore::load()
{
    if (!m_client)
        return;
    m_client->call(QStringLiteral("settings/get"), {},
        [this](const QJsonObject &r, const RpcError &err) {
            if (!err.ok()) {
                emit errorOccurred(tr("Could not read the settings: %1").arg(err.message));
                return;
            }
            apply(r.value(QLatin1String("values")).toObject());
            if (!m_loaded) {
                m_loaded = true;
                emit loadedChanged();
            }
        });
}

void SettingsStore::set(const QString &key, const QJsonValue &value)
{
    if (!m_client)
        return;
    m_client->call(QStringLiteral("settings/set"), { { "values", QJsonObject{ { key, value } } } },
        [this, key](const QJsonObject &, const RpcError &err) {
            if (err.ok())
                return; // the property changes when settings/didChange arrives
            const QString name = err.details.value(QLatin1String("key")).toString(key);
            emit errorOccurred(err.kind == QLatin1String("invalid_params")
                                   ? tr("That value is not valid for %1.").arg(name)
                                   : tr("Could not save the setting: %1").arg(err.message));
            // Nothing was changed on the daemon, so nothing to revert here; make bound
            // controls show the real value again.
            emit changed();
            emit themeChanged();
            emit editorFontSizeChanged();
            emit editorFontFamilyChanged();
            emit editorWordWrapChanged();
            emit reopenLastChanged();
        });
}

void SettingsStore::onNotification(const QString &method, const QJsonObject &params)
{
    if (method == QLatin1String("settings/didChange"))
        apply(params.value(QLatin1String("values")).toObject());
}

void SettingsStore::apply(const QJsonObject &v)
{
    bool any = false;
    auto str = [&](const char *key, QString &field, void (SettingsStore::*sig)()) {
        const auto it = v.find(QLatin1String(key));
        if (it == v.end() || !it->isString() || it->toString() == field)
            return;
        field = it->toString();
        emit(this->*sig)();
        any = true;
    };
    auto boolean = [&](const char *key, bool &field, void (SettingsStore::*sig)()) {
        const auto it = v.find(QLatin1String(key));
        if (it == v.end() || !it->isBool() || it->toBool() == field)
            return;
        field = it->toBool();
        emit(this->*sig)();
        any = true;
    };
    str("appearance.theme", m_theme, &SettingsStore::themeChanged);
    str("editor.fontFamily", m_fontFamily, &SettingsStore::editorFontFamilyChanged);
    boolean("editor.wordWrap", m_wordWrap, &SettingsStore::editorWordWrapChanged);
    boolean("workspace.reopenLast", m_reopenLast, &SettingsStore::reopenLastChanged);
    const auto fs = v.find(QLatin1String("editor.fontSize"));
    if (fs != v.end() && fs->isDouble() && fs->toInt() != m_fontSize) {
        m_fontSize = fs->toInt();
        emit editorFontSizeChanged();
        any = true;
    }
    if (any)
        emit changed();
}
