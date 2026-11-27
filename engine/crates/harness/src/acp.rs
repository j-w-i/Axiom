//! The Agent Client Protocol, versions 1 and 2, as a client. Which one is agreed for each attempt:
//! version 2 is asked for, and what the agent chooses is spoken from then on.
//!
//! The client offers no files and no terminal (version 2 has none to offer): what an agent does, it
//! does itself. Where it has modes, the one that was decided on is set before anything is said to it.
//! Left out: signing in (`authenticate`, `auth/login`: a person does that with the agent's own
//! program), elicitation, MCP servers, slash commands, prompts other than text, and everything that
//! is still a draft (fork, status, end-of-turn usage).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use axiom_policy::{Access, Place, audit::Containment};
use axiom_proto::{Cost, Covers, Outcome, Provenance, Usage};
use serde_json::{Value, json};

use crate::{
    CLIENT, Site, Start, Trust,
    event::*,
    given::Runs,
    rpc::{Calls, Fault, Msg, NO_METHOD, read},
    talk::{Keep, Out, Talk, Word, base64, has, id, list, loose, owned, text},
};

/// An agent that speaks ACP: how it is started, and which of its modes is which access.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Acp {
    /// From the person's own configuration or [`KNOWN`], never from a workspace (SI-5).
    pub args: Vec<String>,
    /// Its mode for each access, by the id its documentation gives. Without one, only `Ask` is
    /// possible: the agent's own way of asking.
    pub read: Option<String>,
    pub ask: Option<String>,
    pub work: Option<String>,
    /// Its terms allow only what a person themselves says.
    pub person_only: bool,
    /// Its documentation says that a sandbox of its own holds back what it runs, and that it fails
    /// closed without one.
    pub sandboxed: bool,
    /// Where Axiom configures it, it can be told to read nothing of the folder's own configuration.
    pub ignores_folder: bool,
}

/// An agent whose ACP command is documented (research/harness-integration.md §2a, 2026-11-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Known {
    pub name: &'static str,
    pub program: &'static str,
    pub args: &'static [&'static str],
    pub person_only: bool,
    /// It can run the models of an account of Axiom's (decision 0023).
    pub runs: Option<Runs>,
}

const fn known(
    name: &'static str,
    program: &'static str,
    args: &'static [&'static str],
    person_only: bool,
) -> Known {
    Known { name, program, args, person_only, runs: None }
}

/// Google's terms allow Antigravity in other clients only where a person is at work (decision 0006).
pub const KNOWN: &[Known] = &[
    known("Antigravity", "agy", &["acp"], true),
    known("Cursor", "agent", &["acp"], false),
    // research/harnesses-for-accounts.md, read 2026-11-27.
    Known {
        runs: Some(Runs::Dsh),
        ..known("DeepSeek Harness", "dsh", &["--profile", "acp"], false)
    },
    known("Gemini CLI", "gemini", &["--acp"], false),
    known("GitHub Copilot CLI", "copilot", &["--acp"], false),
    known("Goose", "goose", &["acp"], false),
    known("Grok Build", "grok", &["agent", "stdio"], false),
    known("Kimi Code", "kimi", &["acp"], false),
    Known { runs: Some(Runs::OpenCode), ..known("OpenCode", "opencode", &["acp"], false) },
    known("Qwen Code", "qwen", &["--acp"], false),
];

impl Known {
    pub fn acp(&self) -> Acp {
        Acp {
            args: self.args.iter().map(|a| (*a).to_owned()).collect(),
            person_only: self.person_only,
            // dsh's sandbox fails closed (`dsh-sandbox-local`); OpenCode is told
            // `OPENCODE_DISABLE_PROJECT_CONFIG` where Axiom configures it.
            sandboxed: self.runs == Some(Runs::Dsh),
            ignores_folder: self.runs == Some(Runs::OpenCode),
            ..Acp::default()
        }
    }
}

impl Acp {
    fn mode(&self, a: Access) -> Option<&String> {
        match a {
            Access::Read => self.read.as_ref(),
            Access::Ask => self.ask.as_ref(),
            Access::Work => self.work.as_ref(),
        }
    }

    pub(crate) fn checked(&self) -> Result<Vec<String>, Error> {
        match self.args.iter().find(|a| loose(a)) {
            Some(a) => Err(Error::Unsafe(a.clone())),
            None => Ok(self.args.clone()),
        }
    }

    pub(crate) fn launch(&self, s: &Start) -> Result<Launch, Error> {
        // The protocol has no way to tell an agent not to read what is in its folder; what Axiom
        // gives one may (decision 0023).
        if s.trust != Trust::Given && !(s.configured && self.ignores_folder) {
            return Err(Error::Untrusted);
        }
        match self.mode(s.access) {
            Some(m) if loose(m) => return Err(Error::Unsafe(m.clone())),
            None if s.access != Access::Ask && !s.configured => {
                return Err(Error::NoMode(s.access));
            }
            _ => {}
        }
        let containment = if self.sandboxed { Containment::Harness } else { Containment::None };
        Ok(Launch { args: self.checked()?, containment })
    }

