//! What is open, whatever a harness says: every turn, text, tool call and question begins before it
//! goes on and ends exactly once, in order, and nothing grows without limit.

use std::collections::{BTreeSet, VecDeque};

use axiom_proto::Outcome;
use serde_json::Value;

use crate::{
    event::{Diff, Done, Event, Plan, Settled, Tool, ToolEnd},
    frames::cut,
    talk::{Keep, Word},
};

pub(crate) const MOST_TOOLS: usize = 512;
pub(crate) const MOST_ASKED: usize = 64;
const MOST_TEXTS: usize = 256;
/// Ids of what has ended, so that a late word for it is not taken for something new.
const REMEMBERED: usize = 4096;

/// Bytes of one tool call's arguments, of one piece of its output, of one side of a diff.
pub(crate) const ARGS: usize = 256 * 1024;
pub(crate) const OUTPUT: usize = 256 * 1024;
const DIFF: usize = 512 * 1024;
const DIFFS: usize = 64;
const RAW: usize = 4 * 1024;
/// Titles, notices and other words of a line or two.
const LINE: usize = 8 * 1024;
const STEPS: usize = 256;
const PLAN: usize = 256 * 1024;

/// Too much is open.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Flood(pub &'static str);

/// A text's length and digest so far (FNV-1a), to know a repetition of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sum(u64, u64);

