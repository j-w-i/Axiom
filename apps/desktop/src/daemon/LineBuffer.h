#pragma once
#include <QByteArray>
#include <QList>

// Accumulates bytes and yields complete lines. One read never equals one
// message: data may end mid-line or hold several lines.
class LineBuffer
{
public:
    // Appends `data` and returns every line completed by it, without the
    // terminator (a trailing '\r' is removed). Empty lines are dropped.
    QList<QByteArray> append(const QByteArray &data);

    // Bytes of the current incomplete line.
    qsizetype pendingBytes() const { return m_buf.size(); }
    void clear();

private:
    QByteArray m_buf;
    qsizetype m_scanned = 0; // bytes of m_buf already known to hold no '\n'
};
