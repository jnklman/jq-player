//! Cue libraries (`.jqlibrary`): ready-made cue presets, e.g. OSC cues for a
//! mixing desk, with parameters filled in when one is inserted.
//!
//! A cue made from a preset holds all of its data; the show never needs the
//! library again. [`jq_show::PresetRef`] only notes where it came from, so it
//! can be reset to its preset or updated from a newer revision.

mod format;
pub mod template;

use std::collections::{BTreeMap, HashSet};

use jq_show::{AdvanceAt, Cue, CueId, CueKind, ParamValue, PresetRef, Targets};
use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value};

pub use format::*;
use template::{Filled, TemplateError, Values};

pub const EXTENSION: &str = "jqlibrary";
/// Larger files are refused unread.
pub const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;

/// What is wrong with a library or a preset's parameters (the UI words it).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LibraryError {
    TooLarge {
        max: u64,
    },
    /// Not readable as a library (JSON or structure).
    Invalid {
        error: String,
    },
    /// Some other file (its `format` field says `found`).
    WrongFormat {
        found: String,
    },
    /// Made for a newer program.
    TooNew {
        found: u32,
        current: u32,
    },
    Duplicate {
        part: Part,
        id: String,
    },
    UnknownCategory {
        preset: String,
        category: String,
    },
    Param {
        preset: String,
        param: String,
        problem: ParamProblem,
    },
    Template {
        preset: String,
        error: TemplateError,
    },
    Cue {
        preset: String,
        problem: CueProblem,
    },
    PresetNotFound {
        preset: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Preset,
    Param,
    Category,
    Device,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParamProblem {
    /// Minimum above maximum, or not finite.
    Range,
    /// A map needs two or more points with rising inputs.
    Map,
    /// The pattern is no valid regular expression.
    Pattern,
    /// A choice without values, or with one twice.
    Choices,
    /// A number where a text belongs, or the other way round.
    WrongType,
    /// A number outside its range (or not whole for an integer).
    OutOfRange {
        min: f64,
        max: f64,
    },
    Required,
    TooLong {
        max: usize,
    },
    NoMatch,
    NotAChoice,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CueProblem {
    /// Not a valid cue once filled in.
    Invalid { error: String },
    /// Sound and playlist cues need media files a library cannot carry.
    Media,
    /// Stopwatch cues need the show's stopwatches.
    Stopwatch,
    /// Hotkeys and MIDI triggers belong to the show.
    Trigger,
    /// Targets a cue outside the preset.
    OutsideTarget,
    /// An OSC cue's device is none of the library's devices.
    UnknownDevice { device: String },
}

/// Reads and validates a library file's content.
pub fn parse(bytes: &[u8]) -> Result<Library, LibraryError> {
    if bytes.len() as u64 > MAX_FILE_SIZE {
        return Err(LibraryError::TooLarge { max: MAX_FILE_SIZE });
    }
    let invalid = |e: serde_json::Error| LibraryError::Invalid { error: e.to_string() };
    let value: Value = serde_json::from_slice(bytes).map_err(invalid)?;
    let found = value.get("format").and_then(Value::as_str).unwrap_or_default();
    if found != FORMAT {
        return Err(LibraryError::WrongFormat { found: found.to_string() });
    }
    if let Some(found) = value.get("version").and_then(Value::as_u64)
        && found > VERSION as u64
    {
        return Err(LibraryError::TooNew { found: found.min(u32::MAX as u64) as u32, current: VERSION });
    }
    let library: Library = serde_json::from_value(value).map_err(invalid)?;
    validate(&library)?;
    Ok(library)
}

/// Checks ids, parameters and that every preset makes a valid cue with its
/// defaults and with each number parameter at its ends.
pub fn validate(lib: &Library) -> Result<(), LibraryError> {
    unique(Part::Category, lib.categories.iter().map(|c| c.id.as_str()))?;
    unique(Part::Device, lib.devices.iter().map(|d| d.id.as_str()))?;
    unique(Part::Preset, lib.presets.iter().map(|p| p.id.as_str()))?;
    for preset in &lib.presets {
        if let Some(category) = &preset.category
            && !lib.categories.iter().any(|c| &c.id == category)
        {
            return Err(LibraryError::UnknownCategory {
                preset: preset.id.clone(),
                category: category.clone(),
            });
        }
        unique(Part::Param, preset.params.iter().map(|p| p.id.as_str()))?;
        for param in &preset.params {
            check_param(param).map_err(|problem| LibraryError::Param {
                preset: preset.id.clone(),
                param: param.id.clone(),
                problem,
            })?;
        }
        make(lib, preset, &BTreeMap::new(), "en", true)?;
        for variant in preset.title.variants() {
            let values = values(preset, &lenient(preset, &BTreeMap::new()), "en");
            template::fill_title(variant, &values)
                .map_err(|error| LibraryError::Template { preset: preset.id.clone(), error })?;
        }
        for param in &preset.params {
            let ends = match &param.kind {
                ParamKind::Int { min, max, .. } => [*min as f64, *max as f64],
                ParamKind::Float { min, max, .. } => [*min, *max],
                _ => continue,
            };
            for end in ends {
                let given = BTreeMap::from([(param.id.clone(), ParamValue::Number(end))]);
                make(lib, preset, &given, "en", true)?;
            }
        }
    }
    Ok(())
}

fn unique<'a>(part: Part, ids: impl Iterator<Item = &'a str>) -> Result<(), LibraryError> {
    let mut seen = HashSet::new();
    for id in ids {
        if id.trim().is_empty() || !seen.insert(id) {
            return Err(LibraryError::Duplicate { part, id: id.to_string() });
        }
    }
    Ok(())
}

fn check_param(param: &Param) -> Result<(), ParamProblem> {
    let check_map = |map: &[[f64; 2]]| {
        let ok = map.is_empty()
            || (map.len() >= 2
                && map.iter().flatten().all(|v| v.is_finite())
                && map.windows(2).all(|w| w[0][0] < w[1][0]));
        if ok { Ok(()) } else { Err(ParamProblem::Map) }
    };
    match &param.kind {
        ParamKind::Int { min, max, map, .. } => {
            if min > max {
                return Err(ParamProblem::Range);
            }
            check_map(map)?;
        }
        ParamKind::Float { min, max, map, step, .. } => {
            if !(min.is_finite() && max.is_finite() && min <= max)
                || step.is_some_and(|s| s.is_nan() || s <= 0.0)
            {
                return Err(ParamProblem::Range);
            }
            check_map(map)?;
        }
        ParamKind::Text { pattern, .. } => {
            if let Some(p) = pattern {
                full_match(p).ok_or(ParamProblem::Pattern)?;
            }
        }
        ParamKind::Choice { values, .. } => {
            let mut seen = Vec::new();
            for choice in values {
                if seen.contains(&&choice.value) {
                    return Err(ParamProblem::Choices);
                }
                seen.push(&choice.value);
            }
            if values.is_empty() {
                return Err(ParamProblem::Choices);
            }
        }
    }
    check_value(&param.kind, &param.kind.default_value(), true)
}

fn full_match(pattern: &str) -> Option<Regex> {
    Regex::new(&format!("^(?:{pattern})$")).ok()
}

/// Checks a value against its parameter; `lenient` lets an empty required
/// text pass (a default the operator has to fill in).
fn check_value(kind: &ParamKind, value: &ParamValue, lenient: bool) -> Result<(), ParamProblem> {
    match (kind, value) {
        (ParamKind::Int { min, max, .. }, ParamValue::Number(n)) => {
            if n.fract() != 0.0 || *n < *min as f64 || *n > *max as f64 {
                return Err(ParamProblem::OutOfRange { min: *min as f64, max: *max as f64 });
            }
        }
        (ParamKind::Float { min, max, .. }, ParamValue::Number(n)) => {
            if !n.is_finite() || n < min || n > max {
                return Err(ParamProblem::OutOfRange { min: *min, max: *max });
            }
        }
        (ParamKind::Text { max_len, pattern, required, .. }, ParamValue::Text(t)) => {
            if t.is_empty() {
                if *required && !lenient {
                    return Err(ParamProblem::Required);
                }
                if *required {
                    return Ok(());
                }
            }
            if let Some(max) = max_len
                && t.chars().count() > *max
            {
                return Err(ParamProblem::TooLong { max: *max });
            }
            if let Some(p) = pattern
                && !full_match(p).is_some_and(|re| re.is_match(t))
            {
                return Err(ParamProblem::NoMatch);
            }
        }
        (ParamKind::Choice { values, .. }, v) => {
            if !values.iter().any(|c| &c.value == v) {
                return Err(ParamProblem::NotAChoice);
            }
        }
        _ => return Err(ParamProblem::WrongType),
    }
    Ok(())
}

/// The preset's parameters: those given (checked), the rest at their
/// defaults; values for parameters it does not have are dropped.
pub fn resolve_params(
    preset: &Preset,
    given: &BTreeMap<String, ParamValue>,
) -> Result<BTreeMap<String, ParamValue>, LibraryError> {
    resolve(preset, given, false)
}

fn resolve(
    preset: &Preset,
    given: &BTreeMap<String, ParamValue>,
    lenient: bool,
) -> Result<BTreeMap<String, ParamValue>, LibraryError> {
    preset
        .params
        .iter()
        .map(|param| {
            let value = given.get(&param.id).cloned().unwrap_or_else(|| param.kind.default_value());
            check_value(&param.kind, &value, lenient).map_err(|problem| LibraryError::Param {
                preset: preset.id.clone(),
                param: param.id.clone(),
                problem,
            })?;
            Ok((param.id.clone(), value))
        })
        .collect()
}

/// [`resolve`] for validation: given values first, defaults for the rest,
/// without checking (the checks run on the parameters themselves).
fn lenient(preset: &Preset, given: &BTreeMap<String, ParamValue>) -> BTreeMap<String, ParamValue> {
    preset
        .params
        .iter()
        .map(|p| (p.id.clone(), given.get(&p.id).cloned().unwrap_or_else(|| p.kind.default_value())))
        .collect()
}

fn values(preset: &Preset, params: &BTreeMap<String, ParamValue>, lang: &str) -> Values {
    preset
        .params
        .iter()
        .filter_map(|param| {
            let raw = params.get(&param.id)?.clone();
            let mapped = match &raw {
                ParamValue::Number(n) => ParamValue::Number(map_value(param.kind.map(), *n)),
                text => text.clone(),
            };
            let label = match &param.kind {
                ParamKind::Choice { values, .. } => {
                    values.iter().find(|c| c.value == raw).map(|c| c.label.get(lang).to_string())
                }
                _ => None,
            };
            Some((param.id.clone(), Filled { raw, mapped, label }))
        })
        .collect()
}

/// A new cue from `preset` with `params` (missing ones at their defaults),
/// its title in `lang`. An OSC cue's `device` is still the library's device
/// template id; the caller maps it to one of the show's devices.
pub fn instantiate(
    lib: &Library,
    preset: &Preset,
    params: &BTreeMap<String, ParamValue>,
    lang: &str,
) -> Result<Cue, LibraryError> {
    make(lib, preset, params, lang, false)
}

fn make(
    lib: &Library,
    preset: &Preset,
    given: &BTreeMap<String, ParamValue>,
    lang: &str,
    validating: bool,
) -> Result<Cue, LibraryError> {
    let params = if validating {
        resolve(preset, &lenient(preset, given), true)?
    } else {
        resolve(preset, given, false)?
    };
    let values = values(preset, &params, lang);
    let template_error = |error| LibraryError::Template { preset: preset.id.clone(), error };
    let cue_problem = |problem| LibraryError::Cue { preset: preset.id.clone(), problem };
    let mut json = template::fill(&preset.cue, &values).map_err(template_error)?;
    let Value::Object(map) = &mut json else {
        return Err(cue_problem(CueProblem::Invalid { error: "not an object".into() }));
    };
    for key in ["id", "number", "link", "preset"] {
        map.remove(key);
    }
    if map.get("title").and_then(Value::as_str).is_none_or(str::is_empty) {
        let title = template::fill_title(preset.title.get(lang), &values).map_err(template_error)?;
        map.insert("title".into(), Value::String(title));
    }
    add_ids(map);
    let mut cue: Cue = serde_json::from_value(json)
        .map_err(|e| cue_problem(CueProblem::Invalid { error: e.to_string() }))?;
    let own: HashSet<CueId> = std::iter::once(cue.id).chain(cue.descendants()).collect();
    check_cue(lib, &cue, &own).map_err(cue_problem)?;
    cue.renew_ids();
    cue.preset =
        Some(PresetRef { library: lib.id, preset: preset.id.clone(), revision: preset.revision, params });
    Ok(cue)
}

/// Gives a cue and the cues of a group in it ids where they have none.
fn add_ids(map: &mut Map<String, Value>) {
    if !map.contains_key("id") {
        map.insert("id".into(), serde_json::to_value(CueId::new()).expect("id serializes"));
    }
    if let Some(Value::Array(cues)) = map.get_mut("cues") {
        for cue in cues {
            if let Value::Object(inner) = cue {
                add_ids(inner);
            }
        }
    }
}

fn check_cue(lib: &Library, cue: &Cue, own: &HashSet<CueId>) -> Result<(), CueProblem> {
    if cue.hotkey.is_some() || cue.midi_trigger.is_some() {
        return Err(CueProblem::Trigger);
    }
    let inside = |id: &CueId| own.contains(id);
    if let AdvanceAt::LoopExit { of: Some(of) } = &cue.advance.at
        && !inside(of)
    {
        return Err(CueProblem::OutsideTarget);
    }
    if let Some(Targets::Cues(ids)) = cue.kind.targets()
        && !ids.iter().all(inside)
    {
        return Err(CueProblem::OutsideTarget);
    }
    match &cue.kind {
        CueKind::Sound(s) if !s.is_placeholder() => Err(CueProblem::Media),
        CueKind::Playlist(_) => Err(CueProblem::Media),
        CueKind::Stopwatch(_) => Err(CueProblem::Stopwatch),
        CueKind::GoTo(g) if g.target.is_some_and(|t| !inside(&t)) => Err(CueProblem::OutsideTarget),
        CueKind::Osc(osc) => match &osc.device {
            Some(device) if lib.device(device).is_none() => {
                Err(CueProblem::UnknownDevice { device: device.clone() })
            }
            _ => Ok(()),
        },
        CueKind::Group(g) => g.cues.iter().try_for_each(|c| check_cue(lib, c, own)),
        _ => Ok(()),
    }
}

/// The library device template ids `cue` (and a group's cues) send to.
pub fn device_templates(cue: &Cue) -> Vec<String> {
    let mut found = Vec::new();
    fn walk(cue: &Cue, found: &mut Vec<String>) {
        match &cue.kind {
            CueKind::Osc(osc) => found.extend(osc.device.clone()),
            CueKind::Group(g) => g.cues.iter().for_each(|c| walk(c, found)),
            _ => {}
        }
    }
    walk(cue, &mut found);
    found.sort();
    found.dedup();
    found
}

/// Replaces the device of each OSC cue in `cue` by `map(device)`.
pub fn map_devices(cue: &mut Cue, map: &impl Fn(&str) -> Option<String>) {
    match &mut cue.kind {
        CueKind::Osc(osc) => {
            if let Some(device) = &osc.device {
                osc.device = map(device);
            }
        }
        CueKind::Group(g) => g.cues.iter_mut().for_each(|c| map_devices(c, map)),
        _ => {}
    }
}

/// Fields that belong to a cue's place in the show, not to its preset.
const INSTANCE_FIELDS: [&str; 6] = ["id", "number", "link", "preset", "hotkey", "midi_trigger"];

/// A cue's fields as reset and update see them: its JSON (the type's
/// fields flattened in) without [`INSTANCE_FIELDS`].
fn fields(cue: &Cue) -> Map<String, Value> {
    let Ok(Value::Object(mut map)) = serde_json::to_value(cue) else {
        return Map::new();
    };
    for key in INSTANCE_FIELDS {
        map.remove(key);
    }
    map
}

/// The preset's title in each of its languages, filled in with `params`.
pub fn title_text(preset: &Preset, params: &BTreeMap<String, ParamValue>) -> Text {
    Text(
        preset
            .title
            .0
            .keys()
            .filter_map(|lang| {
                let title =
                    template::fill_title(preset.title.get(lang), &values(preset, params, lang)).ok()?;
                Some((lang.clone(), title))
            })
            .collect(),
    )
}

/// [`title_text`] as a list, for [`diff`].
pub fn titles(preset: &Preset, params: &BTreeMap<String, ParamValue>) -> Vec<String> {
    title_text(preset, params).0.into_values().collect()
}

/// The fields of `cue` that differ from `made` (the cue its preset makes
/// now). The title counts as the preset's in any of its languages.
pub fn diff(cue: &Cue, made: &Cue, titles: &[String]) -> Vec<String> {
    let (a, b) = (fields(cue), fields(made));
    let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|key| {
            if key.as_str() == "title" && titles.contains(&cue.title) {
                return false;
            }
            a.get(*key) != b.get(*key)
        })
        .cloned()
        .collect()
}