    pub(crate) fn talk(&self, site: &Site, start: Option<Start>) -> Client {
        let mut access = vec![];
        for a in [Access::Read, Access::Ask, Access::Work] {
            if a == Access::Ask || self.mode(a).is_some() {
                access.push(a);
            }
        }
        let mode = start.as_ref().and_then(|s| self.mode(s.access).cloned());
        Client {
            calls: Calls::new(true),
            program: site.program.place().clone(),
            version: 2,
            name: None,
            agent_version: None,
            caps: Caps { approvals: true, usage: true, plan: true, access, ..Caps::default() },
            closes: false,
            loads: false,
            mode,
            start,
            session: None,
            loading: false,
            ready: false,
            options: vec![],
            unasked: None,
            modes: vec![],
            todo: VecDeque::new(),
            active: false,
            asks: BTreeMap::new(),
            made: 0,
            current: None,
            terminals: BTreeMap::new(),
        }
    }
}

#[derive(Debug)]
enum Want {
    Initialize {
        again: bool,
    },
    Session,
    /// With `true`, its answer must say what is so now.
    Set(Set, bool),
    Prompt,
    Other,
}

/// What is chosen before the first prompt.
#[derive(Debug, Clone)]
enum Set {
    Mode(String),
    Model(String),
    Effort(String),
}

#[derive(Debug, Clone)]
struct Opt {
    id: String,
    category: String,
    current: String,
    values: Vec<Name>,
}

#[derive(Debug)]
struct Asked {
    rpc: Value,
    options: Vec<Choice>,
}

/// A terminal of the agent's, whose output belongs to a tool call.
#[derive(Debug, Default)]
struct Terminal {
    tool: Option<String>,
    /// Output from before it was known whose it is.
    early: Vec<u8>,
    /// The beginning of a letter.
    half: Vec<u8>,
}

const MOST_TERMINALS: usize = 64;
const EARLY: usize = 64 * 1024;

#[derive(Debug)]
pub(crate) struct Client {
    calls: Calls<Want>,
    program: Place,
    version: u64,
    name: Option<String>,
    agent_version: Option<String>,
    caps: Caps,
    closes: bool,
    loads: bool,
    mode: Option<String>,
    /// Nothing: it is only asked what it can do.
    start: Option<Start>,
    session: Option<String>,
    /// Its history is told again, which the journal has.
    loading: bool,
    ready: bool,
    options: Vec<Opt>,
    /// A switch that is on, by which it would not ask.
    unasked: Option<String>,
    modes: Vec<String>,
    todo: VecDeque<Set>,
    /// Version 2: at work, by its own account.
    active: bool,
    asks: BTreeMap<String, Asked>,
    made: u64,
    /// The text that goes on, where texts have no ids.
    current: Option<(Kind, String)>,
    terminals: BTreeMap<String, Terminal>,
}

impl Talk for Client {
    fn open(&mut self, o: &mut Out) {
        self.initialize(2, false, o);
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
        let Some(session) = self.session.clone().filter(|_| self.ready) else {
            return Err(Refused::NotReady);
        };
        let busy = self.active || self.calls.waits(|w| matches!(w, Want::Prompt));
        // Version 1 takes one at a time.
        if busy && self.version == 1 {
            return Err(Refused::Busy);
        }
        self.current = None;
        o.event(Event::Began { by_agent: false });
        let params = json!({"sessionId": session, "prompt": [{"type": "text", "text": text}]});
        self.calls.call("session/prompt", params, Want::Prompt, o);
        Ok(if busy { Took::Left } else { Took::Began })
    }

    fn answer(&mut self, id: &str, a: &Answer, o: &mut Out) -> Result<(), Refused> {
        let q = self.asks.get(id).ok_or(Refused::NotAsked)?;
        let outcome = match a {
            Answer::Choose(c) if q.options.iter().any(|x| x.id == *c) => {
                json!({"outcome": "selected", "optionId": c})
            }
            Answer::Choose(_) => return Err(Refused::NoSuchOption),
            Answer::Refuse => {
                // Version 2 has a word for no. In version 1 it would end the turn.
                let no = q.options.iter().find(|x| x.kind == ChoiceKind::RejectOnce);
                match no.filter(|_| self.version == 1) {
                    Some(no) => json!({"outcome": "selected", "optionId": no.id}),
                    None => json!({"outcome": "cancelled"}),
                }
            }
        };
        let rpc = q.rpc.clone();
        self.asks.remove(id);
        self.calls.reply(&rpc, json!({"outcome": outcome}), o);
        Ok(())
    }

    fn interrupt(&mut self, o: &mut Out) -> bool {
        let Some(s) = &self.session else { return false };
        self.calls.note("session/cancel", json!({"sessionId": s}), o);
        // Version 1 wants every question answered; in version 2 the agent takes back those that are
        // concerned.
        if self.version == 1 {
            for id in self.asks.keys().cloned().collect::<Vec<_>>() {
                self.withdraw(&id, o);
            }
        }
        true
    }

    fn close(&mut self, o: &mut Out) {
        if let Some(s) = self.session.clone().filter(|_| self.closes) {
            self.calls.call("session/close", json!({"sessionId": s}), Want::Other, o);
        }
    }
}

