//! Tool schemas for a provider that does not take all of JSON Schema. Nothing that limits a value is
//! dropped: it is said in another keyword, or in the description, or the schema is refused.

use serde_json::Value;

use super::object::{Array, Object};
use crate::types::Json;

/// Deeper than this, and it is not a schema anyone wrote.
const DEPTH: usize = 64;

/// "Only the following properties are supported" (v1beta discovery document of 2026-11-23, of the
/// JSON schema of an answer; of that of a tool nothing is said but that it is one). The rest may be
/// sent and is not heeded.
const GEMINI: [&str; 21] = [
    "$id",
    "$defs",
    "$ref",
    "$anchor",
    "type",
    "format",
    "title",
    "description",
    "enum",
    "items",
    "prefixItems",
    "minItems",
    "maxItems",
    "minimum",
    "maximum",
    "anyOf",
    "oneOf",
    "properties",
    "additionalProperties",
    "required",
    "propertyOrdering",
];

/// These limit nothing.
const REMARKS: [&str; 8] = [
    "$schema",
    "$comment",
    "default",
    "deprecated",
    "example",
    "examples",
    "readOnly",
    "writeOnly",
];

/// `schema` as Gemini heeds it, to the byte where it is already; or why it cannot be.
pub(super) fn gemini(schema: &Json) -> Result<Json, String> {
    let root: Object =
        serde_json::from_str(schema.get()).map_err(|e| format!("it is not an object: {e}"))?;
    if root.read::<String>("type").as_deref() != Some("object") {
        return Err("it does not describe an object".into());
    }
    let mut walk = Walk { changed: false };
    let done = walk.object(root, 0)?;
    match walk.changed {
        true => json(&done),
        false => Ok(schema.clone()),
    }
}

struct Walk {
    changed: bool,
}

impl Walk {
    fn object(&mut self, from: Object, depth: usize) -> Result<Object, String> {
        if depth > DEPTH {
            return Err(format!("it is more than {DEPTH} deep"));
        }
        let (mut out, mut also) = (Object::default(), Object::default());
        for (key, value) in from.members() {
            match key {
                "properties" | "$defs" => out.set(key, self.named(key, value, depth)?),
                // Its older name.
                "definitions" if from.get("$defs").is_none() => {
                    self.changed = true;
                    out.set("$defs", self.named(key, value, depth)?);
                }
                "definitions" => return Err("it has both definitions and $defs".into()),
                "additionalProperties" if matches!(value.get(), "true" | "false") => {
                    out.set(key, value.clone());
                }
                "items" if value.get() == "false" => also.set(key, value.clone()),
                "items" | "additionalProperties" => out.set(key, self.schema(key, value, depth)?),
                "anyOf" | "prefixItems" => out.set(key, self.several(key, value, depth)?),
                "oneOf" => {
                    out.set(key, self.several(key, value, depth)?);
                    // Taken for `anyOf`.
                    put(&mut also, key, "exactly one")?;
                }
                "enum" if plain(value) => out.set(key, value.clone()),
                "enum" => also.set(key, value.clone()),
                "const" if from.get("enum").is_none() && is_plain(value) => {
                    self.changed = true;
                    out.set("enum", json(&Array(std::slice::from_ref(value)))?);
                }
                "$ref" => out.set(key, self.reference(value)?),
                key if GEMINI.contains(&key) => out.set(key, value.clone()),
                key if REMARKS.contains(&key) => self.changed = true,
                // Also what is newer than this build.
                key => also.set(key, value.clone()),
            }
        }
        if !also.is_empty() {
            self.changed = true;
            let also = format!("Must also satisfy: {}", json(&also)?.get());
            let said = match out.read::<String>("description").filter(|d| !d.is_empty()) {
                Some(d) => format!("{d}\n{also}"),
                None => also,
            };
            put(&mut out, "description", &said)?;
        }
        self.alone(out)
    }

    /// Beside `$ref` nothing may be set that does not begin with `$`.
    fn alone(&mut self, object: Object) -> Result<Object, String> {
        let beside = object.members().any(|(k, _)| !k.starts_with('$'));
        let Some(to) = object.get("$ref").filter(|_| beside) else { return Ok(object) };
        if object.get("anyOf").is_some() {
            return Err("it has anyOf beside $ref".into());
        }
        self.changed = true;
        let mut reference = Object::default();
        reference.set("$ref", to.clone());
        let mut out = Object::default();
        put(&mut out, "anyOf", &[reference])?;
        for (key, value) in object.members().filter(|(k, _)| *k != "$ref") {
            out.set(key, value.clone());
        }
        Ok(out)
    }

    /// What `definitions` became is referred to by its new name.
    fn reference(&mut self, to: &Json) -> Result<Json, String> {
        let text: String =
            serde_json::from_str(to.get()).map_err(|_| "a $ref is not a text".to_owned())?;
        match text.strip_prefix("#/definitions/") {
            Some(name) => {
                self.changed = true;
                json(&format!("#/$defs/{name}"))
            }
            None => Ok(to.clone()),
        }
    }

