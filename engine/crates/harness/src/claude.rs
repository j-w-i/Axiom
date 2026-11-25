//! Claude Code, by its streamed JSON (`-p` with `stream-json` in and out).
//!
//! Its documentation describes the messages, the flags and that a client may drive the control
//! protocol directly; the shape of three control requests (`initialize`, `interrupt`,
//! `can_use_tool`) is from the source of the vendor's own published SDK, as research notes have it.
//! **How a program other than that SDK becomes the one that is asked is not documented**: the flag
//! for it is documented for an MCP tool only. So asking is off unless the caller turns it on
//! ([`Prompts::Stdio`]), and without it there is no `Ask`.
//!
//! Left out: plan mode and its hand-over to another mode, remembered permissions (they would be
//! written into the workspace), its own questions (`AskUserQuestion` is taken away), hooks, SDK
//! servers, rewinding, and what replaces an earlier message (`supersedes`).

use std::collections::{BTreeMap, BTreeSet};

use axiom_policy::{Access, audit::Containment};
use axiom_proto::{Cost, Covers, ModelTotal, Outcome, Provenance, Usage, limits};
use serde_json::{Value, json};

use crate::{
    Server, Start, Trust,
    event::*,
    talk::{Keep, Out, Talk, Word, has, id, list, loose, owned, text},
    tasks::Tasks,
    vars::only,
};

/// Who answers when Claude Code asks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Prompts {
    /// Nobody: what would need an answer is refused by Claude Code itself (`--permission-prompts
    /// none`, documented).
    #[default]
    Refused,
    /// A person, through Axiom: `--permission-prompt-tool stdio`, which the vendor's published SDK
    /// passes and the documentation does not describe.
    Stdio,
}

pub(crate) const PROTOCOL: &str = "claude stream-json";

/// What it is told beside its flags. `contained`: commands run in its own sandbox or not at all; what
/// runs there is not asked about.
fn settings(contained: bool, s: &Start) -> Result<Option<String>, Error> {
    let mut all = serde_json::Map::new();
    if contained {
        let sandbox =
            json!({"enabled": true, "failIfUnavailable": true, "allowUnsandboxedCommands": false});
        all.insert("sandbox".into(), sandbox);
    }
    match s.speed.as_deref() {
        None => {}
        Some("fast") => {
            all.insert("fastMode".into(), json!(true));
        }
        Some(other) => return Err(Error::BadValue("a speed", other.to_owned())),
    }
    Ok((!all.is_empty()).then(|| Value::Object(all).to_string()))
}

/// Its modes by the names its answers use.
fn mode(a: Access) -> &'static str {
    match a {
        Access::Read => "dontAsk",
        Access::Ask => "default",
        Access::Work => "acceptEdits",
    }
}

/// The model's loop alone: it has no tools but the server's, and reads no settings but its person's.
fn hands(s: &Start, server: &Server) -> Result<Vec<String>, Error> {
    let config = json!({"mcpServers": {server.name.as_str():
        {"type": "stdio", "command": server.command.as_str(), "args": server.args}}});
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        // Whatever is not allowed here is refused; whoever is behind the server does the asking.
        "--permission-mode",
        "dontAsk",
        "--permission-prompts",
        "none",
        "--tools",
        "",
        "--setting-sources",
        "user",
        "--strict-mcp-config",
        "--mcp-config",
    ]
    .map(String::from)
    .into();
    args.extend([config.to_string(), "--allowedTools".into(), format!("mcp__{}__*", server.name)]);
    if let Some(f) = &s.system {
        args.extend(["--append-system-prompt-file".into(), f.to_string()]);
    }
    Ok(args)
}

