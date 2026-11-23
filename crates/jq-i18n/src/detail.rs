//! What the UI words: a message of a kind with its data (`{ "kind": …, … }`),
//! or a text from the system (a device, file or network error) that is shown
//! as it is. The backend's layers build these instead of texts; the UI has the
//! wording (`app/src/lib/errors.ts`) and the types (`Detail` in
//! `app/src/lib/api.ts`).

use std::fmt;

use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Detail(Value);

impl Detail {
    /// A message of `kind` with `data` (see [`detail!`](crate::detail)).
    pub fn new(kind: &str, data: Vec<(&str, Value)>) -> Self {
        let mut map = Map::new();
        map.insert("kind".into(), kind.into());
        for (name, value) in data {
            map.insert(name.into(), value);
        }
        Self(Value::Object(map))
    }

    /// A message of a type that serializes as one (`{ "kind": …, … }`).
    pub fn of(message: &impl Serialize) -> Self {
        Self(value(message))
    }

    /// A text from the system, shown as it is.
    pub fn text(text: impl Into<String>) -> Self {
        Self(Value::String(text.into()))
    }

    /// The kind of a message; `None` for a text.
    pub fn kind(&self) -> Option<&str> {
        self.0.get("kind")?.as_str()
    }

    /// A field of a message.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.get(name)
    }

    /// The text, if it is one.
    pub fn as_text(&self) -> Option<&str> {
        self.0.as_str()
    }
}

/// A value of a [`detail!`](crate::detail) field.
pub fn value(value: impl Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

impl From<String> for Detail {
    fn from(text: String) -> Self {
        Self::text(text)
    }
}

impl From<&str> for Detail {
    fn from(text: &str) -> Self {
        Self::text(text)
    }
}

/// For logs: a text as it is, a message as its data.
impl fmt::Display for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Value::String(text) => f.write_str(text),
            other => write!(f, "{other}"),
        }
    }
}

/// `detail!("kind", name = value, …)`: a [`Detail`] of that kind; each value
/// is anything serializable (a nested `Detail` too).
#[macro_export]
macro_rules! detail {
    ($kind:expr $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::Detail::new($kind, vec![$((stringify!($name), $crate::detail::value(&$value))),*])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_has_its_kind_and_data_and_a_text_stays_text() {
        let inner = crate::detail!("media_corrupt");
        let d = crate::detail!("cue_failed", item = 2, error = inner);
        assert_eq!(
            serde_json::to_value(&d).unwrap(),
            serde_json::json!({ "kind": "cue_failed", "item": 2, "error": { "kind": "media_corrupt" } })
        );
        assert_eq!(d.kind(), Some("cue_failed"));
        let text = Detail::text("disk full");
        assert_eq!((text.kind(), text.as_text()), (None, Some("disk full")));
        assert_eq!(serde_json::to_value(&text).unwrap(), serde_json::json!("disk full"));
    }
}
