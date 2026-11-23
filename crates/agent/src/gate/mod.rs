//! The permission gate (`docs/protocols/agents.md` §Execution pipeline, §Permission policy;
//! M3-GATE-001). One gate per daemon, in front of every tool call from every principal.
//!
//! [`call_tool`] is the whole pipeline: prepare → decide → (ask) → before-execute hook (the
//! checkpoint, M3-CKPT-001/LOOP-001) → execute → audit. It is the only code that holds an
//! [`Allowed`] token.

pub mod policy;

use crate::tools::{Allowed, Prepared, Registry, ToolCtx, ToolError, ToolOutput};
use axiom_protocol::agent::{
    ApprovalDecision, ApprovalRequest, ApprovalResolution, Decision, Effect, Policy, PolicyRule,
    SessionId,
};
use axiom_protocol::{ErrorKind, WorkspaceId, WorkspacePath};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

/// Where approval requests go: the connections that declared `capabilities.approvals`.
pub trait Approvers: Send + Sync {
    /// Sends the request to every approving connection; returns how many received it. The gate
    /// listens for the answer while this runs, and may drop it once the request is answered.
    fn request<'a>(&'a self, req: &'a ApprovalRequest) -> BoxFuture<'a, usize>;
    /// Tells every approving connection the request is settled. Must not wait on a connection
    /// that is not reading: the reply to `agent/approve` and the gated call wait for it.
    fn resolved<'a>(&'a self, res: &'a ApprovalResolution) -> BoxFuture<'a, ()>;
}

/// The policies that apply to a call.
#[derive(Debug, Clone)]
pub struct Policies {
    pub user: Policy,
    /// `.axiom/project.toml` `[agent.policy]`: stricter only.
    pub workspace: Option<Policy>,
    /// More paths that are protected as [`protected_path`]s are, from what git reads for the
    /// workspace's repository, where they are inside the workspace (review E2): the hooks
    /// directory git uses (`core.hooksPath` wherever it is set), the git directory when it is
    /// not `.git`, every configuration file git reads and every file a configuration includes.
    /// Workspace paths, through any symlink. A path protects itself and what is under it.
    pub protected: Vec<WorkspacePath>,
    /// Git could not be asked what it reads (no `git`, an error, a timeout): every name that
    /// may be git's configuration or hooks is protected instead ([`git_config_like`]).
    pub git_unknown: bool,
    /// The workspace root, to tell whether a `<name>.git` directory on a written path is a
    /// bare repository, or a written git entry would make a folder a git directory
    /// ([`Policies::protects_any`]). `None`: every such `<name>.git` directory is taken to be a
    /// repository; other folders are not looked at.
    pub root: Option<PathBuf>,
    /// git's answer, asked again for each call (review G6), beside [`Self::protected`] and
    /// [`Self::git_unknown`]: the daemon's, which follows configuration changed mid-turn.
    pub git: Option<GitSource>,
}

/// What git's configuration makes protected in a workspace now (reviews E2, G2–G4, G6, G7):
/// asked by the gate for every write, so a change made mid-turn (an approved command, the
/// person's own edit) counts from the next call.
pub trait GitProtection: Send + Sync {
    /// The answer for now (cheap when nothing changed).
    fn current(&self) -> BoxFuture<'_, GitAnswer>;
    /// A call ran that may have changed git's configuration or added a repository unseen (a
    /// command): the next answer is computed afresh.
    fn changed(&self);
}

/// A [`GitProtection`], shareable in [`Policies`].
#[derive(Clone)]
pub struct GitSource(pub Arc<dyn GitProtection>);

impl std::fmt::Debug for GitSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GitSource")
    }
}

/// What git's configuration protects, as workspace paths (each protects itself and what is
/// under it), and how much git could not say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitAnswer {
    pub protected: Vec<WorkspacePath>,
    pub unknown: GitUnknown,
}

/// How much of git's configuration is unknown (review G7).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GitUnknown {
    /// git answered for every repository in the workspace.
    #[default]
    No,
    /// Not every repository could be found (a limit on the walk): what was found is protected
    /// as git answered, and every name that may be git's is protected too
    /// ([`git_config_like`]).
    Names,
    /// git could not answer for a repository that exists here (no `git`, too old, a
    /// configuration it refuses, a timeout), or its configuration names a program in a way
    /// that cannot be placed (shell syntax, `git -c` in an alias; review rev6): any file may be
    /// an include, a hooks folder or a program, so every write asks.
    Everything,
}

/// Why a write that the policy allows asks: a protected path.
const PROTECTED: &str = "protected: Axiom configuration, git's directories, configuration, \
attributes and hooks, and the programs git's configuration names, always ask";
/// Why every write asks while git cannot say what it reads (review G7), or names a program in
/// a way that cannot be placed (review rev6).
const GIT_UNKNOWN: &str = "protected: Axiom cannot tell what git's configuration runs in this \
workspace (git 2.31 or later is needed, its configuration must be valid, and a program it \
names must be plain commands: no `$`, backquote, subshell, function, glob, `cd`, `-C` or `-c`), \
so every write asks";

impl Policies {
    /// Whether writing `p` always asks, by its name alone: a [`protected_path`], under
    /// [`Self::protected`], or [`git_config_like`] while git could not be asked.
    pub fn protects(&self, p: &WorkspacePath) -> bool {
        protected_path(p)
            || self.protected.iter().any(|d| within(p, d))
            || (self.git_unknown && git_config_like(p))
    }

    /// Whether writing any of `paths` always asks ([`Self::protection`]).
    pub async fn protects_any(&self, paths: &[WorkspacePath]) -> bool {
        self.protection(paths).await.is_some()
    }

    /// Why writing any of `paths` always asks, if it does: [`Self::protects`]; git's answer
    /// now ([`Self::git`]); a path inside a `<name>.git` directory that is a bare repository
    /// on disk ([`bare_repository`]); or one of git's own entries written into a folder that
    /// already holds one that makes a git directory ([`makes_git_dir`], review G5). The disk
    /// is looked at off the async thread, only for paths with such names.
    pub async fn protection(&self, paths: &[WorkspacePath]) -> Option<&'static str> {
        if paths.is_empty() {
            return None;
        }
        let answer = match &self.git {
            Some(g) => g.0.current().await,
            None => GitAnswer::default(),
        };
        if answer.unknown == GitUnknown::Everything {
            return Some(GIT_UNKNOWN);
        }
        let names = self.git_unknown || answer.unknown == GitUnknown::Names;
        if paths.iter().any(|p| {
            self.protects(p)
                || answer.protected.iter().any(|d| within(p, d))
                || (names && git_config_like(p))
        }) {
            return Some(PROTECTED);
        }
        let dirs: Vec<String> = paths.iter().flat_map(|p| bare_candidates(&p.0)).collect();
        let entries: Vec<String> = paths.iter().flat_map(|p| entry_folders(&p.0)).collect();
        if dirs.is_empty() && entries.is_empty() {
            return None;
        }
        let Some(root) = self.root.clone() else {
            return (!dirs.is_empty()).then_some(PROTECTED);
        };
        tokio::task::spawn_blocking(move || {
            dirs.iter().any(|d| bare_repository(&root, d))
                || entries.iter().any(|d| makes_git_dir(&root, d))
        })
        .await
        .unwrap_or(true)
        .then_some(PROTECTED)
    }

    /// A call ran that may have changed git's configuration ([`GitProtection::changed`]).
    fn changed(&self) {
        if let Some(g) = &self.git {
            g.0.changed();
        }
    }
}