pub(crate) fn launch(p: Prompts, s: &Start) -> Result<Launch, Error> {
    if let (true, Some(server)) = (s.hands, &s.tools) {
        let mut args = hands(s, server)?;
        if let Some(all) = settings(false, s)? {
            args.extend(["--settings".into(), all]);
        }
        chosen(&mut args, s)?;
        return Ok(Launch { args, containment: Containment::None });
    }
    if s.access == Access::Ask && p == Prompts::Refused {
        return Err(Error::NoMode(s.access));
    }
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-mode",
        mode(s.access),
    ]
    .map(String::from)
    .into();
    let mut add = |all: &[&str]| args.extend(all.iter().map(|a| (*a).to_owned()));
    match p {
        Prompts::Refused => add(&["--permission-prompts", "none"]),
        Prompts::Stdio => add(&["--permission-prompt-tool", "stdio"]),
    }
    let mut containment = Containment::None;
    match s.access {
        // Nothing that changes anything: no commands, no edits, no tools of servers.
        Access::Read => {
            add(&["--tools", "Read,Glob,Grep", "--disallowedTools", "AskUserQuestion,mcp__*"]);
        }
        Access::Ask => add(&["--disallowedTools", "AskUserQuestion"]),
        Access::Work => {
            add(&["--disallowedTools", "AskUserQuestion"]);
            containment = Containment::Harness;
        }
    }
    if let Some(all) = settings(s.access == Access::Work, s)? {
        add(&["--settings", &all]);
    }
    // Who only looks has no tools of servers; whoever is asked about none of them could not use it.
    if let Some(t) = s.tools.as_ref().filter(|_| s.access == Access::Work) {
        let server = json!({"mcpServers": {t.name.as_str():
            {"type": "stdio", "command": t.command.as_str(), "args": t.args}}});
        add(&["--mcp-config", &server.to_string()]);
        add(&["--allowedTools", &format!("mcp__{}__{}", t.name, t.tool)]);
    }
    if s.trust == Trust::Withheld {
        // It shows no trust dialog in this mode, and runs what a project's settings and `.mcp.json`
        // name. So it reads neither.
        add(&["--setting-sources", "user"]);
    }
    chosen(&mut args, s)?;
    Ok(Launch { args, containment })
}

/// code.claude.com/docs/en/cli-reference, read 2026-11-26: none of these changes what it may do.
pub(crate) fn passes(extra: &[String]) -> Result<(), Error> {
    let flags = ["--fallback-model", "--max-turns", "--max-budget-usd", "--betas"];
    only(extra, &flags, |_, _| true).map_err(Error::NotPassed)
}

/// What a person chose, and the session to go on with.
fn chosen(args: &mut Vec<String>, s: &Start) -> Result<(), Error> {
    let mut add = |all: &[&str]| args.extend(all.iter().map(|a| (*a).to_owned()));
    if let Some(m) = &s.model {
        add(&["--model", m]);
    }
    if let Some(e) = &s.effort {
        add(&["--effort", e]);
    }
    match &s.resume {
        Some(r) => add(&["--resume", r]),
        None => {
            let fresh = uuid().map_err(|e| Error::BadValue("a session", e))?;
            add(&["--session-id", &fresh]);
        }
    }
    Ok(())
}

pub(crate) fn caps(p: Prompts) -> Caps {
    let mut access = vec![Access::Read, Access::Work];
    if p == Prompts::Stdio {
        access.insert(1, Access::Ask);
    }
    Caps {
        resume: true,
        steer: Steer::Yes,
        interrupt: true,
        approvals: p == Prompts::Stdio,
        usage: true,
        quota: true,
        plan: true,
        access,
        models: Names::Any,
        efforts: Names::Any,
    }
}

/// Of what `--version` says: its first word, if it is one.
pub(crate) fn version(s: &str) -> Option<String> {
    let w = s.split_whitespace().next()?;
    let parts: Vec<&str> = w.split('.').collect();
    let number = |p: &&str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    (parts.len() == 3 && w.len() <= 32 && parts.iter().all(number)).then(|| w.to_owned())
}

/// A random one, version 4.
fn uuid() -> Result<String, String> {
    let mut b = [0u8; 16];
    axiom_proto::random(&mut b)?;
    b[6] = b[6] & 0x0f | 0x40;
    b[8] = b[8] & 0x3f | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok([&h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..]].join("-"))
}

