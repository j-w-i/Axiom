//! `agent/*` handlers (protocol 0.3, ADR-0015). Thin: read what the service needs from the
//! store, call it, map its error.

use crate::app::App;
use axiom_agent::credentials::CredentialError;
use axiom_agent::gate::{GitAnswer, GitProtection, GitSource, GitUnknown, Policies};
use axiom_agent::providers::ProvidersError;
use axiom_agent::tools::ToolCtx;
use axiom_protocol::agent::{
    ApproveParams, ConfigureProviderParams, ModelsParams, ModelsResult, Policy, PolicyParams,
    PolicyResult, ProviderConfig, ToolCallParams, ToolCallResult, ToolsResult,
};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::{ErrorKind, RpcError};
use axiom_protocol::{WorkspaceId, WorkspacePath};
use axiom_rpc::{Notifier, RequestCtx};
use futures_util::future::BoxFuture;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant, SystemTime};

pub(crate) const PROVIDERS_KEY: &str = "agent.providers";

pub(crate) async fn provider_configs(app: &App) -> Result<Vec<ProviderConfig>, RpcError> {
    let v = app.store_call(|s| s.setting_get(PROVIDERS_KEY)).await?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

pub(crate) fn providers_error(e: ProvidersError) -> RpcError {
    match e {
        ProvidersError::NotFound(id) => RpcError::new(
            ErrorKind::NotFound,
            format!("No provider `{id}` is configured."),
        ),
        ProvidersError::Invalid(m) => RpcError::new(ErrorKind::InvalidParams, m),
        ProvidersError::Credentials(e @ CredentialError::Unavailable(_)) => {
            RpcError::new(ErrorKind::Unsupported, sentence(e.to_string()))
        }
        ProvidersError::Credentials(e @ CredentialError::Failed(_)) => {
            RpcError::new(ErrorKind::Io, sentence(e.to_string()))
        }
    }
}

fn sentence(mut s: String) -> String {
    if let Some(c) = s.get(..1) {
        let up = c.to_uppercase();
        s.replace_range(..1, &up);
    }
    if !s.ends_with('.') {
        s.push('.');
    }
    s
}

pub async fn models(app: Arc<App>, p: ModelsParams) -> Result<ModelsResult, RpcError> {
    let configs = provider_configs(&app).await?;
    Ok(ModelsResult {
        providers: app
            .agent
            .providers
            .status(&configs, p.provider_id.as_deref())
            .await,
    })
}

pub async fn configure_provider(
    app: Arc<App>,
    p: ConfigureProviderParams,
) -> Result<Empty, RpcError> {
    let configs = provider_configs(&app).await?;
    let next = app
        .agent
        .providers
        .configure(configs, p)
        .await
        .map_err(providers_error)?;
    let value = serde_json::to_value(next).map_err(|_| crate::app::internal())?;
    app.store_call(move |s| {
        s.settings_set(&BTreeMap::from([(PROVIDERS_KEY.to_string(), Some(value))]))
    })
    .await?;
    Ok(Empty {})
}

/// The tool registry offered in this workspace (built-in tools; MCP tools join per session).
pub async fn tools(app: Arc<App>, p: WorkspaceRef) -> Result<ToolsResult, RpcError> {
    app.workspaces.get(&p.workspace_id)?;
    Ok(ToolsResult {
        tools: app.agent.tools.infos(),
    })
}

pub(crate) const POLICY_KEY: &str = "agent.policy";

/// The user policy (settings), or the built-in one.
pub(crate) async fn user_policy(app: &App) -> Result<Policy, RpcError> {
    let v = app.store_call(|s| s.setting_get(POLICY_KEY)).await?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(axiom_agent::gate::policy::default_policy))
}