/// `p` is `dir` or under it, each segment compared without regard to ASCII case (a
/// case-insensitive disk takes either spelling).
fn within(p: &WorkspacePath, dir: &WorkspacePath) -> bool {
    let mut ps = p.0.split('/');
    !dir.0.is_empty()
        && dir
            .0
            .split('/')
            .all(|d| ps.next().is_some_and(|s| s.eq_ignore_ascii_case(d)))
}

struct Pending {
    tx: oneshot::Sender<ApprovalDecision>,
    request: ApprovalRequest,
}

/// One "allow for session" grant: exactly the call the person approved, in that workspace
/// (AUD-009). Not a policy rule: a rule's `argvPrefix` matches longer argvs and its path is a
/// glob, a grant matches only the same tool, effect, paths, argv and conditions.
#[derive(Debug, Clone, PartialEq)]
struct Grant {
    workspace: WorkspaceId,
    tool: String,
    effect: Effect,
    paths: Vec<WorkspacePath>,
    argv: Option<Vec<String>>,
    conditions: Vec<String>,
}

impl Grant {
    fn covers(&self, workspace: &WorkspaceId, p: &Prepared) -> bool {
        self.workspace == *workspace
            && self.tool == p.name
            && self.effect == p.effect
            && self.paths == p.targets.paths
            && self.argv == p.targets.argv
            && self.conditions == p.targets.conditions
    }

    /// The grant as the most specific allow rule for the call it covers.
    fn rule(&self) -> PolicyRule {
        PolicyRule {
            effect: Some(self.effect),
            tool: Some(self.tool.clone()),
            path: (self.paths.len() == 1).then(|| self.paths[0].0.clone()),
            argv_prefix: self.argv.clone(),
            decision: Decision::Allow,
        }
    }
}

pub struct Gate {
    approvers: Arc<dyn Approvers>,
    pending: Mutex<HashMap<String, Pending>>,
    /// "Allow for session" grants, by principal.
    grants: Mutex<HashMap<String, Vec<Grant>>>,
    next: AtomicU64,
}

/// Paths an agent may write only with the person's explicit approval each time, whatever the
/// policy or a grant says: the workspace's own Axiom configuration (`.axiom/project.toml` holds
/// the workspace policy, `.axiom/providers/` defines tasks and language servers), git's
/// directory (hooks run on the person's next commit) at any depth (a nested repository or a
/// submodule has its own), what makes a `<name>.git` directory a bare repository or changes
/// what git runs in one (its `HEAD`, `config`, `hooks/`, …: [`GIT_DIR_ENTRIES`]; git runs its
/// `hooks/` for pushes to it), and husky's `.husky/` (the hooks directory `core.hooksPath`
/// names in a repository that uses it). AUD-104, review rev3, E3. The callers pass the path
/// through any symlink (`Workspace::canonical_path`, review B4). By name only: a `<name>.git`
/// directory that is already a repository is found on disk ([`Policies::protects_any`]), and
/// what git's configuration names is added per workspace ([`Policies::protected`]).
pub fn protected_path(p: &WorkspacePath) -> bool {
    let mut parts = p.0.split('/');
    let first = parts.next().unwrap_or("");
    first.eq_ignore_ascii_case(".axiom")
        || p.0.split('/').any(|s| {
            s.eq_ignore_ascii_case(".husky")
                // Which filter and diff driver each file goes through (review G3).
                || s.eq_ignore_ascii_case(".gitattributes")
        })
        || git_dir_by_name(&p.0).is_some()
}

/// What git looks for in a repository's directory, and what changes what git runs there: a
/// write of one of these directly inside a `<name>.git` directory always asks, so an agent
/// cannot make one a bare repository with hooks one unasked file at a time (review E3).
pub const GIT_DIR_ENTRIES: &[&str] = &[
    "HEAD",
    "config",
    "config.worktree",
    "commondir",
    "gitdir",
    "hooks",
    "objects",
    "refs",
    "packed-refs",
    "worktrees",
];

/// `.git`, compared without regard to ASCII case.
fn dot_git(s: &str) -> bool {
    s.eq_ignore_ascii_case(".git")
}

/// `<name>.git` (not `.git` itself): the name of a bare repository's directory, if it is one.
fn bare_name(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() > 4 && b[b.len() - 4..].eq_ignore_ascii_case(b".git")
}

/// The git directory `rel` is in by its name alone, as the path of that directory: a `.git`
/// segment, or a `<name>.git` segment followed by one of [`GIT_DIR_ENTRIES`].
pub(crate) fn git_dir_by_name(rel: &str) -> Option<String> {
    let parts: Vec<&str> = rel.split('/').collect();
    parts.iter().enumerate().find_map(|(i, s)| {
        let named = dot_git(s)
            || (bare_name(s)
                && parts
                    .get(i + 1)
                    .is_some_and(|n| GIT_DIR_ENTRIES.iter().any(|e| e.eq_ignore_ascii_case(n))));
        named.then(|| parts[..=i].join("/"))
    })
}

/// The `<name>.git` directories on `rel` (as workspace paths, `rel` itself included) that may be
/// bare repositories: [`bare_repository`] tells.
pub(crate) fn bare_candidates(rel: &str) -> Vec<String> {
    let parts: Vec<&str> = rel.split('/').collect();
    (0..parts.len())
        .filter(|&i| bare_name(parts[i]))
        .map(|i| parts[..=i].join("/"))
        .collect()
}

/// Whether the workspace directory `dir` under `root` is a git repository's directory: it has
/// a `HEAD` (what git looks for first). Blocking.
pub fn bare_repository(root: &Path, dir: &str) -> bool {
    let mut p = root.to_path_buf();
    p.extend(dir.split('/'));
    std::fs::symlink_metadata(p.join("HEAD")).is_ok()
}

/// The git directory `rel` is in, if any: by name (as [`protected_path`] tells) or on disk
/// ([`bare_repository`]). Blocking.
pub fn git_dir_of(root: &Path, rel: &str) -> Option<String> {
    git_dir_by_name(rel).or_else(|| {
        bare_candidates(rel)
            .into_iter()
            .find(|d| bare_repository(root, d))
    })
}

/// While git cannot say all it reads (reviews E2, G7): a path with a segment that may be git's
/// configuration or attributes (`.git*`, `*gitconfig`), a hooks directory (`*hooks`) or one of
/// git's own entries in a git directory ([`GIT_DIR_ENTRIES`]), compared without regard to ASCII
/// case.
pub fn git_config_like(p: &WorkspacePath) -> bool {
    p.0.split('/').any(|s| {
        let l = s.to_ascii_lowercase();
        l.starts_with(".git")
            || l.ends_with("gitconfig")
            || l.ends_with("hooks")
            || GIT_DIR_ENTRIES.iter().any(|e| e.eq_ignore_ascii_case(s))
    })
}

