//! Placeholders in a preset's texts and cue: `{p}` (the parameter's value,
//! after its map), `{p:02}` (zero padded to two digits), `{p+1}`/`{p-1}`,
//! `{p:label}` (a choice's label) and `{p:raw}` (the value before the map);
//! names are letters, digits and `_`. `{{` and `}}` are literal braces. A JSON string that is exactly one
//! placeholder without a format becomes a number where the value is one.

use std::collections::BTreeMap;

use jq_show::ParamValue;
use serde::Serialize;
use serde_json::Value;

/// A parameter's value as the placeholders use it.
#[derive(Clone, Debug, PartialEq)]
pub struct Filled {
    /// As entered.
    pub raw: ParamValue,
    /// After the parameter's map.
    pub mapped: ParamValue,
    /// A choice's label in the language of the insert.
    pub label: Option<String>,
}

pub type Values = BTreeMap<String, Filled>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemplateError {
    /// A `{` without its `}`, or a format that does not exist.
    Syntax {
        text: String,
    },
    UnknownParam {
        param: String,
    },
    /// `:label` on a parameter that is not a choice.
    NoLabel {
        param: String,
    },
    /// `+`/`-` or zero padding on a text.
    NotANumber {
        param: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum Spec {
    Plain,
    Pad(usize),
    Label,
    Raw,
}

#[derive(Clone, Debug, PartialEq)]
struct Placeholder {
    name: String,
    offset: i64,
    spec: Spec,
}

#[derive(Clone, Debug, PartialEq)]
enum Piece {
    Text(String),
    Value(Placeholder),
}

fn parse(text: &str) -> Result<Vec<Piece>, TemplateError> {
    let syntax = || TemplateError::Syntax { text: text.to_string() };
    let mut pieces = Vec::new();
    let mut lit = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                lit.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                lit.push('}');
            }
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => inner.push(c),
                        None => return Err(syntax()),
                    }
                }
                if !lit.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut lit)));
                }
                pieces.push(Piece::Value(placeholder(&inner).ok_or_else(syntax)?));
            }
            '}' => return Err(syntax()),
            c => lit.push(c),
        }
    }
    if !lit.is_empty() {
        pieces.push(Piece::Text(lit));
    }
    Ok(pieces)
}

fn placeholder(inner: &str) -> Option<Placeholder> {
    let (head, spec) = match inner.split_once(':') {
        None => (inner, Spec::Plain),
        Some((head, "label")) => (head, Spec::Label),
        Some((head, "raw")) => (head, Spec::Raw),
        Some((head, pad))
            if pad.len() >= 2 && pad.starts_with('0') && pad.bytes().all(|b| b.is_ascii_digit()) =>
        {
            (head, Spec::Pad(pad[1..].parse().ok()?))
        }
        Some(_) => return None,
    };
    let (name, offset) = match head.find(['+', '-']) {
        None => (head, 0),
        Some(at) => {
            let n: i64 = head[at + 1..].parse().ok()?;
            (&head[..at], if head.as_bytes()[at] == b'-' { -n } else { n })
        }
    };
    let valid = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| Placeholder { name: name.to_string(), offset, spec })
}

/// Names of the parameters `text` uses.
pub fn names(text: &str) -> Result<Vec<String>, TemplateError> {
    Ok(parse(text)?
        .into_iter()
        .filter_map(|p| match p {
            Piece::Value(ph) => Some(ph.name),
            Piece::Text(_) => None,
        })
        .collect())
}

/// Names of the parameters used anywhere in `value`'s strings.
pub fn names_in(value: &Value) -> Result<Vec<String>, TemplateError> {
    let mut found = Vec::new();
    walk_strings(value, &mut |s| {
        found.extend(names(s)?);
        Ok(())
    })?;
    Ok(found)
}

fn walk_strings(
    value: &Value,
    f: &mut impl FnMut(&str) -> Result<(), TemplateError>,
) -> Result<(), TemplateError> {
    match value {
        Value::String(s) => f(s),
        Value::Array(items) => items.iter().try_for_each(|v| walk_strings(v, f)),
        Value::Object(map) => map.values().try_for_each(|v| walk_strings(v, f)),
        _ => Ok(()),
    }
}