/// `[agent.policy]` in the workspace's `.axiom/project.toml` (stricter only), if any.
/// A malformed table is `invalid_params` with the reason, as for the project file's
/// `[environment]`.
pub(crate) async fn workspace_policy(
    app: &App,
    id: &WorkspaceId,
) -> Result<Option<Policy>, RpcError> {
    let ws = app.workspaces.get(id)?;
    let path = WorkspacePath(".axiom/project.toml".into());
    let text = match ws.read_text(&path).await {
        Ok(f) => f.text,
        Err(axiom_workspace::WsError::NotFound(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    parse_workspace_policy(&text)
}

pub(crate) fn parse_workspace_policy(text: &str) -> Result<Option<Policy>, RpcError> {
    axiom_agent::gate::policy::from_project_toml(text).map_err(|reason| {
        RpcError::new(
            ErrorKind::InvalidParams,
            "`.axiom/project.toml` has an invalid `[agent.policy]`.",
        )
        .with_details(serde_json::json!({ "reason": reason }))
    })
}

pub(crate) async fn policies(app: &App, id: &WorkspaceId) -> Result<Policies, RpcError> {
    let ws = app.workspaces.get(id)?;
    let watch = git_watch(&ws);
    Ok(Policies {
        user: user_policy(app).await?,
        workspace: workspace_policy(app, id).await?,
        protected: Vec::new(),
        git_unknown: false,
        root: Some(ws.root().to_path_buf()),
        git: Some(GitSource(Arc::new(WorkspaceGit { ws, watch }))),
    })
}

/// How long a walk for a workspace's repositories, and git's answer made with it, is reused:
/// a repository made since by the person (a clone) is seen within this time; one made by a
/// command the gate ran, at once ([`GitProtection::changed`]). A walk looks at every folder
/// (`git::repositories`), which takes long in a large workspace.
const WALK_FRESH: Duration = Duration::from_secs(30);

/// The [`GitWatch`] of each open workspace (review H7): one per workspace, shared by every
/// session's turns and every call from outside a session, so git is asked once for all of
/// them, and a command any of them runs makes all of them ask again. An entry goes with its
/// workspace (a workspace opened again gets a new one).
static GIT_WATCHES: Mutex<Vec<(Weak<axiom_workspace::Workspace>, Arc<GitWatch>)>> =
    Mutex::new(Vec::new());

/// The [`GitWatch`] of `ws`.
fn git_watch(ws: &Arc<axiom_workspace::Workspace>) -> Arc<GitWatch> {
    let mut w = GIT_WATCHES.lock().unwrap_or_else(|p| p.into_inner());
    w.retain(|(k, _)| k.strong_count() > 0);
    if let Some((_, g)) = w
        .iter()
        .find(|(k, _)| std::ptr::eq(k.as_ptr(), Arc::as_ptr(ws)))
    {
        return g.clone();
    }
    let g = Arc::new(GitWatch::default());
    w.push((Arc::downgrade(ws), g.clone()));
    g
}

/// The [`GitProtection`] a call's [`Policies`] hold: the workspace and its shared watch.
struct WorkspaceGit {
    ws: Arc<axiom_workspace::Workspace>,
    watch: Arc<GitWatch>,
}

/// git's answer for a workspace (reviews E2, G2–G4, G6, G7, H5, H7), asked for every write the
/// gate judges: reused only while every file it came from (each configuration file git reads
/// or an include names, each repository's `HEAD` and `.git`, the person's global configuration
/// files) has the same size and time, no command ran since in any session or call
/// ([`GitProtection::changed`] moves [`Self::generation`]: git's global configuration may be
/// new, a repository may have been made) and it is younger than [`WALK_FRESH`]. The walk for
/// the workspace's repositories is reused under the same two last conditions. An answer with
/// unknowns is not reused, unless it is that a value cannot be placed (which the files say).
#[derive(Default)]
struct GitWatch {
    /// Moved by every command that ran: an answer or a walk made before it is not used.
    generation: AtomicU64,
    walk: Mutex<Option<Walk>>,
    cache: Mutex<Option<Cached>>,
}

struct Walk {
    generation: u64,
    at: Instant,
    found: axiom_workspace::git::Repositories,
}

struct Cached {
    generation: u64,
    answer: GitAnswer,
    stamps: Vec<(PathBuf, Option<Stamp>)>,
    /// When the walk it used was made.
    walked: Instant,
}

/// A file's modification time, size and kind; `None` when it does not exist.
type Stamp = (Option<SystemTime>, u64, bool);

fn stamp(p: &Path) -> Option<Stamp> {
    let m = std::fs::symlink_metadata(p).ok()?;
    Some((m.modified().ok(), m.len(), m.is_dir()))
}

impl GitWatch {
    /// git's answer for `ws` now. Blocking; one at a time per workspace (a second waits and
    /// then reuses the first's answer).
    fn answer(&self, ws: &axiom_workspace::Workspace) -> GitAnswer {
        let mut c = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        let generation = self.generation.load(Ordering::SeqCst);
        if let Some(k) = c.as_ref() {
            if k.generation == generation
                && k.walked.elapsed() < WALK_FRESH
                && k.stamps.iter().all(|(p, s)| stamp(p) == *s)
            {
                return k.answer.clone();
            }
        }
        let (walked, found) = self.walk(ws.root(), generation);
        let (answer, files, keep) = git_answer(ws, found);
        // What git could not answer is asked again at the next call (it may be fixed).
        *c = keep.then(|| Cached {
            generation,
            answer: answer.clone(),
            stamps: files
                .into_iter()
                .map(|p| {
                    let s = stamp(&p);
                    (p, s)
                })
                .collect(),
            walked,
        });
        answer
    }

    /// The repositories in `root`, walked again when the last walk is older than
    /// [`WALK_FRESH`] or a command ran since (review H5: a clone or `git init` an approved
    /// command made is asked about at the next write). Blocking.
    fn walk(&self, root: &Path, generation: u64) -> (Instant, axiom_workspace::git::Repositories) {
        let mut w = self.walk.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(k) = w
            .as_ref()
            .filter(|k| k.generation == generation && k.at.elapsed() < WALK_FRESH)
        {
            return (k.at, k.found.clone());
        }
        let at = Instant::now();
        let found = axiom_workspace::git::repositories(root);
        *w = Some(Walk {
            generation,
            at,
            found: found.clone(),
        });
        (at, found)
    }
}

impl GitProtection for WorkspaceGit {
    fn current(&self) -> BoxFuture<'_, GitAnswer> {
        let ws = self.ws.clone();
        let watch = self.watch.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || watch.answer(&ws))
                .await
                .unwrap_or(GitAnswer {
                    protected: Vec::new(),
                    unknown: GitUnknown::Everything,
                })
        })
    }

    fn changed(&self) {
        self.watch.generation.fetch_add(1, Ordering::SeqCst);
    }
}

