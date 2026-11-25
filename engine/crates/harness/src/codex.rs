//! Codex, by its app-server: lines of JSON-RPC without the version, on the stable part of its protocol
//! (`experimentalApi` is never asked for).
//!
//! Its own sandbox contains it, and what it says it runs with is compared with what it was told
//! before the first prompt. Left out: signing in, rate-limit resets, reviews, fork and revert,
//! dynamic tools, questions of tools to a person (`item/tool/requestUserInput`) and of MCP servers
//! (both are refused), amendments to its own rules, prompts other than text, and how full its context
//! is (its documentation does not say how its figures add up to that).

use std::collections::BTreeMap;

use axiom_policy::{Access, Place, audit::Containment};
use axiom_proto::{ModelTotal, Outcome, Provenance, Usage};
use serde_json::{Value, json};

use crate::{
    CLIENT, Site, Start, Trust,
    event::*,
    rpc::{Calls, Fault, Msg, NO_METHOD, read},
    talk::{Keep, Out, Talk, Word, has, id, list, owned, text},
    vars::only,
};

pub(crate) const ARGS: [&str; 1] = ["app-server"];

/// The sandbox for each access, as `thread/start` takes it and as its answer names it. With
/// `on-request`, what the sandbox allows is done without asking, and what it does not is asked for;
/// in `Read` the answer to that is always no. (Its policy that asked more, `untrusted`, is retired.)
fn sandbox(a: Access) -> (&'static str, &'static str) {
    match a {
        Access::Read | Access::Ask => ("read-only", "readOnly"),
        Access::Work => ("workspace-write", "workspaceWrite"),
    }
}

/// learn.chatgpt.com/docs/config-file/config-reference, read 2026-11-26: how it answers and how much it
/// reads, and nothing it may do.
pub(crate) fn passes(extra: &[String]) -> Result<(), Error> {
    let keys = [
        "model_verbosity",
        "model_reasoning_summary",
        "model_context_window",
        "model_auto_compact_token_limit",
        "tool_output_token_limit",
        "service_tier",
    ];
    let known = |_: &str, v: &str| v.split_once('=').is_some_and(|(k, _)| keys.contains(&k));
    only(extra, &["-c", "--config"], known).map_err(Error::NotPassed)
}

pub(crate) fn launch(s: &Start) -> Result<Launch, Error> {
    // A project's own configuration takes effect where the person's says the project is trusted.
    if s.trust != Trust::Given {
        return Err(Error::Untrusted);
    }
    Ok(Launch { args: ARGS.map(String::from).into(), containment: Containment::Harness })
}

#[derive(Debug)]
enum Want {
    Initialize,
    Models,
    Thread,
    Turn,
    Steer,
    Other,
}

/// What an answer is made of.
#[derive(Debug)]
enum Form {
    Decision,
    /// What was asked for, to be given back as granted.
    Permissions(Value),
}

#[derive(Debug)]
struct Asked {
    rpc: Value,
    form: Form,
    options: Vec<Choice>,
}

#[derive(Debug)]
pub(crate) struct Codex {
    calls: Calls<Want>,
    program: Place,
    start: Option<Start>,
    version: Option<String>,
    caps: Caps,
    thread: Option<String>,
    ready: bool,
    /// A turn was started, or began by itself.
    busy: bool,
    turn: Option<String>,
    /// It is to be interrupted as soon as its id is known.
    stop: bool,
    asks: BTreeMap<String, Asked>,
    made: u64,
    error: Option<String>,
}

impl Codex {
    pub(crate) fn new(site: &Site, start: Option<Start>) -> Self {
        Self {
            calls: Calls::new(false),
            program: site.program.place().clone(),
            start,
            version: None,
            caps: Caps {
                resume: true,
                steer: Steer::Yes,
                interrupt: true,
                approvals: true,
                usage: true,
                quota: true,
                plan: true,
                access: vec![Access::Read, Access::Ask, Access::Work],
                models: Names::Any,
                efforts: Names::Any,
            },
            thread: None,
            ready: false,
            busy: false,
            turn: None,
            stop: false,
            asks: BTreeMap::new(),
            made: 0,
            error: None,
        }
    }
}