impl Sum {
    const NONE: Self = Self(0, 0xcbf2_9ce4_8422_2325);
    fn add(mut self, s: &str) -> Self {
        for b in s.bytes() {
            self.1 = (self.1 ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        self.0 += s.len() as u64;
        self
    }
}

#[derive(Debug, Default)]
pub(crate) struct Ledger {
    turn: bool,
    /// In the order they began.
    texts: Vec<(String, Sum)>,
    tools: BTreeSet<String>,
    /// Of those, the ones whose arguments, or whose diffs, as they are now were too long to keep.
    cut: BTreeSet<(String, bool)>,
    asked: BTreeSet<String>,
    ended: BTreeSet<String>,
    order: VecDeque<String>,
}

impl Ledger {
    pub fn turn(&self) -> bool {
        self.turn
    }
    pub fn asked(&self) -> usize {
        self.asked.len()
    }
    /// Whether all that was told of this tool call was kept, and could be shown.
    pub fn is_whole(&self, tool: &str) -> bool {
        [true, false].iter().all(|args| !self.cut.contains(&(tool.to_owned(), *args)))
    }

    fn over(&mut self, what: char, id: &str) {
        let k = format!("{what}{id}");
        if self.ended.insert(k.clone()) {
            self.order.push_back(k);
        }
        if self.order.len() > REMEMBERED
            && let Some(old) = self.order.pop_front()
        {
            self.ended.remove(&old);
        }
    }

    fn is_over(&self, what: char, id: &str) -> bool {
        self.ended.contains(&format!("{what}{id}"))
    }

    /// Adds to `out` what is to be told of this.
    pub fn take(&mut self, w: Word, out: &mut Vec<Event>) -> Result<(), Flood> {
        let e = match w {
            Word::Done(d, keep) => {
                self.done(d, &keep, out);
                return Ok(());
            }
            Word::Event(e) => e,
        };
        match e {
            Event::Began { by_agent } => {
                if !std::mem::replace(&mut self.turn, true) {
                    out.push(Event::Began { by_agent });
                }
            }
            Event::Say(s) => {
                if self.is_over('s', &s.id) {
                    return Ok(());
                }
                let at = match self.texts.iter().position(|t| t.0 == s.id) {
                    Some(at) => at,
                    None if s.text.is_empty() => return Ok(()),
                    None => {
                        if self.texts.len() >= MOST_TEXTS {
                            let (id, _) = self.texts.remove(0);
                            self.said(id, out);
                        }
                        self.texts.push((s.id.clone(), Sum::NONE));
                        self.texts.len() - 1
                    }
                };
                let sum = &mut self.texts[at].1;
                let new = if s.whole { Sum::NONE.add(&s.text) } else { sum.add(&s.text) };
                // What says nothing new is not told.
                if std::mem::replace(sum, new) != new {
                    out.push(Event::Say(s));
                }
            }
            Event::Said { id } => {
                if let Some(at) = self.texts.iter().position(|t| t.0 == id) {
                    self.texts.remove(at);
                    self.said(id, out);
                }
            }
            Event::ToolStarted(t) | Event::ToolUpdated(t) => {
                if self.is_over('t', &t.id) {
                    return Ok(());
                }
                let t = Box::new(bound(*t));
                if self.tools.contains(&t.id) {
                    out.push(Event::ToolUpdated(t.clone()));
                } else {
                    self.begin(&t.id)?;
                    out.push(Event::ToolStarted(t.clone()));
                }
                // What was said before stands where nothing is said now.
                if t.args.is_some() || t.args_cut > 0 {
                    self.is_cut((t.id.clone(), true), t.args_cut > 0);
                }
                if !t.diffs.is_empty() {
                    self.is_cut((t.id.clone(), false), t.diffs.iter().any(|d| d.cut > 0));
                }
            }
            Event::ToolOutput { id, text, whole, cut: c } => {
                if self.known(&id, out)? {
                    let (text, more) = head(text, OUTPUT);
                    out.push(Event::ToolOutput { id, text, whole, cut: c + more });
                }
            }
            Event::ToolEnded(t) => {
                if self.known(&t.id, out)? {
                    self.tools.remove(&t.id);
                    self.whole(&t.id);
                    self.over('t', &t.id);
                    out.push(Event::ToolEnded(Box::new(bound_end(*t))));
                }
            }
            Event::Asked(q) => {
                if self.asked.len() >= MOST_ASKED {
                    return Err(Flood("it asks too many things at once"));
                }
                if !self.is_over('q', &q.id) && self.asked.insert(q.id.clone()) {
                    out.push(Event::Asked(q));
                }
            }
            Event::AskClosed { id, how } => {
                if self.asked.remove(&id) {
                    self.over('q', &id);
                    out.push(Event::AskClosed { id, how });
                }
            }
            Event::Plan(p) => out.push(Event::Plan(bound_plan(p))),
            Event::Notice { level, text } => {
                out.push(Event::Notice { level, text: head(text, LINE).0 });
            }
            Event::NotHeard { why } => out.push(Event::NotHeard { why: head(why, LINE).0 }),
            Event::Unknown { what, raw, .. } => {
                let bytes = raw.len() as u64;
                let (what, raw) = (head(what, 256).0, head(raw, RAW).0);
                out.push(Event::Unknown { what, raw, bytes });
            }
            e => out.push(e),
        }
        Ok(())
    }

    fn said(&mut self, id: String, out: &mut Vec<Event>) {
        self.over('s', &id);
        out.push(Event::Said { id });
    }

    fn is_cut(&mut self, what: (String, bool), cut: bool) {
        if cut {
            self.cut.insert(what);
        } else {
            self.cut.remove(&what);
        }
    }

    fn whole(&mut self, tool: &str) {
        self.is_cut((tool.to_owned(), true), false);
        self.is_cut((tool.to_owned(), false), false);
    }

    fn begin(&mut self, id: &str) -> Result<(), Flood> {
        if self.tools.len() >= MOST_TOOLS {
            return Err(Flood("it has too many tool calls open at once"));
        }
        self.tools.insert(id.to_owned());
        Ok(())
    }

    /// Whether more may be told of this tool call. One that was never announced is now.
    fn known(&mut self, id: &str, out: &mut Vec<Event>) -> Result<bool, Flood> {
        if self.is_over('t', id) {
            return Ok(false);
        }
        if !self.tools.contains(id) {
            self.begin(id)?;
            out.push(Event::ToolStarted(Box::new(Tool { id: id.to_owned(), ..Tool::default() })));
        }
        Ok(true)
    }

    fn done(&mut self, d: Done, keep: &Keep, out: &mut Vec<Event>) {
        if !self.turn {
            return;
        }
        let stays = |id: &String| match keep {
            Keep::Nothing => false,
            Keep::All => true,
            Keep::These(k) => k.contains(id),
        };
        for (id, _) in std::mem::take(&mut self.texts) {
            self.said(id, out);
        }
        let outcome = match d.outcome {
            // It ended well, and this did not end: that is all that is known of it.
            Outcome::Completed => Outcome::Unknown,
            o => o,
        };
        for id in self.tools.iter().filter(|id| !stays(id)).cloned().collect::<Vec<_>>() {
            self.tools.remove(&id);
            self.whole(&id);
            self.over('t', &id);
            out.push(Event::ToolEnded(Box::new(ToolEnd {
                id,
                outcome,
                output: None,
                output_cut: 0,
                diffs: vec![],
                error: None,
            })));
        }
        if *keep == Keep::Nothing {
            for id in std::mem::take(&mut self.asked) {
                self.over('q', &id);
                out.push(Event::AskClosed { id, how: Settled::Withdrawn });
            }
        }
        self.turn = false;
        out.push(Event::Done(Done { error: d.error.map(|e| head(e, LINE).0), ..d }));
    }

    /// Nothing goes on: what is open ends with `d`.
    pub fn close(&mut self, d: Done, out: &mut Vec<Event>) {
        let turn = std::mem::replace(&mut self.turn, true);
        let at = out.len();
        self.done(d, &Keep::Nothing, out);
        // What was open besides a turn ends without one.
        if !turn && out.len() > at {
            out.pop();
        }
    }
}

/// `s`, at most `most` bytes of it, and how many were left out.
fn head(mut s: String, most: usize) -> (String, u64) {
    let (n, more) = (cut(&s, most).0.len(), cut(&s, most).1);
    s.truncate(n);
    (s, more)
}

fn bound(mut t: Tool) -> Tool {
    if let Some(a) = &t.args {
        let n = size(a);
        if n > ARGS {
            (t.args, t.args_cut) = (None, n as u64);
        }
    }
    t.name = head(t.name, 256).0;
    t.title = t.title.map(|x| head(x, LINE).0);
    t.diffs = diffs(t.diffs);
    t
}

/// Of `v` as it is written.
pub(crate) fn size(v: &Value) -> usize {
    struct N(usize);
    impl std::io::Write for N {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 += b.len();
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut n = N(0);
    // Into nothing: it does not fail.
    let _ = serde_json::to_writer(&mut n, v);
    n.0
}

fn bound_end(mut t: ToolEnd) -> ToolEnd {
    if let Some(o) = t.output.take() {
        let (o, more) = head(o, OUTPUT);
        t.output = Some(o);
        t.output_cut += more;
    }
    t.error = t.error.map(|e| head(e, LINE).0);
    t.diffs = diffs(t.diffs);
    t
}

fn diffs(mut all: Vec<Diff>) -> Vec<Diff> {
    let left = all.len().saturating_sub(DIFFS) as u64;
    all.truncate(DIFFS);
    if let Some(last) = all.last_mut() {
        // As many files more, at least.
        last.cut += left;
    }
    for d in &mut all {
        d.path = head(std::mem::take(&mut d.path), 4096).0;
        for part in [&mut d.patch, &mut d.old, &mut d.new] {
            if let Some(s) = part.take() {
                let (s, more) = head(s, DIFF);
                d.cut += more;
                *part = Some(s);
            }
        }
    }
    all
}

fn bound_plan(mut p: Plan) -> Plan {
    p.steps.truncate(STEPS);
    for s in &mut p.steps {
        s.0 = head(std::mem::take(&mut s.0), LINE).0;
    }
    p.text = p.text.map(|t| head(t, PLAN).0);
    p
}

/// What it costs to hold `e`, roughly, in bytes.
pub(crate) fn weight(e: &Event) -> usize {
    let o = |s: &Option<String>| s.as_ref().map_or(0, String::len);
    let d = |all: &[Diff]| -> usize {
        all.iter().map(|d| d.path.len() + o(&d.patch) + o(&d.old) + o(&d.new)).sum()
    };
    256 + match e {
        Event::Say(s) => s.text.len(),
        Event::ToolStarted(t) | Event::ToolUpdated(t) => {
            t.args.as_ref().map_or(0, size) + d(&t.diffs) + o(&t.title)
        }
        Event::ToolOutput { text, .. } => text.len(),
        Event::ToolEnded(t) => o(&t.output) + d(&t.diffs) + o(&t.error),
        Event::Plan(p) => o(&p.text) + p.steps.iter().map(|s| s.0.len()).sum::<usize>(),
        Event::Asked(q) => size(&q.subject) + o(&q.detail),
        Event::Unknown { raw, .. } => raw.len(),
        Event::Notice { text, .. } => text.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use crate::event::{Kind, Level, Progress, Question, Say};
    use serde_json::json;

    fn say(id: &str, t: &str, whole: bool) -> Word {
        let (id, text) = (id.into(), t.into());
        Word::Event(Event::Say(Say { id, kind: Kind::Prose, text, whole, parent: None }))
    }
    fn said(id: &str) -> Word {
        Word::Event(Event::Said { id: id.into() })
    }
    fn tool(id: &str) -> Word {
        Word::Event(Event::ToolStarted(Box::new(Tool { id: id.into(), ..Tool::default() })))
    }
    fn output(id: &str, t: &str) -> Word {
        Word::Event(Event::ToolOutput { id: id.into(), text: t.into(), whole: false, cut: 0 })
    }
    fn end(id: &str) -> Word {
        Word::Event(Event::ToolEnded(Box::new(ToolEnd {
            id: id.into(),
            outcome: Outcome::Completed,
            output: None,
            output_cut: 0,
            diffs: vec![],
            error: None,
        })))
    }
    fn ask(id: &str) -> Word {
        Word::Event(Event::Asked(Box::new(Question {
            id: id.into(),
            tool: None,
            title: "t".into(),
            detail: None,
            subject: Value::Null,
            options: vec![],
            by: None,
        })))
    }
    fn closed(id: &str) -> Word {
        Word::Event(Event::AskClosed { id: id.into(), how: Settled::Answered })
    }
    fn began() -> Word {
        Word::Event(Event::Began { by_agent: false })
    }
    fn done(o: Outcome, k: Keep) -> Word {
        Word::Done(Done { outcome: o, stop: "s".into(), error: None }, k)
    }
    fn run(l: &mut Ledger, all: Vec<Word>) -> Vec<String> {
        let mut out = Vec::new();
        for w in all {
            l.take(w, &mut out).unwrap();
        }
        out.iter().map(show).collect()
    }
    fn show(e: &Event) -> String {
        match e {
            Event::Began { .. } => "began".into(),
            Event::Say(s) => format!("say {} {}{}", s.id, s.text, if s.whole { "!" } else { "" }),
            Event::Said { id } => format!("said {id}"),
            Event::ToolStarted(t) => format!("tool {}", t.id),
            Event::ToolUpdated(t) => format!("more {}", t.id),
            Event::ToolOutput { id, text, .. } => format!("out {id} {text}"),
            Event::ToolEnded(t) => format!("end {} {:?}", t.id, t.outcome),
            Event::Asked(q) => format!("asked {}", q.id),
            Event::AskClosed { id, how } => format!("closed {id} {how:?}"),
            Event::Done(d) => format!("done {:?}", d.outcome),
            e => format!("{e:?}"),
        }
    }

    #[test]
    fn a_turn_begins_and_ends_once() {
        let mut l = Ledger::default();
        let d = || done(Outcome::Completed, Keep::Nothing);
        assert_eq!(run(&mut l, vec![d(), began(), began(), d(), d()]), ["began", "done Completed"]);
        assert!(!l.turn());
        run(&mut l, vec![began()]);
        assert!(l.turn());
    }

    #[test]
    fn what_is_open_ends_with_the_turn() {
        let mut l = Ledger::default();
        let all = vec![began(), say("b", "x", false), say("a", "y", false), tool("t"), ask("q")];
        run(&mut l, all);
        assert_eq!(l.asked(), 1);
        assert_eq!(
            run(&mut l, vec![done(Outcome::Cancelled, Keep::Nothing)]),
            ["said b", "said a", "end t Cancelled", "closed q Withdrawn", "done Cancelled"]
        );
        assert_eq!(l.asked(), 0);
        // Nothing of it comes back.
        let late = vec![say("a", "z", false), output("t", "o"), end("t"), closed("q"), ask("q")];
        assert!(run(&mut l, late).is_empty());
    }

    #[test]
    fn a_tool_call_left_open_by_a_turn_that_went_well_is_not_known_to_have() {
        let mut l = Ledger::default();
        let r = run(&mut l, vec![began(), tool("t"), done(Outcome::Completed, Keep::Nothing)]);
        assert_eq!(r[2], "end t Unknown");
    }

    #[test]
    fn what_goes_on_in_the_background_goes_on() {
        let mut l = Ledger::default();
        let keep = Keep::These(["t".to_owned()].into());
        let all = vec![began(), tool("t"), tool("u"), ask("q"), done(Outcome::Completed, keep)];
        assert_eq!(run(&mut l, all)[4..], ["end u Unknown", "done Completed"]);
        assert_eq!(run(&mut l, vec![output("t", "o"), end("t")]), ["out t o", "end t Completed"]);
        assert_eq!(run(&mut l, vec![closed("q")]), ["closed q Answered"]);
        let all = vec![began(), tool("v"), done(Outcome::Completed, Keep::All), end("v")];
        assert_eq!(run(&mut l, all), ["began", "tool v", "done Completed", "end v Completed"]);
    }

    #[test]
    fn a_tool_call_is_announced_before_anything_is_told_of_it() {
        let mut l = Ledger::default();
        assert_eq!(run(&mut l, vec![output("a", "x"), tool("a")]), ["tool a", "out a x", "more a"]);
        assert_eq!(run(&mut l, vec![end("b"), end("b")]), ["tool b", "end b Completed"]);
        let twice = vec![tool("c"), tool("c"), end("c"), tool("c")];
        assert_eq!(run(&mut l, twice), ["tool c", "more c", "end c Completed"]);
    }

    #[test]
    fn a_text_that_is_said_again_is_not_told_again() {
        let mut l = Ledger::default();
        let all = vec![
            say("a", "", true),
            say("a", "he", false),
            say("a", "llo", false),
            say("a", "hello", true),
        ];
        assert_eq!(run(&mut l, all), ["say a he", "say a llo"]);
        assert_eq!(run(&mut l, vec![say("a", "hallo", true)]), ["say a hallo!"]);
        assert_eq!(run(&mut l, vec![say("a", "", false)]), [""; 0]);
        assert_eq!(run(&mut l, vec![said("a"), said("a"), say("a", "x", true)]), ["said a"]);
        assert_eq!(run(&mut l, vec![say("b", "x", true), said("b")]), ["say b x!", "said b"]);
        assert!(run(&mut l, vec![said("never")]).is_empty());
    }

    #[test]
    fn a_question_is_asked_and_closed_once() {
        let mut l = Ledger::default();
        let all = vec![ask("q"), ask("q"), closed("q"), closed("q"), ask("q"), closed("x")];
        assert_eq!(run(&mut l, all), ["asked q", "closed q Answered"]);
    }

    #[test]
    fn what_was_cut_is_known_until_it_is_replaced_or_over() {
        let mut l = Ledger::default();
        let big = "x".repeat(ARGS.max(DIFF) + 1);
        let diff = |s: &str| Diff { new: Some(s.to_owned()), ..Diff::default() };
        let t = |args: Option<&str>, diffs: Vec<Diff>| {
            let args = args.map(|a| json!(a));
            let t = Tool { id: "t".into(), args, diffs, ..Tool::default() };
            Word::Event(Event::ToolUpdated(Box::new(t)))
        };
        let mut whole = |w: Word| {
            run(&mut l, vec![w]);
            l.is_whole("t")
        };
        assert!(whole(t(Some("a"), vec![diff("b")])));
        assert!(!whole(t(Some(&big), vec![])));
        assert!(!whole(t(None, vec![diff("b")])), "of the arguments nothing new was said");
        assert!(whole(t(Some("a"), vec![])));
        assert!(!whole(t(None, vec![diff("b"), diff(&big)])));
        assert!(!whole(t(Some("a"), vec![])));
        assert!(whole(t(None, vec![diff("b")])));
        assert!(!whole(t(None, vec![diff("b"); DIFFS + 1])), "some were left out");
        assert!(!whole(t(Some(&big), vec![])));
        assert!(whole(end("t")));
        assert!(l.is_whole("never heard of"));
        run(&mut l, vec![began(), tool("u")]);
        let t = Tool { id: "u".into(), args: Some(json!(big)), ..Tool::default() };
        run(&mut l, vec![Word::Event(Event::ToolUpdated(Box::new(t)))]);
        assert!(!l.is_whole("u"));
        run(&mut l, vec![done(Outcome::Completed, Keep::Nothing)]);
        assert!(l.is_whole("u") && l.cut.is_empty());
    }

    #[test]
    fn too_much_at_once_is_a_flood() {
        let mut l = Ledger::default();
        let mut out = Vec::new();
        for n in 0..MOST_TOOLS {
            l.take(tool(&n.to_string()), &mut out).unwrap();
        }
        assert!(l.take(tool("one more"), &mut out).is_err());
        assert!(l.take(output("another", "x"), &mut out).is_err());
        assert!(l.take(end("third"), &mut out).is_err());
        l.take(tool("0"), &mut out).unwrap();
        l.take(end("0"), &mut out).unwrap();
        l.take(tool("now"), &mut out).unwrap();
        for n in 0..MOST_ASKED {
            l.take(ask(&n.to_string()), &mut out).unwrap();
        }
        assert!(l.take(ask("more"), &mut out).is_err());
        // Texts give way instead: the oldest is as it is.
        let mut l = Ledger::default();
        let all = (0..=MOST_TEXTS).map(|n| say(&n.to_string(), "x", false)).collect();
        let r = run(&mut l, all);
        assert_eq!(r[MOST_TEXTS], "said 0");
        assert_eq!(r.len(), MOST_TEXTS + 2);
    }

    #[test]
    fn what_has_ended_is_remembered_for_a_while() {
        let mut l = Ledger::default();
        for n in 0..REMEMBERED + 10 {
            run(&mut l, vec![tool(&n.to_string()), end(&n.to_string())]);
        }
        assert_eq!(l.order.len(), REMEMBERED);
        assert_eq!(l.ended.len(), REMEMBERED);
        assert!(run(&mut l, vec![tool("10")]).is_empty());
        // Those of one kind are not those of another.
        assert_eq!(run(&mut l, vec![say("10", "x", false), ask("10")]).len(), 2);
    }

    #[test]
    fn closing_ends_all_and_begins_nothing() {
        let d = || Done { outcome: Outcome::Failed, stop: "x".into(), error: Some("w".into()) };
        let mut l = Ledger::default();
        let mut out = Vec::new();
        l.close(d(), &mut out);
        assert!(out.is_empty());
        run(&mut l, vec![tool("t"), ask("q")]);
        l.close(d(), &mut out);
        let r: Vec<_> = out.iter().map(show).collect();
        assert_eq!(r, ["end t Failed", "closed q Withdrawn"]);
        run(&mut l, vec![began(), tool("u")]);
        let mut out = Vec::new();
        l.close(d(), &mut out);
        let r: Vec<_> = out.iter().map(show).collect();
        assert_eq!(r, ["end u Failed", "done Failed"]);
    }

    #[test]
    fn everything_has_a_size() {
        let mut l = Ledger::default();
        let big = "é".repeat(1024 * 1024);
        let d = Diff {
            path: big.clone(),
            patch: Some(big.clone()),
            old: None,
            new: Some(big.clone()),
            cut: 1,
        };
        let t = Tool {
            id: "t".into(),
            name: big.clone(),
            title: Some(big.clone()),
            args: Some(json!({"a": big.clone()})),
            args_cut: 0,
            diffs: vec![d.clone(); DIFFS + 1],
            parent: None,
        };
        let e = ToolEnd {
            id: "t".into(),
            outcome: Outcome::Failed,
            output: Some(big.clone()),
            output_cut: 5,
            diffs: vec![d],
            error: Some(big.clone()),
        };
        let steps = vec![(big.clone(), Progress::Todo); STEPS + 1];
        let mut out = Vec::new();
        for x in [
            Event::ToolStarted(Box::new(t.clone())),
            Event::ToolUpdated(Box::new(t)),
            Event::ToolOutput { id: "t".into(), text: big.clone(), whole: true, cut: 3 },
            Event::ToolEnded(Box::new(e)),
            Event::Plan(Plan { id: "p".into(), steps, text: Some(big.clone()) }),
            Event::Notice { level: Level::Info, text: big.clone() },
            Event::NotHeard { why: big.clone() },
            Event::Unknown { what: big.clone(), raw: big.clone(), bytes: 0 },
        ] {
            l.take(Word::Event(x), &mut out).unwrap();
        }
        assert_eq!(out.len(), 8);
        let Event::ToolStarted(t) = &out[0] else { panic!() };
        assert!(t.args.is_none() && t.args_cut == 2 * 1024 * 1024 + 8);
        assert!(t.name.len() <= 256 && t.title.as_ref().unwrap().len() <= LINE);
        assert_eq!(t.diffs.len(), DIFFS);
        assert!(t.diffs[0].path.len() <= 4096);
        assert_eq!(t.diffs[0].cut as usize, 1 + 2 * (2 * 1024 * 1024 - DIFF));
        let Event::ToolOutput { text, cut, whole: true, .. } = &out[2] else { panic!() };
        assert_eq!((text.len(), *cut as usize), (OUTPUT, 3 + 2 * 1024 * 1024 - OUTPUT));
        let Event::ToolEnded(e) = &out[3] else { panic!() };
        assert_eq!(e.output_cut as usize, 5 + 2 * 1024 * 1024 - OUTPUT);
        assert!(e.error.as_ref().unwrap().len() <= LINE);
        let Event::Plan(p) = &out[4] else { panic!() };
        assert!(p.steps.len() == STEPS && p.steps[0].0.len() <= LINE);
        assert!(p.text.as_ref().unwrap().len() <= PLAN);
        let Event::Unknown { raw, bytes, what } = &out[7] else { panic!() };
        assert!(raw.len() <= RAW && *bytes == 2 * 1024 * 1024 && what.len() <= 256);
        let Event::Notice { text, .. } = &out[5] else { panic!() };
        let Event::NotHeard { why } = &out[6] else { panic!() };
        assert!(text.len() <= LINE && why.len() <= LINE);
        // Small ones are as they were.
        let t = Tool { id: "s".into(), args: Some(json!({"a": 1})), ..Tool::default() };
        l.take(Word::Event(Event::ToolStarted(Box::new(t.clone()))), &mut out).unwrap();
        assert_eq!(out[8], Event::ToolStarted(Box::new(t)));
        assert!(weight(&out[8]) < weight(&out[0]));
        for e in &out {
            assert!(weight(e) >= 256);
        }
        assert!(weight(&out[2]) >= OUTPUT && weight(&out[3]) >= OUTPUT + DIFF);
        assert!(weight(&out[4]) >= PLAN && weight(&out[0]) >= 2 * DIFF * DIFFS);
        assert!(weight(&out[5]) >= LINE && weight(&out[7]) >= RAW);
        let Word::Event(a) = ask("q") else { panic!() };
        assert!(weight(&a) < 300);
    }
}