#[derive(Debug)]
enum Want {
    Initialize,
    Other,
}

#[derive(Debug)]
struct Asked {
    request: String,
    /// Given back unchanged when it is allowed.
    input: Value,
}

#[derive(Debug)]
pub(crate) struct Claude {
    prompts: Prompts,
    mode: &'static str,
    /// What the names of all its tools begin with, if it has only those it was given.
    only: Option<String>,
    model: Option<String>,
    session: String,
    caps: Caps,
    ready: bool,
    made: u64,
    wants: BTreeMap<String, Want>,
    asks: BTreeMap<String, Asked>,
    busy: bool,
    /// Prompts whose result has not come, by the id they were sent with.
    unanswered: BTreeSet<String>,
    interrupted: bool,
    cancels_queued: bool,
    /// The message that is being streamed.
    streaming: Option<String>,
    /// How many of a message's blocks have come whole: they come in order, several messages with the
    /// same id.
    blocks: BTreeMap<String, u64>,
    error: Option<String>,
    tasks: Tasks,
}

const MESSAGES: usize = 64;

impl Claude {
    pub(crate) fn new(prompts: Prompts, s: Start, l: &Launch) -> Self {
        let made = l.args.iter().position(|a| a == "--session-id");
        let made = made.and_then(|i| l.args.get(i + 1)).cloned();
        Self {
            prompts,
            mode: if s.hands { "dontAsk" } else { mode(s.access) },
            only: s.tools.as_ref().filter(|_| s.hands).map(|t| format!("mcp__{}__", t.name)),
            model: s.model,
            session: s.resume.or(made).unwrap_or_default(),
            caps: caps(prompts),
            ready: false,
            made: 0,
            wants: BTreeMap::new(),
            asks: BTreeMap::new(),
            busy: false,
            unanswered: BTreeSet::new(),
            interrupted: false,
            cancels_queued: false,
            streaming: None,
            blocks: BTreeMap::new(),
            error: None,
            tasks: Tasks::default(),
        }
    }

    fn control(&mut self, request: Value, want: Want, o: &mut Out) {
        self.made += 1;
        let id = format!("axiom-{}", self.made);
        o.send.push(json!({"type": "control_request", "request_id": id, "request": request}));
        if self.wants.len() >= 64 {
            return o.end(Why::Deaf, "it does not answer what it is asked");
        }
        self.wants.insert(id, want);
    }
}

impl Talk for Claude {
    fn open(&mut self, o: &mut Out) {
        self.control(json!({"subtype": "initialize", "hooks": null}), Want::Initialize, o);
    }

    fn heard(&mut self, v: Value, o: &mut Out) {
        match text(&v, "/type") {
            Some("control_response") => self.replied(&v, o),
            Some("control_request") => self.asked(&v, o),
            Some("control_cancel_request") => {
                let r = text(&v, "/request_id");
                let id = self.asks.iter().find(|a| Some(a.1.request.as_str()) == r);
                if let Some(id) = id.map(|a| a.0.clone()) {
                    self.asks.remove(&id);
                    o.event(Event::AskClosed { id, how: Settled::Withdrawn });
                }
            }
            Some("system") => self.system(&v, o),
            Some("stream_event") => self.streamed(&v, o),
            Some("assistant") => self.assistant(&v, o),
            Some("user") => self.results(&v, o),
            Some("result") => self.result(&v, o),
            Some("rate_limit_event") => limit(&v, o),
            Some("tool_progress" | "tool_use_summary" | "keep_alive" | "auth_status") => {}
            Some(t) => o.unknown(t, &v),
            None => o.bad = true,
        }
    }

