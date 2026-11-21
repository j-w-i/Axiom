//! Verification (`ui/verify`; `inspection.md` §Verification, ADR-0019 §1): after a change, wait
//! for the page to settle, find the stated nodes again, evaluate the stated checks, add the
//! daemon's own, and give a verdict that is `verified` only when a stated geometry or property
//! check passed and nothing failed. M5 has no pixel capture: the pixel checks are `skipped`.

use crate::session::now_ms;
use crate::{Inspect, InspectError, SNAPSHOT_TIMEOUT};
use axiom_protocol::inspection::{
    Check, CheckKind, NodeId, NodeRef, Rect, Snapshot, SnapshotParams, UiNode, Verdict,
    VerificationReport,
};
use axiom_protocol::preview::PreviewId;
use axiom_protocol::ui::{Assertion, Comparison, Edge, NodeTarget, VerifyParams};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

/// `eq` holds within this many logical pixels.
pub const TOLERANCE: f64 = 0.5;
/// Quiet time before the page counts as settled.
pub const QUIET: Duration = Duration::from_millis(150);

/// What the page looked like before and after the change, and what went wrong meanwhile.
#[derive(Debug, Clone)]
pub struct Observed {
    pub before: Option<Snapshot>,
    pub after: Snapshot,
    /// Computed properties of the nodes property assertions name (after the change).
    pub properties: HashMap<NodeId, BTreeMap<String, Value>>,
    pub runtime_errors: Vec<String>,
    /// Build errors since the change (`None`: unknown).
    pub build_errors: Option<Vec<String>>,
    /// The page did not settle in time.
    pub unsettled: bool,
}

/// Finds a stated node in a snapshot: its stable key; else its node id (valid while the page
/// has not changed); else, from the node as it was before, the same type, text and ancestor
/// types, when exactly one node matches. Never a guess between several.
pub fn find<'a>(
    t: &NodeTarget,
    before: Option<&Snapshot>,
    after: &'a Snapshot,
) -> Option<&'a UiNode> {
    if let Some(k) = &t.stable_key {
        if let Some(n) = after
            .nodes
            .iter()
            .find(|n| n.stable_key.as_deref() == Some(k))
        {
            return Some(n);
        }
    }
    if let Some(id) = &t.node {
        if let Some(n) = after.nodes.iter().find(|n| &n.id == id) {
            return Some(n);
        }
    }
    let before = before?;
    let old = before.nodes.iter().find(|n| {
        t.stable_key.is_some() && n.stable_key == t.stable_key
            || t.node.as_ref().is_some_and(|id| &n.id == id)
    })?;
    let path = |s: &Snapshot, n: &UiNode| -> Vec<String> {
        let by: HashMap<&NodeId, &UiNode> = s.nodes.iter().map(|x| (&x.id, x)).collect();
        let mut out = Vec::new();
        let mut p = n.parent.as_ref();
        while let Some(id) = p {
            let Some(x) = by.get(id) else { break };
            out.push(x.type_name.clone());
            p = x.parent.as_ref();
            if out.len() > 64 {
                break;
            }
        }
        out
    };
    let want = path(before, old);
    let mut matches = after.nodes.iter().filter(|n| {
        n.type_name == old.type_name
            && (old.text.is_some() && n.text == old.text
                || old.name.is_some() && n.name == old.name)
            && path(after, n) == want
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn edge(b: &Rect, e: Edge) -> f64 {
    match e {
        Edge::Left => b.x,
        Edge::Right => b.x + b.width,
        Edge::Top => b.y,
        Edge::Bottom => b.y + b.height,
        Edge::Width => b.width,
        Edge::Height => b.height,
        Edge::CenterX => b.x + b.width / 2.0,
        Edge::CenterY => b.y + b.height / 2.0,
    }
}

fn holds(a: f64, op: Comparison, b: f64) -> bool {
    match op {
        Comparison::Eq => (a - b).abs() <= TOLERANCE,
        Comparison::Lt => a < b - TOLERANCE,
        Comparison::Le => a <= b + TOLERANCE,
        Comparison::Gt => a > b + TOLERANCE,
        Comparison::Ge => a >= b - TOLERANCE,
    }
}

fn op_text(op: Comparison) -> &'static str {
    match op {
        Comparison::Eq => "=",
        Comparison::Lt => "<",
        Comparison::Le => "≤",
        Comparison::Gt => ">",
        Comparison::Ge => "≥",
    }
}

fn edge_text(e: Edge) -> &'static str {
    match e {
        Edge::Left => "left",
        Edge::Right => "right",
        Edge::Top => "top",
        Edge::Bottom => "bottom",
        Edge::Width => "width",
        Edge::Height => "height",
        Edge::CenterX => "centre x",
        Edge::CenterY => "centre y",
    }
}

