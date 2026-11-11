#include "editor/AssetSchemeHandler.h"
#include "editor/EditorSetup.h"

#include <QDir>
#include <QFile>
#include <QTemporaryDir>
#include <QtTest>

// The scheme handler's path logic: serve only files inside the asset directory.
class TstAssetHandler : public QObject
{
    Q_OBJECT
    QTemporaryDir m_root;   // the asset directory
    QTemporaryDir m_outside; // a sibling that must stay unreachable

    static void write(const QString &path, const QByteArray &data = "x")
    {
        QDir().mkpath(QFileInfo(path).absolutePath());
        QFile f(path);
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.write(data);
    }

private slots:
    void initTestCase()
    {
        QVERIFY(m_root.isValid() && m_outside.isValid());
        write(m_root.path() + "/index.html");
        write(m_root.path() + "/assets/app.js");
        write(m_outside.path() + "/secret.txt", "secret");
    }

    void resolvesFilesInsideTheDirectory()
    {
        const QString root = QFileInfo(m_root.path()).canonicalFilePath();
        QCOMPARE(AssetSchemeHandler::resolve(root, "/index.html"), root + "/index.html");
        QCOMPARE(AssetSchemeHandler::resolve(root, "/assets/app.js"), root + "/assets/app.js");
    }

    void rejectsTraversalAndOddPaths()
    {
        const QString root = QFileInfo(m_root.path()).canonicalFilePath();
        const QString up = QDir(root).relativeFilePath(m_outside.path() + "/secret.txt");
        for (const QString &bad : { QStringLiteral("/../secret.txt"), "/" + up, QStringLiteral("/assets/../../x"),
                                    QStringLiteral("/assets/..\\..\\x"), QStringLiteral("/a\0b"), m_outside.path() + "/secret.txt" }) {
            QVERIFY2(AssetSchemeHandler::resolve(root, bad).isEmpty(), qPrintable(bad));
        }
    }

    void directoriesAndMissingFilesAreNotServed()
    {
        const QString root = QFileInfo(m_root.path()).canonicalFilePath();
        QVERIFY(AssetSchemeHandler::resolve(root, "/assets").isEmpty());
        QVERIFY(AssetSchemeHandler::resolve(root, "/nope.js").isEmpty());
        QVERIFY(AssetSchemeHandler::resolve(QString(), "/index.html").isEmpty());
    }

    void symlinkOutsideIsDenied()
    {
#ifdef Q_OS_UNIX
        const QString root = QFileInfo(m_root.path()).canonicalFilePath();
        QVERIFY(QFile::link(m_outside.path() + "/secret.txt", root + "/link.txt"));
        QVERIFY(QFile::link(m_outside.path(), root + "/outdir"));
        QVERIFY(AssetSchemeHandler::resolve(root, "/link.txt").isEmpty());
        QVERIFY(AssetSchemeHandler::resolve(root, "/outdir/secret.txt").isEmpty());
#else
        QSKIP("symlinks are unix-only here");
#endif
    }

    void siblingDirectoryWithSharedPrefixIsDenied()
    {
        // /tmp/x/editor vs /tmp/x/editor-evil: a naive startsWith(root) would allow it.
        QTemporaryDir parent;
        QVERIFY(parent.isValid());
        write(parent.path() + "/editor/index.html");
        write(parent.path() + "/editor-evil/secret.txt");
        const QString root = QFileInfo(parent.path() + "/editor").canonicalFilePath();
        QVERIFY(AssetSchemeHandler::resolve(root, "/../editor-evil/secret.txt").isEmpty());
    }

    void mimeTypes()
    {
        QCOMPARE(AssetSchemeHandler::mimeFor("a.html"), QByteArray("text/html"));
        QCOMPARE(AssetSchemeHandler::mimeFor("a.js"), QByteArray("text/javascript"));
        QCOMPARE(AssetSchemeHandler::mimeFor("a.css"), QByteArray("text/css"));
        QCOMPARE(AssetSchemeHandler::mimeFor("a.json"), QByteArray("application/json"));
        QCOMPARE(AssetSchemeHandler::mimeFor("a.ttf"), QByteArray("font/ttf"));
        QCOMPARE(AssetSchemeHandler::mimeFor("a.woff2"), QByteArray("font/woff2"));
        QCOMPARE(AssetSchemeHandler::mimeFor("a.svg"), QByteArray("image/svg+xml"));
        QCOMPARE(AssetSchemeHandler::mimeFor("a.map"), QByteArray("application/json"));
        QCOMPARE(AssetSchemeHandler::mimeFor("a.wasm"), QByteArray("application/wasm"));
        QCOMPARE(AssetSchemeHandler::mimeFor("A.JS"), QByteArray("text/javascript"));
        QCOMPARE(AssetSchemeHandler::mimeFor("a.unknown"), QByteArray("application/octet-stream"));
    }

    void assetLookupOrder()
    {
        QTemporaryDir a;
        write(a.path() + "/index.html");
        QCOMPARE(locateEditorAssets(a.path()), QFileInfo(a.path()).absoluteFilePath());
        QTemporaryDir empty; // no index.html: not an editor bundle
        QVERIFY(locateEditorAssets(empty.path()) != empty.path());
        qputenv("AXIOM_EDITOR_ASSETS", a.path().toUtf8());
        QCOMPARE(locateEditorAssets(QString()), QFileInfo(a.path()).absoluteFilePath());
        qunsetenv("AXIOM_EDITOR_ASSETS");
    }
};

QTEST_GUILESS_MAIN(TstAssetHandler)
#include "tst_assethandler.moc"
