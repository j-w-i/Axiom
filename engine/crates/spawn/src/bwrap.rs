//! Linux: bubblewrap. A file system of its own in which only the profile's places exist, no network,
//! its own processes, and a filter for the calls that lead out of such rooms. What is protected is
//! mounted read-only where it exists; a name that appears while the process runs is looked for
//! afterwards (`made`), for a mount can only cover what is there.

use std::{
    ffi::OsString,
    io::Write,
    os::fd::OwnedFd,
    path::{Path, PathBuf},
};

use axiom_policy::{Place, Profile, paths};

use crate::{Job, Real, SpawnError, seccomp, tree::FILTER};

/// Of the system's own: never from a folder an agent can write to.
const BWRAP: &[&str] = &["/usr/bin/bwrap", "/bin/bwrap", "/usr/local/bin/bwrap"];

/// More names than this are not looked through: then nothing is started.
const MOST: usize = 400_000;

pub fn wrap(job: &Job, real: &Real) -> Result<(OsString, Vec<OsString>), SpawnError> {
    let no = |why: &str| SpawnError::NoSandbox(why.into());
    let bwrap = BWRAP
        .iter()
        .find(|p| Path::new(p).is_file())
        .ok_or_else(|| no("bubblewrap is not installed"))?;
    let mut a: Vec<OsString> = Vec::new();
    let mut add =
        |xs: &[&dyn AsRef<std::ffi::OsStr>]| a.extend(xs.iter().map(|x| x.as_ref().to_os_string()));
    add(&[
        &"--unshare-user",
        &"--unshare-pid",
        &"--unshare-net",
        &"--unshare-ipc",
        &"--unshare-uts",
        &"--unshare-cgroup-try",
    ]);
    add(&[
        &"--die-with-parent",
        &"--disable-userns",
        &"--cap-drop",
        &"ALL",
        &"--hostname",
        &"sandbox",
    ]);
    add(&[&"--seccomp", &FILTER.to_string()]);
    add(&[&"--dev", &"/dev", &"--proc", &"/proc", &"--tmpfs", &"/tmp"]);
    // Outermost first: what is inside another is mounted over it.
    let mut read = real.read.clone();
    read.sort();
    for p in &read {
        add(&[&"--ro-bind", &p.as_path(), &p.as_path()]);
    }
    let mut write = real.write.clone();
    write.sort();
    for p in &write {
        add(&[&"--bind", &p.as_path(), &p.as_path()]);
    }
    for root in &write {
        for p in names(root, MOST).ok_or_else(too_many)? {
            // A git folder a person's commit writes is written, but for what is kept in it.
            if real.vcs.iter().any(|v| v.as_path() == p) {
                continue;
            }
            add(&[&"--ro-bind", &p, &p]);
        }
    }
    for k in &real.kept {
        if k.as_path().exists() {
            add(&[&"--ro-bind", &k.as_path(), &k.as_path()]);
        }
    }
    // Links of the system's old names: /bin -> usr/bin.
    for l in ["/bin", "/sbin", "/lib", "/lib64", "/lib32"] {
        if let Ok(to) = std::fs::read_link(l) {
            add(&[&"--symlink", &to, &l]);
        }
    }
    for h in &real.hidden {
        if !real.read.iter().chain(&real.write).any(|r| r.covers(h)) {
            continue;
        }
        match std::fs::symlink_metadata(h.as_path()) {
            Ok(m) if m.is_dir() => add(&[&"--tmpfs", &h.as_path(), &"--remount-ro", &h.as_path()]),
            Ok(m) if m.is_file() => add(&[&"--ro-bind", &"/dev/null", &h.as_path()]),
            // A link is hidden where it leads: that is in the list too.
            Ok(_) | Err(_) => {}
        }
    }
    // The folders that only lead to what is mounted: nothing is kept in them.
    add(&[&"--remount-ro", &"/"]);
    add(&[&"--chdir", &job.cwd.as_path(), &"--", &job.program.as_path()]);
    a.extend(job.args.iter().map(OsString::from));
    Ok((OsString::from(*bwrap), a))
}

/// The calls that are refused, to be read by bubblewrap: less than a pipe holds.
pub fn filter() -> Result<OwnedFd, SpawnError> {
    let calls = seccomp::here()
        .ok_or_else(|| SpawnError::NoSandbox("there is no filter for this processor".into()))?;
    let (read, mut write) = std::io::pipe().map_err(SpawnError::Start)?;
    write.write_all(&seccomp::program(calls)).map_err(SpawnError::Start)?;
    Ok(read.into())
}

/// What is protected below `root` and exists; nothing if there is too much to look through. Not
/// followed into links, and not into what is found.
pub fn names(root: &Place, most: usize) -> Option<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut todo = vec![root.as_path().to_path_buf()];
    let mut seen = 0;
    while let Some(d) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            seen += 1;
            if seen > most {
                return None;
            }
            let name = e.file_name();
            if name.to_str().is_some_and(|n| paths::protected_name(n).is_some()) {
                found.push(e.path());
            } else if e.file_type().is_ok_and(|t| t.is_dir()) {
                todo.push(e.path());
            }
        }
    }
    Some(found)
}

fn too_many() -> SpawnError {
    SpawnError::NoSandbox("the folder has too many files to protect".into())
}

/// The profile's writable folders with the protected names in them.
pub fn before(p: &Profile) -> Result<Vec<(Place, Vec<PathBuf>)>, SpawnError> {
    p.write()
        .roots()
        .iter()
        .map(|r| {
            let r = crate::real_place(r)?;
            let n = names(&r, MOST).ok_or_else(too_many)?;
            Ok((r, n))
        })
        .collect()
}

/// The protected names there are now and were not. More than can be looked through is said as the
/// folder itself.
pub fn made(before: &[(Place, Vec<PathBuf>)]) -> Vec<PathBuf> {
    let new = before.iter().flat_map(|(root, old)| match names(root, MOST) {
        Some(now) => now.into_iter().filter(|p| !old.contains(p)).collect(),
        None => vec![root.as_path().to_path_buf()],
    });
    new.collect()
}