impl Client {
    fn initialize(&mut self, version: u64, again: bool, o: &mut Out) {
        let info = json!({"name": CLIENT.0, "title": CLIENT.1, "version": CLIENT.2});
        let params = match version {
            1 => json!({
                "protocolVersion": 1,
                "clientInfo": info,
                "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false},
            }),
            v => json!({"protocolVersion": v, "info": info, "capabilities": {}}),
        };
        self.calls.call("initialize", params, Want::Initialize { again }, o);
    }

    fn reply(&mut self, w: Want, r: Result<Value, Fault>, o: &mut Out) {
        match (w, r) {
            (Want::Initialize { .. }, Ok(v)) => self.initialized(&v, o),
            (Want::Initialize { again }, Err(f)) => {
                // Before the first success it may be tried again: an agent that knows only version 1
                // may not have known what to do with the other's words.
                let offers = list(&f.data, "/supported").iter().any(|v| v.as_u64() == Some(1));
                if !again && (f.code != -32022 || offers) {
                    self.initialize(1, true, o);
                } else if f.code == -32022 {
                    o.end(
                        Why::Version,
                        format!("it speaks no version of ACP that Axiom does: {}", f.data),
                    );
                } else {
                    o.end(Why::Refused, format!("it would not begin: {}", f.message));
                }
            }
            (Want::Session, Ok(v)) => self.in_session(&v, o),
            (Want::Session, Err(f)) if f.code == -32000 => o.end(Why::SignIn, SIGN_IN),
            (Want::Session, Err(f)) => {
                o.end(Why::Refused, format!("it would not open the session: {}", f.message));
            }
            (Want::Set(s, says), Ok(v)) => self.was_set(&s, says, &v, o),
            (Want::Set(s, _), Err(f)) => {
                o.end(Why::Refused, format!("it would not be set to {}: {}", s.value(), f.message));
            }
            (Want::Prompt, Ok(v)) if self.version == 1 => {
                self.done(owned(&v, "/stopReason"), None, Keep::Nothing, o);
            }
            // It is in the conversation; what comes of it is told by the state.
            (Want::Prompt, Ok(_)) => {}
            (Want::Prompt, Err(f)) => self.not_prompted(&f, o),
            (Want::Other, _) => {}
        }
    }

    fn initialized(&mut self, v: &Value, o: &mut Out) {
        self.version = match v.pointer("/protocolVersion").and_then(Value::as_u64) {
            Some(n @ (1 | 2)) => n,
            n => {
                let n = n.map_or("no version".into(), |n| format!("version {n}"));
                return o.end(Why::Version, format!("it speaks {n} of ACP, which Axiom does not"));
            }
        };
        let info = if self.version == 1 { "/agentInfo" } else { "/info" };
        self.name = owned(v, &format!("{info}/title")).or(owned(v, &format!("{info}/name")));
        self.agent_version = owned(v, &format!("{info}/version"));
        self.caps.interrupt = true;
        if self.version == 1 {
            let all = "/agentCapabilities/sessionCapabilities";
            self.caps.resume = has(v, &format!("{all}/resume"));
            self.loads = v.pointer("/agentCapabilities/loadSession") == Some(&json!(true))
                && !self.caps.resume;
            self.caps.resume |= self.loads;
            self.closes = has(v, &format!("{all}/close"));
        } else {
            if !has(v, "/capabilities/session") {
                return o.end(Why::Refused, "it has no sessions");
            }
            self.caps.resume = true;
            self.closes = true;
            self.caps.steer = Steer::AgentDecides;
        }
        let Some(s) = &self.start else {
            o.probe = Some(Probe {
                program: self.program.clone(),
                name: self.name.clone(),
                version: self.agent_version.clone(),
                protocol: self.protocol(),
                caps: self.caps.clone(),
            });
            return;
        };
        // Version 1's own form; what version 2 takes is a preview still.
        let servers: Vec<Value> = s
            .tools
            .iter()
            .filter(|_| self.version == 1)
            .map(|t| json!({"name": t.name, "command": t.command.as_str(), "args": t.args, "env": []}))
            .collect();
        let mut params = json!({"cwd": s.workspace.as_str(), "mcpServers": servers});
        let method = match &s.resume {
            None => "session/new",
            Some(_) if !self.caps.resume => {
                return o.end(Why::Refused, "it cannot go on with a session");
            }
            Some(id) => {
                params["sessionId"] = json!(id);
                self.session = Some(id.clone());
                self.loading = self.loads;
                if self.loads { "session/load" } else { "session/resume" }
            }
        };
        self.calls.call(method, params, Want::Session, o);
    }

    fn protocol(&self) -> String {
        format!("acp/{}", self.version)
    }