/// What makes a folder a git directory, beside `HEAD`: `objects/` and `refs/`, or a
/// `commondir` naming a folder that has them (git's `is_git_directory`).
const GIT_DIR_MARKERS: &[&str] = &["HEAD", "commondir", "objects", "refs"];

/// The folders (workspace paths, the root as `""`) into which `rel` writes one of git's own
/// entries ([`GIT_DIR_ENTRIES`]) directly: [`makes_git_dir`] tells whether that may make one a
/// git directory.
pub(crate) fn entry_folders(rel: &str) -> Vec<String> {
    let parts: Vec<&str> = rel.split('/').collect();
    (0..parts.len())
        .filter(|&i| {
            GIT_DIR_ENTRIES
                .iter()
                .any(|e| e.eq_ignore_ascii_case(parts[i]))
        })
        .map(|i| parts[..i].join("/"))
        .collect()
}

/// Whether the workspace folder `dir` under `root` already holds one of what makes a git
/// directory ([`GIT_DIR_MARKERS`]): then writing another of git's entries there could make it
/// one, its `config` or `config.worktree` then read by git, its `hooks/` run (review G5: an
/// agent's `src/HEAD`, then `src/commondir` naming `../.git`, then `src/config.worktree` with
/// `core.sshCommand`). The first of them alone, in an ordinary folder, does not ask. Blocking.
pub fn makes_git_dir(root: &Path, dir: &str) -> bool {
    let mut p = root.to_path_buf();
    if !dir.is_empty() {
        p.extend(dir.split('/'));
    }
    GIT_DIR_MARKERS
        .iter()
        .any(|m| std::fs::symlink_metadata(p.join(m)).is_ok())
}

/// The hooks git runs, by name (`githooks(5)`): where the hooks directory is the workspace root
/// itself (`core.hooksPath = .`), these names in it always ask, not the whole workspace.
pub const HOOK_NAMES: &[&str] = &[
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-receive",
    "update",
    "proc-receive",
    "post-receive",
    "post-update",
    "reference-transaction",
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
    "fsmonitor-watchman",
    "p4-changelist",
    "p4-prepare-changelist",
    "p4-post-changelist",
    "p4-pre-submit",
    "post-index-change",
];

fn writes(effect: Effect) -> bool {
    !matches!(effect, Effect::Read)
}

/// Why a call that the policy allows still asks, if it does. `protected`: why it writes a
/// protected path ([`Policies::protection`]).
fn must_ask(protected: Option<&str>, escalate: bool) -> Option<String> {
    if let Some(why) = protected {
        return Some(why.into());
    }
    escalate.then(|| "the call sets its environment or working directory".into())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Why a call did not run.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    /// `approval_denied` (policy or person), `approval_required` (nobody can approve),
    /// `cancelled`.
    pub kind: ErrorKind,
    pub message: String,
    pub rule: Option<String>,
}

impl Refusal {
    pub fn into_tool_error(self) -> ToolError {
        let hint = match self.kind {
            ErrorKind::ApprovalRequired => Some(
                "No one is attached to approve this. Ask the user to allow it in Axiom's agent policy, or continue without it.",
            ),
            ErrorKind::ApprovalDenied => {
                Some("Do not retry the same call. Ask the user, or take a different approach.")
            }
            _ => None,
        };
        let mut e = ToolError::new(self.kind, self.message);
        e.hint = hint.map(String::from);
        e
    }
}

/// `Gate::approve`: no request with that id is waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no approval with that id is pending")]
pub struct NotPending;

/// How the gate let a call through.
#[derive(Debug, Clone, PartialEq)]
pub enum Via {
    Policy,
    Approved(ApprovalDecision),
}

impl Gate {
    pub fn new(approvers: Arc<dyn Approvers>) -> Self {
        Self {
            approvers,
            pending: Mutex::default(),
            grants: Mutex::default(),
            next: AtomicU64::new(1),
        }
    }

    /// Decides, asking when the policy says so. On `Ok` the caller may execute.
    #[allow(clippy::too_many_arguments)]
    pub async fn check(
        &self,
        principal: &str,
        workspace: &WorkspaceId,
        session: Option<&SessionId>,
        prepared: &Prepared,
        policies: &Policies,
        checkpoint_planned: bool,
        cancel: &CancellationToken,
    ) -> Result<(Allowed, Via, Option<String>), Refusal> {
        self.check_with(
            principal,
            workspace,
            session,
            prepared,
            policies,
            checkpoint_planned,
            cancel,
            &[],
        )
        .await
    }

    /// [`Gate::check`] with extra allow rules that hold for this call only: what the user already
    /// approved for it through another prompt (an ACP agent's own permission request). They
    /// can never override a deny: the workspace's stricter rules still apply. They never
    /// cover a protected path or a call that sets its own environment ([`Targets::escalate`]).
    ///
    /// [`Targets::escalate`]: crate::tools::Targets::escalate
    #[allow(clippy::too_many_arguments)]
    pub async fn check_with(
        &self,
        principal: &str,
        workspace: &WorkspaceId,
        session: Option<&SessionId>,
        prepared: &Prepared,
        policies: &Policies,
        checkpoint_planned: bool,
        cancel: &CancellationToken,
        extra: &[PolicyRule],
    ) -> Result<(Allowed, Via, Option<String>), Refusal> {
        // Only a grant for exactly this call applies (AUD-009).
        let granted: Vec<PolicyRule> = lock(&self.grants)
            .get(principal)
            .into_iter()
            .flatten()
            .filter(|g| g.covers(workspace, prepared))
            .map(Grant::rule)
            .collect();
        let exact = !granted.is_empty();
        let call = policy::Call {
            tool: &prepared.name,
            effect: prepared.effect,
            paths: &prepared.targets.paths,
            argv: prepared.targets.argv.as_deref(),
        };
        let protection = if writes(prepared.effect) {
            policies.protection(&prepared.targets.paths).await
        } else {
            None
        };
        let protected = protection.is_some();
        let (mut decision, mut rule) = {
            let mut rules = granted;
            if !protected && !prepared.targets.escalate {
                rules.extend(extra.iter().cloned());
            }
            policy::decide(&policies.user, &rules, policies.workspace.as_ref(), &call)
        };
        if decision == Decision::Allow {
            let why = must_ask(protection, prepared.targets.escalate);
            // A protected path asks every time; a call with its own environment asks unless
            // the person approved exactly it for the session.
            if let Some(why) = why.filter(|_| protected || !exact) {
                decision = Decision::Ask;
                rule = Some(why);
            }
        }
        match decision {
            Decision::Allow => Ok((Allowed::granted(), Via::Policy, rule)),
            Decision::Deny => Err(Refusal {
                kind: ErrorKind::ApprovalDenied,
                message: format!(
                    "The agent policy denies `{}`{}.",
                    prepared.name,
                    rule.as_ref().map(|r| format!(" ({r})")).unwrap_or_default()
                ),
                rule,
            }),
            Decision::Ask => {
                let t = &prepared.targets;
                // "For this session" only where the targets say what later calls may do.
                let can_remember = !matches!(prepared.effect, Effect::Credentials | Effect::Deploy)
                    && !protected
                    && !t.opaque
                    && (prepared.effect == Effect::Read || !t.paths.is_empty() || t.argv.is_some());
                let request = ApprovalRequest {
                    request_id: format!("r{}", self.next.fetch_add(1, Ordering::Relaxed)),
                    session_id: session.cloned(),
                    principal: principal.to_string(),
                    tool: prepared.name.clone(),
                    effect: prepared.effect,
                    targets: t.display(),
                    argv: t.argv.clone(),
                    preview: t.preview.clone(),
                    rule: rule.clone(),
                    checkpoint_planned,
                    can_remember,
                };
                let decision = self.ask(request, cancel).await?;
                match decision {
                    ApprovalDecision::Deny => Err(Refusal {
                        kind: ErrorKind::ApprovalDenied,
                        message: format!("The user denied `{}`.", prepared.name),
                        rule,
                    }),
                    ApprovalDecision::Session if can_remember => {
                        self.grant(principal, workspace, prepared);
                        Ok((Allowed::granted(), Via::Approved(decision), rule))
                    }
                    // Remembering was not offered: this call only.
                    ApprovalDecision::Session => Ok((
                        Allowed::granted(),
                        Via::Approved(ApprovalDecision::Once),
                        rule,
                    )),
                    d => Ok((Allowed::granted(), Via::Approved(d), rule)),
                }
            }
        }
    }

