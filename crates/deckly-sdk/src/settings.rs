//! Reading settings that may not fit their type: saved by an older version
//! of the plugin, edited by hand, or missing fields added later.
//!
//! The saved JSON is laid over the type's `Default`, so a missing field
//! keeps its default value without `#[serde(default)]`. A field that does
//! not fit (a string where a number belongs) falls back to its default too,
//! and the other fields keep their saved values.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// Settings with more fields than this are not repaired field by field
/// when they do not fit; they read as `Default` instead.
const MAX_REPAIRED_FIELDS: usize = 256;

/// Reads `saved` as `T`. The text says what was not used, for the log.
pub(crate) fn read<T>(saved: &Value) -> (T, Option<String>)
where
    T: Serialize + DeserializeOwned + Default,
{
    let base = match serde_json::to_value(T::default()) {
        Ok(Value::Object(base)) => base,
        // Not a struct: nothing to lay the saved value over.
        _ => {
            return match serde_json::from_value(saved.clone()) {
                Ok(v) => (v, None),
                Err(e) => (T::default(), Some(format!("{e}; using defaults"))),
            };
        }
    };
    let saved = match saved {
        Value::Object(saved) => saved,
        Value::Null => return parse(Value::Object(base)),
        other => {
            let what = kind(other);
            let (v, _) = parse(Value::Object(base));
            return (
                v,
                Some(format!("saved as {what}, not an object; using defaults")),
            );
        }
    };
    let mut merged = base.clone();
    for (k, v) in saved {
        overlay(&mut merged, k, v);
    }
    if let Ok(v) = serde_json::from_value(Value::Object(merged)) {
        return (v, None);
    }
    if saved.len() > MAX_REPAIRED_FIELDS {
        let (v, _) = parse(Value::Object(base));
        return (v, Some("they do not fit; using defaults".into()));
    }
    // Keep each saved field that fits, one at a time.
    let mut good = base;
    let mut dropped = Vec::new();
    for (k, v) in saved {
        let mut trial = good.clone();
        overlay(&mut trial, k, v);
        if serde_json::from_value::<T>(Value::Object(trial.clone())).is_ok() {
            good = trial;
        } else {
            dropped.push(k.as_str());
        }
    }
    let note = format!("{} did not fit; using the default", dropped.join(", "));
    match serde_json::from_value(Value::Object(good)) {
        Ok(v) => (v, Some(note)),
        Err(e) => (T::default(), Some(format!("{e}; using defaults"))),
    }
}

fn parse<T: DeserializeOwned + Default>(value: Value) -> (T, Option<String>) {
    match serde_json::from_value(value) {
        Ok(v) => (v, None),
        Err(e) => (T::default(), Some(format!("{e}; using defaults"))),
    }
}

/// Sets `key` to `value`, merging objects into objects field by field.
fn overlay(into: &mut Map<String, Value>, key: &str, value: &Value) {
    match (into.get_mut(key), value) {
        (Some(Value::Object(old)), Value::Object(new)) => {
            for (k, v) in new {
                overlay(old, k, v);
            }
        }
        _ => {
            into.insert(key.to_owned(), value.clone());
        }
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Look {
        size: u32,
        color: String,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct S {
        name: String,
        step: u32,
        look: Look,
    }

    impl Default for S {
        fn default() -> Self {
            Self {
                name: "World".into(),
                step: 1,
                look: Look {
                    size: 12,
                    color: "#fff".into(),
                },
            }
        }
    }

    #[test]
    fn missing_fields_keep_their_defaults() {
        let (s, note) = read::<S>(&json!({ "name": "Ada", "look": { "size": 20 } }));
        assert_eq!(note, None);
        assert_eq!(s.name, "Ada");
        assert_eq!(s.step, 1);
        assert_eq!(s.look.size, 20);
        assert_eq!(s.look.color, "#fff");
    }

    #[test]
    fn a_wrong_field_does_not_reset_the_others() {
        let (s, note) = read::<S>(&json!({ "name": "Ada", "step": "three" }));
        assert_eq!(s.name, "Ada");
        assert_eq!(s.step, 1);
        assert!(note.is_some_and(|n| n.contains("step")));
    }

    #[test]
    fn empty_and_odd_settings_read_as_defaults() {
        assert_eq!(read::<S>(&json!({})).0, S::default());
        assert_eq!(read::<S>(&Value::Null), (S::default(), None));
        let (s, note) = read::<S>(&json!([1, 2]));
        assert_eq!(s, S::default());
        assert!(note.is_some());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let (s, note) = read::<S>(&json!({ "old": true, "step": 4 }));
        assert_eq!((s.step, note), (4, None));
    }

    #[test]
    fn values_that_are_not_structs_read_as_saved() {
        let (v, note) = read::<Value>(&json!({ "a": 1 }));
        assert_eq!((v, note), (json!({ "a": 1 }), None));
    }
}