    fn in_session(&mut self, v: &Value, o: &mut Out) {
        self.loading = false;
        if self.session.is_none() {
            let Some(s) = id(v, "/sessionId") else {
                return o.end(Why::Nonsense, "it opened a session without an id");
            };
            self.session = Some(s);
        }
        self.read_options(v);
        self.modes = list(v, "/modes/availableModes").iter().filter_map(|m| id(m, "/id")).collect();
        let Some(s) = &self.start else { return };
        self.todo.extend(self.mode.clone().map(Set::Mode));
        self.todo.extend(s.model.clone().map(Set::Model));
        self.todo.extend(s.effort.clone().map(Set::Effort));
        if self.mode.is_none() {
            // Its own default, unless that asks nobody.
            let now =
                text(v, "/modes/currentModeId").or(self.option("mode").map(|m| m.current.as_str()));
            if let Some(m) = now.filter(|m| loose(m)) {
                return o.end(Why::Boundary, format!("it is set not to ask: {m}"));
            }
        }
        self.next(o);
    }

    fn read_options(&mut self, v: &Value) {
        let key = if self.version == 1 { "/id" } else { "/configId" };
        let mut all = vec![];
        self.unasked = None;
        for c in list(v, "/configOptions").iter().take(64) {
            if c.pointer("/currentValue") == Some(&json!(true)) {
                let words = [key, "/name", "/description"].map(|k| text(c, k).unwrap_or_default());
                if words.iter().any(|w| loose(w)) {
                    self.unasked = Some(words[1].to_owned());
                }
            }
            let (Some(cid), Some(current)) = (id(c, key), id(c, "/currentValue")) else { continue };
            let groups = list(c, "/options").iter().flat_map(|x| match list(x, "/options") {
                [] => std::slice::from_ref(x),
                inner => inner,
            });
            let values = groups.take(512).filter_map(|x| {
                Some(Name { id: id(x, "/value")?, label: owned(x, "/name"), ..Name::default() })
            });
            let category = owned(c, "/category").unwrap_or_default();
            all.push(Opt { id: cid, category, current, values: values.collect() });
        }
        self.options = all;
        let names = |c: &str| self.option(c).map_or(Names::None, |m| Names::Of(m.values.clone()));
        (self.caps.models, self.caps.efforts) = (names("model"), names("thought_level"));
    }

    fn option(&self, category: &str) -> Option<&Opt> {
        self.options.iter().find(|c| c.category == category)
    }

    /// Sets the next thing, or is ready.
    fn next(&mut self, o: &mut Out) {
        let Some(set) = self.todo.pop_front() else { return self.is_ready(o) };
        let session = json!(self.session);
        let found = self.option(set.category());
        let has = found.filter(|f| f.values.iter().any(|v| v.id == *set.value()));
        if let Some(f) = has {
            let mut params = json!({"sessionId": session, "configId": f.id, "value": set.value()});
            if self.version != 1 {
                params["type"] = json!("id");
            }
            self.calls.call("session/set_config_option", params, Want::Set(set, true), o);
        } else if let Set::Mode(m) = &set
            && self.modes.contains(m)
        {
            let params = json!({"sessionId": session, "modeId": m});
            self.calls.call("session/set_mode", params, Want::Set(set, false), o);
        } else {
            let why = if matches!(set, Set::Mode(_)) { Why::Boundary } else { Why::Refused };
            o.end(why, format!("it has no {} {}", set.what(), set.value()));
        }
    }

    fn was_set(&mut self, s: &Set, says: bool, v: &Value, o: &mut Out) {
        // `session/set_mode` answers with nothing.
        if says {
            self.read_options(v);
            let now = self.option(s.category()).map(|c| c.current.clone());
            if now.as_ref() != Some(s.value()) {
                let why = if matches!(s, Set::Mode(_)) { Why::Boundary } else { Why::Refused };
                return o.end(why, format!("it did not take the {} {}", s.what(), s.value()));
            }
        }
        self.next(o);
    }

    fn is_ready(&mut self, o: &mut Out) {
        let Some(session) = self.session.clone() else { return };
        // Setting the one may have changed the other.
        let now = self.option("mode").map(|m| &m.current);
        if let (Some(m), Some(now)) = (&self.mode, now)
            && m != now
            && !self.modes.contains(m)
        {
            return o.end(Why::Boundary, format!("it did not keep the mode {m}"));
        }
        if let Some(u) = &self.unasked {
            return o.end(Why::Boundary, format!("it is set not to ask: {u}"));
        }
        self.ready = true;
        o.event(Event::Ready(Box::new(Ready {
            session,
            protocol: self.protocol(),
            version: self.agent_version.clone(),
            model: self.option("model").map(|m| m.current.clone()),
            caps: self.caps.clone(),
        })));
        if self.active {
            o.event(Event::Began { by_agent: true });
        }
    }

    fn not_prompted(&mut self, f: &Fault, o: &mut Out) {
        let cancelled = f.code == -32800;
        let words = if f.code == -32000 { SIGN_IN.to_owned() } else { f.message.clone() };
        if self.version == 1 {
            let stop = if cancelled { "cancelled" } else { "error" };
            return self.done(Some(stop.into()), Some(words), Keep::Nothing, o);
        }
        o.event(Event::NotHeard { why: words.clone() });
        if !self.active && !self.calls.waits(|w| matches!(w, Want::Prompt)) {
            let stop = if cancelled { "cancelled" } else { "error" };
            self.done(Some(stop.into()), Some(words), Keep::All, o);
        }
    }