/// A number as text: whole numbers without a fraction, others with at most
/// six decimals.
pub fn number_text(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{n:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// A number as JSON: whole numbers as integers (so they read as `i32`/`u8`).
fn number_json(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 1e15 { Value::from(n as i64) } else { Value::from(n) }
}

fn resolve(ph: &Placeholder, values: &Values) -> Result<ParamValue, TemplateError> {
    let filled =
        values.get(&ph.name).ok_or_else(|| TemplateError::UnknownParam { param: ph.name.clone() })?;
    let value = match ph.spec {
        Spec::Label => {
            let label =
                filled.label.clone().ok_or_else(|| TemplateError::NoLabel { param: ph.name.clone() })?;
            return Ok(ParamValue::Text(label));
        }
        Spec::Raw => &filled.raw,
        Spec::Plain | Spec::Pad(_) => &filled.mapped,
    };
    match (value, ph.offset, &ph.spec) {
        (ParamValue::Number(n), offset, _) => Ok(ParamValue::Number(n + offset as f64)),
        (ParamValue::Text(_), 0, Spec::Plain | Spec::Raw) => Ok(value.clone()),
        (ParamValue::Text(_), _, _) => Err(TemplateError::NotANumber { param: ph.name.clone() }),
    }
}

fn piece_text(ph: &Placeholder, values: &Values) -> Result<String, TemplateError> {
    Ok(match (resolve(ph, values)?, &ph.spec) {
        (ParamValue::Number(n), Spec::Pad(width)) => {
            let n = n.round() as i64;
            if n < 0 { format!("-{:0width$}", -n) } else { format!("{n:0width$}") }
        }
        (ParamValue::Number(n), _) => number_text(n),
        (ParamValue::Text(t), _) => t,
    })
}

/// `text` with its placeholders filled in.
pub fn fill_text(text: &str, values: &Values) -> Result<String, TemplateError> {
    parse(text)?
        .iter()
        .map(|piece| match piece {
            Piece::Text(t) => Ok(t.clone()),
            Piece::Value(ph) => piece_text(ph, values),
        })
        .collect()
}

/// A title with its placeholders filled in, runs of spaces left by empty
/// values collapsed.
pub fn fill_title(text: &str, values: &Values) -> Result<String, TemplateError> {
    Ok(fill_text(text, values)?.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// `value` with the placeholders in its strings filled in.
pub fn fill(value: &Value, values: &Values) -> Result<Value, TemplateError> {
    Ok(match value {
        Value::String(s) => {
            let pieces = parse(s)?;
            match pieces.as_slice() {
                [Piece::Value(ph)] if matches!(ph.spec, Spec::Plain | Spec::Raw) => {
                    match resolve(ph, values)? {
                        ParamValue::Number(n) => number_json(n),
                        ParamValue::Text(t) => Value::String(t),
                    }
                }
                _ => Value::String(fill_text(s, values)?),
            }
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| fill(v, values)).collect::<Result<_, _>>()?),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), fill(v, values)?)))
                .collect::<Result<_, TemplateError>>()?,
        ),
        other => other.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> Values {
        let num = |raw: f64, mapped: f64| Filled {
            raw: ParamValue::Number(raw),
            mapped: ParamValue::Number(mapped),
            label: None,
        };
        BTreeMap::from([
            ("ch".into(), num(5.0, 5.0)),
            ("level".into(), num(0.0, 0.75)),
            (
                "state".into(),
                Filled {
                    raw: ParamValue::Number(0.0),
                    mapped: ParamValue::Number(0.0),
                    label: Some("Mute".into()),
                },
            ),
            (
                "name".into(),
                Filled {
                    raw: ParamValue::Text("Vox".into()),
                    mapped: ParamValue::Text("Vox".into()),
                    label: None,
                },
            ),
            (
                "cue".into(),
                Filled {
                    raw: ParamValue::Text(String::new()),
                    mapped: ParamValue::Text(String::new()),
                    label: None,
                },
            ),
        ])
    }

    #[test]
    fn fills_texts() {
        let v = values();
        assert_eq!(fill_text("/ch/{ch:02}/mix/on", &v).unwrap(), "/ch/05/mix/on");
        assert_eq!(fill_text("{ch+1}|{ch-1}|{ch:03}", &v).unwrap(), "6|4|005");
        assert_eq!(fill_text("Ch {ch}: {state:label}", &v).unwrap(), "Ch 5: Mute");
        assert_eq!(fill_text("{level} / {level:raw} dB", &v).unwrap(), "0.75 / 0 dB");
        assert_eq!(fill_text("{{literal}} {name}", &v).unwrap(), "{literal} Vox");
        assert_eq!(fill_title("MSC GO {cue}", &v).unwrap(), "MSC GO");
    }

    #[test]
    fn whole_placeholders_keep_their_type() {
        let v = values();
        let template = serde_json::json!({ "a": "{ch}", "b": "{level}", "c": "{name}", "d": "{ch:02}", "e": ["{state}"] });
        let filled = fill(&template, &v).unwrap();
        assert_eq!(filled, serde_json::json!({ "a": 5, "b": 0.75, "c": "Vox", "d": "05", "e": [0] }));
        assert!(filled["a"].is_i64());
    }

    #[test]
    fn rejects_bad_placeholders() {
        let v = values();
        assert!(matches!(fill_text("{ch", &v), Err(TemplateError::Syntax { .. })));
        assert!(matches!(fill_text("a}b", &v), Err(TemplateError::Syntax { .. })));
        assert!(matches!(fill_text("{ch:x}", &v), Err(TemplateError::Syntax { .. })));
        assert!(matches!(fill_text("{nope}", &v), Err(TemplateError::UnknownParam { .. })));
        assert!(matches!(fill_text("{ch:label}", &v), Err(TemplateError::NoLabel { .. })));
        assert!(matches!(fill_text("{name+1}", &v), Err(TemplateError::NotANumber { .. })));
        assert_eq!(names("/x/{ch:02}/{state:label}").unwrap(), vec!["ch", "state"]);
    }

    #[test]
    fn writes_numbers_plainly() {
        assert_eq!(number_text(3.0), "3");
        assert_eq!(number_text(-10.5), "-10.5");
        assert_eq!(number_text(0.1 + 0.2), "0.3");
    }
}
