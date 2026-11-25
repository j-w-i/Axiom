//! Where the host lives. Everything is inside the data directory, private to the user.

use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Paths {
    pub data: PathBuf,
}

impl Paths {
    pub fn new(data: PathBuf) -> Self {
        Self { data }
    }

    /// `AXIOM_DATA`, else the platform's place for application data.
    pub fn discover() -> Option<Self> {
        let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let data = var("AXIOM_DATA").or_else(|| {
            if cfg!(target_os = "macos") {
                var("HOME").map(|h| h.join("Library/Application Support/Axiom"))
            } else if cfg!(windows) {
                var("LOCALAPPDATA").map(|h| h.join("Axiom"))
            } else {
                var("XDG_DATA_HOME")
                    .or_else(|| var("HOME").map(|h| h.join(".local/share")))
                    .map(|h| h.join("axiom"))
            }
        })?;
        Some(Self { data })
    }

    /// Where agents' temporary folders go: the platform's place for what may be lost. `None`: beside
    /// the data directory, which was chosen.
    pub fn caches() -> Option<PathBuf> {
        let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        if var("AXIOM_DATA").is_some() {
            None
        } else if cfg!(target_os = "macos") {
            var("HOME").map(|h| h.join("Library/Caches/Axiom/scratch"))
        } else if cfg!(windows) {
            var("LOCALAPPDATA").map(|h| h.join("Axiom-scratch"))
        } else {
            var("XDG_CACHE_HOME")
                .or_else(|| var("HOME").map(|h| h.join(".cache")))
                .map(|h| h.join("axiom/scratch"))
        }
    }

    pub fn run(&self) -> PathBuf {
        self.data.join("run")
    }
    pub fn journal(&self) -> PathBuf {
        self.data.join("journal.db")
    }
    pub fn token(&self) -> PathBuf {
        self.run().join("token")
    }
    /// Where the socket is.
    pub fn endpoint_file(&self) -> PathBuf {
        self.run().join("endpoint")
    }
    /// Beside the journal, because that is what it protects: `run` may be removed at any time.
    pub fn lock(&self) -> PathBuf {
        self.data.join("journal.lock")
    }

    /// What hosts before schema 2 hold. This one holds it too, so that they exclude each other.
    pub fn old_lock(&self) -> PathBuf {
        self.run().join("lock")
    }

    /// The socket of the host that serves, or last served, this directory.
    pub fn endpoint(&self) -> io::Result<String> {
        std::fs::read_to_string(self.endpoint_file())
    }

    fn hash(&self) -> u64 {
        // FNV-1a: stable between builds, and only used to tell names apart.
        self.data.as_os_str().as_encoded_bytes().iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }
}

/// Which file a name stands for, to see later whether it still does. The file is held open all that
/// time, so its number is not given to another.
#[derive(Debug)]
pub struct Same {
    path: PathBuf,
    id: Id,
}

impl Same {
    pub fn of(path: &Path) -> io::Result<Self> {
        Ok(Self { path: path.into(), id: id(path)? })
    }
    pub fn still(&self) -> bool {
        id(&self.path).is_ok_and(|i| i == self.id)
    }
}

type Id = (u64, u64);
#[cfg(unix)]
fn id(path: &Path) -> io::Result<Id> {
    Ok(id_of(&std::fs::symlink_metadata(path)?))
}
#[cfg(unix)]
fn id_of(m: &std::fs::Metadata) -> Id {
    use std::os::unix::fs::MetadataExt;
    (m.dev(), m.ino())
}

/// Files that are open are neither removed nor replaced there, and it is not said of every kind of
/// file (a socket) what it is: whether there is one has to do.
#[cfg(not(unix))]
fn id(path: &Path) -> io::Result<Id> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(e),
        _ => Ok((0, 0)),
    }
}

/// Held for the life of the host; the system releases it if the process dies.
#[derive(Debug)]
pub struct Lock {
    _file: std::fs::File,
    same: Same,
}