    fn done(&mut self, stop: Option<String>, error: Option<String>, keep: Keep, o: &mut Out) {
        self.current = None;
        let stop = stop.unwrap_or_default();
        let (outcome, words) = match stop.as_str() {
            "end_turn" => (Outcome::Completed, None),
            "cancelled" => (Outcome::Cancelled, None),
            "max_tokens" => (Outcome::Failed, Some("it reached the limit of one answer")),
            "max_turn_requests" => (Outcome::Failed, Some("it reached its limit of requests")),
            "refusal" => (Outcome::Failed, Some("the agent refused to go on")),
            "error" => (Outcome::Failed, Some("it failed and did not say how")),
            _ => (Outcome::Unknown, None),
        };
        let error = match outcome {
            Outcome::Failed => error.or(words.map(str::to_owned)),
            _ => None,
        };
        o.say.push(Word::Done(Done { outcome, stop, error }, keep));
    }

    fn note(&mut self, method: &str, params: &Value, o: &mut Out) {
        match method {
            "session/update" => self.update(params, o),
            "$/cancel_request" => {
                let rpc = params.pointer("/requestId");
                let id = self.asks.iter().find(|a| Some(&a.1.rpc) == rpc).map(|a| a.0.clone());
                if let Some(id) = id {
                    self.withdraw(&id, o);
                }
            }
            _ => o.unknown(method, params),
        }
    }

    /// Answers a question that nobody needs an answer to.
    fn withdraw(&mut self, id: &str, o: &mut Out) {
        if let Some(q) = self.asks.remove(id) {
            self.calls.reply(&q.rpc, json!({"outcome": {"outcome": "cancelled"}}), o);
            o.event(Event::AskClosed { id: id.to_owned(), how: Settled::Withdrawn });
        }
    }

    fn asked(&mut self, rpc: Value, method: &str, params: &Value, o: &mut Out) {
        if method != "session/request_permission" {
            // Nothing else was offered.
            self.calls.refuse(&rpc, NO_METHOD, "not offered", o);
            return o.unknown(method, params);
        }
        if self.asks.values().any(|a| a.rpc == rpc) || rpc.as_str().is_some_and(|s| s.len() > 256) {
            return o.unknown("a question with an id that cannot be used", params);
        }
        let mut options = vec![];
        for x in list(params, "/options").iter().take(32) {
            let Some(id) = id(x, "/optionId") else { continue };
            let kind = match text(x, "/kind") {
                Some("allow_once") => ChoiceKind::AllowOnce,
                Some("allow_always") => ChoiceKind::AllowAlways,
                Some("reject_once") => ChoiceKind::RejectOnce,
                Some("reject_always") => ChoiceKind::RejectAlways,
                k => ChoiceKind::Other(k.unwrap_or_default().to_owned()),
            };
            let name = owned(x, "/name").unwrap_or_else(|| id.clone());
            options.push(Choice { id, name, kind });
        }
        let scope = self.scope(params);
        let (call, subject) = if self.version == 1 {
            (params.pointer("/toolCall"), params.pointer("/toolCall"))
        } else {
            (params.pointer("/subject/toolCall"), params.pointer("/subject"))
        };
        let call_id = call.and_then(|c| id(c, "/toolCallId"));
        let tool = call_id.or(id(params, "/subject/toolCallId")).map(|t| scoped(&scope, &t));
        // What is asked about is told like any other word of it, before the question.
        if let Some(c) = call {
            self.tool(c, &scope, o);
        }
        self.made += 1;
        let qid = format!("q{}", self.made);
        let title = text(params, "/title").or(call.and_then(|c| text(c, "/title")));
        let q = Question {
            id: qid.clone(),
            tool,
            title: title.unwrap_or("it asks to go on").to_owned(),
            detail: owned(params, "/description"),
            subject: subject.cloned().unwrap_or_default(),
            options: options.clone(),
            by: scope,
        };
        self.asks.insert(qid, Asked { rpc, options });
        o.event(Event::Asked(Box::new(q)));
    }

    /// The child session that is spoken of. Nothing: our own.
    fn scope(&self, params: &Value) -> Option<String> {
        let s = id(params, "/sessionId")?;
        (Some(&s) != self.session.as_ref()).then_some(s)
    }

