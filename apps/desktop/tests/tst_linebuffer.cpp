#include "daemon/LineBuffer.h"

#include <QtTest>

class TstLineBuffer : public QObject
{
    Q_OBJECT
private slots:
    void oneLine()
    {
        LineBuffer b;
        QCOMPARE(b.append("hello\n"), QList<QByteArray>{ "hello" });
        QCOMPARE(b.pendingBytes(), 0);
    }
    void severalLinesInOneRead()
    {
        LineBuffer b;
        QCOMPARE(b.append("a\nb\nc\n"), (QList<QByteArray>{ "a", "b", "c" }));
    }
    void lineSplitAcrossReads()
    {
        LineBuffer b;
        QVERIFY(b.append("{\"jsonrpc\":").isEmpty());
        QCOMPARE(b.pendingBytes(), 11);
        QVERIFY(b.append("\"2.0\",\"id\"").isEmpty());
        QCOMPARE(b.append(":1}\n"), QList<QByteArray>{ "{\"jsonrpc\":\"2.0\",\"id\":1}" });
    }
    void splitAtEveryByte()
    {
        const QByteArray all = "one\ntwo\r\nthree\n";
        LineBuffer b;
        QList<QByteArray> got;
        for (char c : all)
            got += b.append(QByteArray(1, c));
        QCOMPARE(got, (QList<QByteArray>{ "one", "two", "three" }));
    }
    void terminatorSplitFromItsLine()
    {
        LineBuffer b;
        QVERIFY(b.append("abc").isEmpty());
        QCOMPARE(b.append("\n"), QList<QByteArray>{ "abc" });
    }
    void crlfIsAcceptedAndEmptyLinesIgnored()
    {
        LineBuffer b;
        QCOMPARE(b.append("a\r\n\r\n\nb\n"), (QList<QByteArray>{ "a", "b" }));
    }
    void completeLineFollowedByPartial()
    {
        LineBuffer b;
        QCOMPARE(b.append("done\npart"), QList<QByteArray>{ "done" });
        QCOMPARE(b.pendingBytes(), 4);
        QCOMPARE(b.append("ial\n"), QList<QByteArray>{ "partial" });
    }
    void multiByteUtf8SplitInsideACharacter()
    {
        const QByteArray line = QStringLiteral("héllo ✓").toUtf8();
        LineBuffer b;
        QVERIFY(b.append(line.left(2)).isEmpty()); // splits 'é'
        const auto got = b.append(line.mid(2) + "\n");
        QCOMPARE(got.size(), 1);
        QCOMPARE(QString::fromUtf8(got.first()), QStringLiteral("héllo ✓"));
    }
    void clearDropsPartialData()
    {
        LineBuffer b;
        b.append("stale");
        b.clear();
        QCOMPARE(b.append("fresh\n"), QList<QByteArray>{ "fresh" });
    }
    void largeLine()
    {
        const QByteArray big(1 << 20, 'x');
        LineBuffer b;
        QVERIFY(b.append(big).isEmpty());
        QVERIFY(b.append(big).isEmpty());
        const auto got = b.append("\n");
        QCOMPARE(got.size(), 1);
        QCOMPARE(got.first().size(), 2 << 20);
    }
};

QTEST_APPLESS_MAIN(TstLineBuffer)
#include "tst_linebuffer.moc"