    fn prompt(&mut self, text: &str, by: By, o: &mut Out) -> Result<Took, Refused> {
        if !self.ready {
            return Err(Refused::NotReady);
        }
        let mut m = json!({
            "type": "user",
            "session_id": self.session,
            "parent_tool_use_id": null,
            "message": {"role": "user", "content": text},
            // Delivered as it is: no files are pulled in, no command of its own is run.
            "client_composed": true,
        });
        match by {
            By::Person => m["origin"] = json!({"kind": "human"}),
            By::Agent => m["isSynthetic"] = json!(true),
        }
        let took = if self.busy { Took::Steered } else { Took::Began };
        if self.busy {
            m["priority"] = json!("next");
        } else {
            (self.busy, self.error) = (true, None);
            o.event(Event::Began { by_agent: false });
        }
        // Without an id it is one that its result does not name.
        if let Ok(id) = uuid()
            && self.unanswered.len() < 64
        {
            m["uuid"] = json!(id);
            self.unanswered.insert(id);
        }
        o.send.push(m);
        Ok(took)
    }

    fn answer(&mut self, id: &str, a: &Answer, o: &mut Out) -> Result<(), Refused> {
        if !self.asks.contains_key(id) {
            return Err(Refused::NotAsked);
        }
        let allow = match a {
            Answer::Choose(c) if c == "allow" => true,
            Answer::Choose(c) if c == "deny" => false,
            Answer::Choose(_) => return Err(Refused::NoSuchOption),
            Answer::Refuse => false,
        };
        let Some(q) = self.asks.remove(id) else { return Err(Refused::NotAsked) };
        let answer = match allow {
            true => json!({"behavior": "allow", "updatedInput": q.input}),
            false => json!({"behavior": "deny", "message": "This was not allowed."}),
        };
        respond(&q.request, json!({"subtype": "success", "response": answer}), o);
        Ok(())
    }

    fn interrupt(&mut self, o: &mut Out) -> bool {
        self.interrupted = true;
        let mut r = json!({"subtype": "interrupt"});
        if self.cancels_queued {
            r["cancel_queued"] = json!(true);
        }
        self.control(r, Want::Other, o);
        true
    }

    /// The end of its input is the end of the session.
    fn close(&mut self, _: &mut Out) {}
}

fn respond(request: &str, mut response: Value, o: &mut Out) {
    response["request_id"] = json!(request);
    o.send.push(json!({"type": "control_response", "response": response}));
}

impl Claude {
    fn replied(&mut self, v: &Value, o: &mut Out) {
        let Some(want) = text(v, "/response/request_id").and_then(|r| self.wants.remove(r)) else {
            return o.unknown("a reply to nothing", v);
        };
        match (want, text(v, "/response/subtype")) {
            (Want::Initialize, Some("success")) => {
                self.models(v);
                self.ready = true;
                o.event(Event::Ready(Box::new(Ready {
                    session: self.session.clone(),
                    protocol: PROTOCOL.into(),
                    version: owned(v, "/response/response/claude_code_version"),
                    model: self.model.clone(),
                    caps: self.caps.clone(),
                })));
            }
            (Want::Initialize, _) => {
                let words = text(v, "/response/error").unwrap_or("it did not say why");
                o.end(Why::Refused, format!("it would not begin: {words}"));
            }
            (Want::Other, _) => {}
        }
    }

    fn models(&mut self, v: &Value) {
        let mut all = vec![];
        let mut every: Vec<Name> = vec![];
        for m in list(v, "/response/response/models").iter().take(200) {
            let Some(id) = id(m, "/value") else { continue };
            let efforts: Vec<String> = list(m, "/supportedEffortLevels")
                .iter()
                .filter_map(|e| e.as_str().map(str::to_owned))
                .take(16)
                .collect();
            for e in &efforts {
                if !every.iter().any(|x| x.id == *e) {
                    every.push(Name { id: e.clone(), ..Name::default() });
                }
            }
            let fast = m.pointer("/supportsFastMode") == Some(&json!(true));
            all.push(Name { id, label: owned(m, "/displayName"), efforts, fast });
        }
        if !all.is_empty() {
            (self.caps.models, self.caps.efforts) = (Names::Of(all), Names::Of(every));
        }
    }

