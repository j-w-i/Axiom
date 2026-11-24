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

    pub fn run(&self) -> PathBuf {
        self.data.join("run")
    }
    pub fn journal(&self) -> PathBuf {
        self.data.join("journal.db")
    }
    pub fn token(&self) -> PathBuf {
        self.run().join("token")
    }
    /// Where the socket is, for clients that do not compute it.
    pub fn endpoint_file(&self) -> PathBuf {
        self.run().join("endpoint")
    }
    pub fn lock(&self) -> PathBuf {
        self.run().join("lock")
    }

    /// The socket. Unix paths are limited to about 100 bytes, so a long data directory gets a
    /// short private directory instead; its name is kept in `run/endpoint`.
    pub fn endpoint(&self) -> io::Result<String> {
        let near = self.run().join("host.sock");
        if near.as_os_str().len() <= 100 {
            return near
                .into_os_string()
                .into_string()
                .map_err(|_| io::Error::other("the data directory's name is not UTF-8"));
        }
        let dir = std::env::temp_dir().join(format!("axiom-{:016x}", self.hash()));
        private_dir(&dir)?;
        dir.join("host.sock")
            .into_os_string()
            .into_string()
            .map_err(|_| io::Error::other("the temporary directory's name is not UTF-8"))
    }

    fn hash(&self) -> u64 {
        // FNV-1a: stable between builds, and only used to make a distinct name.
        self.data.as_os_str().as_encoded_bytes().iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }
}

/// Creates a directory only this user can enter, or checks that an existing one is so.
pub fn private_dir(p: &Path) -> io::Result<()> {
    std::fs::create_dir_all(p)?;
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
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let mut f = o.open(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(tmp, p)
}
