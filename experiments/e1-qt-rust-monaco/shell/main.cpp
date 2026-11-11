// E1 experiment: C++/QML shell + Monaco in Qt WebEngine (custom secure scheme)
// + QWebChannel bridge + Rust daemon over NDJSON JSON-RPC on stdio.
#include <QtCore>
#include <QtGui>
#include <QtQml>
#include <QtQuick>
#include <QtWebEngineQuick>
#include <QWebEngineUrlScheme>
#include <QWebEngineUrlSchemeHandler>
#include <QWebEngineUrlRequestJob>
#include <QQuickWebEngineProfile>
#include <functional>

class AssetHandler : public QWebEngineUrlSchemeHandler {
public:
    explicit AssetHandler(QString root, QObject *p = nullptr) : QWebEngineUrlSchemeHandler(p), m_root(std::move(root)) {}
    void requestStarted(QWebEngineUrlRequestJob *job) override {
        QString rel = job->requestUrl().path();
        QString file = rel == "/qwebchannel.js" ? QStringLiteral(":/qtwebchannel/qwebchannel.js")
                                                : QDir::cleanPath(m_root + rel);
        if (!file.startsWith(':') && !file.startsWith(m_root)) { job->fail(QWebEngineUrlRequestJob::RequestDenied); return; }
        auto *f = new QFile(file, job);
        if (!f->open(QIODevice::ReadOnly)) { job->fail(QWebEngineUrlRequestJob::UrlNotFound); return; }
        static const QHash<QString, QByteArray> types{{"html","text/html"},{"js","text/javascript"},{"css","text/css"},
            {"json","application/json"},{"ttf","font/ttf"},{"svg","image/svg+xml"},{"map","application/json"}};
        job->reply(types.value(QFileInfo(file).suffix(), "application/octet-stream"), f);
    }
private:
    QString m_root;
};

class Daemon : public QObject {
    Q_OBJECT
public:
    using Cb = std::function<void(const QJsonObject &result, const QJsonObject &error)>;
    explicit Daemon(const QString &exe, QObject *p = nullptr) : QObject(p) {
        connect(&m_proc, &QProcess::readyReadStandardOutput, this, [this] {
            while (m_proc.canReadLine()) {
                const QJsonObject m = QJsonDocument::fromJson(m_proc.readLine()).object();
                if (m.contains("method")) { emit notification(m["method"].toString(), m["params"].toObject()); continue; }
                if (auto cb = m_pending.take(m["id"].toInt())) cb(m["result"].toObject(), m["error"].toObject());
            }
        });
        m_proc.setProcessChannelMode(QProcess::ForwardedErrorChannel);
        m_proc.start(exe, {});
    }
    void call(const QString &method, const QJsonObject &params, Cb cb) {
        const int id = ++m_next;
        m_pending.insert(id, std::move(cb));
        m_proc.write(QJsonDocument(QJsonObject{{"jsonrpc","2.0"},{"id",id},{"method",method},{"params",params}}).toJson(QJsonDocument::Compact) + '\n');
    }
    void stop() { m_proc.closeWriteChannel(); m_proc.waitForFinished(2000); }
signals:
    void notification(const QString &method, const QJsonObject &params);
private:
    QProcess m_proc; int m_next = 0; QHash<int, Cb> m_pending;
};

