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
use axiom_protocol::ErrorKind;
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

pub struct Gate {
    approvers: Arc<dyn Approvers>,
    pending: Mutex<HashMap<String, Pending>>,
    /// "Allow for session" grants, by principal.
    grants: Mutex<HashMap<String, Vec<PolicyRule>>>,
    next: AtomicU64,
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
        session: Option<&SessionId>,
        prepared: &Prepared,
        policies: &Policies,
        checkpoint_planned: bool,
        cancel: &CancellationToken,
    ) -> Result<(Allowed, Via, Option<String>), Refusal> {
        let grants = lock(&self.grants)
            .get(principal)
            .cloned()
            .unwrap_or_default();
        let call = policy::Call {
            tool: &prepared.name,
            effect: prepared.effect,
            paths: &prepared.targets.paths,
            argv: prepared.targets.argv.as_deref(),
        };
        let (decision, rule) =
            policy::decide(&policies.user, &grants, policies.workspace.as_ref(), &call);
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
                let can_remember = !matches!(prepared.effect, Effect::Credentials | Effect::Deploy);
                let request = ApprovalRequest {
                    request_id: format!("r{}", self.next.fetch_add(1, Ordering::Relaxed)),
                    session_id: session.cloned(),
                    principal: principal.to_string(),
                    tool: prepared.name.clone(),
                    effect: prepared.effect,
                    targets: prepared.targets.display(),
                    argv: prepared.targets.argv.clone(),
                    preview: prepared.targets.preview.clone(),
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
                        self.grant(principal, prepared);
                        Ok((Allowed::granted(), Via::Approved(decision), rule))
                    }
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

    /// The "allow for session" rule: this tool on exactly these targets.
    fn grant(&self, principal: &str, p: &Prepared) {
        let rule = PolicyRule {
            effect: Some(p.effect),
            tool: Some(p.name.clone()),
            path: (p.targets.paths.len() == 1).then(|| p.targets.paths[0].0.clone()),
            argv_prefix: p.targets.argv.clone(),
            decision: Decision::Allow,
        };
        lock(&self.grants)
            .entry(principal.to_string())
            .or_default()
            .push(rule);
    }

    pub fn clear_grants(&self, principal: &str) {
        lock(&self.grants).remove(principal);
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

/// One audit record per call (`<data dir>/audit/<workspace hash>.jsonl`, append-only).
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
            );
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
    let (name, effect, targets) = (
        prepared.name.clone(),
        prepared.effect,
        prepared.targets.display(),
    );
    let planned = effect != Effect::Read;
    let (allowed, via, rule) = match gate
        .check(
            &ctx.principal,
            session,
            &prepared,
            policies,
            planned,
            &ctx.cancel,
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
    .with_rule(rule)
}

impl CallOutcome {
    fn with_rule(mut self, rule: Option<String>) -> Self {
        self.rule = rule;
        self
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn finish(
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
        audit_append(path, &record);
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