    fn system(&mut self, v: &Value, o: &mut Out) {
        match text(v, "/subtype") {
            Some("init") => {
                if let Some(s) = id(v, "/session_id").filter(|s| *s != self.session) {
                    self.session.clone_from(&s);
                    o.event(Event::Session(s));
                }
                self.cancels_queued = list(v, "/capabilities")
                    .iter()
                    .any(|c| c.as_str() == Some("interrupt_cancel_queued_v1"));
                // Who was told to have only these, has only these.
                if let Some(only) = &self.only
                    && let Some(own) = list(v, "/tools")
                        .iter()
                        .find(|t| !t.as_str().is_some_and(|t| t.starts_with(only.as_str())))
                {
                    let own: String = own.to_string().chars().take(80).collect();
                    o.end(Why::Boundary, format!("it has a tool it was told not to have: {own}"));
                }
                let mode = text(v, "/permissionMode").unwrap_or("none it names");
                if !self.keeps_to(mode) {
                    o.end(
                        Why::Boundary,
                        format!("it runs in the mode {mode}, which is not what it was told"),
                    );
                }
            }
            Some("status") => {
                // It may have been changed: by a plan's approval, by its own settings.
                if let Some(m) = text(v, "/permissionMode")
                    && !self.keeps_to(m)
                {
                    if loose(m) {
                        o.end(
                            Why::Boundary,
                            format!("it switched to a mode in which it does not ask: {m}"),
                        );
                    } else {
                        o.notice(Level::Warning, format!("Claude Code switched to its mode {m}"));
                    }
                }
            }
            Some("compact_boundary") => o.notice(Level::Info, "The agent shortened its context"),
            Some("informational") => {
                let level = match text(v, "/level") {
                    Some("warning") => Level::Warning,
                    _ => Level::Info,
                };
                o.notice(level, text(v, "/content").unwrap_or_default());
            }
            Some("api_retry") => {
                let n = v.pointer("/attempt").and_then(Value::as_u64).unwrap_or(0);
                let e = text(v, "/error").unwrap_or("unknown");
                o.notice(Level::Warning, format!("It tries again (attempt {n}): {e}"));
            }
            Some("permission_denied") => {
                let tool = text(v, "/tool_name").unwrap_or("a tool");
                let why = text(v, "/decision_reason").or(text(v, "/message")).unwrap_or_default();
                o.notice(Level::Warning, format!("Claude Code did not allow {tool}: {why}"));
            }
            Some("model_refusal_fallback") => {
                let to = text(v, "/fallback_model").unwrap_or("another model");
                o.notice(Level::Warning, format!("Another model answers: {to}"));
            }
            Some(s) if QUIET.contains(&s) => {}
            s => o.unknown(&format!("system {}", s.unwrap_or_default()), v),
        }
    }

    /// `manual` is another name for `default`.
    fn keeps_to(&self, mode: &str) -> bool {
        mode == self.mode || (mode == "manual" && self.mode == "default")
    }

    /// What the agent does without having been told anything is a turn of its own.
    fn at_work(&mut self, v: &Value, o: &mut Out) {
        if !self.busy && !has(v, "/parent_tool_use_id") {
            (self.busy, self.error) = (true, None);
            o.event(Event::Began { by_agent: true });
        }
    }

    fn streamed(&mut self, v: &Value, o: &mut Out) {
        // Of sub-agents only whole messages come.
        if has(v, "/parent_tool_use_id") {
            return;
        }
        self.at_work(v, o);
        match text(v, "/event/type") {
            Some("message_start") => self.streaming = id(v, "/event/message/id"),
            Some("content_block_delta") => {
                let (kind, t) = match text(v, "/event/delta/type") {
                    Some("text_delta") => (Kind::Prose, text(v, "/event/delta/text")),
                    Some("thinking_delta") => (Kind::Reasoning, text(v, "/event/delta/thinking")),
                    _ => return,
                };
                let index = v.pointer("/event/index").and_then(Value::as_u64);
                if let (Some(m), Some(i), Some(t)) = (&self.streaming, index, t) {
                    let (id, text) = (format!("{m}:{i}"), t.to_owned());
                    o.event(Event::Say(Say { id, kind, text, whole: false, parent: None }));
                }
            }
            Some("message_stop") => self.streaming = None,
            _ => {}
        }
    }

