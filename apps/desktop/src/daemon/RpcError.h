#pragma once
#include <QJsonObject>
#include <QString>

// An error answer from the daemon, or one synthesised by the client
// (`timeout`, `internal`). Clients branch on `kind`, never on `message`.
struct RpcError
{
    QString kind;
    QString message;
    QJsonObject details;
    int code = 0;

    bool ok() const { return kind.isEmpty(); }

    static RpcError make(const QString &kind, const QString &message)
    {
        RpcError e;
        e.kind = kind;
        e.message = message;
        return e;
    }

    // From the `error` member of a JSON-RPC response (data.kind carries the kind).
    static RpcError fromJson(const QJsonObject &error)
    {
        RpcError e;
        e.code = error.value(QLatin1String("code")).toInt();
        e.message = error.value(QLatin1String("message")).toString();
        const QJsonObject data = error.value(QLatin1String("data")).toObject();
        e.kind = data.value(QLatin1String("kind")).toString();
        if (e.kind.isEmpty())
            e.kind = QStringLiteral("unknown");
        e.details = data.value(QLatin1String("details")).toObject();
        return e;
    }
};