impl Talk for Codex {
    fn open(&mut self, o: &mut Out) {
        let (name, title, version) = CLIENT;
        let params = json!({
            "clientInfo": {"name": name, "title": title, "version": version},
            "capabilities": {"experimentalApi": false},
        });
        self.calls.call("initialize", params, Want::Initialize, o);
    }

    fn heard(&mut self, v: Value, o: &mut Out) {
        match read(v) {
            Msg::Reply { id, result } => match self.calls.answered(&id) {
                Some(w) => self.reply(w, result, o),
                None => o.unknown("a reply to nothing", &json!({"id": id})),
            },
            Msg::Note { method, params } => self.note(&method, &params, o),
            Msg::Call { id, method, params } => self.asked(id, &method, &params, o),
            Msg::Bad => o.bad = true,
        }
    }

    fn prompt(&mut self, text: &str, _: By, o: &mut Out) -> Result<Took, Refused> {
        let Some(thread) = self.thread.clone().filter(|_| self.ready) else {
            return Err(Refused::NotReady);
        };
        let input = json!([{"type": "text", "text": text}]);
        if self.busy {
            // It must be told which turn is meant.
            let Some(turn) = &self.turn else { return Err(Refused::NotReady) };
            let params = json!({"threadId": thread, "expectedTurnId": turn, "input": input});
            self.calls.call("turn/steer", params, Want::Steer, o);
            return Ok(Took::Steered);
        }
        let mut params = json!({"threadId": thread, "input": input});
        if let Some(e) = self.start.as_ref().and_then(|s| s.effort.as_ref()) {
            params["effort"] = json!(e);
        }
        (self.busy, self.error) = (true, None);
        o.event(Event::Began { by_agent: false });
        self.calls.call("turn/start", params, Want::Turn, o);
        Ok(Took::Began)
    }

    fn answer(&mut self, id: &str, a: &Answer, o: &mut Out) -> Result<(), Refused> {
        let q = self.asks.get(id).ok_or(Refused::NotAsked)?;
        let choice = match a {
            Answer::Choose(c) if q.options.iter().any(|x| x.id == *c) => c.as_str(),
            Answer::Choose(_) => return Err(Refused::NoSuchOption),
            Answer::Refuse => "decline",
        };
        let answer = match &q.form {
            Form::Decision => json!({"decision": choice}),
            Form::Permissions(p) if choice == "accept" => {
                json!({"permissions": p, "scope": "turn"})
            }
            // Nothing of it.
            Form::Permissions(_) => json!({"permissions": {}, "scope": "turn"}),
        };
        let rpc = q.rpc.clone();
        self.asks.remove(id);
        self.calls.reply(&rpc, answer, o);
        Ok(())
    }

    fn interrupt(&mut self, o: &mut Out) -> bool {
        if self.turn.is_none() {
            self.stop = true;
        }
        self.interrupt_turn(o);
        true
    }

    fn close(&mut self, _: &mut Out) {}
}

impl Codex {
    fn interrupt_turn(&mut self, o: &mut Out) {
        if let (Some(th), Some(t)) = (&self.thread, &self.turn) {
            let params = json!({"threadId": th, "turnId": t});
            self.calls.call("turn/interrupt", params, Want::Other, o);
        }
    }

    fn reply(&mut self, w: Want, r: Result<Value, Fault>, o: &mut Out) {
        match (w, r) {
            (Want::Initialize, Ok(v)) => {
                // The string it presents itself with: its name, its version, and more.
                self.version = text(&v, "/userAgent").and_then(version);
                self.calls.note("initialized", json!({}), o);
                let params = json!({"limit": 100, "includeHidden": false});
                self.calls.call("model/list", params, Want::Models, o);
            }
            (Want::Initialize, Err(f)) => {
                o.end(Why::Refused, format!("it would not begin: {}", f.message));
            }
            (Want::Models, r) => {
                // Without a list, names are taken as they are.
                if let Ok(v) = r {
                    self.models(&v);
                }
                self.begin(o);
            }
            (Want::Thread, Ok(v)) => self.in_thread(&v, o),
            (Want::Thread, Err(f)) => {
                o.end(Why::Refused, format!("it would not open the thread: {}", f.message));
            }
            (Want::Turn, Ok(v)) => self.in_turn(id(&v, "/turn/id"), o),
            (Want::Turn, Err(f)) => {
                (self.busy, self.stop) = (false, false);
                done(Outcome::Failed, "error", Some(f.message), o);
            }
            (Want::Steer, Err(f)) => o.event(Event::NotHeard { why: f.message }),
            (Want::Steer | Want::Other, _) => {}
        }
    }