fn name_of(t: &NodeTarget) -> String {
    t.stable_key
        .clone()
        .or_else(|| t.node.as_ref().map(|n| format!("node {}", n.0)))
        .unwrap_or_else(|| "an unnamed node".into())
}

fn not_found(kind: CheckKind, t: &NodeTarget) -> Check {
    Check {
        kind,
        passed: false,
        skipped: true,
        summary: format!("{} was not found after the change", name_of(t)),
        expected: None,
        actual: None,
    }
}

/// Values compare as numbers when both read as numbers (`"16px"` and `16`), else as trimmed text.
fn same(a: &Value, b: &Value) -> bool {
    let num = |v: &Value| match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().trim_end_matches("px").trim().parse::<f64>().ok(),
        _ => None,
    };
    match (num(a), num(b)) {
        (Some(x), Some(y)) => (x - y).abs() <= 1e-6,
        _ => match (a, b) {
            (Value::String(x), Value::String(y)) => x.trim() == y.trim(),
            _ => a == b,
        },
    }
}

/// The report for [assertions] about [target], given what was observed. Pure.
pub fn evaluate(
    assertions: &[Assertion],
    target: Option<&NodeTarget>,
    o: &Observed,
) -> VerificationReport {
    let mut checks = Vec::new();
    let mut reidentified = true;
    if let Some(t) = target {
        let found = find(t, o.before.as_ref(), &o.after);
        reidentified = found.is_some();
        checks.push(Check {
            kind: CheckKind::NodeReidentified,
            passed: found.is_some(),
            skipped: false,
            summary: match found {
                Some(n) => format!("{} found again ({})", name_of(t), n.type_name),
                None => format!("{} could not be found again", name_of(t)),
            },
            expected: None,
            actual: None,
        });
    }
    for a in assertions {
        match a {
            Assertion::Geometry {
                node,
                edge: e,
                op,
                value,
                other,
                offset,
            } => {
                let Some(n) = find(node, o.before.as_ref(), &o.after) else {
                    checks.push(not_found(CheckKind::Geometry, node));
                    continue;
                };
                let Some(b) = n.bounds else {
                    checks.push(Check {
                        kind: CheckKind::Geometry,
                        passed: false,
                        skipped: true,
                        summary: format!("{} has no box", name_of(node)),
                        expected: None,
                        actual: None,
                    });
                    continue;
                };
                let actual = edge(&b, *e);
                let (expected, what) = match (other, value) {
                    (Some(r), _) => {
                        let Some(m) = find(&r.node, o.before.as_ref(), &o.after) else {
                            checks.push(not_found(CheckKind::Geometry, &r.node));
                            continue;
                        };
                        let Some(mb) = m.bounds else {
                            checks.push(not_found(CheckKind::Geometry, &r.node));
                            continue;
                        };
                        let base = edge(&mb, r.edge);
                        let what = if *offset == 0.0 {
                            format!("{} of {}", edge_text(r.edge), name_of(&r.node))
                        } else {
                            format!("{} of {} {:+}", edge_text(r.edge), name_of(&r.node), offset)
                        };
                        (base + offset, what)
                    }
                    (None, Some(v)) => (*v, format!("{v}")),
                    (None, None) => {
                        checks.push(Check {
                            kind: CheckKind::Geometry,
                            passed: false,
                            skipped: true,
                            summary: "a geometry check needs a value or another node".into(),
                            expected: None,
                            actual: None,
                        });
                        continue;
                    }
                };
                let passed = holds(actual, *op, expected);
                checks.push(Check {
                    kind: CheckKind::Geometry,
                    passed,
                    skipped: false,
                    summary: format!(
                        "{} of {} is {:.1}; expected {} {} ({:.1})",
                        edge_text(*e),
                        name_of(node),
                        actual,
                        op_text(*op),
                        what,
                        expected
                    ),
                    expected: Some(json!(expected)),
                    actual: Some(json!(actual)),
                });
            }
            Assertion::Property { node, name, equals } => {
                let Some(n) = find(node, o.before.as_ref(), &o.after) else {
                    checks.push(not_found(CheckKind::Property, node));
                    continue;
                };
                let actual = o
                    .properties
                    .get(&n.id)
                    .and_then(|p| p.get(name))
                    .cloned()
                    .unwrap_or(Value::Null);
                let passed = same(&actual, equals);
                checks.push(Check {
                    kind: CheckKind::Property,
                    passed,
                    skipped: false,
                    summary: format!(
                        "{name} of {} is {}; expected {}",
                        name_of(node),
                        if actual.is_null() {
                            "not set".to_string()
                        } else {
                            actual.to_string()
                        },
                        equals
                    ),
                    expected: Some(equals.clone()),
                    actual: Some(actual),
                });
            }
        }
    }
    match &o.build_errors {
        Some(e) => checks.push(Check {
            kind: CheckKind::BuildClean,
            passed: e.is_empty(),
            skipped: false,
            summary: if e.is_empty() {
                "no build errors since the change".into()
            } else {
                format!("{} build error(s): {}", e.len(), e[0])
            },
            expected: None,
            actual: None,
        }),
        None => checks.push(Check {
            kind: CheckKind::BuildClean,
            passed: false,
            skipped: true,
            summary: "the build's output is not known".into(),
            expected: None,
            actual: None,
        }),
    }
    checks.push(Check {
        kind: CheckKind::RuntimeClean,
        passed: o.runtime_errors.is_empty(),
        skipped: false,
        summary: if o.runtime_errors.is_empty() {
            "no runtime errors since the change".into()
        } else {
            format!(
                "{} runtime error(s): {}",
                o.runtime_errors.len(),
                o.runtime_errors[0]
            )
        },
        expected: None,
        actual: None,
    });
    for kind in [
        CheckKind::PixelsChangedInRegion,
        CheckKind::PixelsStableElsewhere,
    ] {
        checks.push(Check {
            kind,
            passed: false,
            skipped: true,
            summary: "no pixel capture in M5".into(),
            expected: None,
            actual: None,
        });
    }
    let failed = checks
        .iter()
        .any(|c| !c.passed && !c.skipped && c.kind != CheckKind::NodeReidentified);
    let stated_passed = checks.iter().any(|c| {
        c.passed && !c.skipped && matches!(c.kind, CheckKind::Geometry | CheckKind::Property)
    });
    let verdict = if failed {
        Verdict::Failed
    } else if stated_passed && reidentified && !o.unsettled {
        Verdict::Verified
    } else {
        Verdict::Inconclusive
    };
    VerificationReport {
        verdict,
        checks,
        changed_region: None,
        changed_fraction: None,
    }
}