    /// A schema, which in its short forms anything or nothing satisfies.
    fn schema(&mut self, key: &str, value: &Json, depth: usize) -> Result<Json, String> {
        match value.get() {
            "true" => {
                self.changed = true;
                json(&Object::default())
            }
            "false" => Err(format!("{key} has a schema that nothing satisfies")),
            text => {
                let object: Object =
                    serde_json::from_str(text).map_err(|e| format!("a part is no schema: {e}"))?;
                json(&self.object(object, depth + 1)?)
            }
        }
    }

    fn named(&mut self, key: &str, value: &Json, depth: usize) -> Result<Json, String> {
        let all: Object =
            serde_json::from_str(value.get()).map_err(|e| format!("{key} are not schemas: {e}"))?;
        let mut out = Object::default();
        for (name, schema) in all.members() {
            out.set(name, self.schema(key, schema, depth)?);
        }
        json(&out)
    }

    fn several(&mut self, key: &str, value: &Json, depth: usize) -> Result<Json, String> {
        let all: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_str(value.get()).map_err(|e| format!("{key} is no list: {e}"))?;
        let all: Result<Vec<Json>, String> =
            all.iter().map(|raw| self.schema(key, &Json::from(&**raw), depth)).collect();
        json(&Array(&all?))
    }
}

/// Texts and numbers, which an `enum` may have.
fn plain(list: &Json) -> bool {
    serde_json::from_str::<Vec<Value>>(list.get())
        .is_ok_and(|all| all.iter().all(|v| v.is_string() || v.is_number()))
}
fn is_plain(value: &Json) -> bool {
    serde_json::from_str::<Value>(value.get()).is_ok_and(|v| v.is_string() || v.is_number())
}