    fn models(&mut self, v: &Value) {
        let mut all = vec![];
        let mut every = vec![];
        for m in list(v, "/data").iter().take(200) {
            let Some(id) = id(m, "/model").or(id(m, "/id")) else { continue };
            let efforts: Vec<String> = list(m, "/supportedReasoningEfforts")
                .iter()
                .filter_map(|e| crate::talk::id(e, "/reasoningEffort"))
                .collect();
            for e in &efforts {
                if !every.iter().any(|x: &Name| x.id == *e) {
                    every.push(Name { id: e.clone(), ..Name::default() });
                }
            }
            all.push(Name { id, label: owned(m, "/displayName"), efforts });
        }
        if !all.is_empty() {
            (self.caps.models, self.caps.efforts) = (Names::Of(all), Names::Of(every));
        }
    }

    fn begin(&mut self, o: &mut Out) {
        let Some(s) = &self.start else {
            o.probe = Some(Probe {
                program: self.program.clone(),
                name: Some("Codex".into()),
                version: self.version.clone(),
                protocol: PROTOCOL.into(),
                caps: self.caps.clone(),
            });
            return;
        };
        let mut params = json!({
            "cwd": s.workspace.as_str(),
            "sandbox": sandbox(s.access).0,
            "approvalPolicy": "on-request",
            // Not a reviewer of its own in a person's place.
            "approvalsReviewer": "user",
        });
        if let Some(m) = &s.model {
            params["model"] = json!(m);
        }
        let method = match &s.resume {
            Some(t) => {
                params["threadId"] = json!(t);
                // The journal has them.
                params["excludeTurns"] = json!(true);
                "thread/resume"
            }
            None => "thread/start",
        };
        self.calls.call(method, params, Want::Thread, o);
    }

    fn in_thread(&mut self, v: &Value, o: &mut Out) {
        let Some(s) = &self.start else { return };
        let Some(thread) = id(v, "/thread/id") else {
            return o.end(Why::Nonsense, "it opened a thread without an id");
        };
        // What a person's or an organisation's settings made of it may be something else.
        let runs = text(v, "/sandbox/type").unwrap_or("nothing it names");
        if runs != sandbox(s.access).1 {
            return o.end(
                Why::Boundary,
                format!("it runs with the sandbox {runs}, which is not what it was told"),
            );
        }
        if text(v, "/approvalPolicy") != Some("on-request") {
            return o.end(Why::Boundary, "it does not ask as it was told to");
        }
        if text(v, "/approvalsReviewer").is_some_and(|r| r != "user") {
            return o.end(Why::Boundary, "it lets something else than a person answer");
        }
        self.thread = Some(thread.clone());
        self.ready = true;
        o.event(Event::Ready(Box::new(Ready {
            session: thread,
            protocol: PROTOCOL.into(),
            version: owned(v, "/thread/cliVersion").or(self.version.clone()),
            model: owned(v, "/model"),
            caps: self.caps.clone(),
        })));
    }

    fn in_turn(&mut self, turn: Option<String>, o: &mut Out) {
        if !self.busy || turn.is_none() {
            return;
        }
        self.turn = turn;
        if std::mem::take(&mut self.stop) {
            self.interrupt_turn(o);
        }
    }

    /// Another thread than ours, whose agent it is.
    fn scope(&self, p: &Value) -> Option<String> {
        let t = id(p, "/threadId")?;
        (Some(&t) != self.thread.as_ref()).then_some(t)
    }

