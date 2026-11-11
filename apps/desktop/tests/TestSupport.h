#pragma once
// Cross-platform helpers for the desktop tests. No test may depend on a POSIX
// userland: commands go through the `emit` helper (tests/helpers/emit), and
// processes are controlled through these functions instead of <csignal>.

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QProcess>
#include <QString>
#include <QStringList>

#ifdef Q_OS_WIN
#  include <windows.h>
#  include <tlhelp32.h>
#else
#  include <csignal>
#  include <sys/types.h>
#  include <unistd.h>
#endif

namespace TestSupport {

// Path of the `emit` helper binary (env AXIOM_EMIT, set by CMake).
inline QString emitPath()
{
    return qEnvironmentVariable("AXIOM_EMIT");
}

// A command line for the daemon's shell form (`sh -c` / `cmd /C`) that runs `emit` with `args`.
// The path and every argument are quoted for the platform's shell.
inline QString emitCommand(const QStringList &args)
{
    auto quote = [](const QString &s) {
#ifdef Q_OS_WIN
        // cmd.exe: double quotes protect spaces; inner quotes are not used by the tests.
        return QStringLiteral("\"") + QString(s).replace(QLatin1Char('"'), QStringLiteral("\"\"")) + QStringLiteral("\"");
#else
        return QStringLiteral("'") + QString(s).replace(QLatin1Char('\''), QStringLiteral("'\\''")) + QStringLiteral("'");
#endif
    };
    QStringList parts{ quote(QDir::toNativeSeparators(emitPath())) };
    for (const QString &a : args)
        parts << quote(a);
    QString line = parts.join(QLatin1Char(' '));
#ifdef Q_OS_WIN
    // cmd /C strips the outer quotes of a line that starts with one; wrap the whole command.
    line = QStringLiteral("\"") + line + QStringLiteral("\"");
#endif
    return line;
}

// Terminates a process immediately (the equivalent of SIGKILL / TerminateProcess).
inline bool killProcess(qint64 pid)
{
#ifdef Q_OS_WIN
    HANDLE h = OpenProcess(PROCESS_TERMINATE, FALSE, static_cast<DWORD>(pid));
    if (!h)
        return false;
    const BOOL ok = TerminateProcess(h, 1);
    CloseHandle(h);
    return ok != 0;
#else
    return ::kill(static_cast<pid_t>(pid), SIGKILL) == 0;
#endif
}

// Freezes / resumes a process (SIGSTOP / SIGCONT). Not available on Windows.
inline bool canSuspend()
{
#ifdef Q_OS_WIN
    return false;
#else
    return true;
#endif
}
inline void suspendProcess([[maybe_unused]] qint64 pid)
{
#ifndef Q_OS_WIN
    ::kill(static_cast<pid_t>(pid), SIGSTOP);
#endif
}
inline void resumeProcess([[maybe_unused]] qint64 pid)
{
#ifndef Q_OS_WIN
    ::kill(static_cast<pid_t>(pid), SIGCONT);
#endif
}

inline bool processAlive(qint64 pid)
{
#ifdef Q_OS_WIN
    HANDLE h = OpenProcess(SYNCHRONIZE, FALSE, static_cast<DWORD>(pid));
    if (!h)
        return false;
    const DWORD r = WaitForSingleObject(h, 0);
    CloseHandle(h);
    return r == WAIT_TIMEOUT;
#else
    return ::kill(static_cast<pid_t>(pid), 0) == 0;
#endif
}

inline bool runningAsRoot()
{
#ifdef Q_OS_WIN
    return false;
#else
    return ::geteuid() == 0;
#endif
}

// Human-readable list of this process's descendants (pid, parent, command line), for failure
// messages: a test that cannot find a process must say what it saw instead.
inline QString describeDescendants();

// Direct and indirect child processes of this process whose command line contains `needle`.
// Linux: /proc. Windows: a toolhelp snapshot. macOS: `ps`. Only descendants of THIS process
// are ever returned, so a test cannot touch the user's own browsers or other applications.
struct ProcInfo
{
    qint64 parent;
    QByteArray cmd;
};

inline QHash<qint64, ProcInfo> snapshotProcesses()
{
    using P = ProcInfo;
    QHash<qint64, P> procs;
#if defined(Q_OS_LINUX)
    for (const QString &e : QDir(QStringLiteral("/proc")).entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
        bool ok = false;
        const qint64 pid = e.toLongLong(&ok);
        if (!ok)
            continue;
        QFile stat(QStringLiteral("/proc/%1/stat").arg(pid));
        if (!stat.open(QIODevice::ReadOnly))
            continue;
        const QByteArray st = stat.readAll();
        const QList<QByteArray> rest = st.mid(st.lastIndexOf(')') + 2).split(' ');
        if (rest.size() < 2)
            continue;
        QFile cmd(QStringLiteral("/proc/%1/cmdline").arg(pid));
        procs.insert(pid, { rest.at(1).toLongLong(), cmd.open(QIODevice::ReadOnly) ? cmd.readAll() : QByteArray() });
    }
#elif defined(Q_OS_WIN)
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (snap != INVALID_HANDLE_VALUE) {
        PROCESSENTRY32W pe;
        pe.dwSize = sizeof(pe);
        for (BOOL ok = Process32FirstW(snap, &pe); ok; ok = Process32NextW(snap, &pe))
            procs.insert(pe.th32ProcessID, { static_cast<qint64>(pe.th32ParentProcessID), QString::fromWCharArray(pe.szExeFile).toUtf8() });
        CloseHandle(snap);
    }
#else
    QProcess ps;
    ps.start(QStringLiteral("/bin/ps"), { QStringLiteral("-axo"), QStringLiteral("pid=,ppid=,command=") });
    if (ps.waitForFinished(5000)) {
        for (const QByteArray &line : ps.readAllStandardOutput().split('\n')) {
            // `ps` right-aligns its columns, so fields are separated by runs of spaces.
            const QByteArray t = line.trimmed();
            const qsizetype a = t.indexOf(' ');
            if (a <= 0)
                continue;
            const QByteArray rest = t.mid(a).trimmed();
            const qsizetype b = rest.indexOf(' ');
            if (b <= 0)
                continue;
            bool okPid = false, okPpid = false;
            const qint64 pid = t.left(a).toLongLong(&okPid);
            const qint64 ppid = rest.left(b).toLongLong(&okPpid);
            if (okPid && okPpid)
                procs.insert(pid, { ppid, rest.mid(b).trimmed() });
        }
    }
#endif
    return procs;
}

inline bool isDescendant(const QHash<qint64, ProcInfo> &procs, qint64 pid)
{
    const qint64 me = QCoreApplication::applicationPid();
    for (int depth = 0; depth < 32 && procs.contains(pid); ++depth) {
        pid = procs.value(pid).parent;
        if (pid == me)
            return true;
    }
    return false;
}

inline QList<qint64> descendantsMatching(const QByteArray &needle)
{
    const QHash<qint64, ProcInfo> procs = snapshotProcesses();
    QList<qint64> out;
    for (auto it = procs.constBegin(); it != procs.constEnd(); ++it)
        if (it.value().cmd.contains(needle) && isDescendant(procs, it.key()))
            out << it.key();
    return out;
}

// Processes that belong to this test run but may have been reparented out of the tree
// (Chromium's zygote can outlive its parent). Matched by kernel session id on POSIX, which
// still cannot reach another user's applications: they run in other sessions.
inline QList<qint64> sessionMatching(const QByteArray &needle)
{
    QList<qint64> out;
#if !defined(Q_OS_WIN)
    const auto mine = ::getsid(0);
    if (mine < 0)
        return out;
    for (const QString &e : QDir(QStringLiteral("/proc")).entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
        bool ok = false;
        const qint64 pid = e.toLongLong(&ok);
        if (!ok)
            continue;
        QFile cmd(QStringLiteral("/proc/%1/cmdline").arg(pid));
        if (!cmd.open(QIODevice::ReadOnly))
            continue;
        if (!cmd.readAll().contains(needle))
            continue;
        // /proc/<pid>/stat field 6 is the session id.
        QFile stat(QStringLiteral("/proc/%1/stat").arg(pid));
        if (!stat.open(QIODevice::ReadOnly))
            continue;
        const QByteArray st = stat.readAll();
        const QList<QByteArray> rest = st.mid(st.lastIndexOf(')') + 2).split(' ');
        if (rest.size() >= 4 && rest.at(3).toLongLong() == mine)
            out << pid;
    }
#endif
    return out;
}

inline QString describeDescendants()
{
    const QHash<qint64, ProcInfo> procs = snapshotProcesses();
    QStringList lines;
    lines << QStringLiteral("this process: %1; %2 processes visible").arg(QCoreApplication::applicationPid()).arg(procs.size());
    for (auto it = procs.constBegin(); it != procs.constEnd(); ++it)
        if (isDescendant(procs, it.key()))
            lines << QStringLiteral("  pid %1 ppid %2: %3").arg(it.key()).arg(it.value().parent)
                         .arg(QString::fromUtf8(QByteArray(it.value().cmd).replace('\0', ' ').left(160)));
    return lines.join(QLatin1Char('\n'));
}

} // namespace TestSupport