    fn update(&mut self, params: &Value, o: &mut Out) {
        if self.loading {
            return;
        }
        let Some(u) = params.pointer("/update") else { return o.unknown("session/update", params) };
        let scope = self.scope(params);
        // One that is not ours and before ours is there is nobody's.
        if self.session.is_none() {
            return;
        }
        let kind = text(u, "/sessionUpdate").unwrap_or_default();
        match kind {
            "agent_message_chunk" => self.chunk(Kind::Prose, u, &scope, o),
            "agent_thought_chunk" => self.chunk(Kind::Reasoning, u, &scope, o),
            "agent_message" => self.message(Kind::Prose, u, &scope, o),
            "agent_thought" => self.message(Kind::Reasoning, u, &scope, o),
            "tool_call" | "tool_call_update" => self.tool(u, &scope, o),
            "tool_call_content_chunk" => self.tool_chunk(u, &scope, o),
            "terminal_update" => self.terminal(u, &scope, o),
            "terminal_output_chunk" => self.terminal_chunk(u, &scope, o),
            "plan" => self.plan("plan", u, o),
            "plan_update" => {
                let id = id(u, "/plan/planId").unwrap_or_else(|| "plan".into());
                let v = u.pointer("/plan").cloned().unwrap_or_default();
                self.plan(&id, &v, o);
            }
            "plan_removed" => {
                if let Some(id) = id(u, "/planId") {
                    o.event(Event::Plan(Plan { id, ..Plan::default() }));
                }
            }
            "state_update" if scope.is_none() => self.state(u, o),
            "usage_update" if scope.is_none() => usage(u, o),
            "notice" => {
                let level = match text(u, "/severity") {
                    Some("error") => Level::Error,
                    Some("warning") => Level::Warning,
                    _ => Level::Info,
                };
                let detail = text(u, "/description").map(|d| format!(": {d}")).unwrap_or_default();
                o.notice(level, format!("{}{detail}", text(u, "/title").unwrap_or_default()));
            }
            "compaction_update" => match text(u, "/status") {
                Some("completed") => o.notice(Level::Info, "The agent shortened its context"),
                Some("failed") => {
                    o.notice(Level::Warning, "The agent could not shorten its context")
                }
                _ => {}
            },
            "current_mode_update" | "config_option_update" if scope.is_none() => {
                self.changed(u, o);
            }
            // Known, and nothing to tell: what a person said is on record already.
            "user_message_chunk"
            | "user_message"
            | "available_commands_update"
            | "session_info_update"
            | "replay_complete"
            | "compaction_summary_chunk"
            | "subagent_update"
            | "state_update"
            | "usage_update"
            | "current_mode_update"
            | "config_option_update" => {}
            kind => o.unknown(&format!("session/update {kind}"), u),
        }
    }

    /// The agent changed its own mode, or an option.
    fn changed(&mut self, u: &Value, o: &mut Out) {
        if has(u, "/configOptions") {
            self.read_options(u);
        }
        if let Some(u) = self.unasked.as_ref().filter(|_| self.ready) {
            return o.end(Why::Boundary, format!("it was set not to ask: {u}"));
        }
        let now = owned(u, "/currentModeId").or(self.option("mode").map(|m| m.current.clone()));
        let Some(now) = now else { return };
        if loose(&now) {
            o.end(Why::Boundary, format!("it switched to a mode in which it does not ask: {now}"));
        } else if self.ready && self.mode.as_ref().is_some_and(|m| *m != now) {
            o.notice(Level::Warning, format!("The agent switched to its mode {now}"));
        }
    }

    fn state(&mut self, u: &Value, o: &mut Out) {
        if self.version == 1 {
            return;
        }
        match text(u, "/state") {
            Some("running" | "requires_action") => {
                if !std::mem::replace(&mut self.active, true) && self.ready {
                    o.event(Event::Began { by_agent: true });
                }
            }
            // Said again, it is no second end.
            Some("idle") if std::mem::take(&mut self.active) => {
                let keep = match u.pointer("/background") {
                    Some(Value::Array(all)) => {
                        let mut ids = BTreeSet::new();
                        for x in all {
                            ids.extend(id(x, "/toolCallId"));
                            let t = id(x, "/terminalId").and_then(|t| self.terminals.get(&t));
                            ids.extend(t.and_then(|t| t.tool.clone()));
                        }
                        Keep::These(ids)
                    }
                    _ => Keep::All,
                };
                let error = owned(u, "/error/message");
                self.done(owned(u, "/stopReason"), error, keep, o);
            }
            Some("unknown") => {
                o.notice(Level::Warning, "The agent does not know whether it is at work")
            }
            _ => {}
        }
    }

    fn chunk(&mut self, kind: Kind, u: &Value, scope: &Option<String>, o: &mut Out) {
        let t = block(u.pointer("/content").unwrap_or(&Value::Null));
        let id = match id(u, "/messageId") {
            Some(id) => {
                self.current = None;
                entity(kind, &id)
            }
            // Without ids, what follows the same kind of text belongs to it.
            None => match &self.current {
                Some((k, id)) if *k == kind => id.clone(),
                _ => {
                    self.made += 1;
                    let id = entity(kind, &format!("#{}", self.made));
                    self.current = Some((kind, id.clone()));
                    id
                }
            },
        };
        let id = scoped(scope, &id);
        o.event(Event::Say(Say { id, kind, text: t, whole: false, parent: scope.clone() }));
    }

    fn message(&mut self, kind: Kind, u: &Value, scope: &Option<String>, o: &mut Out) {
        let Some(id) = id(u, "/messageId") else { return o.unknown("a message without an id", u) };
        // Left out, it is as it was.
        let Some(c) = u.pointer("/content") else { return };
        let t = c.as_array().map(|a| a.iter().map(block).collect::<String>()).unwrap_or_default();
        let id = scoped(scope, &entity(kind, &id));
        o.event(Event::Say(Say { id, kind, text: t, whole: true, parent: scope.clone() }));
    }