    fn note(&mut self, method: &str, p: &Value, o: &mut Out) {
        let scope = self.scope(p);
        let ours = scope.is_none();
        let item = || id(p, "/itemId").map(|i| scoped(&scope, &i));
        let delta = || owned(p, "/delta").unwrap_or_default();
        let say = |id: String, kind, o: &mut Out| {
            let parent = scope.clone();
            o.event(Event::Say(Say { id, kind, text: delta(), whole: false, parent }));
        };
        match method {
            "turn/started" if ours => {
                if !self.busy {
                    (self.busy, self.error) = (true, None);
                    o.event(Event::Began { by_agent: true });
                }
                self.in_turn(id(p, "/turn/id"), o);
            }
            "turn/completed" if ours => self.ended(p, o),
            "item/started" | "item/completed" => {
                self.item(p, method == "item/completed", &scope, o);
            }
            "item/agentMessage/delta" => {
                if let Some(id) = item() {
                    say(id, Kind::Prose, o);
                }
            }
            "item/reasoning/summaryTextDelta" => {
                if let Some(id) = item() {
                    say(id, Kind::Reasoning, o);
                }
            }
            "item/reasoning/summaryPartAdded" => {
                let first = p.pointer("/summaryIndex").and_then(Value::as_u64) == Some(0);
                if let Some(id) = item().filter(|_| !first) {
                    let parent = scope.clone();
                    let (kind, text) = (Kind::Reasoning, "\n\n".to_owned());
                    o.event(Event::Say(Say { id, kind, text, whole: false, parent }));
                }
            }
            "item/reasoning/textDelta" => {
                if let Some(id) = item() {
                    say(raw(&id), Kind::Reasoning, o);
                }
            }
            "item/commandExecution/outputDelta" => {
                if let Some(id) = item() {
                    o.event(Event::ToolOutput { id, text: delta(), whole: false, cut: 0 });
                }
            }
            "item/mcpToolCall/progress" => {
                if let Some(id) = item() {
                    let text = owned(p, "/message").unwrap_or_default() + "\n";
                    o.event(Event::ToolOutput { id, text, whole: false, cut: 0 });
                }
            }
            "item/fileChange/patchUpdated" => {
                if let Some(id) = item() {
                    let diffs = changes(p);
                    let t = Tool { id, diffs, parent: scope, ..Tool::default() };
                    o.event(Event::ToolUpdated(Box::new(t)));
                }
            }
            "turn/plan/updated" if ours => {
                let steps = list(p, "/plan").iter().map(|s| {
                    let at = match text(s, "/status") {
                        Some("pending") => Progress::Todo,
                        Some("inProgress") => Progress::Doing,
                        Some("completed") => Progress::Done,
                        _ => Progress::Unknown,
                    };
                    (owned(s, "/step").unwrap_or_default(), at)
                });
                let (id, steps) = ("plan".to_owned(), steps.collect());
                o.event(Event::Plan(Plan { id, steps, text: owned(p, "/explanation") }));
            }
            "thread/tokenUsage/updated" if ours => usage(p, o),
            "account/rateLimits/updated" => limits(p, o),
            "serverRequest/resolved" => {
                let rpc = p.pointer("/requestId");
                let id = self.asks.iter().find(|a| Some(&a.1.rpc) == rpc).map(|a| a.0.clone());
                if let Some(id) = id {
                    self.asks.remove(&id);
                    o.event(Event::AskClosed { id, how: Settled::Withdrawn });
                }
            }
            "error" if ours => {
                let words = fault(p.pointer("/error"));
                if p.pointer("/willRetry") == Some(&json!(true)) {
                    o.notice(Level::Warning, format!("It tries again: {words}"));
                } else {
                    self.error = Some(words);
                }
            }
            "warning" | "guardianWarning" => {
                o.notice(Level::Warning, text(p, "/message").unwrap_or_default());
            }
            "configWarning" => o.notice(Level::Warning, summary(p)),
            "deprecationNotice" => o.notice(Level::Info, summary(p)),
            "model/rerouted" if ours => {
                let to = text(p, "/toModel").unwrap_or("another model");
                o.notice(Level::Warning, format!("Another model answers: {to}"));
            }
            "thread/closed" if ours => o.end(Why::Died, "it closed the thread"),
            m if QUIET.contains(&m) => {}
            m => o.unknown(m, p),
        }
    }

