//! An adapter without a process: told lines and commands, it says what it would send and tell.

use axiom_policy::{Access, Place, Reach};
use serde_json::Value;

use crate::{
    Limits, Program, Site, Start, Trust,
    event::*,
    ledger::Ledger,
    talk::{Out, Talk},
};

pub fn place(s: &str) -> Place {
    Place::new(std::env::temp_dir().join(s)).unwrap()
}

pub fn site() -> Site {
    Site {
        program: Program::for_test(place("bin/agent")),
        path: vec![place("bin")],
        home: place("home"),
        user: "me".into(),
        tmp: place("tmp"),
        written: Reach::none(),
        limits: Limits::default(),
    }
}

pub fn start(access: Access) -> Start {
    Start {
        workspace: place("w"),
        model: None,
        effort: None,
        access,
        resume: None,
        trust: Trust::Given,
        by: By::Person,
    }
}

pub struct Desk<T: Talk> {
    pub talk: T,
    ledger: Ledger,
    pub sent: Vec<Value>,
    pub told: Vec<Event>,
    pub end: Option<(Why, String)>,
    pub probe: Option<Probe>,
    pub bad: u32,
}

impl<T: Talk> Desk<T> {
    pub fn new(talk: T) -> Self {
        let mut d = Self {
            talk,
            ledger: Ledger::default(),
            sent: vec![],
            told: vec![],
            end: None,
            probe: None,
            bad: 0,
        };
        d.with(|t, o| t.open(o));
        d
    }

    fn with<R>(&mut self, f: impl FnOnce(&mut T, &mut Out) -> R) -> R {
        let mut o = Out::default();
        let r = f(&mut self.talk, &mut o);
        self.sent.extend(o.send);
        for w in o.say {
            self.ledger.take(w, &mut self.told).unwrap();
        }
        if self.end.is_none() {
            self.end = o.end;
        }
        self.probe = self.probe.take().or(o.probe);
        self.bad += u32::from(o.bad);
        r
    }

    pub fn hear(&mut self, v: Value) {
        self.with(|t, o| t.heard(v, o));
    }
    pub fn prompt(&mut self, s: &str) -> Result<Took, Refused> {
        self.with(|t, o| t.prompt(s, By::Person, o))
    }
    pub fn prompt_by(&mut self, s: &str, by: By) -> Result<Took, Refused> {
        self.with(|t, o| t.prompt(s, by, o))
    }
    pub fn choose(&mut self, q: &str, c: &str) -> Result<(), Refused> {
        self.with(|t, o| t.answer(q, &Answer::Choose(c.into()), o))
    }
    pub fn refuse(&mut self, q: &str) -> Result<(), Refused> {
        self.with(|t, o| t.answer(q, &Answer::Refuse, o))
    }
    pub fn interrupt(&mut self) -> bool {
        self.with(|t, o| t.interrupt(o))
    }
    pub fn close(&mut self) {
        self.with(|t, o| t.close(o));
    }

    /// What was sent since the last time.
    pub fn sent(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.sent)
    }
    pub fn one(&mut self) -> Value {
        let mut all = self.sent();
        assert_eq!(all.len(), 1, "{all:?}");
        all.remove(0)
    }
    /// What was told since the last time.
    pub fn events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.told)
    }
    pub fn told(&mut self) -> Vec<String> {
        self.events().iter().map(show).collect()
    }
    pub fn ends(&mut self) -> (Why, String) {
        self.end.take().expect("it ends")
    }
}

/// In a few words, for comparing.
pub fn show(e: &Event) -> String {
    let d = |all: &[Diff]| -> String {
        all.iter()
            .map(|d| {
                format!(" [{} {:?} {:?} {:?}]", d.path, d.patch, d.old, d.new).replace("None", "-")
            })
            .collect()
    };
    match e {
        Event::Ready(r) => format!("ready {} {}", r.session, r.protocol),
        Event::Session(s) => format!("session {s}"),
        Event::Began { by_agent } => if *by_agent { "began by agent" } else { "began" }.into(),
        Event::Say(s) => {
            let parent = s.parent.as_ref().map(|p| format!(" of {p}")).unwrap_or_default();
            let kind = if s.kind == Kind::Prose { "say" } else { "think" };
            format!("{kind} {}{parent}: {}{}", s.id, s.text, if s.whole { " (whole)" } else { "" })
        }
        Event::Said { id } => format!("said {id}"),
        Event::ToolStarted(t) | Event::ToolUpdated(t) => {
            let w = if matches!(e, Event::ToolStarted(_)) { "tool" } else { "more" };
            let title = t.title.as_ref().map(|t| format!(" '{t}'")).unwrap_or_default();
            let args = t.args.as_ref().map(|a| format!(" {a}")).unwrap_or_default();
            let parent = t.parent.as_ref().map(|p| format!(" of {p}")).unwrap_or_default();
            format!("{w} {} {}{title}{args}{}{parent}", t.id, t.name, d(&t.diffs))
        }
        Event::ToolOutput { id, text, whole, .. } => {
            format!("output {id}: {text}{}", if *whole { " (whole)" } else { "" })
        }
        Event::ToolEnded(t) => {
            let out = t.output.as_ref().map(|o| format!(" {o:?}")).unwrap_or_default();
            let err = t.error.as_ref().map(|o| format!(" ({o})")).unwrap_or_default();
            format!("ended {} {:?}{out}{err}{}", t.id, t.outcome, d(&t.diffs))
        }
        Event::Plan(p) => format!("plan {} {:?} {:?}", p.id, p.steps, p.text),
        Event::Asked(q) => {
            let o: Vec<_> = q.options.iter().map(|o| o.id.as_str()).collect();
            format!("asked {} '{}' {:?} {}", q.id, q.title, q.tool, o.join(","))
        }
        Event::AskClosed { id, how } => format!("closed {id} {how:?}"),
        Event::Notice { level, text } => format!("notice {level:?}: {text}"),
        Event::NotHeard { why } => format!("not heard: {why}"),
        Event::Unknown { what, .. } => format!("unknown {what}"),
        Event::Done(d) => {
            let err = d.error.as_ref().map(|o| format!(" ({o})")).unwrap_or_default();
            format!("done {:?} {}{err}", d.outcome, d.stop)
        }
        e => format!("{e:?}"),
    }
}