/// The files git reads the person's global configuration from, whether they exist or not
/// (`$XDG_CONFIG_HOME/git/config` or `~/.config/git/config`, and `~/.gitconfig`): one made
/// since an answer was computed changes it.
fn global_config_files() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|x| !x.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".config")));
    xdg.map(|x| x.join("git").join("config"))
        .into_iter()
        .chain(home.map(|h| h.join(".gitconfig")))
        .collect()
}

/// What git reads for every repository in the workspace, where it is inside the workspace, as
/// workspace paths through any symlink (reviews E2, G2–G4): writes there always ask. Asked of
/// git itself ([`axiom_workspace::git::config_files`]) for the root and for each repository
/// `found` inside it ([`axiom_workspace::git::repositories`]: nested clones, submodules, bare
/// repositories, a root that is no repository but holds some): the hooks directories (the
/// hook names alone where one is a repository's top, `core.hooksPath = .`), the git directory
/// and the common one, every configuration file git reads or an include names, and every
/// program the configuration names (`core.fsmonitor`, filters, `core.sshCommand`, …). With the
/// files the answer came from, to tell when it changes, and whether it may be reused. Where
/// git cannot answer for a repository that exists, or a program value there cannot be placed
/// (review rev6: shell syntax, `git -c` in an alias), every write asks
/// ([`GitUnknown::Everything`]); where it cannot answer outside any repository, or the walk was
/// cut short, names that may be git's ask ([`GitUnknown::Names`]). Blocking.
fn git_answer(
    ws: &axiom_workspace::Workspace,
    found: axiom_workspace::git::Repositories,
) -> (GitAnswer, Vec<PathBuf>, bool) {
    use axiom_workspace::git;
    let root = ws.root();
    let mut unknown = if found.complete {
        GitUnknown::No
    } else {
        tracing::warn!("not every repository in the workspace could be looked for");
        GitUnknown::Names
    };
    let in_a_repository = !found.dirs.is_empty()
        || root
            .ancestors()
            .skip(1)
            .any(|a| std::fs::symlink_metadata(a.join(".git")).is_ok());
    let mut dirs = vec![root.to_path_buf()];
    for d in found.dirs {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    let tops: Vec<WorkspacePath> = dirs
        .iter()
        .filter_map(|d| ws.workspace_path_of(d))
        .collect();
    let mut out: Vec<WorkspacePath> = Vec::new();
    let mut files: Vec<PathBuf> = global_config_files();
    let mut add = |p: WorkspacePath| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    let everything = GitAnswer {
        protected: Vec::new(),
        unknown: GitUnknown::Everything,
    };
    for (i, dir) in dirs.iter().enumerate() {
        files.push(dir.join(".git"));
        let c = match git::config_files(dir) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("cannot ask git about {}: {e}", dir.display());
                if i > 0 || in_a_repository {
                    return (everything, files, false);
                }
                unknown = GitUnknown::Names;
                continue;
            }
        };
        files.extend(c.git_dirs.iter().map(|g| g.join("HEAD")));
        if !c.unplaced.is_empty() {
            // Its files say when it changes: kept until one does.
            tracing::warn!(
                "git's configuration for {} names programs that cannot be placed ({}): every \
                 write asks",
                dir.display(),
                c.unplaced.join(", ")
            );
            files.extend(c.files);
            return (everything, files, true);
        }
        for h in &c.hooks {
            let Some(hooks) = ws.workspace_path_of(h) else {
                continue;
            };
            if hooks.0.is_empty() || tops.contains(&hooks) {
                for n in axiom_agent::gate::HOOK_NAMES {
                    add(WorkspacePath(if hooks.0.is_empty() {
                        (*n).to_string()
                    } else {
                        format!("{}/{n}", hooks.0)
                    }));
                }
            } else {
                add(hooks);
            }
        }
        for p in c.git_dirs.iter().chain(&c.files) {
            if let Some(p) = ws.workspace_path_of(p).filter(|p| !p.0.is_empty()) {
                add(p);
            }
        }
        // A word that names a repository's own folder (`cd ./`) is no program in it.
        for p in &c.programs {
            if let Some(p) = ws.workspace_path_of(p).filter(|p| !tops.contains(p)) {
                add(p);
            }
        }
        files.extend(c.files);
    }
    let keep = unknown == GitUnknown::No;
    (
        GitAnswer {
            protected: out,
            unknown,
        },
        files,
        keep,
    )
}