    fn ended(&mut self, p: &Value, o: &mut Out) {
        // One that is not the one that runs changes nothing.
        let turn = id(p, "/turn/id");
        if !self.busy || (self.turn.is_some() && turn != self.turn) {
            return;
        }
        (self.busy, self.turn, self.stop) = (false, None, false);
        let status = text(p, "/turn/status").unwrap_or_default();
        let error = p.pointer("/turn/error").filter(|e| !e.is_null()).map(|e| fault(Some(e)));
        let error = error.or(self.error.take());
        match status {
            "completed" => done(Outcome::Completed, status, None, o),
            "interrupted" => done(Outcome::Cancelled, status, None, o),
            "failed" => {
                let words = error.unwrap_or_else(|| "it failed and did not say how".into());
                done(Outcome::Failed, status, Some(words), o);
            }
            s => done(Outcome::Unknown, s, error, o),
        }
    }

    fn item(&mut self, p: &Value, completed: bool, scope: &Option<String>, o: &mut Out) {
        let Some(i) = p.pointer("/item") else { return o.unknown("item", p) };
        let Some(id) = id(i, "/id").map(|x| scoped(scope, &x)) else {
            return o.unknown("an item without an id", i);
        };
        let kind = text(i, "/type").unwrap_or_default();
        let parent = scope.clone();
        let whole = |id: String, kind, text: String, o: &mut Out| {
            let parent = scope.clone();
            o.event(Event::Say(Say { id: id.clone(), kind, text, whole: true, parent }));
            o.event(Event::Said { id });
        };
        let (name, title, args) = match kind {
            "agentMessage" => {
                if completed {
                    whole(id, Kind::Prose, owned(i, "/text").unwrap_or_default(), o);
                }
                return;
            }
            "reasoning" => {
                if completed {
                    let join = |at, by: &str| {
                        let all: Vec<_> = list(i, at).iter().filter_map(Value::as_str).collect();
                        all.join(by)
                    };
                    whole(id.clone(), Kind::Reasoning, join("/summary", "\n\n"), o);
                    whole(raw(&id), Kind::Reasoning, join("/content", ""), o);
                }
                return;
            }
            "plan" => {
                if completed {
                    let text = owned(i, "/text");
                    o.event(Event::Plan(Plan { id, steps: vec![], text }));
                }
                return;
            }
            "contextCompaction" => {
                if completed {
                    o.notice(Level::Info, "The agent shortened its context");
                }
                return;
            }
            // What was said to it is on record.
            "userMessage" | "hookPrompt" | "functionCallOutput" | "enteredReviewMode"
            | "exitedReviewMode" => return,
            "commandExecution" => (
                "command".to_owned(),
                owned(i, "/command"),
                json!({"command": i.pointer("/command"), "cwd": i.pointer("/cwd")}),
            ),
            "fileChange" => ("file change".to_owned(), None, Value::Null),
            "mcpToolCall" => (
                format!(
                    "{}/{}",
                    text(i, "/server").unwrap_or("?"),
                    text(i, "/tool").unwrap_or("?")
                ),
                None,
                i.pointer("/arguments").cloned().unwrap_or_default(),
            ),
            "dynamicToolCall" => (
                owned(i, "/tool").unwrap_or_default(),
                None,
                i.pointer("/arguments").cloned().unwrap_or_default(),
            ),
            "webSearch" => (
                "web search".to_owned(),
                owned(i, "/query"),
                json!({"query": i.pointer("/query"), "action": i.pointer("/action")}),
            ),
            "collabAgentToolCall"
            | "subAgentActivity"
            | "imageView"
            | "imageGeneration"
            | "sleep" => (kind.to_owned(), owned(i, "/prompt"), i.clone()),
            kind => return o.unknown(&format!("item {kind}"), i),
        };
        let args = (!args.is_null()).then_some(args);
        let diffs = changes(i);
        let t =
            Tool { id: id.clone(), name, title, args, args_cut: 0, diffs: diffs.clone(), parent };
        o.event(Event::ToolStarted(Box::new(t)));
        if !completed {
            return;
        }
        let (outcome, error) = match text(i, "/status") {
            Some("completed") => (Outcome::Completed, None),
            Some("failed") => (Outcome::Failed, owned(i, "/error/message")),
            Some("declined") => (Outcome::Cancelled, Some("it was not allowed".to_owned())),
            // Kinds without one have ended, and that is all.
            None => (Outcome::Completed, None),
            Some(_) => (Outcome::Unknown, None),
        };
        let code = i.pointer("/exitCode").and_then(Value::as_i64).filter(|c| *c != 0);
        let error = error.or(code.map(|c| format!("exit {c}")));
        let mcp = i.pointer("/result/content").filter(|r| !r.is_null()).map(|_| {
            list(i, "/result/content").iter().filter_map(|c| text(c, "/text")).collect::<String>()
        });
        let output = owned(i, "/aggregatedOutput").or(mcp);
        let end = ToolEnd { id, outcome, output, output_cut: 0, diffs, error };
        o.event(Event::ToolEnded(Box::new(end)));
    }

