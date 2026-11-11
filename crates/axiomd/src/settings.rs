//! Known settings: key, default, validator (docs/protocols/ipc.md §Settings).

use serde_json::{json, Value};

pub struct Known {
    pub key: &'static str,
    pub default: fn() -> Value,
    pub valid: fn(&Value) -> bool,
}

fn is_theme(v: &Value) -> bool {
    matches!(v.as_str(), Some("system" | "light" | "dark"))
}
fn is_font_size(v: &Value) -> bool {
    v.as_i64().is_some_and(|n| (8..=40).contains(&n))
}
fn is_string(v: &Value) -> bool {
    v.is_string()
}
fn is_bool(v: &Value) -> bool {
    v.is_boolean()
}
fn is_string_or_null(v: &Value) -> bool {
    v.is_string() || v.is_null()
}

pub const KNOWN: &[Known] = &[
    Known {
        key: "appearance.theme",
        default: || json!("system"),
        valid: is_theme,
    },
    Known {
        key: "editor.fontSize",
        default: || json!(13),
        valid: is_font_size,
    },
    Known {
        key: "editor.fontFamily",
        default: || json!(""),
        valid: is_string,
    },
    Known {
        key: "editor.wordWrap",
        default: || json!(false),
        valid: is_bool,
    },
    Known {
        key: "workspace.reopenLast",
        default: || json!(true),
        valid: is_bool,
    },
    Known {
        key: "workspace.lastRoot",
        default: || Value::Null,
        valid: is_string_or_null,
    },
];

pub fn known(key: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.key == key)
}