    fn assistant(&mut self, v: &Value, o: &mut Out) {
        self.at_work(v, o);
        let parent = id(v, "/parent_tool_use_id");
        if let Some(e) = text(v, "/error") {
            o.notice(Level::Error, format!("The model's answer failed: {e}"));
            if parent.is_none() {
                self.error = Some(e.to_owned());
            }
        }
        let Some(m) = id(v, "/message/id") else { return o.unknown("a message without an id", v) };
        for b in list(v, "/message/content") {
            if self.blocks.len() >= MESSAGES && !self.blocks.contains_key(&m) {
                // Of those, only the last can go on.
                self.blocks.clear();
            }
            let n = self.blocks.entry(m.clone()).or_default();
            let id = format!("{m}:{n}");
            *n += 1;
            let said = match text(b, "/type") {
                Some("text") => Some((Kind::Prose, text(b, "/text"))),
                Some("thinking") => Some((Kind::Reasoning, text(b, "/thinking"))),
                Some("tool_use" | "server_tool_use" | "mcp_tool_use") => {
                    self.tool(b, &parent, o);
                    None
                }
                Some("redacted_thinking") => None,
                // Results of what the provider itself did belong to its calls.
                Some(t) if t.ends_with("_tool_result") => {
                    self.ended(b, &Value::Null, o);
                    None
                }
                t => {
                    o.unknown(&format!("block {}", t.unwrap_or_default()), b);
                    None
                }
            };
            if let Some((kind, Some(t))) = said {
                let (text, parent) = (t.to_owned(), parent.clone());
                o.event(Event::Say(Say { id: id.clone(), kind, text, whole: true, parent }));
                o.event(Event::Said { id });
            }
        }
    }

    fn tool(&mut self, b: &Value, parent: &Option<String>, o: &mut Out) {
        let Some(id) = id(b, "/id") else { return o.unknown("a tool call without an id", b) };
        // What its own agents mean to do is theirs.
        if parent.is_none() {
            self.tasks.called(&id, text(b, "/name").unwrap_or_default(), b.pointer("/input"));
        }
        o.event(Event::ToolStarted(Box::new(Tool {
            id,
            name: owned(b, "/name").unwrap_or_default(),
            args: b.pointer("/input").cloned(),
            parent: parent.clone(),
            ..Tool::default()
        })));
    }

    fn results(&mut self, v: &Value, o: &mut Out) {
        // Told again; or only text, which is on record.
        if v.pointer("/isReplay") == Some(&json!(true)) {
            return;
        }
        let structured = v.pointer("/tool_use_result").unwrap_or(&Value::Null);
        for b in list(v, "/message/content") {
            if text(b, "/type") == Some("tool_result") {
                self.ended(b, structured, o);
            }
        }
    }