    fn tool(&mut self, u: &Value, scope: &Option<String>, o: &mut Out) {
        let Some(id) = id(u, "/toolCallId") else {
            return o.unknown("a tool call without an id", u);
        };
        let id = scoped(scope, &id);
        self.current = None;
        let c = self.content(list(u, "/content"), &id, scope);
        o.event(Event::ToolUpdated(Box::new(Tool {
            id: id.clone(),
            name: text(u, "/name").or(text(u, "/kind")).unwrap_or_default().to_owned(),
            title: owned(u, "/title"),
            args: u.pointer("/rawInput").filter(|v| !v.is_null()).cloned(),
            args_cut: 0,
            diffs: c.diffs.clone(),
            parent: scope.clone(),
        })));
        for (id, text) in c.early {
            o.event(Event::ToolOutput { id, text, whole: false, cut: 0 });
        }
        let outcome = match text(u, "/status") {
            Some("completed") => Outcome::Completed,
            Some("failed") => Outcome::Failed,
            Some("cancelled") => Outcome::Cancelled,
            _ => {
                // A list takes the place of what was there; one that only names a terminal says
                // nothing of its own.
                if u.pointer("/content").is_some_and(|x| x.is_array() || self.version != 1)
                    && !(c.terminal && c.text.is_empty())
                {
                    o.event(Event::ToolOutput { id, text: c.text, whole: true, cut: 0 });
                }
                return;
            }
        };
        let raw = u.pointer("/rawOutput").filter(|v| !v.is_null());
        let output = match raw {
            _ if !c.text.is_empty() => Some(c.text),
            Some(Value::String(s)) => Some(s.clone()),
            Some(v) if c.diffs.is_empty() => Some(v.to_string()),
            _ => None,
        };
        let end = ToolEnd { id, outcome, output, output_cut: 0, diffs: c.diffs, error: None };
        o.event(Event::ToolEnded(Box::new(end)));
    }

    fn tool_chunk(&mut self, u: &Value, scope: &Option<String>, o: &mut Out) {
        let Some(id) = id(u, "/toolCallId") else {
            return o.unknown("a tool call without an id", u);
        };
        let id = scoped(scope, &id);
        let one = u.pointer("/content").map(std::slice::from_ref).unwrap_or_default();
        let c = self.content(one, &id, scope);
        if !c.diffs.is_empty() {
            let t =
                Tool { id: id.clone(), diffs: c.diffs, parent: scope.clone(), ..Tool::default() };
            o.event(Event::ToolUpdated(Box::new(t)));
        }
        for (id, text) in c.early {
            o.event(Event::ToolOutput { id, text, whole: false, cut: 0 });
        }
        if !c.text.is_empty() {
            o.event(Event::ToolOutput { id, text: c.text, whole: false, cut: 0 });
        }
    }

    fn content(&mut self, all: &[Value], tool: &str, scope: &Option<String>) -> Content {
        let mut c = Content::default();
        for x in all.iter().take(1024) {
            match text(x, "/type") {
                Some("content") => c.text += &block(x.pointer("/content").unwrap_or(&Value::Null)),
                Some("diff") if self.version == 1 => c.diffs.push(Diff {
                    path: owned(x, "/path").unwrap_or_default(),
                    old: owned(x, "/oldText"),
                    new: owned(x, "/newText"),
                    ..Diff::default()
                }),
                Some("diff") => {
                    let mut patch = owned(x, "/patch/text");
                    for d in list(x, "/changes") {
                        let path = owned(d, "/path").unwrap_or_default();
                        c.diffs.push(Diff { path, patch: patch.take(), ..Diff::default() });
                    }
                }
                Some("terminal") => {
                    c.terminal = true;
                    let Some(t) = id(x, "/terminalId").and_then(|t| self.term(&scoped(scope, &t)))
                    else {
                        continue;
                    };
                    t.tool = Some(tool.to_owned());
                    let early = std::mem::take(&mut t.early);
                    let said = whole(&mut t.half, &early);
                    if !said.is_empty() {
                        c.early.push((tool.to_owned(), said));
                    }
                }
                _ => {}
            }
        }
        c
    }

    fn term(&mut self, id: &str) -> Option<&mut Terminal> {
        if self.terminals.len() >= MOST_TERMINALS && !self.terminals.contains_key(id) {
            return None;
        }
        Some(self.terminals.entry(id.to_owned()).or_default())
    }

    fn terminal(&mut self, u: &Value, scope: &Option<String>, o: &mut Out) {
        let Some(tid) = id(u, "/terminalId").map(|t| scoped(scope, &t)) else { return };
        if let Some(data) = u.pointer("/output") {
            let bytes = text(data, "/data").and_then(base64).unwrap_or_default();
            if let Some(t) = self.term(&tid) {
                t.half.clear();
                t.early.clear();
                match t.tool.clone() {
                    Some(id) => {
                        let text = whole(&mut t.half, &bytes);
                        o.event(Event::ToolOutput { id, text, whole: true, cut: 0 });
                    }
                    None => t.early.extend(bytes.iter().take(EARLY)),
                }
            }
        }
        if has(u, "/exitStatus") {
            // What comes after is of no use; its room is.
            self.terminals.remove(&tid);
        }
    }

