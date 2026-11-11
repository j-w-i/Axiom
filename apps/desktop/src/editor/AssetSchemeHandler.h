#pragma once
#include <QWebEngineUrlSchemeHandler>

// Serves `axiom://editor/<path>` from one directory, and the Qt-provided
// qwebchannel.js. Anything resolving outside the directory is denied.
class AssetSchemeHandler : public QWebEngineUrlSchemeHandler
{
    Q_OBJECT
public:
    AssetSchemeHandler(const QString &root, QObject *parent = nullptr);

    void requestStarted(QWebEngineUrlRequestJob *job) override;

    // Maps a URL path ("/assets/x.js") to a file below the root, or an empty
    // string if the path would escape it (or does not name a file).
    static QString resolve(const QString &root, const QString &urlPath);
    static QByteArray mimeFor(const QString &fileName);

private:
    QString m_root; // canonical, no trailing slash
};