fn json(value: &(impl serde::Serialize + ?Sized)) -> Result<Json, String> {
    serde_json::value::to_raw_value(value).map(|raw| Json::from(&*raw)).map_err(|e| e.to_string())
}
fn put(
    object: &mut Object,
    key: &str,
    value: &(impl serde::Serialize + ?Sized),
) -> Result<(), String> {
    object.put(key, value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn of(schema: &str) -> Result<String, String> {
        gemini(&Json::parse(schema).unwrap()).map(|j| j.get().to_owned())
    }
    /// Of a schema of one property, what became of the property's.
    fn property(schema: &str) -> Result<String, String> {
        let whole = of(&format!(r#"{{"type":"object","properties":{{"p":{schema}}}}}"#))?;
        let inner = whole.strip_prefix(r#"{"type":"object","properties":{"p":"#).unwrap();
        Ok(inner.strip_suffix("}}").unwrap().to_owned())
    }

    #[test]
    fn what_is_heeded_is_not_touched() {
        let all = r##"{ "type":"object", "$id":"i","$anchor":"a", "title":"t", "description":"d",
            "properties":{"z":{"type":["string","null"],"format":"date"},"n":{"type":"integer","minimum":1,"maximum":2.0},
            "a":{"type":"array","items":{"$ref":"#/$defs/D"},"minItems":1,"maxItems":9},"b":{"prefixItems":[{"enum":["x",1]}],"items":{}},
            "c":{"anyOf":[{"type":"string"},{}]}},"required":["z"],"additionalProperties":false,"propertyOrdering":["z"],
            "$defs":{"D":{"type":"object","additionalProperties":{"type":"string"}}}}"##;
        assert_eq!(of(all).unwrap(), all);
        for true_or_so in ["true", "{}"] {
            let s = format!(r#"{{"type":"object","additionalProperties": {true_or_so} }}"#);
            assert_eq!(of(&s).unwrap(), s);
        }
        // So are Axiom's own, which have no more than this.
        let own = json!({"type":"object","properties":{"path":{"type":"string","description":"d"},"from":{"type":"integer","minimum":1,"description":"d"},"all":{"type":"boolean","description":"d"}},"required":["path"],"additionalProperties":false});
        assert_eq!(of(&own.to_string()).unwrap(), own.to_string());
    }

    #[test]
    fn a_limit_that_is_not_heeded_is_said() {
        let said =
            |schema: &str, want: &str| assert_eq!(property(schema).unwrap(), want, "{schema}");
        said(
            r#"{"type":"string","pattern":"^a","minLength": 2}"#,
            r#"{"type":"string","description":"Must also satisfy: {\"pattern\":\"^a\",\"minLength\":2}"}"#,
        );
        said(
            r#"{"description":"d","type":"number","exclusiveMinimum":0}"#,
            r#"{"description":"d\nMust also satisfy: {\"exclusiveMinimum\":0}","type":"number"}"#,
        );
        said(
            r#"{"description":"","uniqueItems":true}"#,
            r#"{"description":"Must also satisfy: {\"uniqueItems\":true}"}"#,
        );
        said(
            r#"{"enum":["a",null]}"#,
            r#"{"description":"Must also satisfy: {\"enum\":[\"a\",null]}"}"#,
        );
        said(r#"{"const":true}"#, r#"{"description":"Must also satisfy: {\"const\":true}"}"#);
        said(
            r#"{"prefixItems":[{}],"items":false}"#,
            r#"{"prefixItems":[{}],"description":"Must also satisfy: {\"items\":false}"}"#,
        );
        said(
            r#"{"oneOf":[{"type":"string"},{"type":"null"}]}"#,
            r#"{"oneOf":[{"type":"string"},{"type":"null"}],"description":"Must also satisfy: {\"oneOf\":\"exactly one\"}"}"#,
        );
        // However deep, and whatever it is.
        said(
            r#"{"items":{"anyOf":[{"not":{"type":"null"}}]}}"#,
            r#"{"items":{"anyOf":[{"description":"Must also satisfy: {\"not\":{\"type\":\"null\"}}"}]}}"#,
        );
        said(
            r#"{"brand-new":{"x":[1.0]}}"#,
            r#"{"description":"Must also satisfy: {\"brand-new\":{\"x\":[1.0]}}"}"#,
        );
        let root = of(r#"{"type":"object","allOf":[{"required":["a"]}]}"#).unwrap();
        assert!(root.contains(r#"{\"allOf\":[{\"required\":[\"a\"]}]}"#));
    }

    #[test]
    fn the_same_in_other_words() {
        assert_eq!(property(r#"{"const":"a"}"#).unwrap(), r#"{"enum":["a"]}"#);
        assert_eq!(property(r#"{"const":1.50}"#).unwrap(), r#"{"enum":[1.50]}"#);
        assert_eq!(
            property(r#"{"enum":["b"],"const":"a"}"#).unwrap(),
            r#"{"enum":["b"],"description":"Must also satisfy: {\"const\":\"a\"}"}"#
        );
        // What limits nothing goes.
        assert_eq!(
            property(r##"{"$comment":"c","default":1,"deprecated":true,"example":2,"examples":[3],"readOnly":true,"writeOnly":true,"type":"integer"}"##)
                .unwrap(),
            r#"{"type":"integer"}"#
        );
        assert_eq!(of(r#"{"$schema":"s","type":"object"}"#).unwrap(), r#"{"type":"object"}"#);
        assert_eq!(property("true").unwrap(), "{}");
        assert_eq!(property(r#"{"items":true}"#).unwrap(), r#"{"items":{}}"#);
        assert_eq!(property(r#"{"anyOf":[true]}"#).unwrap(), r#"{"anyOf":[{}]}"#);
        // A reference stands alone.
        let refers = |p: &str| property(p).unwrap();
        for alone in [r##"{"$ref":"#/$defs/D"}"##, r##"{"$anchor":"a", "$ref":"#/$defs/D"}"##] {
            assert_eq!(refers(alone), alone);
        }
        assert_eq!(
            refers(r##"{"$ref":"#/$defs/D","description":"d","$anchor":"a"}"##),
            r##"{"anyOf":[{"$ref":"#/$defs/D"}],"description":"d","$anchor":"a"}"##
        );
        assert_eq!(
            refers(r##"{"$ref":"#/$defs/D","minLength":1}"##),
            r##"{"anyOf":[{"$ref":"#/$defs/D"}],"description":"Must also satisfy: {\"minLength\":1}"}"##
        );
        // By its older name.
        assert_eq!(
            of(r##"{"type":"object","properties":{"p":{"$ref":"#/definitions/D"}},"definitions":{"D":{"type":"string"}}}"##).unwrap(),
            r##"{"type":"object","properties":{"p":{"$ref":"#/$defs/D"}},"$defs":{"D":{"type":"string"}}}"##
        );
    }

    #[test]
    fn what_cannot_be_said_is_refused() {
        for bad in [
            "[]",
            "true",
            "{}",
            r#"{"type":"string"}"#,
            r#"{"type":["object"]}"#,
            r#"{"type":"object","properties":[]}"#,
            r#"{"type":"object","properties":{"p":1}}"#,
            r#"{"type":"object","properties":{"p":false}}"#,
            r#"{"type":"object","properties":{"p":{"anyOf":{}}}}"#,
            r#"{"type":"object","properties":{"p":{"anyOf":[false]}}}"#,
            r#"{"type":"object","properties":{"p":{"items":[{}]}}}"#,
            r##"{"type":"object","properties":{"p":{"$ref":"#","anyOf":[]}}}"##,
            r#"{"type":"object","properties":{"p":{"$ref":1}}}"#,
            r#"{"type":"object","$defs":{},"definitions":{}}"#,
        ] {
            assert!(of(bad).is_err(), "{bad}");
        }
        let deep = |n: usize| {
            let mut s = r#"{"type":"object","items":"#.repeat(n);
            s.push_str(r#"{"type":"object"}"#);
            s.push_str(&"}".repeat(n));
            of(&s)
        };
        assert!(deep(DEPTH).is_ok() && deep(DEPTH + 1).is_err());
    }
}
