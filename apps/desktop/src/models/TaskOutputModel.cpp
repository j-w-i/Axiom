#include "TaskOutputModel.h"

#include <QRegularExpression>

TaskOutputModel::TaskOutputModel(QObject *parent) : QAbstractListModel(parent)
{
    m_timer.setSingleShot(true);
    m_timer.setInterval(FlushIntervalMs);
    connect(&m_timer, &QTimer::timeout, this, &TaskOutputModel::flush);
}

int TaskOutputModel::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : static_cast<int>(m_rows.size()) + visibleOffset();
}

QVariant TaskOutputModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= rowCount())
        return {};
    if (visibleOffset() && index.row() == 0) {
        switch (role) {
        case Qt::DisplayRole:
        case TextRole: return tr("… earlier output truncated (%1 lines)").arg(m_dropped);
        case KindRole: return QStringLiteral("notice");
        case StreamRole: return QString();
        default: return {};
        }
    }
    const Row &r = m_rows.at(index.row() - visibleOffset());
    switch (role) {
    case Qt::DisplayRole:
    case TextRole: return r.text;
    case StreamRole: return r.stream;
    case KindRole: return r.kind;
    default: return {};
    }
}

QHash<int, QByteArray> TaskOutputModel::roleNames() const
{
    return { { TextRole, "text" }, { StreamRole, "stream" }, { KindRole, "kind" } };
}

QString TaskOutputModel::textAt(int row) const
{
    return data(index(row), TextRole).toString();
}

QString TaskOutputModel::allText() const
{
    QStringList lines;
    lines.reserve(rowCount());
    for (int i = 0; i < rowCount(); ++i)
        lines << textAt(i);
    return lines.join(QLatin1Char('\n'));
}

void TaskOutputModel::reset()
{
    m_timer.stop();
    beginResetModel();
    m_rows.clear();
    m_pending.clear();
    m_early.clear();
    m_nextSeq = 0;
    m_partial[0].clear();
    m_partial[1].clear();
    const bool wasTruncated = m_dropped > 0;
    m_dropped = 0;
    endResetModel();
    emit countChanged();
    if (wasTruncated)
        emit truncatedChanged();
}

// ---- ANSI ----------------------------------------------------------------------

QString TaskOutputModel::stripAnsi(const QString &s)
{
    // CSI: ESC [ params intermediates final.  OSC: ESC ] ... (BEL | ESC \).  Other two-byte ESC sequences.
    static const QRegularExpression re(
        QStringLiteral("\\x1B\\[[0-?]*[ -/]*[@-~]"
                       "|\\x1B\\][^\\x07\\x1B]*(?:\\x07|\\x1B\\\\)"
                       "|\\x1B[@-Z\\\\-_]"),
        QRegularExpression::UseUnicodePropertiesOption);
    QString out = s;
    out.remove(re);
    // Remaining control characters would render as boxes; keep newline and tab.
    out.removeIf([](QChar c) {
        return c.unicode() < 0x20 && c != QLatin1Char('\n') && c != QLatin1Char('\t') && c != QLatin1Char('\r');
    });
    return out;
}

// ---- input ---------------------------------------------------------------------

void TaskOutputModel::append(quint64 seq, const QString &stream, const QString &data)
{
    if (seq < m_nextSeq)
        return; // duplicate
    if (seq != m_nextSeq) {
        m_early.insert(seq, { stream, data });
        return;
    }
    applyChunk(stream, data);
    ++m_nextSeq;
    for (auto it = m_early.begin(); it != m_early.end() && it.key() == m_nextSeq;) {
        applyChunk(it.value().first, it.value().second);
        ++m_nextSeq;
        it = m_early.erase(it);
    }
}

