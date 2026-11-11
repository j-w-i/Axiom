#pragma once
#include <QJsonObject>
#include <QObject>
#include <QString>

// The QWebChannel object `host` (docs/protocols/editor-bridge.md). Its
// exposed surface is exactly the contract: nine signals (shell -> page) and
// seven slots (page -> shell), each with one JSON object. Nothing else is
// reachable by the page: there are deliberately no properties, no other
// slots, and the object this class reports to is a plain C++ interface.
class EditorBridge : public QObject
{
    Q_OBJECT
public:
    static constexpr int Version = 1;

    // Receives what the page says. Implemented by OpenDocumentsModel.
    class Listener
    {
    public:
        virtual ~Listener() = default;
        virtual void bridgeReady(int bridgeVersion, const QString &monacoVersion) = 0;
        virtual void bridgeSaveRequested(const QString &path, const QString &text) = 0;
        virtual void bridgeDirtyChanged(const QString &path, bool dirty) = 0;
        virtual void bridgeCursorChanged(const QString &path, int line, int column, int selectionLength) = 0;
        virtual void bridgeTextSnapshot(const QString &path, const QString &text) = 0;
        virtual void bridgeOpenLink(const QString &url) = 0;
        virtual void bridgeReportError(const QString &message, const QString &detail) = 0;
    };

    explicit EditorBridge(QObject *parent = nullptr) : QObject(parent) {}
    void setListener(Listener *l) { m_listener = l; }

public slots: // page -> shell
    void ready(const QJsonObject &p);
    void saveRequested(const QJsonObject &p);
    void dirtyChanged(const QJsonObject &p);
    void cursorChanged(const QJsonObject &p);
    void textSnapshot(const QJsonObject &p);
    void openLink(const QJsonObject &p);
    void reportError(const QJsonObject &p);

signals: // shell -> page
    void configure(const QJsonObject &p);
    void openDocument(const QJsonObject &p);
    void activateDocument(const QJsonObject &p);
    void closeDocument(const QJsonObject &p);
    void requestSave(const QJsonObject &p);
    void documentSaved(const QJsonObject &p);
    void saveFailed(const QJsonObject &p);
    void revealPosition(const QJsonObject &p);
    void runCommand(const QJsonObject &p);

private:
    Listener *m_listener = nullptr;
};
