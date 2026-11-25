//! The `.jqlibrary` file: a library of cue presets.

use std::collections::BTreeMap;

use jq_show::ParamValue;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const FORMAT: &str = "jqlibrary";
pub const VERSION: u32 = 1;

/// Text in several languages, by language code ("en", "de").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Text(pub BTreeMap<String, String>);

impl Text {
    /// The text in `lang`, else in English, else in the first language it
    /// has; empty if it has none.
    pub fn get(&self, lang: &str) -> &str {
        self.0
            .get(lang)
            .or_else(|| self.0.get("en"))
            .or_else(|| self.0.values().next())
            .map_or("", String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.values().all(|t| t.trim().is_empty())
    }

    /// The text in each of its languages.
    pub fn variants(&self) -> impl Iterator<Item = &str> {
        self.0.values().map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub format: String,
    pub version: u32,
    /// Stable identity; cues made from its presets refer to it.
    pub id: Uuid,
    /// Raised with every published change.
    pub revision: u32,
    pub name: Text,
    #[serde(default)]
    pub description: Text,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    /// Oldest program version the library is meant for (informational).
    #[serde(default)]
    pub min_app: String,
    /// Network devices its OSC presets send to.
    #[serde(default)]
    pub devices: Vec<DeviceTemplate>,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub presets: Vec<Preset>,
}

impl Library {
    pub fn preset(&self, id: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }

    pub fn device(&self, id: &str) -> Option<&DeviceTemplate> {
        self.devices.iter().find(|d| d.id == id)
    }

    /// The languages its texts come in (from the library's name and the
    /// presets' titles).
    pub fn languages(&self) -> Vec<String> {
        let mut langs: Vec<String> =
            self.name.0.keys().chain(self.presets.iter().flat_map(|p| p.title.0.keys())).cloned().collect();
        langs.sort();
        langs.dedup();
        langs
    }
}

/// A network device a library's OSC presets address, e.g. "X32" on port
/// 10023; the address is the operator's to set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceTemplate {
    pub id: String,
    pub name: Text,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: Text,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// Stable within the library.
    pub id: String,
    /// Raised with every change; a cue made from an older one can be updated.
    pub revision: u32,
    #[serde(default)]
    pub category: Option<String>,
    pub title: Text,
    #[serde(default)]
    pub description: Text,
    /// Search words, by language.
    #[serde(default)]
    pub keywords: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub params: Vec<Param>,
    /// The cue, as in a show file, without `id`; strings may hold
    /// placeholders (see [`crate::template`]). An OSC cue's `device` names
    /// one of the library's [`DeviceTemplate`]s.
    pub cue: serde_json::Value,
}

impl Preset {
    pub fn param(&self, id: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub id: String,
    pub label: Text,
    #[serde(flatten)]
    pub kind: ParamKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamKind {
    Int {
        min: i64,
        max: i64,
        default: i64,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        unit: String,
        /// Piecewise-linear map applied to the value (see [`map_value`]).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        map: Vec<[f64; 2]>,
    },
    Float {
        min: f64,
        max: f64,
        default: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        unit: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        map: Vec<[f64; 2]>,
    },
    Text {
        #[serde(default)]
        default: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_len: Option<usize>,
        /// Regular expression the whole text must match.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
        #[serde(default)]
        required: bool,
    },
    Choice {
        values: Vec<Choice>,
        default: ParamValue,
    },
}

impl ParamKind {
    pub fn default_value(&self) -> ParamValue {
        match self {
            ParamKind::Int { default, .. } => ParamValue::Number(*default as f64),
            ParamKind::Float { default, .. } => ParamValue::Number(*default),
            ParamKind::Text { default, .. } => ParamValue::Text(default.clone()),
            ParamKind::Choice { default, .. } => default.clone(),
        }
    }

    pub fn map(&self) -> &[[f64; 2]] {
        match self {
            ParamKind::Int { map, .. } | ParamKind::Float { map, .. } => map,
            _ => &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub value: ParamValue,
    pub label: Text,
}

/// `x` through the piecewise-linear `map` (points by ascending input),
/// clamped to its ends; `x` itself without a map.
pub fn map_value(map: &[[f64; 2]], x: f64) -> f64 {
    let (Some(first), Some(last)) = (map.first(), map.last()) else {
        return x;
    };
    if x <= first[0] {
        return first[1];
    }
    if x >= last[0] {
        return last[1];
    }
    for pair in map.windows(2) {
        let ([x0, y0], [x1, y1]) = (pair[0], pair[1]);
        if x <= x1 {
            if x1 == x0 {
                return y1;
            }
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        }
    }
    last[1]
}
