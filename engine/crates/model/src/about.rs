//! What a provider's own list says of a model beside its name. What it does not say is absent, and
//! what a list says is a claim: a request is judged by the provider, not here.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct About {
    /// Tokens it takes.
    pub context: Option<u64>,
    /// How hard it can be told to try, the least first. Empty: it cannot.
    pub efforts: Option<Vec<String>>,
    /// It can be told to think as much as it needs.
    pub adaptive: Option<bool>,
}

const MOST: usize = 12;

fn word(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    let plain = |c: char| c.is_ascii_alphanumeric() || "-_.".contains(c);
    (!s.is_empty() && s.len() <= 32 && s.chars().all(plain)).then(|| s.to_owned())
}

/// platform.claude.com/docs/en/api/models/list, read 2026-11-26: every leaf of `capabilities` says
/// whether it is `supported`, and a type of thinking is when a request with it is not refused.
pub(crate) fn anthropic(m: &Value) -> About {
    let has =
        |at: &str| m.pointer(&format!("/capabilities/{at}/supported")).and_then(Value::as_bool);
    let efforts = has("effort").map(|any| {
        let of = ["low", "medium", "high", "xhigh", "max"];
        let takes = |e: &&str| any && has(&format!("effort/{e}")) == Some(true);
        of.into_iter().filter(takes).map(str::to_owned).collect()
    });
    let adaptive = has("thinking/types/adaptive");
    About { context: m.get("max_input_tokens").and_then(Value::as_u64), efforts, adaptive }
}

/// A ChatGPT plan's list, where it says which efforts a model takes (`supported_reasoning_levels`, as
/// the Codex client reads it); where it does not, its model's page says.
pub(crate) fn chatgpt(m: &Value) -> About {
    let efforts = m
        .get("supported_reasoning_levels")
        .and_then(Value::as_array)
        .map(|all| all.iter().take(MOST).filter_map(|l| l.get("effort").and_then(word)).collect());
    About { efforts, ..About::default() }
}

/// aistudio.google.com/api-reference/models and /docs/thinking, read 2026-11-26: the list says whether
/// a model thinks, and the page which levels each takes.
pub(crate) fn gemini(id: &str, m: &Value) -> About {
    let levels: Option<&[&str]> = match id {
        "gemini-4-neon" | "gemini-4-hydrogen" => Some(&["low", "medium", "high", "xhigh"]),
        "gemini-3.6-flash"
        | "gemini-3.5-flash-lite"
        | "gemini-3.1-flash-lite"
        | "gemini-3-flash-preview" => Some(&["minimal", "low", "medium", "high"]),
        "gemini-3-pro-preview" => Some(&["low", "high"]),
        _ => None,
    };
    let efforts = match m.get("thinking").and_then(Value::as_bool) {
        Some(false) => Some(Vec::new()),
        _ => levels.map(|l| l.iter().map(|s| (*s).to_owned()).collect()),
    };
    About { context: m.get("inputTokenLimit").and_then(Value::as_u64), efforts, adaptive: None }
}