pub(crate) fn audit_path(app: &App, root: &std::path::Path) -> std::path::PathBuf {
    app.env
        .data_dir
        .join("audit")
        .join(format!("{}.jsonl", axiom_agent::workspace_key(root)))
}

pub async fn get_policy(app: Arc<App>, p: PolicyParams) -> Result<PolicyResult, RpcError> {
    Ok(PolicyResult {
        user: user_policy(&app).await?,
        workspace: match &p.workspace_id {
            Some(id) => workspace_policy(&app, id).await?,
            None => None,
        },
    })
}

pub async fn set_policy(app: Arc<App>, p: Policy) -> Result<Empty, RpcError> {
    let v = serde_json::to_value(p).map_err(|_| crate::app::internal())?;
    app.store_call(move |s| s.settings_set(&BTreeMap::from([(POLICY_KEY.to_string(), Some(v))])))
        .await?;
    Ok(Empty {})
}

pub async fn approve(app: Arc<App>, p: ApproveParams) -> Result<Empty, RpcError> {
    app.agent
        .gate
        .approve(&p.request_id, p.decision)
        .await
        .map_err(|_| {
            RpcError::new(
                ErrorKind::NotFound,
                "No approval with that id is pending (it may have been answered already).",
            )
        })?;
    Ok(Empty {})
}

/// The connections that made tool calls from outside a session, numbered.
static CONNECTIONS: Mutex<Vec<(Notifier, u64)>> = Mutex::new(Vec::new());

/// The principal a call from outside a session runs under: the name it declared, made this
/// connection's own (`mcp:<client>#c<n>`). Two clients that both call themselves
/// `claude-code` share no grants, and a connection's grants end with it (AUD-009, AUD-109).
fn connection_principal(app: &App, declared: &str, notifier: &Notifier) -> String {
    let mut c = CONNECTIONS.lock().unwrap_or_else(|p| p.into_inner());
    let closed: Vec<String> = c
        .iter()
        .filter(|(n, _)| n.is_closed())
        .map(|(_, id)| format!("#c{id}"))
        .collect();
    if !closed.is_empty() {
        c.retain(|(n, _)| !n.is_closed());
        app.agent
            .gate
            .retain_grants(|p| !closed.iter().any(|tag| p.ends_with(tag.as_str())));
    }
    let id = match c.iter().find(|(n, _)| n.same_connection(notifier)) {
        Some((_, id)) => *id,
        None => {
            let id = c.iter().map(|(_, id)| id + 1).max().unwrap_or(1);
            c.push((notifier.clone(), id));
            id
        }
    };
    format!("{declared}#c{id}")
}