/// `ui/verify` against a connected page: settle, snapshot, find, read properties, evaluate.
/// [build_errors] are the dev server's error lines since the change (`None`: unknown).
pub async fn verify(
    inspect: &Inspect,
    preview: &PreviewId,
    p: &VerifyParams,
    build_errors: impl FnOnce(u64) -> Option<Vec<String>>,
) -> Result<(VerificationReport, u64), InspectError> {
    let s = inspect.session(preview)?;
    let since = p.since_ms.unwrap_or_else(|| s.last_verify_ms());
    let limit = Duration::from_millis(p.timeout_ms.unwrap_or(5000).min(30_000));
    // A hot reload starts a moment after the file is written: give it the chance to.
    tokio::time::sleep(Duration::from_millis(250).min(limit)).await;
    let settled = s.quiescence(QUIET, limit).await;
    let before = s.latest_snapshot();
    let after: Snapshot = s
        .request(
            "inspect/snapshot",
            &SnapshotParams {
                root: None,
                max_nodes: None,
                include_invisible: true,
            },
            SNAPSHOT_TIMEOUT,
        )
        .await?;
    let after = s.normalizer().snapshot(after);
    let mut properties = HashMap::new();
    for a in &p.assertions {
        if let Assertion::Property { node, .. } = a {
            if let Some(n) = find(node, before.as_ref(), &after) {
                if properties.contains_key(&n.id) {
                    continue;
                }
                let d: UiNode = s
                    .request(
                        "inspect/nodeDetails",
                        &NodeRef { node: n.id.clone() },
                        crate::REQUEST_TIMEOUT,
                    )
                    .await?;
                properties.insert(n.id.clone(), d.properties);
            }
        }
    }
    let observed = Observed {
        before,
        after: after.clone(),
        properties,
        runtime_errors: s.errors_since(since),
        build_errors: build_errors(since),
        unsettled: !settled,
    };
    let report = evaluate(&p.assertions, p.target.as_ref(), &observed);
    s.set_snapshot(after);
    let now = now_ms();
    s.mark_verified(now);
    Ok((report, since))
}
