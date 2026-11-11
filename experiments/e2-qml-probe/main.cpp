// E2 experiment: in-process QML probe. Walks the QQuickItem tree and reports, for each
// item, type, scene geometry and the QML source location that instantiated it.
// Uses PRIVATE Qt API (QQmlData) for line/column: version-locked, see results.
#include <QtCore>
#include <QtGui>
#include <QtQml>
#include <QtQuick>
#include <private/qqmldata_p.h>
#include <private/qqmlcontextdata_p.h>

static QJsonObject describe(QQuickItem *item, const QString &root) {
    QJsonObject o;
    o["type"] = QString::fromLatin1(item->metaObject()->className());
    if (!item->objectName().isEmpty()) o["objectName"] = item->objectName();
    const QRectF r = item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
    o["bounds"] = QJsonObject{{"x", r.x()}, {"y", r.y()}, {"width", r.width()}, {"height", r.height()}};
    o["visible"] = item->isVisible();
    if (QQmlData *d = QQmlData::get(item); d && d->outerContext) {
        QString file = d->outerContext->url().toString();
        // Public-API alternative for the file only:
        QQmlContext *ctx = qmlContext(item);
        o["source"] = QJsonObject{{"file", file}, {"line", int(d->lineNumber)}, {"column", int(d->columnNumber)},
                                  {"publicApiBaseUrl", ctx ? ctx->baseUrl().toString() : QString()}};
    }
    QJsonArray kids;
    for (QQuickItem *c : item->childItems()) kids.append(describe(c, root));
    if (!kids.isEmpty()) o["children"] = kids;
    return o;
}

static QQuickItem *hit(QQuickItem *item, const QPointF &scenePt) {
    const auto kids = item->childItems();
    for (auto it = kids.crbegin(); it != kids.crend(); ++it)
        if (QQuickItem *h = hit(*it, scenePt)) return h;
    return item->isVisible() && item->contains(item->mapFromScene(scenePt)) ? item : nullptr;
}

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    QQmlApplicationEngine engine;
    engine.loadFromModule("E2", "Main");
    if (engine.rootObjects().isEmpty()) return 2;
    auto *win = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    QTimer::singleShot(300, [&] {
        QJsonObject out;
        out["tree"] = describe(win->contentItem(), {});
        // Select the second card's button by coordinate: column at (20,20), card 2 at y=20+96+12, button at (12,64).
        const QPointF pt(20 + 12 + 40, 20 + 96 + 12 + 64 + 12);
        QQuickItem *h = hit(win->contentItem(), pt);
        QJsonArray chain;
        for (QQuickItem *i = h; i; i = i->parentItem()) { QJsonObject d = describe(i, {}); d.remove("children"); chain.append(d); }
        out["hitTest"] = QJsonObject{{"point", QJsonObject{{"x", pt.x()}, {"y", pt.y()}}}, {"chain", chain}};
        out["devicePixelRatio"] = win->effectiveDevicePixelRatio();
        const bool grabbed = win->grabWindow().save("capture.png");
        out["screenshotSaved"] = grabbed;
        QTextStream(stdout) << QJsonDocument(out).toJson() << Qt::endl;
        app.quit();
    });
    return app.exec();
}