impl Lock {
    /// `None` if somebody else holds it.
    pub fn take(path: &Path) -> io::Result<Option<Self>> {
        let file = private_options().write(true).create(true).truncate(false).open(path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
            Err(std::fs::TryLockError::Error(e)) => return Err(e),
        }
        let same = Same::of(path)?;
        #[cfg(unix)]
        if same.id != id_of(&file.metadata()?) {
            return Err(io::Error::other(format!("{} is being replaced", path.display())));
        }
        Ok(Some(Self { _file: file, same }))
    }
    pub fn still(&self) -> bool {
        self.same.still()
    }
}

/// The place of a socket, emptied when dropped.
pub struct Place {
    pub path: String,
    /// Made for this socket alone.
    dir: Option<PathBuf>,
    /// The socket itself, once it is there.
    socket: Option<Same>,
}

impl Place {
    /// Unix paths are limited to about 100 bytes, so a long data directory gets a short private
    /// directory instead. Its name is random: nobody can make it first, and clients read it.
    /// The caller holds the lock, so what the last host of this directory left is stale.
    pub fn new(paths: &Paths) -> io::Result<Self> {
        // The name says whose it is: a copied data directory must not clean up after its original.
        let ours = format!("axiom-{:016x}-", paths.hash());
        if let Ok(path) = paths.endpoint() {
            let dir = Path::new(&path).parent();
            let name = dir.and_then(|d| d.file_name()).and_then(|n| n.to_str());
            if name.is_some_and(|n| n.starts_with(&ours)) {
                drop(Self { dir: dir.map(Into::into), path, socket: None });
            }
        }
        let near = paths.run().join("host.sock");
        let (path, dir) = if near.as_os_str().len() <= 100 {
            (near, None)
        } else {
            let mut b = [0u8; 4];
            axiom_proto::random(&mut b).map_err(io::Error::other)?;
            let temp = std::env::temp_dir();
            make_dir(&temp, true)?;
            let dir = temp.join(format!("{ours}{:08x}", u32::from_ne_bytes(b)));
            make_dir(&dir, false)?;
            (dir.join("host.sock"), Some(dir))
        };
        let path = path
            .into_os_string()
            .into_string()
            .map_err(|_| io::Error::other("the socket's directory has a name that is not UTF-8"))?;
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(Self { path, dir, socket: None }),
        }
    }

    pub fn made(&mut self) -> io::Result<()> {
        self.socket = Some(Same::of(Path::new(&self.path))?);
        Ok(())
    }
    pub fn still(&self) -> bool {
        self.socket.as_ref().is_some_and(Same::still)
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        // Somebody else's socket of that name stays.
        if self.socket.as_ref().is_none_or(Same::still) {
            let _ = std::fs::remove_file(&self.path);
        }
        if let Some(d) = &self.dir {
            let _ = std::fs::remove_dir(d);
        }
    }
}

fn make_dir(p: &Path, parents: bool) -> io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(parents);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
    b.create(p)
}

/// Creates a directory only this user can enter, or checks that an existing one is so.
pub fn private_dir(p: &Path) -> io::Result<()> {
    make_dir(p, true)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let m = std::fs::symlink_metadata(p)?;
        if !m.is_dir() {
            return Err(io::Error::other(format!("{} is not a directory", p.display())));
        }
        if m.permissions().mode() & 0o077 != 0 {
            // Succeeds only for the owner, so a directory planted by someone else fails here.
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700))?;
        }
        let again = std::fs::symlink_metadata(p)?;
        if again.ino() != m.ino() || again.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::other(format!("{} is not private", p.display())));
        }
    }
    Ok(())
}

/// Replaces `p` with `text`, readable by this user only.
pub fn private_file(p: &Path, text: &str) -> io::Result<()> {
    let tmp = p.with_extension("new");
    let _ = std::fs::remove_file(&tmp);
    let mut f = private_options().write(true).create_new(true).open(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(tmp, p)
}

fn private_options() -> std::fs::OpenOptions {
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut o = std::fs::OpenOptions::new();
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o
}