    fn ended(&mut self, b: &Value, structured: &Value, o: &mut Out) {
        let Some(id) = id(b, "/tool_use_id") else {
            return o.unknown("a result without its call", b);
        };
        let output = match b.pointer("/content") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Array(a)) => Some(a.iter().filter_map(|x| text(x, "/text")).collect()),
            _ => None,
        };
        let failed = b.pointer("/is_error") == Some(&json!(true));
        let outcome = if failed { Outcome::Failed } else { Outcome::Completed };
        let diffs = patch(structured).into_iter().collect();
        let plan = self.tasks.ended(&id, !failed, structured);
        let end = ToolEnd { id, outcome, output, output_cut: 0, diffs, error: None };
        o.event(Event::ToolEnded(Box::new(end)));
        if let Some(p) = plan {
            o.event(Event::Plan(p));
        }
    }

    fn result(&mut self, v: &Value, o: &mut Out) {
        usage(v, o);
        if let Some(s) = id(v, "/session_id").filter(|s| *s != self.session) {
            self.session.clone_from(&s);
            o.event(Event::Session(s));
        }
        let denied = list(v, "/permission_denials").len();
        if denied > 0 {
            o.notice(
                Level::Warning,
                format!("Claude Code did not allow {denied} tool calls of this turn"),
            );
        }
        // Which prompts it answers. Without their names: all, unless it is of a turn that was not for
        // any, such as one for a task in the background that has ended.
        let mut named: Vec<&str> =
            list(v, "/user_message_uuids").iter().filter_map(Value::as_str).collect();
        named.extend(text(v, "/user_message_uuid"));
        for u in &named {
            self.unanswered.remove(*u);
        }
        let other = text(v, "/origin/kind") == Some("task-notification");
        if self.interrupted || (named.is_empty() && !other) {
            self.unanswered.clear();
        }
        let subtype = text(v, "/subtype").unwrap_or_default();
        let failed = subtype != "success" || v.pointer("/is_error") == Some(&json!(true));
        let error = failed.then(|| {
            let all: Vec<&str> = list(v, "/errors").iter().filter_map(Value::as_str).collect();
            match (all.is_empty(), text(v, "/result"), self.error.take()) {
                (false, ..) => all.join("; "),
                (_, Some(r), _) if !r.is_empty() => r.to_owned(),
                (.., Some(e)) => e,
                _ => format!("it failed: {subtype}"),
            }
        });
        if !self.busy {
            return;
        }
        if !self.unanswered.is_empty() {
            // What was said meanwhile begins the next turn at once: for who listens it is one.
            if let Some(e) = error {
                o.notice(Level::Error, e);
            }
            return;
        }
        (self.busy, self.streaming) = (false, None);
        self.blocks.clear();
        let (outcome, error) = match error {
            _ if std::mem::take(&mut self.interrupted) => (Outcome::Cancelled, None),
            Some(e) => (Outcome::Failed, Some(e)),
            None => (Outcome::Completed, None),
        };
        let stop = text(v, "/terminal_reason").unwrap_or(subtype).to_owned();
        o.say.push(Word::Done(Done { outcome, stop, error }, Keep::Nothing));
    }

    fn asked(&mut self, v: &Value, o: &mut Out) {
        let Some(request) = id(v, "/request_id") else {
            return o.unknown("a question without an id", v);
        };
        if text(v, "/request/subtype") != Some("can_use_tool") {
            let error = "not offered";
            respond(&request, json!({"subtype": "error", "error": error}), o);
            return o.unknown("control_request", v);
        }
        // Told again after a gap: the one that is open stands.
        if self.asks.values().any(|a| a.request == request) {
            return;
        }
        let name = text(v, "/request/tool_name").unwrap_or("a tool").to_owned();
        let input = v.pointer("/request/input").cloned().unwrap_or_default();
        if self.prompts == Prompts::Refused {
            let deny = json!({"behavior": "deny", "message": "Nobody can be asked."});
            respond(&request, json!({"subtype": "success", "response": deny}), o);
            return o.notice(Level::Warning, format!("Not allowed, nobody can be asked: {name}"));
        }
        self.made += 1;
        let qid = format!("q{}", self.made);
        let choice = |id: &str, name: &str, kind| Choice { id: id.into(), name: name.into(), kind };
        let q = Question {
            id: qid.clone(),
            tool: id(v, "/request/tool_use_id"),
            title: text(v, "/request/title").unwrap_or(&name).to_owned(),
            detail: owned(v, "/request/decision_reason").or(owned(v, "/request/description")),
            subject: json!({"tool": name, "input": input, "blocked": v.pointer("/request/blocked_path")}),
            options: vec![
                choice("allow", "Allow once", ChoiceKind::AllowOnce),
                choice("deny", "Do not allow", ChoiceKind::RejectOnce),
            ],
            by: id(v, "/request/agent_id"),
        };
        self.asks.insert(qid, Asked { request, input });
        o.event(Event::Asked(Box::new(q)));
    }
}