    fn asked(&mut self, rpc: Value, method: &str, p: &Value, o: &mut Out) {
        if self.asks.values().any(|a| a.rpc == rpc) || rpc.as_str().is_some_and(|s| s.len() > 256) {
            return o.unknown("a question with an id that cannot be used", p);
        }
        let choice = |id: &str, name: &str, kind| Choice { id: id.into(), name: name.into(), kind };
        let mut options = vec![choice("accept", "Allow once", ChoiceKind::AllowOnce)];
        let session = choice("acceptForSession", "Allow for this session", ChoiceKind::AllowAlways);
        let no = choice("decline", "Do not allow", ChoiceKind::RejectOnce);
        let stop = choice("cancel", "Do not allow, and stop the turn", ChoiceKind::RejectOnce);
        let (title, form) = match method {
            "item/commandExecution/requestApproval" => {
                // Other kinds than a plain command take fewer answers.
                let plain = text(p, "/kind").is_none_or(|k| k == "command");
                options.extend(plain.then_some(session));
                options.extend([no, stop]);
                let title = match (text(p, "/networkApprovalContext/host"), text(p, "/command")) {
                    (Some(h), _) => format!("Connect to {h}"),
                    (None, Some(c)) => c.to_owned(),
                    _ => "Run a command".to_owned(),
                };
                (title, Form::Decision)
            }
            "item/fileChange/requestApproval" => {
                options.extend([session, no, stop]);
                ("Change files".to_owned(), Form::Decision)
            }
            "item/permissions/requestApproval" => {
                options.push(no);
                let asked = p.pointer("/permissions").cloned().unwrap_or_default();
                ("More permissions, for this turn".to_owned(), Form::Permissions(asked))
            }
            "mcpServer/elicitation/request" => {
                let decline = json!({"action": "decline", "content": null});
                self.calls.reply(&rpc, decline, o);
                let words = text(p, "/message").unwrap_or_default();
                return o.notice(
                    Level::Warning,
                    format!("Not asked: an MCP server wants to know: {words}"),
                );
            }
            _ => {
                self.calls.refuse(&rpc, NO_METHOD, "not offered", o);
                return o.unknown(method, p);
            }
        };
        let scope = self.scope(p);
        self.made += 1;
        let qid = format!("q{}", self.made);
        let q = Question {
            id: qid.clone(),
            tool: id(p, "/itemId").map(|i| scoped(&scope, &i)),
            title,
            detail: owned(p, "/reason"),
            subject: p.clone(),
            options: options.clone(),
            by: scope,
        };
        self.asks.insert(qid, Asked { rpc, form, options });
        o.event(Event::Asked(Box::new(q)));
    }
}

pub(crate) const PROTOCOL: &str = "codex app-server";

/// Of the protocol's schema, and nothing for a transcript.
const QUIET: &[&str] = &[
    "thread/started",
    "thread/status/changed",
    "thread/readState/changed",
    "thread/archived",
    "thread/unarchived",
    "thread/deleted",
    "thread/name/updated",
    "thread/section/changed",
    "thread/attachment/updated",
    "thread/goal/updated",
    "thread/goal/cleared",
    "thread/prediction/updated",
    "thread/queue/changed",
    "thread/project/updated",
    "thread/settings/updated",
    "thread/environment/connected",
    "thread/environment/disconnected",
    "thread/compacted",
    "thread/closed",
    "thread/tokenUsage/updated",
    "project/changed",
    "skills/changed",
    "turn/started",
    "turn/completed",
    "turn/diff/updated",
    "turn/plan/updated",
    "turn/moderationMetadata",
    "hook/started",
    "hook/completed",
    "item/plan/delta",
    "item/autoApprovalReview/started",
    "item/autoApprovalReview/completed",
    "item/commandExecution/terminalInteraction",
    "item/fileChange/outputDelta",
    "mcpServer/startupStatus/updated",
    "account/updated",
    "app/list/updated",
    "fs/changed",
    "model/rerouted",
    "model/verification",
    "model/safetyBuffering/updated",
    "error",
];