    async fn ask(
        &self,
        request: ApprovalRequest,
        cancel: &CancellationToken,
    ) -> Result<ApprovalDecision, Refusal> {
        let id = request.request_id.clone();
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(
            id.clone(),
            Pending {
                tx,
                request: request.clone(),
            },
        );
        // The answer and the cancel are listened for while the prompt is still being delivered:
        // an approver that stopped reading (its delivery waits up to its bound) holds up no
        // call another approver answers (review D1). Refused only once every delivery has
        // ended and none reached anyone, unless an answer came meanwhile (a client that
        // attached late and found the request in [`Gate::pending`]).
        let mut delivery = self.approvers.request(&request);
        let mut delivering = true;
        let mut rx = rx;
        loop {
            tokio::select! {
                sent = &mut delivery, if delivering => {
                    delivering = false;
                    if sent == 0 && lock(&self.pending).remove(&id).is_some() {
                        return Err(Refusal {
                            kind: ErrorKind::ApprovalRequired,
                            message: format!(
                                "`{}` needs approval and no one is attached to give it.",
                                request.tool
                            ),
                            rule: request.rule.clone(),
                        });
                    }
                }
                d = &mut rx => return d.map_err(|_| Refusal {
                    kind: ErrorKind::Cancelled,
                    message: "The approval was withdrawn.".into(),
                    rule: None,
                }),
                _ = cancel.cancelled() => {
                    if lock(&self.pending).remove(&id).is_some() {
                        self.approvers
                            .resolved(&ApprovalResolution { request_id: id, decision: None })
                            .await;
                    }
                    return Err(Refusal {
                        kind: ErrorKind::Cancelled,
                        message: "Cancelled while waiting for approval.".into(),
                        rule: None,
                    });
                }
            }
        }
    }

    /// Answers a pending request; the first answer wins. `Err` if unknown or already answered.
    pub async fn approve(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), NotPending> {
        let p = lock(&self.pending).remove(request_id).ok_or(NotPending)?;
        let _ = p.tx.send(decision);
        self.approvers
            .resolved(&ApprovalResolution {
                request_id: request_id.to_string(),
                decision: Some(decision),
            })
            .await;
        Ok(())
    }

    /// Pending requests (all, or one session's), for a client that attaches late.
    pub fn pending(&self, session: Option<&SessionId>) -> Vec<ApprovalRequest> {
        let mut v: Vec<ApprovalRequest> = lock(&self.pending)
            .values()
            .filter(|p| session.is_none() || p.request.session_id.as_ref() == session)
            .map(|p| p.request.clone())
            .collect();
        v.sort_by(|a, b| a.request_id.cmp(&b.request_id));
        v
    }

    /// The "allow for session" grant: this tool on exactly these targets, in this workspace.
    fn grant(&self, principal: &str, workspace: &WorkspaceId, p: &Prepared) {
        let g = Grant {
            workspace: workspace.clone(),
            tool: p.name.clone(),
            effect: p.effect,
            paths: p.targets.paths.clone(),
            argv: p.targets.argv.clone(),
            conditions: p.targets.conditions.clone(),
        };
        let mut grants = lock(&self.grants);
        let list = grants.entry(principal.to_string()).or_default();
        if !list.contains(&g) {
            list.push(g);
        }
    }

    /// Asks directly (an ACP agent's permission request, which has no Axiom tool). Returns the
    /// policy decision if it does not ask.
    #[allow(clippy::too_many_arguments)]
    pub async fn decide_or_ask(
        &self,
        principal: &str,
        session: Option<&SessionId>,
        call: &policy::Call<'_>,
        targets: Vec<String>,
        preview: Option<String>,
        policies: &Policies,
        cancel: &CancellationToken,
    ) -> Result<(Via, Option<String>), Refusal> {
        // A command the agent runs itself, once allowed, may change git's configuration: the
        // next write asks git again (review G6).
        if !matches!(
            call.effect,
            Effect::Read | Effect::WriteWorkspace | Effect::Delete
        ) {
            policies.changed();
        }
        // No session grant covers a permission request (none is ever made for one).
        let (mut decision, mut rule) =
            policy::decide(&policies.user, &[], policies.workspace.as_ref(), call);
        let protection = if writes(call.effect) {
            policies.protection(call.paths).await
        } else {
            None
        };
        let protected = protection.is_some();
        if decision == Decision::Allow {
            if let Some(why) = must_ask(protection, false) {
                decision = Decision::Ask;
                rule = Some(why);
            }
        }
        match decision {
            Decision::Allow => Ok((Via::Policy, rule)),
            Decision::Deny => Err(Refusal {
                kind: ErrorKind::ApprovalDenied,
                message: format!(
                    "The agent policy denies `{}`{}.",
                    call.tool,
                    rule.as_ref().map(|r| format!(" ({r})")).unwrap_or_default()
                ),
                rule,
            }),
            Decision::Ask => {
                let can_remember =
                    !matches!(call.effect, Effect::Credentials | Effect::Deploy) && !protected;
                let request = ApprovalRequest {
                    request_id: format!("r{}", self.next.fetch_add(1, Ordering::Relaxed)),
                    session_id: session.cloned(),
                    principal: principal.to_string(),
                    tool: call.tool.to_string(),
                    effect: call.effect,
                    targets,
                    argv: call.argv.map(<[String]>::to_vec),
                    preview,
                    rule: rule.clone(),
                    checkpoint_planned: call.effect != Effect::Read,
                    can_remember,
                };
                match self.ask(request, cancel).await? {
                    ApprovalDecision::Deny => Err(Refusal {
                        kind: ErrorKind::ApprovalDenied,
                        message: format!("The user denied `{}`.", call.tool),
                        rule,
                    }),
                    d => Ok((Via::Approved(d), rule)),
                }
            }
        }
    }