class Host : public QObject {
    Q_OBJECT
public:
    Host(Daemon *d, QString file, bool selfTest) : m_d(d), m_file(std::move(file)), m_selfTest(selfTest) {
        connect(d, &Daemon::notification, this, [this](const QString &m, const QJsonObject &p) {
            if (m == "process/output") m_output.append(QJsonObject{{"stream", p["stream"]}, {"text", p["text"]}});
        });
    }
    QQuickWindow *window = nullptr;
signals:
    void openDocument(const QString &path, const QString &text, const QString &languageId);
    void applyEdit(int line, int column, const QString &text);
    void requestSave();
    void requestState();
public slots:
    void consoleMessage(int level, const QString &msg, const QString &where) { if (level >= 2) m_consoleErrors.append(msg + " @ " + where); }
    void editorReady(const QString &) {
        m_timer.start(); note("editorReady", true);
        m_d->call("initialize", {}, [this](auto r, auto) {
            note("daemonInitialize", r["server"].toString() == "e1-daemon");
            m_d->call("fs/readFile", {{"path", m_file}}, [this](auto r, auto e) {
                note("backendReadFile", e.isEmpty());
                emit openDocument(m_file, r["text"].toString(), "typescript");
            });
        });
    }
    void documentOpened(const QString &, int lines) {
        note("documentOpenedInMonaco", lines > 0);
        if (!m_selfTest) return;
        emit applyEdit(1, 1, "// edited-by-host\n");
        // Real key events through the Qt -> Chromium input path.
        QTimer::singleShot(600, this, [this] {
            for (QChar c : QStringLiteral("//K")) key(c.unicode() == 'K' ? Qt::Key_K : Qt::Key_Slash, c.isUpper() ? Qt::ShiftModifier : Qt::NoModifier, QString(c));
            QTimer::singleShot(400, this, [this] {
                key(Qt::Key_S, Qt::ControlModifier, QString());
                QTimer::singleShot(1500, this, [this] { if (!m_saved) { m_saveVia = "host-request (Ctrl+S key event did not reach Monaco)"; emit requestSave(); } });
            });
        });
    }
    void saveDocument(const QString &path, const QString &text) {
        if (m_saved) return;
        m_saved = true;
        m_d->call("fs/writeFile", {{"path", path}, {"text", text}}, [this, path](auto, auto e) {
            note("backendWriteFile", e.isEmpty());
            if (!m_selfTest) return;
            QFile f(path); const QString disk = f.open(QIODevice::ReadOnly) ? QString::fromUtf8(f.readAll()) : QString();
            note("hostEditOnDisk", disk.contains("// edited-by-host"));
            note("typedKeysOnDisk", disk.contains("//K"));
            m_report["saveTriggeredBy"] = m_saveVia;
            m_d->call("process/run", {{"runId", "r1"}, {"argv", QJsonArray{"sh", "-c", "echo hello-out; echo hello-err >&2; exit 3"}}}, [this](auto r, auto) {
                note("commandExitCode3", r["exitCode"].toInt() == 3);
                m_report["commandOutput"] = m_output;
                note("commandStreamsSeparated", m_output.size() == 2);
                m_d->call("process/run", {{"runId", "r2"}, {"argv", QJsonArray{"axiom-no-such-binary"}}}, [this](auto, auto e) {
                    note("structuredSpawnError", e["data"].toObject()["kind"].toString() == "spawn_failed");
                    m_report["spawnError"] = e;
                    emit requestState();
                });
            });
        });
    }
    void stateReport(const QString &json) {
        const QJsonObject s = QJsonDocument::fromJson(json.toUtf8()).object();
        note("secureContext", s["secureContext"].toBool());
        note("tsWorkerDiagnostics", !s["markers"].toArray().isEmpty());
        m_report["origin"] = s["origin"]; m_report["markers"] = s["markers"]; m_report["userAgent"] = s["userAgent"];
        m_report["monacoHasTextFocus"] = s["hasTextFocus"];
        finish();
    }
    void fail(const QString &why) { note("completed:" + why, false); finish(); }
    void finish() {
        m_report["elapsedMsSinceEditorReady"] = m_timer.elapsed();
        m_report["consoleErrors"] = QJsonArray::fromStringList(m_consoleErrors);
        m_report["checks"] = m_checks; m_report["allPassed"] = m_ok;
        QTextStream(stdout) << QJsonDocument(m_report).toJson() << Qt::endl;
        m_d->stop();
        QCoreApplication::exit(m_ok ? 0 : 1);
    }
private:
    void note(const QString &k, bool ok) { m_checks[k] = ok; m_ok = m_ok && ok; }
    void key(int k, Qt::KeyboardModifiers mods, const QString &text) {
        QObject *target = window->activeFocusItem() ? static_cast<QObject *>(window->activeFocusItem()) : window;
        // WebEngine forwards input from its focus proxy; deliver to the item with active focus.
        QCoreApplication::sendEvent(target, new QKeyEvent(QEvent::KeyPress, k, mods, text));
        QCoreApplication::sendEvent(target, new QKeyEvent(QEvent::KeyRelease, k, mods, text));
    }
    Daemon *m_d; QString m_file; bool m_selfTest; bool m_saved = false; bool m_ok = true;
    QString m_saveVia = "ctrl+s-key-event";
    QJsonObject m_checks, m_report; QJsonArray m_output; QStringList m_consoleErrors; QElapsedTimer m_timer;
};

int main(int argc, char **argv) {
    QWebEngineUrlScheme scheme("axiom");
    scheme.setSyntax(QWebEngineUrlScheme::Syntax::Host);
    scheme.setFlags(QWebEngineUrlScheme::SecureScheme | QWebEngineUrlScheme::CorsEnabled | QWebEngineUrlScheme::FetchApiAllowed);
    QWebEngineUrlScheme::registerScheme(scheme);
    QtWebEngineQuick::initialize();
    QGuiApplication app(argc, argv);
    QCommandLineParser cli;
    cli.addOptions({{"daemon", "daemon exe", "path"}, {"assets", "editor dist dir", "path"}, {"file", "file to open", "path"}, {"self-test", "run scripted checks and exit"}});
    cli.process(app);

    QQuickWebEngineProfile::defaultProfile()->installUrlSchemeHandler("axiom", new AssetHandler(QDir(cli.value("assets")).absolutePath(), &app));
    Daemon daemon(cli.value("daemon"));
    Host host(&daemon, QFileInfo(cli.value("file")).absoluteFilePath(), cli.isSet("self-test"));
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("host", &host);
    engine.loadFromModule("E1", "Main");
    if (engine.rootObjects().isEmpty()) return 2;
    host.window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (cli.isSet("self-test")) QTimer::singleShot(60000, &host, [&host] { qWarning("self-test timeout"); host.fail("timeout"); });
    return app.exec();
}
#include "main.moc"
