#include "LineBuffer.h"

QList<QByteArray> LineBuffer::append(const QByteArray &data)
{
    QList<QByteArray> lines;
    m_buf.append(data);
    qsizetype start = 0;
    for (;;) {
        const qsizetype nl = m_buf.indexOf('\n', qMax(start, m_scanned));
        if (nl < 0)
            break;
        QByteArray line = m_buf.mid(start, nl - start);
        if (line.endsWith('\r'))
            line.chop(1);
        if (!line.isEmpty())
            lines.append(line);
        start = nl + 1;
        m_scanned = start;
    }
    if (start > 0) {
        m_buf.remove(0, start);
        m_scanned = 0;
    } else {
        m_scanned = m_buf.size();
    }
    return lines;
}

void LineBuffer::clear()
{
    m_buf.clear();
    m_scanned = 0;
}