    /// Forgets a principal's grants: its session was deleted or rewound, its connection closed.
    pub fn clear_grants(&self, principal: &str) {
        lock(&self.grants).remove(principal);
    }

    /// Forgets every grant whose principal `keep` rejects (connections that closed).
    pub fn retain_grants(&self, keep: impl Fn(&str) -> bool) {
        lock(&self.grants).retain(|p, _| keep(p));
    }

    /// TEST ONLY (feature `test-support`): whether `principal` holds any grant, for tests that
    /// a grant is forgotten where nothing else can observe it (a deleted session, review N6).
    #[cfg(any(test, feature = "test-support"))]
    pub fn has_grants(&self, principal: &str) -> bool {
        lock(&self.grants)
            .get(principal)
            .is_some_and(|g| !g.is_empty())
    }
}

/// Called after the gate allowed a call and before it runs (the checkpoint). An error stops
/// the call: nothing runs without its checkpoint.
pub trait BeforeExecute: Send + Sync {
    fn before<'a>(&'a self, prepared: &'a Prepared) -> BoxFuture<'a, Result<(), ToolError>>;
}

/// No hook (reads, tests).
pub struct NoHook;
impl BeforeExecute for NoHook {
    fn before<'a>(&'a self, _p: &'a Prepared) -> BoxFuture<'a, Result<(), ToolError>> {
        Box::pin(std::future::ready(Ok(())))
    }
}

/// What a call produced, for the journal, the model and the audit log.
#[derive(Debug, Clone)]
pub struct CallOutcome {
    pub tool: String,
    pub effect: Effect,
    pub targets: Vec<String>,
    pub output: ToolOutput,
    pub is_error: bool,
    pub rule: Option<String>,
    /// Set when the gate refused.
    pub refusal: Option<Refusal>,
}

/// One audit record per call (`<data dir>/audit/<workspace hash>.jsonl`, append-only). Blocking
/// file I/O: async callers use [`audit_append_async`].
pub fn audit_append(path: &PathBuf, record: &Value) {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let line = format!("{record}\n");
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(mut f) => {
            if let Err(e) = f.write_all(line.as_bytes()) {
                tracing::error!("cannot write the audit log {}: {e}", path.display());
            }
        }
        Err(e) => tracing::error!("cannot open the audit log {}: {e}", path.display()),
    }
}

/// [`audit_append`] on the blocking pool, so a slow disk never stalls a runtime worker
/// (AUD-031). Awaited, so records keep their order and are on disk when the call returns.
pub async fn audit_append_async(path: PathBuf, record: Value) {
    if tokio::task::spawn_blocking(move || audit_append(&path, &record))
        .await
        .is_err()
    {
        tracing::error!("the audit log writer panicked");
    }
}

/// The pipeline for one call. Never returns an error: a refused or failed call is an outcome
/// with `is_error`, so the model always gets a result.
#[allow(clippy::too_many_arguments)]
pub async fn call_tool(
    registry: &Registry,
    gate: &Gate,
    ctx: &ToolCtx,
    session: Option<&SessionId>,
    name: &str,
    input: Value,
    policies: &Policies,
    hook: &dyn BeforeExecute,
    audit: Option<&PathBuf>,
) -> CallOutcome {
    let started = Instant::now();
    let prepared = match registry.prepare(ctx, name, input).await {
        Ok(p) => p,
        Err(e) => {
            let effect = registry.get(name).map_or(Effect::Read, |t| t.spec().effect);
            return finish(
                ctx,
                audit,
                started,
                name,
                effect,
                vec![],
                None,
                Err(e),
                None,
            )
            .await;
        }
    };
    run_prepared(
        registry, gate, ctx, session, prepared, policies, hook, audit, started,
    )
    .await
}

/// The pipeline after `prepare` (the loop journals the call's targets in between).
#[allow(clippy::too_many_arguments)]
pub async fn run_prepared(
    registry: &Registry,
    gate: &Gate,
    ctx: &ToolCtx,
    session: Option<&SessionId>,
    prepared: Prepared,
    policies: &Policies,
    hook: &dyn BeforeExecute,
    audit: Option<&PathBuf>,
    started: Instant,
) -> CallOutcome {
    run_prepared_with(
        registry,
        gate,
        ctx,
        session,
        prepared,
        policies,
        hook,
        audit,
        started,
        &[],
    )
    .await
}

/// [`run_prepared`] with one-call grants (see [`Gate::check_with`]).
#[allow(clippy::too_many_arguments)]
pub async fn run_prepared_with(
    registry: &Registry,
    gate: &Gate,
    ctx: &ToolCtx,
    session: Option<&SessionId>,
    prepared: Prepared,
    policies: &Policies,
    hook: &dyn BeforeExecute,
    audit: Option<&PathBuf>,
    started: Instant,
    extra: &[PolicyRule],
) -> CallOutcome {
    let (name, effect, targets) = (
        prepared.name.clone(),
        prepared.effect,
        prepared.targets.display(),
    );
    let planned = effect != Effect::Read;
    let (allowed, via, rule) = match gate
        .check_with(
            &ctx.principal,
            &ctx.workspace_id,
            session,
            &prepared,
            policies,
            planned,
            &ctx.cancel,
            extra,
        )
        .await
    {
        Ok(x) => x,
        Err(r) => {
            let rule = r.rule.clone();
            return finish(
                ctx,
                audit,
                started,
                &name,
                effect,
                targets,
                None,
                Err(r.clone().into_tool_error()),
                Some(r),
            )
            .await
            .with_rule(rule);
        }
    };
    if let Err(e) = hook.before(&prepared).await {
        return finish(
            ctx,
            audit,
            started,
            &name,
            effect,
            targets,
            Some(&via),
            Err(e),
            None,
        )
        .await
        .with_rule(rule);
    }
    let result = registry.execute(ctx, prepared, allowed).await;
    // A command (or anything else that is not a write the gate saw path by path) may have
    // changed git's configuration or made a repository: git is asked again (review G6).
    if !matches!(
        effect,
        Effect::Read | Effect::WriteWorkspace | Effect::Delete
    ) {
        policies.changed();
    }
    finish(
        ctx,
        audit,
        started,
        &name,
        effect,
        targets,
        Some(&via),
        result,
        None,
    )
    .await
    .with_rule(rule)
}