/// openrouter.ai/docs/guides/best-practices/reasoning-tokens, read 2026-11-26: `reasoning` is there if
/// the model reasons; its efforts come with the most first, and `null` is all of them. That it need
/// not reason is the effort `none`.
pub(crate) fn openrouter(m: &Value) -> About {
    let efforts = match m.get("reasoning") {
        Some(Value::Object(r)) => {
            let mut all: Option<Vec<String>> = match r.get("supported_efforts") {
                Some(Value::Array(a)) => Some(a.iter().take(MOST).rev().filter_map(word).collect()),
                Some(Value::Null) => Some(
                    ["minimal", "low", "medium", "high", "xhigh", "max"].map(str::to_owned).into(),
                ),
                // It is not told how hard.
                _ => Some(Vec::new()),
            };
            if let Some(all) = &mut all
                && !all.is_empty()
                && r.get("mandatory").and_then(Value::as_bool) != Some(true)
                && !all.iter().any(|e| e == "none")
            {
                all.insert(0, "none".into());
            }
            all
        }
        // One that does not reason.
        None | Some(Value::Null) if m.get("supported_parameters").is_some() => Some(Vec::new()),
        _ => None,
    };
    About { context: m.get("context_length").and_then(Value::as_u64), efforts, adaptive: None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn all(a: &[&str]) -> Option<Vec<String>> {
        Some(a.iter().map(|s| (*s).to_owned()).collect())
    }
    fn yes(b: bool) -> Value {
        json!({ "supported": b })
    }

    #[test]
    fn as_anthropic_lists() {
        let m = json!({
            "type": "model", "id": "claude-opus-5", "display_name": "Claude Opus 5",
            "max_input_tokens": 1_000_000, "max_tokens": 128_000,
            "capabilities": {
                "effort": {"supported": true, "low": yes(true), "medium": yes(true), "high": yes(true),
                    "max": yes(true), "xhigh": null},
                "thinking": {"supported": true, "types": {"adaptive": yes(true), "between_tools": yes(false),
                    "disabled": yes(true), "enabled": yes(false)}},
            },
        });
        let a = anthropic(&m);
        assert_eq!(a.context, Some(1_000_000));
        assert_eq!(a.efforts, all(&["low", "medium", "high", "max"]));
        assert_eq!(a.adaptive, Some(true));
        // That it cannot be told not to think is no matter: it is never told that.
        let mut always = m.clone();
        always["capabilities"]["thinking"]["types"]["disabled"] = yes(false);
        assert_eq!(anthropic(&always).adaptive, Some(true));
        let mut fixed = m.clone();
        fixed["capabilities"]["thinking"]["types"]["adaptive"] = yes(false);
        assert_eq!(anthropic(&fixed).adaptive, Some(false));
        let mut none = m.clone();
        none["capabilities"]["effort"] = json!({"supported": false, "low": yes(true)});
        assert_eq!(anthropic(&none).efforts, all(&[]));
        // What is not said is not known.
        for odd in [json!({}), json!({"capabilities": null, "max_input_tokens": null})] {
            assert_eq!(anthropic(&odd), About::default());
        }
    }

    #[test]
    fn as_a_chatgpt_plan_lists() {
        let levels = json!({"supported_reasoning_levels": [
            {"effort": "low", "description": "x"}, {"effort": "xhigh"}, {"effort": "no such word!"}, {}
        ]});
        assert_eq!(chatgpt(&levels).efforts, all(&["low", "xhigh"]));
        assert_eq!(
            chatgpt(&json!({"slug": "m"})),
            About::default(),
            "what is not said is not known"
        );
    }

    #[test]
    fn as_google_lists() {
        let of = |id: &str, m: Value| gemini(id, &m);
        assert_eq!(of("gemini-x", json!({"thinking": false})).efforts, all(&[]));
        assert_eq!(of("gemini-x", json!({"thinking": true})).efforts, None);
        let a = of("gemini-4-neon", json!({"thinking": true, "inputTokenLimit": 1_048_576}));
        assert_eq!(a.efforts, all(&["low", "medium", "high", "xhigh"]));
        assert_eq!((a.context, a.adaptive), (Some(1_048_576), None));
    }

    #[test]
    fn as_openrouter_lists() {
        let of = |r: Value| openrouter(&json!({"reasoning": r, "context_length": 9})).efforts;
        assert_eq!(
            of(json!({"supported_efforts": ["high", "low"], "mandatory": true})),
            all(&["low", "high"])
        );
        assert_eq!(
            of(json!({"supported_efforts": ["high", "low"]})),
            all(&["none", "low", "high"])
        );
        assert_eq!(of(json!({"supported_efforts": ["low", "none"]})), all(&["none", "low"]));
        assert_eq!(of(json!({"supported_efforts": null})).map(|e| e.len()), Some(7));
        assert_eq!(of(json!({"supported_efforts": ["a b", 1, ""], "mandatory": true})), all(&[]));
        assert_eq!(of(json!({})), all(&[]));
        assert_eq!(openrouter(&json!({"supported_parameters": ["tools"]})).efforts, all(&[]));
        assert_eq!(openrouter(&json!({})).efforts, None);
        assert_eq!(openrouter(&json!({"context_length": 9})).context, Some(9));
    }
}