fn done(outcome: Outcome, stop: &str, error: Option<String>, o: &mut Out) {
    o.say.push(Word::Done(Done { outcome, stop: stop.to_owned(), error }, Keep::Nothing));
}

fn scoped(scope: &Option<String>, id: &str) -> String {
    match scope {
        Some(s) => format!("{s}/{id}"),
        None => id.to_owned(),
    }
}

/// Reasoning itself, beside the summary of it.
fn raw(id: &str) -> String {
    format!("{id}#raw")
}

fn summary(p: &Value) -> String {
    let more = text(p, "/details").map(|d| format!(": {d}")).unwrap_or_default();
    format!("{}{more}", text(p, "/summary").unwrap_or_default())
}

fn fault(e: Option<&Value>) -> String {
    let Some(e) = e else { return "it did not say what failed".into() };
    let kind = match e.pointer("/codexErrorInfo") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Object(m)) => m.keys().next().cloned(),
        _ => None,
    };
    let mut words = text(e, "/message").unwrap_or("it did not say what failed").to_owned();
    if let Some(k) = kind {
        words += &format!(" ({k})");
    }
    words
}

fn changes(v: &Value) -> Vec<Diff> {
    let one = |c: &Value| Diff {
        path: owned(c, "/path").unwrap_or_default(),
        patch: owned(c, "/diff"),
        ..Diff::default()
    };
    list(v, "/changes").iter().take(256).map(one).collect()
}

fn usage(p: &Value, o: &mut Out) {
    if !has(p, "/tokenUsage/last") {
        return o.unknown("usage", p);
    }
    let of = |what: &str| {
        let n = |k: &str| p.pointer(&format!("/tokenUsage/{what}/{k}")).and_then(Value::as_u64);
        Usage {
            input: n("inputTokens"),
            cache_read: n("cachedInputTokens"),
            cache_write: n("cacheWriteInputTokens"),
            output: n("outputTokens"),
            reasoning: n("reasoningOutputTokens"),
            ..Usage::default()
        }
    };
    let total = has(p, "/tokenUsage/total").then(|| ModelTotal {
        model: None,
        usage: of("total"),
        cost: None,
    });
    // What the last is the last of, its documentation does not say.
    o.event(Event::Usage(Spent {
        usage: of("last"),
        totals: total.into_iter().collect(),
        provenance: Some(Provenance::Reported),
        ..Spent::default()
    }));
}

/// Only what is in it is new.
fn limits(p: &Value, o: &mut Out) {
    let name = text(p, "/rateLimits/limitName").or(text(p, "/rateLimits/limitId"));
    for w in ["primary", "secondary"] {
        let Some(l) = p.pointer(&format!("/rateLimits/{w}")).filter(|l| !l.is_null()) else {
            continue;
        };
        let n = |at: &str| l.pointer(at).and_then(Value::as_u64);
        o.event(Event::Limit(Limit {
            name: format!("{} {w}", name.unwrap_or("plan")),
            used: n("/usedPercent").map(|u| u as f64 / 100.0),
            resets_at: n("/resetsAt"),
            minutes: n("/windowDurationMins"),
            status: owned(p, "/rateLimits/rateLimitReachedType"),
        }));
    }
}

/// The first thing that looks like one.
fn version(s: &str) -> Option<String> {
    let v = s
        .split(|c: char| c == '/' || c.is_whitespace())
        .find(|w| w.contains('.') && w.starts_with(|c: char| c.is_ascii_digit()))?;
    Some(crate::frames::cut(v, 64).0.to_owned())
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
