//! An agent's own list of things to do. It is the agent's claim, and nothing depends on it (PR-25).

use axiom_engine::{Other, Say};
use axiom_policy::rules::Decision;
use axiom_proto::{Body, Todo, TodoState, limits};
use axiom_tools::{Output, Spec};
use serde_json::{Value, json};

use crate::agents::Floor;

pub const TODO: &str = "todo";

pub fn todo() -> Spec {
    Spec {
        name: TODO,
        description: "Shows the person your list of things to do. Give the whole list each time: it replaces the one before. Use it for work of several steps: write it when you begin, keep one step doing, and mark each done when it is done and not later. An empty list takes it away.",
        schema: json!({
            "type": "object",
            "properties": {
                "steps": {
                    "type": "array",
                    "maxItems": limits::STEPS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "text": {"type": "string", "description": "What is to be done, in a few words."},
                            "state": {"type": "string", "enum": ["todo", "doing", "done"]},
                        },
                        "required": ["text", "state"],
                        "additionalProperties": false,
                    },
                },
            },
            "required": ["steps"],
            "additionalProperties": false,
        }),
    }
}

fn understand(args: &Value) -> Result<Vec<Todo>, String> {
    let all = args.get("steps").and_then(Value::as_array).ok_or("say which: steps")?;
    if all.len() > limits::STEPS {
        return Err(format!("a list has at most {} steps", limits::STEPS));
    }
    let one = |v: &Value| {
        let s = |k: &str| v.get(k).and_then(Value::as_str);
        let text = s("text").map(str::trim).filter(|t| !t.is_empty());
        let state = match s("state") {
            Some("todo") => TodoState::Todo,
            Some("doing") => TodoState::Doing,
            Some("done") => TodoState::Done,
            _ => return Err("a step's state is todo, doing or done".to_owned()),
        };
        Ok(Todo { text: text.ok_or("a step has a text")?.to_owned(), state })
    };
    all.iter().map(one).collect()
}

/// Nothing is asked: it changes nothing but what is shown.
pub fn judge(args: &Value) -> Other {
    let (decision, why) = match understand(args) {
        Ok(_) => (Decision::Allow, "it is only shown".to_owned()),
        Err(why) => (Decision::Deny, why),
    };
    Other {
        decision,
        what: String::new(),
        subject: String::new(),
        scope: String::new(),
        why,
        changes: false,
    }
}

pub async fn perform(f: &Floor, args: &Value) -> Output {
    let out = |ok, text: String, detail| Output { ok, text, rest: None, detail };
    let steps = match understand(args) {
        Ok(s) => s,
        Err(why) => return out(false, format!("Not done: {why}."), Value::Null),
    };
    let done = steps.iter().filter(|s| s.state == TodoState::Done).count();
    let summary = match steps.len() {
        0 => "nothing".to_owned(),
        n => format!("{done} of {n} done"),
    };
    let run = f.run().await;
    let say = Say::Event(Body::Todo { agent: f.agent.clone(), run, steps, text: None });
    match axiom_engine::Stage::say(f, vec![say]).await {
        Ok(()) => out(true, "Shown.".into(), json!({"summary": summary})),
        Err(e) => out(false, format!("Not shown: {e}."), Value::Null),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_list_is_whole_or_refused() {
        let of = |v: Value| understand(&json!({"steps": v}));
        let step = |t: &str, s: &str| json!({"text": t, "state": s});
        assert_eq!(of(json!([])).unwrap(), []);
        let all = of(json!([step(" a ", "done"), step("b", "doing"), step("c", "todo")])).unwrap();
        let want = [("a", TodoState::Done), ("b", TodoState::Doing), ("c", TodoState::Todo)];
        assert_eq!(all, want.map(|(t, state)| Todo { text: t.into(), state }));
        for bad in [
            json!(null),
            json!([1]),
            json!([step("", "todo")]),
            json!([step("a", "finished")]),
            json!([step("a", "todo"), {"text": "b"}]),
            json!(vec![step("a", "todo"); limits::STEPS + 1]),
        ] {
            assert!(of(bad.clone()).is_err(), "{bad}");
            assert_eq!(judge(&json!({"steps": bad})).decision, Decision::Deny);
        }
        assert_eq!(judge(&json!({"steps": []})).decision, Decision::Allow);
    }
}
