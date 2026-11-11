#pragma once
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QString>

class DaemonClient;

// The known settings (docs/protocols/ipc.md §Settings), loaded with
// `settings/get` and changed with `settings/set`. The daemon is the single
// source of truth: a property changes when `settings/didChange` arrives (or
// the initial load answers), never optimistically. A rejected value leaves the
// property unchanged and reports the message through `errorOccurred`.
class SettingsStore : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString theme READ theme WRITE setTheme NOTIFY themeChanged)
    Q_PROPERTY(int editorFontSize READ editorFontSize WRITE setEditorFontSize NOTIFY editorFontSizeChanged)
    Q_PROPERTY(QString editorFontFamily READ editorFontFamily WRITE setEditorFontFamily NOTIFY editorFontFamilyChanged)
    Q_PROPERTY(bool editorWordWrap READ editorWordWrap WRITE setEditorWordWrap NOTIFY editorWordWrapChanged)
    Q_PROPERTY(bool reopenLast READ reopenLast WRITE setReopenLast NOTIFY reopenLastChanged)
    Q_PROPERTY(bool loaded READ loaded NOTIFY loadedChanged)

public:
    explicit SettingsStore(DaemonClient *client, QObject *parent = nullptr);

    QString theme() const { return m_theme; }
    int editorFontSize() const { return m_fontSize; }
    QString editorFontFamily() const { return m_fontFamily; }
    bool editorWordWrap() const { return m_wordWrap; }
    bool reopenLast() const { return m_reopenLast; }
    bool loaded() const { return m_loaded; }

    void setTheme(const QString &v) { set(QStringLiteral("appearance.theme"), v); }
    void setEditorFontSize(int v) { set(QStringLiteral("editor.fontSize"), v); }
    void setEditorFontFamily(const QString &v) { set(QStringLiteral("editor.fontFamily"), v); }
    void setEditorWordWrap(bool v) { set(QStringLiteral("editor.wordWrap"), v); }
    void setReopenLast(bool v) { set(QStringLiteral("workspace.reopenLast"), v); }

    // Reads everything again (done automatically whenever the daemon becomes Ready).
    Q_INVOKABLE void load();

signals:
    void themeChanged();
    void editorFontSizeChanged();
    void editorFontFamilyChanged();
    void editorWordWrapChanged();
    void reopenLastChanged();
    void loadedChanged();
    // The daemon rejected a value (or the call failed): the message for the user.
    void errorOccurred(const QString &message);
    // Any value changed (for consumers that apply them together, e.g. the editor `configure`).
    void changed();

private:
    void set(const QString &key, const QJsonValue &value);
    void apply(const QJsonObject &values);
    void onNotification(const QString &method, const QJsonObject &params);

    QPointer<DaemonClient> m_client;
    // Defaults mirror the table in ipc.md; real values arrive from the daemon.
    QString m_theme = QStringLiteral("system");
    int m_fontSize = 13;
    QString m_fontFamily;
    bool m_wordWrap = false;
    bool m_reopenLast = true;
    bool m_loaded = false;
};
