use crate::error::WsError;
use crate::workspace::Workspace;
use axiom_protocol::workspace::OpenResult;
use axiom_protocol::WorkspaceId;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    next: u64,
    by_id: HashMap<WorkspaceId, Arc<Workspace>>,
    by_root: HashMap<PathBuf, WorkspaceId>,
}

/// Registry of open workspaces. Cheap to clone.
#[derive(Clone, Default)]
pub struct Workspaces {
    state: Arc<Mutex<State>>,
}

impl Workspaces {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Canonicalizes `path`; idempotent per canonical root. Ids are "w1", "w2", …
    pub async fn open(&self, path: &str) -> Result<OpenResult, WsError> {
        let p = Path::new(path);
        if !p.is_absolute() {
            return Err(WsError::InvalidPath(
                "The workspace path must be absolute.".into(),
            ));
        }
        let owned = p.to_path_buf();
        let root = tokio::task::spawn_blocking(move || -> Result<PathBuf, WsError> {
            let root = dunce::canonicalize(&owned).map_err(|e| WsError::from_io(e, ""))?;
            if !root.is_dir() {
                return Err(WsError::InvalidPath(
                    "The workspace path is not a directory.".into(),
                ));
            }
            Ok(root)
        })
        .await
        .map_err(|e| WsError::Io(std::io::Error::other(e)))??;

        let mut st = self.state();
        let ws = if let Some(id) = st.by_root.get(&root).cloned() {
            st.by_id[&id].clone()
        } else {
            st.next += 1;
            let id = WorkspaceId(format!("w{}", st.next));
            let ws = Arc::new(Workspace::new(id.clone(), root.clone()));
            st.by_root.insert(root, id.clone());
            st.by_id.insert(id, ws.clone());
            ws
        };
        Ok(OpenResult {
            workspace_id: ws.id().clone(),
            root: ws.root().to_string_lossy().into_owned(),
            name: ws.name().to_string(),
        })
    }

    pub async fn close(&self, id: &WorkspaceId) -> Result<(), WsError> {
        let mut st = self.state();
        match st.by_id.remove(id) {
            Some(ws) => {
                st.by_root.remove(ws.root());
                Ok(())
            }
            None => Err(WsError::WorkspaceNotOpen(id.0.clone())),
        }
    }

    pub fn get(&self, id: &WorkspaceId) -> Result<Arc<Workspace>, WsError> {
        self.state()
            .by_id
            .get(id)
            .cloned()
            .ok_or_else(|| WsError::WorkspaceNotOpen(id.0.clone()))
    }
}
