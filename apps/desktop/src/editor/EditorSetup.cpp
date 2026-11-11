#include "EditorSetup.h"
#include "AssetSchemeHandler.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QQuickWebEngineProfile>
#include <QWebEngineUrlScheme>
#include <QtWebEngineQuick/qtwebenginequickglobal.h>

void initEditorWebEngine()
{
    QWebEngineUrlScheme scheme("axiom");
    scheme.setSyntax(QWebEngineUrlScheme::Syntax::Host);
    scheme.setFlags(QWebEngineUrlScheme::SecureScheme | QWebEngineUrlScheme::CorsEnabled
                    | QWebEngineUrlScheme::FetchApiAllowed);
    QWebEngineUrlScheme::registerScheme(scheme);
    QtWebEngineQuick::initialize();
}

QString locateEditorAssets(const QString &explicitDir)
{
    QStringList candidates;
    if (!explicitDir.isEmpty())
        candidates << explicitDir;
    const QString env = qEnvironmentVariable("AXIOM_EDITOR_ASSETS");
    if (!env.isEmpty())
        candidates << env;
    candidates << QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("editor"));
    for (const QString &c : std::as_const(candidates))
        if (QFileInfo::exists(QDir(c).filePath(QStringLiteral("index.html"))))
            return QFileInfo(c).absoluteFilePath();
    return {};
}

void installEditorSchemeHandler(const QString &assetDir, QObject *parent)
{
    auto *profile = QQuickWebEngineProfile::defaultProfile();
    // A previous Shell in this process (tests create several) installed one that is
    // about to be, or already is, destroyed. Drop the scheme entirely, then install.
    profile->removeUrlScheme("axiom");
    auto *handler = new AssetSchemeHandler(assetDir, parent);
    profile->installUrlSchemeHandler("axiom", handler);
    // The handler is owned by `parent`; unregister it before it is destroyed so the
    // profile never holds a dangling pointer.
    QObject::connect(handler, &QObject::destroyed, profile, [profile] { profile->removeUrlScheme("axiom"); });
}
