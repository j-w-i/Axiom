//! A JSON object whose members keep their order and their exact text.

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{Error as _, MapAccess, Visitor},
    ser::SerializeMap,
};
use serde_json::value::RawValue;

use crate::types::Json;

/// Looking a member up is a walk, so an object that is mostly members is refused.
const MEMBERS: usize = 1024;

#[derive(Debug, Clone, Default)]
pub(super) struct Object(Vec<(String, Json)>);

impl Object {
    pub fn get(&self, key: &str) -> Option<&Json> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn read<'a, T: Deserialize<'a>>(&'a self, key: &str) -> Option<T> {
        serde_json::from_str(self.get(key)?.get()).ok()
    }
    /// In place if it is there, else at the end.
    pub fn set(&mut self, key: &str, value: Json) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => self.0.push((key.to_owned(), value)),
        }
    }
    pub fn put(&mut self, key: &str, value: &(impl Serialize + ?Sized)) -> serde_json::Result<()> {
        self.set(key, Json::of(&value)?);
        Ok(())
    }
    pub fn with(
        mut self,
        key: &str,
        value: &(impl Serialize + ?Sized),
    ) -> serde_json::Result<Self> {
        self.put(key, value)?;
        Ok(self)
    }
    pub fn members(&self) -> impl Iterator<Item = (&str, &Json)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn json(&self) -> serde_json::Result<Json> {
        Json::of(self)
    }
}

impl Serialize for Object {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v.raw())?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Object {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Members;
        impl<'de> Visitor<'de> for Members {
            type Value = Object;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Object, A::Error> {
                let mut object = Object::default();
                // A repeated member: the last counts, as for every other reader here.
                while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                    object.set(&key, Json::from(&*value));
                    if object.0.len() > MEMBERS {
                        return Err(A::Error::custom("an object has too many members"));
                    }
                }
                Ok(object)
            }
        }
        d.deserialize_map(Members)
    }
}

/// JSON values in order, written as an array.
pub(super) struct Array<'a>(pub &'a [Json]);
impl Serialize for Array<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(Json::raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_order_are_kept() {
        let text = r#"{"z":1.0,"a":{"y": 12345678901234567890123,"b":"\u0041"},"m":[ ]}"#;
        let mut o: Object = serde_json::from_str(text).unwrap();
        assert_eq!(o.json().unwrap().get(), text);
        o.put("z", &2).unwrap();
        o.put("new", "\n").unwrap();
        let want = r#"{"z":2,"a":{"y": 12345678901234567890123,"b":"\u0041"},"m":[ ],"new":"\n"}"#;
        assert_eq!(o.json().unwrap().get(), want);
        assert_eq!(o.read::<u32>("z"), Some(2));
        assert_eq!((o.read::<u32>("m"), o.read::<u32>("none")), (None, None));
    }

    #[test]
    fn the_last_of_a_repeated_member_counts() {
        let o: Object = serde_json::from_str(r#"{"a":1,"b":2,"a":3}"#).unwrap();
        assert_eq!(o.json().unwrap().get(), r#"{"a":3,"b":2}"#);
    }

    #[test]
    fn an_object_is_bounded() {
        let many = |n: usize| {
            let members: Vec<String> = (0..n).map(|i| format!("\"{i}\":0")).collect();
            serde_json::from_str::<Object>(&format!("{{{}}}", members.join(",")))
        };
        assert!(many(MEMBERS).is_ok() && many(MEMBERS + 1).is_err());
        assert!(serde_json::from_str::<Object>("[]").is_err());
    }
}