/// One tool call from outside a session (the MCP front end, the CLI). The principal must say
/// where it comes from (`mcp:<client>` or `cli`), so it can never use a session's grants.
pub async fn tool_call(
    app: Arc<App>,
    p: ToolCallParams,
    ctx: RequestCtx,
) -> Result<ToolCallResult, RpcError> {
    let ok_principal = p.principal == "cli"
        || p.principal
            .strip_prefix("mcp:")
            .is_some_and(|n| !n.is_empty() && n.len() <= 64);
    if !ok_principal {
        return Err(RpcError::new(
            ErrorKind::InvalidParams,
            "The principal must be `cli` or `mcp:<client name>`.",
        ));
    }
    let ws = app.workspaces.get(&p.workspace_id)?;
    // Checkpoints stay under the declared name (one key per client, as before).
    let key: String = format!(
        "ext-{}",
        p.principal
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            })
            .collect::<String>()
    );
    let tctx = ToolCtx {
        workspace_id: p.workspace_id.clone(),
        principal: connection_principal(&app, &p.principal, &ctx.notifier),
        cancel: ctx.cancel.clone(),
    };
    let policies = policies(&app, &p.workspace_id).await?;
    let audit = audit_path(&app, ws.root());
    let shadow = crate::handlers::sessions::shadow(&app, ws.root());
    let hook = ExternalCheckpoint {
        shadow: &shadow,
        key: &key,
        principal: &tctx.principal,
    };
    let out = axiom_agent::gate::call_tool(
        &app.agent.tools,
        &app.agent.gate,
        &tctx,
        None,
        &p.tool,
        p.input,
        &policies,
        &hook,
        Some(&audit),
    )
    .await;
    if out.effect != axiom_protocol::agent::Effect::Read && !out.is_error {
        if let Err(e) = shadow.record_post(&key).await {
            tracing::warn!("cannot record the post-state: {e}");
        }
    }
    if let Some(r) = out.refusal {
        return Err(RpcError::new(r.kind, r.message));
    }
    Ok(ToolCallResult {
        output: out.output.output,
        text: out.output.text,
        is_error: out.is_error,
        truncated: out.output.truncated,
    })
}

/// A checkpoint before every writing call from outside a session (every write is preceded by
/// one, whoever asks; ADR-0015 §5).
struct ExternalCheckpoint<'a> {
    shadow: &'a axiom_agent::checkpoint::Shadow,
    key: &'a str,
    principal: &'a str,
}

impl axiom_agent::gate::BeforeExecute for ExternalCheckpoint<'_> {
    fn before<'b>(
        &'b self,
        p: &'b axiom_agent::tools::Prepared,
    ) -> futures_util::future::BoxFuture<'b, Result<(), axiom_agent::tools::ToolError>> {
        Box::pin(async move {
            if p.effect == axiom_protocol::agent::Effect::Read {
                return Ok(());
            }
            self.shadow
                .create(
                    self.key,
                    &format!("before {} ({})", p.name, self.principal),
                    0,
                )
                .await
                .map(|_| ())
                .map_err(axiom_agent::agent_loop::checkpoint_error)
        })
    }
}

/// `agent/installAgent` (ADR-0020 §6): `npm install --prefix <data dir>/agents <package>`,
/// bounded to ten minutes; the agent's program is then found there.
pub async fn install_agent(
    app: Arc<App>,
    p: axiom_protocol::agent::InstallAgentParams,
) -> Result<axiom_protocol::agent::InstallAgentResult, RpcError> {
    use axiom_agent::acp;
    let package = acp::builtin_package(&p.provider_id).ok_or_else(|| {
        RpcError::new(
            ErrorKind::NotFound,
            format!("`{}` is not an agent Axiom can install.", p.provider_id),
        )
    })?;
    let npm = acp::find_program("npm").ok_or_else(|| {
        RpcError::new(
            ErrorKind::PrerequisiteMissing,
            "Installing an agent needs npm (Node.js). Install Node.js, then try again.",
        )
    })?;
    let prefix = app.env.data_dir.join("agents");
    tokio::fs::create_dir_all(&prefix).await.map_err(|e| {
        RpcError::new(
            ErrorKind::Io,
            format!("Could not create {}: {e}", prefix.display()),
        )
    })?;
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(600),
        tokio::process::Command::new(&npm)
            .args(["install", "--no-fund", "--no-audit", "--prefix"])
            .arg(&prefix)
            .arg(package)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| RpcError::new(ErrorKind::Timeout, "npm did not finish within 10 minutes."))?
    .map_err(|e| RpcError::new(ErrorKind::SpawnFailed, format!("Could not run npm: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = err.lines().rev().take(8).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(RpcError::new(
            ErrorKind::Io,
            format!("npm could not install {package}:\n{}", tail.join("\n")),
        ));
    }
    let configs = provider_configs(&app).await?;
    let program = app
        .agent
        .providers
        .config(&configs, &p.provider_id)
        .and_then(|c| c.command.first().cloned())
        .and_then(|cmd| acp::find_program(&cmd))
        .ok_or_else(|| {
            RpcError::new(
                ErrorKind::Io,
                format!(
                    "{package} was installed, but its program was not found in {}.",
                    prefix.display()
                ),
            )
        })?;
    Ok(axiom_protocol::agent::InstallAgentResult {
        program: program.to_string_lossy().into_owned(),
    })
}