void TaskOutputModel::applyChunk(const QString &stream, const QString &data)
{
    const int slot = stream == QLatin1String("stderr") ? 1 : 0;
    // A carriage return alone rewrites the line in a terminal; without one, keep the last
    // segment, which is what the user would see.
    QString text = m_partial[slot] + stripAnsi(data);
    m_partial[slot].clear();
    qsizetype start = 0;
    for (;;) {
        const qsizetype nl = text.indexOf(QLatin1Char('\n'), start);
        if (nl < 0)
            break;
        QString line = text.mid(start, nl - start);
        if (line.endsWith(QLatin1Char('\r')))
            line.chop(1);
        const qsizetype cr = line.lastIndexOf(QLatin1Char('\r'));
        if (cr >= 0)
            line = line.mid(cr + 1);
        enqueue({ line, stream, QStringLiteral("line") });
        start = nl + 1;
    }
    m_partial[slot] = text.mid(start);
    scheduleFlush();
}

void TaskOutputModel::appendStatus(const QString &text, bool ok)
{
    finish();
    enqueue({ text, QString(), ok ? QStringLiteral("status") : QStringLiteral("statusFailed") });
    flushNow();
}

void TaskOutputModel::appendError(const QString &text)
{
    finish();
    enqueue({ text, QStringLiteral("stderr"), QStringLiteral("error") });
    flushNow();
}

void TaskOutputModel::finish()
{
    // The task is over: a trailing line without newline is a real line.
    for (int slot = 0; slot < 2; ++slot) {
        if (!m_partial[slot].isEmpty()) {
            QString line = m_partial[slot];
            const qsizetype cr = line.lastIndexOf(QLatin1Char('\r'));
            if (cr >= 0)
                line = line.mid(cr + 1);
            enqueue({ line, slot == 1 ? QStringLiteral("stderr") : QStringLiteral("stdout"), QStringLiteral("line") });
            m_partial[slot].clear();
        }
    }
    flushNow();
}

void TaskOutputModel::enqueue(const Row &row)
{
    m_pending.append(row);
}

void TaskOutputModel::scheduleFlush()
{
    if (!m_pending.isEmpty() && !m_timer.isActive())
        m_timer.start();
}

// ---- publishing ----------------------------------------------------------------

void TaskOutputModel::flushNow()
{
    m_timer.stop();
    while (!m_pending.isEmpty())
        flush();
}

void TaskOutputModel::flush()
{
    if (m_pending.isEmpty())
        return;
    // At most a bounded number of rows per tick keeps one flush short; the rest follows.
    const int take = static_cast<int>(qMin<qsizetype>(m_pending.size(), MaxRowsPerFlush));
    QList<Row> batch = m_pending.mid(0, take);
    m_pending.remove(0, take);

    // Anything beyond the cap is dropped before it ever reaches the views.
    const bool hadNotice = m_dropped > 0;
    int overflow = static_cast<int>(m_rows.size()) + take - MaxLines;
    if (overflow > 0) {
        // Drop from the front of the existing rows first, then from the batch itself.
        const int fromExisting = qMin<int>(overflow, static_cast<int>(m_rows.size()));
        if (fromExisting > 0) {
            const int first = visibleOffset();
            beginRemoveRows({}, first, first + fromExisting - 1);
            m_rows.remove(0, fromExisting);
            m_dropped += fromExisting;
            endRemoveRows();
        }
        const int fromBatch = overflow - fromExisting;
        if (fromBatch > 0) {
            batch.remove(0, fromBatch);
            m_dropped += fromBatch;
        }
    }
    if (!hadNotice && m_dropped > 0) {
        // The notice row appears at the top, shifting everything by one.
        beginInsertRows({}, 0, 0);
        // visibleOffset() now reports 1 because m_dropped > 0.
        endInsertRows();
        emit truncatedChanged();
    } else if (hadNotice && overflow > 0) {
        const QModelIndex idx = index(0);
        emit dataChanged(idx, idx, { TextRole });
    }

    if (!batch.isEmpty()) {
        const int first = static_cast<int>(m_rows.size()) + visibleOffset();
        beginInsertRows({}, first, first + static_cast<int>(batch.size()) - 1);
        m_rows.append(batch);
        endInsertRows();
    }
    emit countChanged();
    if (!m_pending.isEmpty())
        m_timer.start(); // the rest on the next tick
}