/// `cue` with the listed fields (all with `None`) taken from `made`, and
/// its preset note from it. Taking any of the type's fields from a cue of
/// another type takes the whole type.
pub fn apply(cue: &Cue, made: &Cue, only: Option<&[String]>) -> Result<Cue, LibraryError> {
    let Ok(Value::Object(mut current)) = serde_json::to_value(cue) else {
        return Ok(cue.clone());
    };
    let new = fields(made);
    let mut keys: Vec<String> = match only {
        None => current
            .keys()
            .chain(new.keys())
            .filter(|k| !INSTANCE_FIELDS.contains(&k.as_str()))
            .cloned()
            .collect(),
        Some(list) => list.iter().filter(|k| !INSTANCE_FIELDS.contains(&k.as_str())).cloned().collect(),
    };
    let kind_keys = |c: &Cue| -> Vec<String> {
        match serde_json::to_value(&c.kind) {
            Ok(Value::Object(m)) => m.keys().cloned().collect(),
            _ => Vec::new(),
        }
    };
    let (old_kind, new_kind) = (kind_keys(cue), kind_keys(made));
    if current.get("type") != new.get("type")
        && keys.iter().any(|k| old_kind.contains(k) || new_kind.contains(k))
    {
        keys.extend(old_kind.into_iter().chain(new_kind));
    }
    keys.sort();
    keys.dedup();
    for key in keys {
        match new.get(&key) {
            Some(value) => current.insert(key, value.clone()),
            None => current.remove(&key),
        };
    }
    let mut out: Cue = serde_json::from_value(Value::Object(current)).map_err(|e| LibraryError::Cue {
        preset: made.preset.as_ref().map(|p| p.preset.clone()).unwrap_or_default(),
        problem: CueProblem::Invalid { error: e.to_string() },
    })?;
    out.preset.clone_from(&made.preset);
    Ok(out)
}

/// Fields whose value depends on the parameters (for changing them on a
/// cue): those whose template uses a placeholder, and the title when the
/// preset's does.
pub fn param_fields(preset: &Preset) -> Vec<String> {
    let mut found = Vec::new();
    if let Value::Object(map) = &preset.cue {
        for (key, value) in map {
            if template::names_in(value).is_ok_and(|names| !names.is_empty()) {
                found.push(key.clone());
            }
        }
    }
    if preset.title.variants().any(|t| template::names(t).is_ok_and(|n| !n.is_empty()))
        && !found.iter().any(|k| k == "title")
    {
        found.push("title".into());
    }
    found
}

#[cfg(test)]
mod tests;
