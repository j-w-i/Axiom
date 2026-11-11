#include "AssetSchemeHandler.h"

#include <QBuffer>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QHash>
#include <QWebEngineUrlRequestJob>

AssetSchemeHandler::AssetSchemeHandler(const QString &root, QObject *parent)
    : QWebEngineUrlSchemeHandler(parent), m_root(QFileInfo(root).canonicalFilePath())
{
}

QByteArray AssetSchemeHandler::mimeFor(const QString &fileName)
{
    static const QHash<QString, QByteArray> types{
        { "html", "text/html" },          { "js", "text/javascript" },
        { "mjs", "text/javascript" },     { "css", "text/css" },
        { "json", "application/json" },   { "ttf", "font/ttf" },
        { "woff2", "font/woff2" },        { "woff", "font/woff" },
        { "svg", "image/svg+xml" },       { "map", "application/json" },
        { "wasm", "application/wasm" },   { "png", "image/png" },
    };
    return types.value(QFileInfo(fileName).suffix().toLower(), "application/octet-stream");
}

QString AssetSchemeHandler::resolve(const QString &root, const QString &urlPath)
{
    if (root.isEmpty() || urlPath.contains(QChar('\0')) || urlPath.contains(QLatin1Char('\\')))
        return {};
    // Canonicalize (resolves ".." and symlinks) and require the result to stay inside.
    const QFileInfo info(QDir(root).filePath(urlPath.startsWith(QLatin1Char('/')) ? urlPath.mid(1) : urlPath));
    const QString canonical = info.canonicalFilePath();
    if (canonical.isEmpty() || !info.isFile())
        return {};
    const QString prefix = root.endsWith(QLatin1Char('/')) ? root : root + QLatin1Char('/');
    return canonical.startsWith(prefix) ? canonical : QString();
}

void AssetSchemeHandler::requestStarted(QWebEngineUrlRequestJob *job)
{
    const QUrl url = job->requestUrl();
    if (url.host() != QLatin1String("editor")) {
        job->fail(QWebEngineUrlRequestJob::UrlInvalid);
        return;
    }
    QString file;
    QString urlPath = url.path();
    if (urlPath.isEmpty() || urlPath == QLatin1String("/"))
        urlPath = QStringLiteral("/index.html");
    if (urlPath == QLatin1String("/qwebchannel.js")) {
        file = QStringLiteral(":/qtwebchannel/qwebchannel.js");
    } else {
        file = resolve(m_root, urlPath);
        if (file.isEmpty()) {
            // Distinguish "outside the directory" from "missing" without leaking paths.
            const bool escapes = urlPath.contains(QLatin1String(".."));
            job->fail(escapes ? QWebEngineUrlRequestJob::RequestDenied
                              : QWebEngineUrlRequestJob::UrlNotFound);
            return;
        }
    }
    auto *f = new QFile(file, job);
    if (!f->open(QIODevice::ReadOnly)) {
        job->fail(QWebEngineUrlRequestJob::UrlNotFound);
        return;
    }
    job->reply(mimeFor(file), f);
}
