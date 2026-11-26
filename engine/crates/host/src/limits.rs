//! The limits this host keeps, as they are told to a person: each from what it is kept by.

use std::time::Duration;

use axiom_journal::FRAMES;
use axiom_proto::{Limit, Unit, limits as wire};
use axiom_tools::{copy, jobs};

use crate::{agents, connections, copies, delegated, peers, server};

/// Of a run of Axiom's own agents.
pub fn run() -> axiom_engine::Limits {
    axiom_engine::Limits::default()
}

/// Of an installed program that is an agent.
pub fn program() -> axiom_harness::Limits {
    axiom_harness::Limits::default()
}

/// Of a copy of a folder.
pub fn copied() -> copy::Most {
    copy::Most::default()
}

pub fn kept() -> Vec<Limit> {
    let (run, program, copied) = (run(), program(), copied());
    let n = |of: &str, what: &str, most: usize| one(of, what, most as u64, Unit::Count);
    let bytes = |of: &str, what: &str, most: u64| one(of, what, most, Unit::Bytes);
    let time = |of: &str, what: &str, most: Duration| one(of, what, most.as_secs(), Unit::Seconds);
    let none = |l: Limit, there: bool| Limit { most: l.most.filter(|_| there), ..l };
    let (r, p, a, s, f, h) = (
        "A run of Axiom's own agents",
        "An installed program's agent",
        "Agents together",
        "What is said and kept",
        "Files",
        "The host",
    );
    vec![
        n(r, "Steps, each an answer of the model", run.steps as usize),
        time(r, "Time, without waiting for you", run.time),
        none(
            one(r, "Output tokens", run.output_tokens.unwrap_or(0), Unit::Count),
            run.output_tokens.is_some(),
        ),
        time(r, "One command", Duration::from_secs(axiom_tools::COMMAND)),
        n(r, "Commands in the background", jobs::MOST),
        none(
            time(
                p,
                "Silence while it works, before it is stopped",
                program.silence.unwrap_or_default(),
            ),
            program.silence.is_some(),
        ),
        time(p, "Nothing to do before its program is ended", delegated::IDLE),
        one(p, "The last messages with it that are kept", FRAMES, Unit::Count),
        n(a, "Agents in a session", agents::MOST),
        n(a, "Agents that give each other work, one below another", peers::DEEPEST),
        n(a, "Runs an agent has begun and not heard the end of", peers::OWED),
        one(
            a,
            "Runs in a row that others may begin, without your word",
            u64::from(server::WAKES),
            Unit::Count,
        ),
        n(a, "Runs at once, in all sessions", server::RUNS),
        n(s, "Messages that wait for an agent at work", agents::INBOX),
        bytes(s, "One message to an agent", wire::TEXT as u64),
        bytes(s, "One message from an agent to another", peers::TEXT as u64),
        bytes(s, "One file of a folder's instructions", agents::INSTRUCTIONS),
        bytes(s, "One part of an answer", wire::TEXT as u64),
        bytes(s, "What a tool is given, and what it gives", wire::VALUE as u64),
        bytes(s, "What a request is about", wire::LONG as u64),
        bytes(f, "A file an agent reads or changes", axiom_tools::LARGEST),
        n(f, "Files in an agent's copy of a folder", copied.files),
        bytes(f, "An agent's copy of a folder", copied.bytes),
        bytes(f, "A file that is applied from a copy", copy::LARGEST),
        n(f, "Points in a conversation whose files are kept", copies::POINTS),
        bytes(f, "What is kept for those, of one agent", copies::KEPT),
        n(h, "Windows and terminals connected", server::CLIENTS),
        n(h, "Accounts and servers", connections::MOST),
    ]
}

fn one(of: &str, what: &str, most: u64, unit: Unit) -> Limit {
    Limit { of: of.into(), what: what.into(), most: Some(most), unit }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Those that no test keeps by trying are at least told from what they are kept by.
    #[test]
    fn each_is_of_what_it_says() {
        let all = kept();
        for (what, most) in [
            ("answer of the model", u64::from(run().steps)),
            ("without waiting", run().time.as_secs()),
            ("One command", axiom_tools::COMMAND),
            ("in the background", jobs::MOST as u64),
            ("Silence", program().silence.map_or(0, |s| s.as_secs())),
            ("Nothing to do", delegated::IDLE.as_secs()),
            ("last messages", FRAMES),
            ("one below another", peers::DEEPEST as u64),
            ("not heard the end of", peers::OWED as u64),
            ("in a row", u64::from(server::WAKES)),
            ("Runs at once", server::RUNS as u64),
            ("from an agent to another", peers::TEXT as u64),
            ("instructions", agents::INSTRUCTIONS),
            ("part of an answer", wire::TEXT as u64),
            ("a tool is given", wire::VALUE as u64),
            ("a request is about", wire::LONG as u64),
            ("reads or changes", axiom_tools::LARGEST),
            ("Files in an agent's copy", copied().files as u64),
            ("An agent's copy of a folder", copied().bytes),
            ("applied from a copy", copy::LARGEST),
            ("whose files are kept", copies::POINTS as u64),
            ("kept for those", copies::KEPT),
        ] {
            let of: Vec<_> = all.iter().filter(|l| l.what.contains(what)).collect();
            assert_eq!(of.iter().map(|l| l.most).collect::<Vec<_>>(), [Some(most)], "{what}");
        }
    }
}
