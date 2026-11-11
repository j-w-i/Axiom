#include "EditorBridge.h"

namespace {
QString str(const QJsonObject &o, const char *k) { return o.value(QLatin1String(k)).toString(); }
int num(const QJsonObject &o, const char *k) { return o.value(QLatin1String(k)).toInt(); }
} // namespace

void EditorBridge::ready(const QJsonObject &p)
{
    if (m_listener)
        m_listener->bridgeReady(num(p, "bridgeVersion"), str(p, "monacoVersion"));
}
void EditorBridge::saveRequested(const QJsonObject &p)
{
    if (m_listener)
        m_listener->bridgeSaveRequested(str(p, "path"), str(p, "text"));
}
void EditorBridge::dirtyChanged(const QJsonObject &p)
{
    if (m_listener)
        m_listener->bridgeDirtyChanged(str(p, "path"), p.value(QLatin1String("dirty")).toBool());
}
void EditorBridge::cursorChanged(const QJsonObject &p)
{
    if (m_listener)
        m_listener->bridgeCursorChanged(str(p, "path"), num(p, "line"), num(p, "column"),
                                        num(p, "selectionLength"));
}
void EditorBridge::textSnapshot(const QJsonObject &p)
{
    if (m_listener)
        m_listener->bridgeTextSnapshot(str(p, "path"), str(p, "text"));
}
void EditorBridge::openLink(const QJsonObject &p)
{
    if (m_listener)
        m_listener->bridgeOpenLink(str(p, "url"));
}
void EditorBridge::reportError(const QJsonObject &p)
{
    if (m_listener)
        m_listener->bridgeReportError(str(p, "message"), str(p, "detail"));
}