impl CallOutcome {
    fn with_rule(mut self, rule: Option<String>) -> Self {
        self.rule = rule;
        self
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn finish(
    ctx: &ToolCtx,
    audit: Option<&PathBuf>,
    started: Instant,
    name: &str,
    effect: Effect,
    targets: Vec<String>,
    via: Option<&Via>,
    result: Result<ToolOutput, ToolError>,
    refusal: Option<Refusal>,
) -> CallOutcome {
    let (output, is_error) = match result {
        Ok(o) => (o, false),
        Err(e) => (e.into_output(), true),
    };
    if let Some(path) = audit {
        let decision = match (&refusal, via) {
            (Some(r), _) => match r.kind {
                ErrorKind::ApprovalRequired => "no approver".to_string(),
                ErrorKind::Cancelled => "cancelled".to_string(),
                _ => "denied".to_string(),
            },
            (None, Some(Via::Policy)) => "allowed by policy".into(),
            (None, Some(Via::Approved(d))) => format!(
                "approved ({})",
                serde_json::to_value(d)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default()
            ),
            (None, None) => "invalid".into(),
        };
        let record = json!({
            "ts": SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64),
            "principal": ctx.principal,
            "tool": name,
            "effect": effect,
            "targets": targets,
            "decision": decision,
            "rule": refusal.as_ref().and_then(|r| r.rule.clone()),
            "outcome": if is_error { "error" } else { "ok" },
            "summary": output.summary,
            "durationMs": started.elapsed().as_millis() as u64,
        });
        audit_append_async(path.clone(), record).await;
    }
    CallOutcome {
        tool: name.to_string(),
        effect,
        targets,
        output,
        is_error,
        rule: None,
        refusal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{Targets, Tool, ToolSpec};
    use std::sync::{OnceLock, Weak};

    /// Answers every request with `answer` and records it.
    struct Auto {
        answer: Mutex<ApprovalDecision>,
        gate: OnceLock<Weak<Gate>>,
        asked: Mutex<Vec<ApprovalRequest>>,
    }

    impl Approvers for Auto {
        fn request<'a>(&'a self, req: &'a ApprovalRequest) -> BoxFuture<'a, usize> {
            lock(&self.asked).push(req.clone());
            let gate = self.gate.get().and_then(Weak::upgrade);
            let id = req.request_id.clone();
            let d = *lock(&self.answer);
            Box::pin(async move {
                if let Some(g) = gate {
                    tokio::spawn(async move {
                        let _ = g.approve(&id, d).await;
                    });
                }
                1
            })
        }
        fn resolved<'a>(&'a self, _res: &'a ApprovalResolution) -> BoxFuture<'a, ()> {
            Box::pin(std::future::ready(()))
        }
    }

    struct Dummy(ToolSpec);
    impl Tool for Dummy {
        fn spec(&self) -> &ToolSpec {
            &self.0
        }
        fn prepare<'a>(
            &'a self,
            _: &'a ToolCtx,
            _: &'a Value,
        ) -> BoxFuture<'a, Result<Targets, ToolError>> {
            Box::pin(std::future::ready(Ok(Targets::default())))
        }
        fn execute<'a>(
            &'a self,
            _: &'a ToolCtx,
            _: Value,
            _: &'a Targets,
            _: Allowed,
        ) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
            Box::pin(std::future::ready(Ok(ToolOutput::json(json!({}), "ok"))))
        }
    }

    fn gate() -> (Arc<Gate>, Arc<Auto>) {
        let auto = Arc::new(Auto {
            answer: Mutex::new(ApprovalDecision::Session),
            gate: OnceLock::new(),
            asked: Mutex::default(),
        });
        let g = Arc::new(Gate::new(auto.clone()));
        let _ = auto.gate.set(Arc::downgrade(&g));
        (g, auto)
    }

    fn prepared(name: &str, effect: Effect, targets: Targets) -> Prepared {
        Prepared {
            tool: Arc::new(Dummy(ToolSpec {
                name: name.into(),
                description: String::new(),
                input_schema: json!({}),
                effect,
                max_result_bytes: 1024,
            })),
            name: name.into(),
            effect,
            input: json!({}),
            targets,
        }
    }

    fn argv(a: &[&str]) -> Targets {
        Targets {
            argv: Some(a.iter().map(|s| s.to_string()).collect()),
            ..Default::default()
        }
    }

    fn ask_all() -> Policies {
        Policies {
            user: policy::default_policy(),
            workspace: None,
            protected: vec![],
            git_unknown: false,
            root: None,
            git: None,
        }
    }

    fn ws(id: &str) -> WorkspaceId {
        WorkspaceId(id.into())
    }

    /// `Ok(true)`: it asked; `Ok(false)`: allowed without asking.
    async fn asks(g: &Gate, a: &Auto, ws: &WorkspaceId, p: &Prepared, pol: &Policies) -> bool {
        let before = lock(&a.asked).len();
        let r = g
            .check(
                "session:s",
                ws,
                None,
                p,
                pol,
                true,
                &CancellationToken::new(),
            )
            .await;
        assert!(r.is_ok(), "{r:?}");
        lock(&a.asked).len() > before
    }

    #[tokio::test]
    async fn session_grants_cover_exactly_the_approved_argv() {
        let (g, a) = gate();
        let w = ws("w1");
        let rm = prepared("task.run", Effect::Execute, argv(&["rm", "a"]));
        assert!(asks(&g, &a, &w, &rm, &ask_all()).await);
        // The same call again: granted.
        assert!(!asks(&g, &a, &w, &rm, &ask_all()).await);
        // A longer argv starting with the approved one is a different command (AUD-009).
        let more = prepared("task.run", Effect::Execute, argv(&["rm", "a", "-rf", "/"]));
        assert!(asks(&g, &a, &w, &more, &ask_all()).await);
    }

    #[tokio::test]
    async fn grants_are_per_workspace_and_cleared() {
        let (g, a) = gate();
        let p = prepared("task.run", Effect::Execute, argv(&["make"]));
        assert!(asks(&g, &a, &ws("A"), &p, &ask_all()).await);
        assert!(!asks(&g, &a, &ws("A"), &p, &ask_all()).await);
        assert!(asks(&g, &a, &ws("B"), &p, &ask_all()).await);
        g.clear_grants("session:s");
        assert!(asks(&g, &a, &ws("A"), &p, &ask_all()).await);
        g.retain_grants(|_| false);
        assert!(asks(&g, &a, &ws("A"), &p, &ask_all()).await);
    }

    #[tokio::test]
    async fn opaque_and_target_less_calls_are_never_remembered() {
        let (g, a) = gate();
        let w = ws("w");
        let install = prepared(
            "env.install",
            Effect::Install,
            Targets {
                opaque: true,
                conditions: vec!["python 3.13".into()],
                ..Default::default()
            },
        );
        assert!(asks(&g, &a, &w, &install, &ask_all()).await);
        assert!(!lock(&a.asked).last().unwrap().can_remember);
        assert!(asks(&g, &a, &w, &install, &ask_all()).await);
        let bare = prepared("x.y", Effect::Execute, Targets::default());
        assert!(asks(&g, &a, &w, &bare, &ask_all()).await);
        assert!(!lock(&a.asked).last().unwrap().can_remember);
        assert!(asks(&g, &a, &w, &bare, &ask_all()).await);
    }

    #[tokio::test]
    async fn an_agent_set_environment_asks_even_when_the_argv_is_allowed() {
        let (g, a) = gate();
        let w = ws("w");
        let pol = Policies {
            user: Policy {
                default: Decision::Ask,
                rules: vec![PolicyRule {
                    effect: None,
                    tool: None,
                    path: None,
                    argv_prefix: Some(vec!["npm".into()]),
                    decision: Decision::Allow,
                }],
            },
            workspace: None,
            protected: vec![],
            git_unknown: false,
            root: None,
            git: None,
        };
        let plain = prepared("acp.terminal", Effect::Execute, argv(&["npm", "test"]));
        assert!(!asks(&g, &a, &w, &plain, &pol).await);
        let mut t = argv(&["npm", "test"]);
        t.conditions = vec!["env LD_PRELOAD=/tmp/x.so".into()];
        t.escalate = true;
        let preload = prepared("acp.terminal", Effect::Execute, t.clone());
        assert!(asks(&g, &a, &w, &preload, &pol).await);
        let req = lock(&a.asked).last().unwrap().clone();
        assert!(
            req.targets.iter().any(|x| x.contains("LD_PRELOAD")),
            "{req:?}"
        );
        // Granted for the session: exactly that environment.
        assert!(!asks(&g, &a, &w, &preload, &pol).await);
        t.conditions = vec!["env LD_PRELOAD=/tmp/y.so".into()];
        let other = prepared("acp.terminal", Effect::Execute, t);
        assert!(asks(&g, &a, &w, &other, &pol).await);
        // One-call rules (an ACP permission) never cover it either.
        let extra = [PolicyRule {
            effect: None,
            tool: Some("acp.terminal".into()),
            path: None,
            argv_prefix: None,
            decision: Decision::Allow,
        }];
        let before = lock(&a.asked).len();
        let _ = g
            .check_with(
                "session:t",
                &w,
                None,
                &other,
                &ask_all(),
                true,
                &CancellationToken::new(),
                &extra,
            )
            .await;
        assert_eq!(lock(&a.asked).len(), before + 1);
    }

    /// Delivers to one approver at once and to a stuck one only after a long time: the delivery
    /// as a whole ends late, as `ConnApprovers::request` does with a full connection queue.
    struct OneStuck;

    impl Approvers for OneStuck {
        fn request<'a>(&'a self, _req: &'a ApprovalRequest) -> BoxFuture<'a, usize> {
            Box::pin(async {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                1
            })
        }
        fn resolved<'a>(&'a self, _res: &'a ApprovalResolution) -> BoxFuture<'a, ()> {
            Box::pin(std::future::ready(()))
        }
    }

    /// Review D1: an approver that answers at once is not held up by another approver whose
    /// delivery is stuck, and neither is a cancel.
    #[tokio::test]
    async fn an_answer_is_taken_while_another_approver_is_still_being_delivered_to() {
        let g = Arc::new(Gate::new(Arc::new(OneStuck)));
        for cancel_it in [false, true] {
            let cancel = CancellationToken::new();
            let call = {
                let p = prepared("t.run", Effect::Execute, argv(&["make"]));
                let (g, cancel) = (g.clone(), cancel.clone());
                tokio::spawn(async move {
                    g.check("session:s", &ws("w"), None, &p, &ask_all(), true, &cancel)
                        .await
                        .map(|(_, via, _)| via)
                })
            };
            let id = loop {
                if let Some(r) = g.pending(None).pop() {
                    break r.request_id;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            };
            if cancel_it {
                cancel.cancel();
            } else {
                g.approve(&id, ApprovalDecision::Once).await.unwrap();
            }
            let r = tokio::time::timeout(std::time::Duration::from_secs(5), call)
                .await
                .expect("the call waited for the stuck delivery")
                .unwrap();
            if cancel_it {
                assert_eq!(r.unwrap_err().kind, ErrorKind::Cancelled);
            } else {
                assert_eq!(r.unwrap(), Via::Approved(ApprovalDecision::Once));
            }
        }
    }

    #[tokio::test]
    async fn protected_paths_always_ask_and_are_never_remembered() {
        let (g, a) = gate();
        let w = ws("w");
        let pol = Policies {
            user: Policy {
                default: Decision::Allow,
                rules: vec![PolicyRule {
                    effect: Some(Effect::WriteWorkspace),
                    tool: None,
                    path: None,
                    argv_prefix: None,
                    decision: Decision::Allow,
                }],
            },
            workspace: None,
            protected: vec![],
            git_unknown: false,
            root: None,
            git: None,
        };
        for path in [
            ".axiom/project.toml",
            ".axiom/providers/x/provider.toml",
            ".git/hooks/pre-commit",
            // Hooks git runs from outside `.git` (review rev3): husky's directory, at any
            // depth, and a bare repository in the workspace.
            ".husky/pre-commit",
            "web/.Husky/_/pre-push",
            "mirror.git/hooks/post-update",
            "vendor/Upstream.GIT/config",
            // Which filters and diff drivers files go through (review G3), at any depth.
            ".gitattributes",
            "web/.GitAttributes",
        ] {
            let p = prepared(
                "fs.applyPatch",
                Effect::WriteWorkspace,
                Targets {
                    paths: vec![WorkspacePath(path.into())],
                    ..Default::default()
                },
            );
            assert!(asks(&g, &a, &w, &p, &pol).await, "{path}");
            assert!(!lock(&a.asked).last().unwrap().can_remember);
            assert!(asks(&g, &a, &w, &p, &pol).await, "{path} again");
        }
        let ordinary = prepared(
            "fs.applyPatch",
            Effect::WriteWorkspace,
            Targets {
                paths: vec![WorkspacePath("src/axiom.rs".into())],
                ..Default::default()
            },
        );
        assert!(!asks(&g, &a, &w, &ordinary, &pol).await);
        for path in [
            ".gitignore",
            ".github/workflows/ci.yml",
            "src/git.rs",
            "husky.md",
        ] {
            let p = prepared(
                "fs.applyPatch",
                Effect::WriteWorkspace,
                Targets {
                    paths: vec![WorkspacePath(path.into())],
                    ..Default::default()
                },
            );
            assert!(!asks(&g, &a, &w, &p, &pol).await, "{path}");
        }
        // The workspace's configured hooks directory (`core.hooksPath`) is protected too.
        let hooks = prepared(
            "fs.applyPatch",
            Effect::WriteWorkspace,
            Targets {
                paths: vec![WorkspacePath("tools/Hooks/pre-commit".into())],
                ..Default::default()
            },
        );
        assert!(!asks(&g, &a, &w, &hooks, &pol).await);
        let with_hooks = Policies {
            protected: vec![WorkspacePath("tools/hooks".into())],
            ..pol.clone()
        };
        assert!(asks(&g, &a, &w, &hooks, &with_hooks).await);
        assert!(!lock(&a.asked).last().unwrap().can_remember);
        let beside = prepared(
            "fs.applyPatch",
            Effect::WriteWorkspace,
            Targets {
                paths: vec![WorkspacePath("tools/hooks-docs.md".into())],
                ..Default::default()
            },
        );
        assert!(!asks(&g, &a, &w, &beside, &with_hooks).await);
        // Reading them is not protected.
        let read = prepared(
            "fs.read",
            Effect::Read,
            Targets {
                paths: vec![WorkspacePath(".axiom/project.toml".into())],
                ..Default::default()
            },
        );
        assert!(!asks(&g, &a, &w, &read, &pol).await);
    }

    fn allow_writes(root: Option<PathBuf>, git_unknown: bool) -> Policies {
        Policies {
            user: Policy {
                default: Decision::Allow,
                rules: vec![],
            },
            workspace: None,
            protected: vec![],
            git_unknown,
            root,
            git: None,
        }
    }

    fn write_to(path: &str) -> Prepared {
        prepared(
            "fs.applyPatch",
            Effect::WriteWorkspace,
            Targets {
                paths: vec![WorkspacePath(path.into())],
                ..Default::default()
            },
        )
    }

    /// Review E3: a name ending in `.git` asks only where it is a repository's directory: one
    /// that is on disk (it has a `HEAD`), or a write that would make one or add hooks to one
    /// (`HEAD`, `config`, `hooks/`, … directly inside it). An ordinary file or folder with such
    /// a name does not ask. `.git` itself always asks. Without the root to look at, every such
    /// directory is taken to be a repository.
    #[tokio::test]
    async fn a_name_ending_in_git_asks_only_for_a_repository() {
        let (g, a) = gate();
        let w = ws("w");
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("vendor/mirror.git")).unwrap();
        std::fs::write(dir.path().join("vendor/mirror.git/HEAD"), "ref: x\n").unwrap();
        std::fs::create_dir_all(dir.path().join("vendor/lib.git")).unwrap();
        let pol = allow_writes(Some(dir.path().to_path_buf()), false);
        for path in [
            "deps.git",
            "vendor/lib.git/README.md",
            "vendor/lib.git/src/hooks.rs",
            "notes/my.git/description",
        ] {
            assert!(!asks(&g, &a, &w, &write_to(path), &pol).await, "{path}");
        }
        for path in [
            ".git/config",
            "sub/.GIT/hooks/pre-commit",
            "vendor/mirror.git/description",
            "vendor/mirror.git",
            "new.git/HEAD",
            "new.git/hooks/post-update",
            "new.git/Config",
            "new.git/objects/aa/bb",
            "vendor/lib.git/refs/heads/main",
        ] {
            assert!(asks(&g, &a, &w, &write_to(path), &pol).await, "{path}");
            assert!(!lock(&a.asked).last().unwrap().can_remember, "{path}");
        }
        let unknown_root = allow_writes(None, false);
        assert!(asks(&g, &a, &w, &write_to("deps.git/README.md"), &unknown_root).await);
        assert!(!asks(&g, &a, &w, &write_to("src/git.rs"), &unknown_root).await);
    }

    /// Review E2: while git cannot be asked what it reads, what may be git's configuration or a
    /// hooks folder asks (an included file can be named anything, so this is by pattern).
    #[tokio::test]
    async fn without_gits_answer_config_like_names_ask() {
        let (g, a) = gate();
        let w = ws("w");
        let known = allow_writes(None, false);
        let unknown = allow_writes(None, true);
        for path in [
            "shared.gitconfig",
            "config/.gitconfig",
            ".githooks/pre-commit",
            "tools/hooks/pre-commit",
            ".gitmodules",
        ] {
            assert!(!asks(&g, &a, &w, &write_to(path), &known).await, "{path}");
            assert!(asks(&g, &a, &w, &write_to(path), &unknown).await, "{path}");
        }
        assert!(!asks(&g, &a, &w, &write_to("src/main.rs"), &unknown).await);
    }
    /// A [`GitProtection`] whose answer the test sets; counts the times it is asked.
    struct Fixed {
        answer: Mutex<GitAnswer>,
        asked: AtomicU64,
    }

    impl GitProtection for Fixed {
        fn current(&self) -> BoxFuture<'_, GitAnswer> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            let a = lock(&self.answer).clone();
            Box::pin(async move { a })
        }
        fn changed(&self) {}
    }

    /// Reviews G6, G7: git's answer is asked for every write (a change between two calls counts
    /// at the second), not for a read; while git cannot find every repository, names that may
    /// be git's ask as well; while it cannot answer for a repository that is there, every write
    /// asks, and the card says why.
    #[tokio::test]
    async fn gits_answer_is_asked_for_each_write_and_what_it_cannot_say_asks() {
        let (g, a) = gate();
        let w = ws("w");
        let fixed = Arc::new(Fixed {
            answer: Mutex::new(GitAnswer {
                protected: vec![WorkspacePath("tools/gh".into())],
                unknown: GitUnknown::No,
            }),
            asked: AtomicU64::new(0),
        });
        let pol = Policies {
            git: Some(GitSource(fixed.clone())),
            ..allow_writes(None, false)
        };
        assert!(asks(&g, &a, &w, &write_to("tools/gh/pre-commit"), &pol).await);
        assert!(!asks(&g, &a, &w, &write_to("tools/gh2/pre-commit"), &pol).await);
        assert!(!asks(&g, &a, &w, &write_to("shared.gitconfig"), &pol).await);
        lock(&fixed.answer).protected = vec![WorkspacePath("tools/gh2".into())];
        assert!(asks(&g, &a, &w, &write_to("tools/gh2/pre-commit"), &pol).await);
        assert!(!asks(&g, &a, &w, &write_to("tools/gh/pre-commit"), &pol).await);
        assert_eq!(fixed.asked.load(Ordering::SeqCst), 5);
        let read = prepared(
            "fs.read",
            Effect::Read,
            Targets {
                paths: vec![WorkspacePath("tools/gh2/pre-commit".into())],
                ..Default::default()
            },
        );
        assert!(!asks(&g, &a, &w, &read, &pol).await);
        assert_eq!(fixed.asked.load(Ordering::SeqCst), 5);

        lock(&fixed.answer).unknown = GitUnknown::Names;
        for path in ["shared.gitconfig", "x/HEAD", "x/config", ".gitmodules"] {
            assert!(asks(&g, &a, &w, &write_to(path), &pol).await, "{path}");
        }
        assert!(!asks(&g, &a, &w, &write_to("src/main.rs"), &pol).await);

        lock(&fixed.answer).unknown = GitUnknown::Everything;
        assert!(asks(&g, &a, &w, &write_to("src/main.rs"), &pol).await);
        let req = lock(&a.asked).last().cloned().unwrap();
        assert!(!req.can_remember);
        assert!(
            req.rule
                .as_deref()
                .unwrap_or_default()
                .contains("every write asks"),
            "{req:?}"
        );
    }
}
