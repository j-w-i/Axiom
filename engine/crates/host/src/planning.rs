//! What an agent means to do: its own list, which is its claim and nothing depends on (PR-25), and a
//! plan it hands in before it does anything, for its person to accept.

use axiom_engine::{Other, Say, Stage};
use axiom_policy::rules::Decision;
use axiom_proto::{Body, ItemId, ItemKind, Outcome, Todo, TodoState, limits};
use axiom_tools::{Output, Spec};
use serde_json::{Value, json};

use crate::agents::Floor;

pub const TODO: &str = "todo";
pub const PLAN: &str = "plan";
/// Bytes of a plan.
pub const MOST: usize = 64 * 1024;

/// For an agent that is to hand in a plan.
pub const WORDS: &str = "You are asked for a plan first. Look as much as you need to; you can change nothing yet. If something is open that only your person can decide, ask them. When you know what to do, hand in the plan with the tool plan: what will be changed, where and in what order, how it will be checked, and what is not certain. Then end your turn, without saying the plan again. You begin when they have accepted it.";

/// What an agent is told whose plan has been accepted.
pub fn accepted(text: Option<String>) -> String {
    match text {
        None => "Your plan is accepted. Do it now.".into(),
        Some(t) => format!("Your plan is accepted as it is here. Do it now.\n\n{}", t.trim()),
    }
}

pub fn plan() -> Spec {
    Spec {
        name: PLAN,
        description: "Hands in your plan, for your person to accept, change or refuse. It is shown to them whole. Another one takes its place.",
        schema: json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "The whole plan, in Markdown."},
            },
            "required": ["text"],
            "additionalProperties": false,
        }),
    }
}

fn planned(args: &Value) -> Result<&str, String> {
    let text = args.get("text").and_then(Value::as_str).map(str::trim);
    let text = text.filter(|t| !t.is_empty()).ok_or("say it: text")?;
    if text.len() > MOST {
        return Err(format!("a plan has at most {MOST} bytes"));
    }
    Ok(text)
}

/// Only from one that was asked for it. It changes nothing: what is accepted is its person's to say.
pub async fn judge_plan(f: &Floor, args: &Value) -> Other {
    match planned(args) {
        Err(why) => seen(Decision::Deny, why),
        Ok(_) if !f.setup().await.is_some_and(|s| s.plan) => {
            seen(Decision::Deny, "nobody asked for a plan".into())
        }
        Ok(_) => seen(Decision::Allow, "it is only shown".into()),
    }
}

pub async fn hand_in(f: &Floor, args: &Value) -> Output {
    let out = |ok, text: String| Output { ok, text, rest: None, detail: Value::Null };
    let text = match planned(args) {
        Ok(t) => t.to_owned(),
        Err(why) => return out(false, format!("Not done: {why}.")),
    };
    let Some(run) = f.run().await else { return out(false, "Not done: there is no run.".into()) };
    let item = ItemId::new();
    let begins = Body::ItemStarted {
        item: item.clone(),
        run,
        agent: f.agent.clone(),
        kind: ItemKind::Plan,
        name: String::new(),
        args: None,
        args_cut: 0,
    };
    let all = vec![
        Say::Event(begins),
        Say::Text(item.clone(), text),
        Say::End(item.clone(), Outcome::Completed, None),
    ];
    let said = async {
        Stage::say(f, all).await?;
        // Shown before it can be accepted.
        Stage::sync(f).await
    };
    if let Err(e) = said.await {
        return out(false, format!("Not handed in: {e}."));
    }
    f.keep(|s| s.planned = Some(item)).await;
    out(true, "Handed in. Your person decides; end your turn now.".into())
}

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
    match understand(args) {
        Ok(_) => seen(Decision::Allow, "it is only shown".into()),
        Err(why) => seen(Decision::Deny, why),
    }
}

fn seen(decision: Decision, why: String) -> Other {
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
    match Stage::say(f, vec![say]).await {
        Ok(()) => out(true, "Shown.".into(), json!({"summary": summary})),
        Err(e) => out(false, format!("Not shown: {e}."), Value::Null),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_has_words_and_an_end() {
        let of = |t: &str| planned(&json!({ "text": t })).map(str::to_owned);
        assert_eq!(of(" # P \n").as_deref(), Ok("# P"));
        assert!(of(&"x".repeat(MOST)).is_ok() && of(&"x".repeat(MOST + 1)).is_err());
        assert!(of(" ").is_err() && planned(&json!({})).is_err());
    }

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
