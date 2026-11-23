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
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

/// Where approval requests go: the connections that declared `capabilities.approvals`.
pub trait Approvers: Send + Sync {
    /// Sends the request to every approving connection; returns how many received it.
    fn request<'a>(&'a self, req: &'a ApprovalRequest) -> BoxFuture<'a, usize>;
    fn resolved<'a>(&'a self, res: &'a ApprovalResolution) -> BoxFuture<'a, ()>;
}

/// The policies that apply to a call.
#[derive(Debug, Clone)]
pub struct Policies {
    pub user: Policy,
    /// `.axiom/project.toml` `[agent.policy]`: stricter only.
    pub workspace: Option<Policy>,
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
/// the workspace policy, `.axiom/providers/` defines tasks and language servers) and git's
/// directory (hooks run on the person's next commit), at any depth (a nested repository or a
/// submodule has its own). AUD-104. The callers pass the path through any symlink
/// (`Workspace::canonical_path`, review B4).
pub fn protected_path(p: &WorkspacePath) -> bool {
    let mut parts = p.0.split('/');
    let first = parts.next().unwrap_or("");
    first.eq_ignore_ascii_case(".axiom")
        || first.eq_ignore_ascii_case(".git")
        || parts.any(|s| s.eq_ignore_ascii_case(".git"))
}

fn writes(effect: Effect) -> bool {
    !matches!(effect, Effect::Read)
}

/// Why a call that the policy allows still asks, if it does.
fn must_ask(effect: Effect, paths: &[WorkspacePath], escalate: bool) -> Option<String> {
    if writes(effect) && paths.iter().any(protected_path) {
        return Some("protected: Axiom configuration and .git always ask".into());
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
        let protected =
            writes(prepared.effect) && prepared.targets.paths.iter().any(protected_path);
        let (mut decision, mut rule) = {
            let mut rules = granted;
            if !protected && !prepared.targets.escalate {
                rules.extend(extra.iter().cloned());
            }
            policy::decide(&policies.user, &rules, policies.workspace.as_ref(), &call)
        };
        if decision == Decision::Allow {
            let why = must_ask(
                prepared.effect,
                &prepared.targets.paths,
                prepared.targets.escalate,
            );
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
        let sent = self.approvers.request(&request).await;
        if sent == 0 {
            lock(&self.pending).remove(&id);
            return Err(Refusal {
                kind: ErrorKind::ApprovalRequired,
                message: format!(
                    "`{}` needs approval and no one is attached to give it.",
                    request.tool
                ),
                rule: request.rule,
            });
        }
        tokio::select! {
            d = rx => d.map_err(|_| Refusal {
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
                Err(Refusal {
                    kind: ErrorKind::Cancelled,
                    message: "Cancelled while waiting for approval.".into(),
                    rule: None,
                })
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
        // No session grant covers a permission request (none is ever made for one).
        let (mut decision, mut rule) =
            policy::decide(&policies.user, &[], policies.workspace.as_ref(), call);
        if decision == Decision::Allow {
            if let Some(why) = must_ask(call.effect, call.paths, false) {
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
                let can_remember = !matches!(call.effect, Effect::Credentials | Effect::Deploy)
                    && !(writes(call.effect) && call.paths.iter().any(protected_path));
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
        };
        for path in [
            ".axiom/project.toml",
            ".axiom/providers/x/provider.toml",
            ".git/hooks/pre-commit",
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
}
