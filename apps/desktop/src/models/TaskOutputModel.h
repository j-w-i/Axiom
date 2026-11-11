#pragma once
#include <QAbstractListModel>
#include <QList>
#include <QMap>
#include <QString>
#include <QTimer>

// The lines of one task's output, fed by `task/output` chunks in `seq` order.
// Chunks are split into lines (a partial trailing line is kept per stream),
// ANSI escape sequences are stripped, and the model is capped. Rows are
// published to views in batches on a short timer so a task that prints
// 50 000 lines cannot stall the event loop.
class TaskOutputModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
    Q_PROPERTY(bool truncated READ truncated NOTIFY truncatedChanged)

public:
    enum Roles { TextRole = Qt::UserRole + 1, StreamRole, KindRole };
    // Row kinds: "line" (output), "status" (exit status), "error" (start failure), "notice" (truncation).
    static constexpr int MaxLines = 10000;
    static constexpr int FlushIntervalMs = 16;
    static constexpr int MaxRowsPerFlush = 500;

    explicit TaskOutputModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    bool truncated() const { return m_dropped > 0; }
    qint64 droppedLines() const { return m_dropped; }

    // Starts a fresh output for a new task.
    void reset();
    // A chunk of output. Chunks are applied in `seq` order; early ones wait for the gap to fill.
    void append(quint64 seq, const QString &stream, const QString &data);
    // Status row for the end of the task.
    void appendStatus(const QString &text, bool ok);
    // A row shown in the danger colour (spawn failure, etc.).
    void appendError(const QString &text);
    // Flushes pending lines and the partial trailing lines of both streams (task ended).
    void finish();
    // Publishes everything pending right now (tests, and `finish`).
    void flushNow();

    // All visible text, one line per row; for "Copy all".
    Q_INVOKABLE QString allText() const;
    Q_INVOKABLE QString textAt(int row) const;

    // Removes CSI/OSC escape sequences and stray control characters (not \n, \t).
    static QString stripAnsi(const QString &s);

signals:
    void countChanged();
    void truncatedChanged();

private:
    struct Row
    {
        QString text;
        QString stream; // "stdout" | "stderr" | ""
        QString kind;
    };
    void applyChunk(const QString &stream, const QString &data);
    void enqueue(const Row &row);
    void trim();
    void scheduleFlush();
    void flush();
    int visibleOffset() const { return m_dropped > 0 ? 1 : 0; }

    QList<Row> m_rows;         // published, without the truncation notice
    QList<Row> m_pending;      // waiting for the next flush
    QMap<quint64, std::pair<QString, QString>> m_early; // seq -> (stream, data)
    quint64 m_nextSeq = 0;
    QString m_partial[2];      // trailing partial line: 0 = stdout, 1 = stderr
    qint64 m_dropped = 0;
    QTimer m_timer;
};