/// Documented, and nothing for a transcript.
const QUIET: &[&str] = &[
    "hook_started",
    "hook_progress",
    "hook_response",
    "plugin_install",
    "task_started",
    "task_progress",
    "task_updated",
    "task_notification",
    "background_tasks_changed",
    "thinking_tokens",
    "session_state_changed",
    "worker_shutting_down",
    "commands_changed",
    "notification",
    "files_persisted",
    "memory_recall",
    "control_request_progress",
    "model_refusal_no_fallback",
    "elicitation_complete",
    "mirror_error",
    "conversation_reset",
];

/// What an edit changed, as its structured output tells it.
fn patch(r: &Value) -> Option<Diff> {
    let path = owned(r, "/filePath")?;
    let mut patch = String::new();
    for h in list(r, "/structuredPatch").iter().take(1024) {
        let n = |at: &str| h.pointer(at).and_then(Value::as_u64).unwrap_or(0);
        let (a, b, c, d) = (n("/oldStart"), n("/oldLines"), n("/newStart"), n("/newLines"));
        patch += &format!("@@ -{a},{b} +{c},{d} @@\n");
        for l in list(h, "/lines").iter().filter_map(Value::as_str) {
            patch += l;
            patch.push('\n');
        }
    }
    if !patch.is_empty() {
        return Some(Diff { path, patch: Some(patch), ..Diff::default() });
    }
    // A file that was not there has no patch.
    let new = owned(r, "/content").filter(|_| text(r, "/type") == Some("create"));
    Some(Diff { path, new, ..Diff::default() })
}

/// `usage` is of this turn, and of the main loop only; the rest is of the whole session so far, what it
/// was resumed with included (agent-sdk/cost-tracking, read 2026-11-26).
fn usage(v: &Value, o: &mut Out) {
    let n = |k: &str| v.pointer(&format!("/usage/{k}")).and_then(Value::as_u64);
    let cost = v.pointer("/total_cost_usd").and_then(dollars);
    let totals: Vec<ModelTotal> = v
        .pointer("/modelUsage")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .take(limits::TOTALS)
        .map(|(model, m)| {
            let n = |k: &str| m.get(k).and_then(Value::as_u64);
            ModelTotal {
                model: Some(model.clone()),
                usage: Usage {
                    input: n("inputTokens"),
                    cache_read: n("cacheReadInputTokens"),
                    cache_write: n("cacheCreationInputTokens"),
                    output: n("outputTokens"),
                    ..Usage::default()
                },
                cost: m.get("costUSD").and_then(dollars),
            }
        })
        .collect();
    if !has(v, "/usage") && cost.is_none() && totals.is_empty() {
        return;
    }
    o.event(Event::Usage(Spent {
        usage: Usage {
            input: n("input_tokens"),
            cache_read: n("cache_read_input_tokens"),
            cache_write: n("cache_creation_input_tokens"),
            output: n("output_tokens"),
            reasoning: n("output_tokens_details/thinking_tokens"),
            cache_write_long: n("cache_creation/ephemeral_1h_input_tokens"),
            ..Usage::default()
        },
        context: None,
        of: Covers::Turn,
        cost,
        totals,
        provenance: Some(Provenance::Reported),
    }));
}

/// Its documentation: an estimate, not a bill.
fn dollars(v: &Value) -> Option<Cost> {
    let amount = v.as_f64().filter(|a| *a >= 0.0)?;
    Some(Cost {
        amount,
        currency: "USD".into(),
        provenance: Provenance::Estimated,
        of: Covers::Session,
    })
}

fn limit(v: &Value, o: &mut Out) {
    let Some(status) = owned(v, "/rate_limit_info/status") else { return o.unknown("limit", v) };
    o.event(Event::Limit(Limit {
        name: "plan".into(),
        used: v.pointer("/rate_limit_info/utilization").and_then(Value::as_f64),
        resets_at: v.pointer("/rate_limit_info/resetsAt").and_then(Value::as_u64),
        minutes: None,
        status: Some(status),
    }));
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