    fn terminal_chunk(&mut self, u: &Value, scope: &Option<String>, o: &mut Out) {
        let Some(tid) = id(u, "/terminalId").map(|t| scoped(scope, &t)) else { return };
        let Some(bytes) = text(u, "/data").and_then(base64) else {
            return o.unknown("terminal output that cannot be read", u);
        };
        let Some(t) = self.term(&tid) else { return };
        match t.tool.clone() {
            Some(id) => {
                let text = whole(&mut t.half, &bytes);
                o.event(Event::ToolOutput { id, text, whole: false, cut: 0 });
            }
            None => {
                let room = EARLY.saturating_sub(t.early.len());
                t.early.extend(bytes.iter().take(room));
            }
        }
    }

    fn plan(&self, id: &str, v: &Value, o: &mut Out) {
        let steps = list(v, "/entries").iter().map(|e| {
            let p = match text(e, "/status") {
                Some("pending") => Progress::Todo,
                Some("in_progress") => Progress::Doing,
                Some("completed") => Progress::Done,
                _ => Progress::Unknown,
            };
            (owned(e, "/content").unwrap_or_default(), p)
        });
        let steps = steps.collect();
        o.event(Event::Plan(Plan { id: id.to_owned(), steps, text: owned(v, "/content") }));
    }
}

#[derive(Default)]
struct Content {
    text: String,
    diffs: Vec<Diff>,
    terminal: bool,
    /// Output that had been waiting for its tool call.
    early: Vec<(String, String)>,
}

const SIGN_IN: &str = "it is not signed in: sign in with its own program, then try again";

impl Set {
    fn value(&self) -> &String {
        match self {
            Set::Mode(v) | Set::Model(v) | Set::Effort(v) => v,
        }
    }
    fn category(&self) -> &'static str {
        match self {
            Set::Mode(_) => "mode",
            Set::Model(_) => "model",
            Set::Effort(_) => "thought_level",
        }
    }
    fn what(&self) -> &'static str {
        match self {
            Set::Mode(_) => "mode",
            Set::Model(_) => "model",
            Set::Effort(_) => "effort",
        }
    }
}

/// A prose message and a thought may have the same id and are two things.
fn entity(kind: Kind, id: &str) -> String {
    match kind {
        Kind::Prose => format!("p:{id}"),
        Kind::Reasoning => format!("r:{id}"),
    }
}

fn scoped(scope: &Option<String>, id: &str) -> String {
    match scope {
        Some(s) => format!("{s}/{id}"),
        None => id.to_owned(),
    }
}

/// What a block of content says. What is not text is named.
fn block(b: &Value) -> String {
    match text(b, "/type") {
        Some("text") => owned(b, "/text").unwrap_or_default(),
        Some("resource_link") => format!("[{}]", text(b, "/uri").unwrap_or("link")),
        Some("resource") => owned(b, "/resource/text").unwrap_or_else(|| "[file]".into()),
        Some(kind @ ("image" | "audio")) => format!("[{kind}]"),
        _ => String::new(),
    }
}

/// Bytes as text, where the last letter may still be on its way.
fn whole(half: &mut Vec<u8>, more: &[u8]) -> String {
    half.extend_from_slice(more);
    let keep = match std::str::from_utf8(half) {
        Err(e) if e.error_len().is_none() => half.len() - e.valid_up_to(),
        _ => {
            // A fault in the middle hides a letter that has only begun at the end.
            let tail = half.len().saturating_sub(3);
            (tail..half.len())
                .find(|i| {
                    std::str::from_utf8(&half[*i..])
                        .is_err_and(|e| e.error_len().is_none() && e.valid_up_to() == 0)
                })
                .map_or(0, |i| half.len() - i)
        }
    };
    let rest = half.split_off(half.len() - keep);
    let said = String::from_utf8_lossy(half).into_owned();
    *half = rest;
    said
}

fn usage(u: &Value, o: &mut Out) {
    let n = |at: &str| u.pointer(at).and_then(Value::as_u64);
    let (Some(used), Some(size)) = (n("/used"), n("/size")) else {
        return o.unknown("usage", u);
    };
    let cost = u.pointer("/cost/amount").and_then(Value::as_f64).zip(owned(u, "/cost/currency"));
    let context = (size > 0).then(|| (used as f64 / size as f64).min(1.0));
    o.event(Event::Usage(Spent {
        usage: Usage { context, ..Usage::default() },
        context: Some((used, size)),
        // "Total cumulative cost for the session" (rfds/session-usage, read 2026-11-26).
        cost: cost.filter(|c| c.0 >= 0.0).map(|(amount, currency)| Cost {
            amount,
            currency,
            provenance: Provenance::Reported,
            of: Covers::Session,
        }),
        provenance: Some(Provenance::Reported),
        ..Spent::default()
    }));
}

#[cfg(test)]
#[path = "acp_tests.rs"]
mod tests;
