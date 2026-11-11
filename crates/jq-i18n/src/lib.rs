//! Localised texts of the backend (errors, notices, names of actions, …).
//!
//! Texts live in `locales/<code>.txt`, one `key = text` per line (`#` starts a comment, `\n` in a text is a line
//! break). `{name}` in a text is replaced by the argument of that name. A text with a plural form has the keys
//! `key.one` and `key.other` and is chosen by the argument `count`. English is the fallback for a missing
//! translation (and for a language the program does not offer); a key missing everywhere shows as the key itself.
//!
//! The language is global ([`set_language`]): the backend produces its messages in the language chosen in the
//! settings at the time they arise. The UI has its own catalog for its texts (`app/src/lib/i18n`).
//!
//! Never call this on the audio thread (it allocates).

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

/// A language the program is available in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    De,
}

impl Lang {
    /// All languages, the fallback (English) first.
    pub const ALL: [Lang; 2] = [Lang::En, Lang::De];
    /// The fallback for everything not translated.
    pub const FALLBACK: Lang = Lang::En;

    /// Language code as stored in the settings and used in the UI ("en", "de").
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
        }
    }

    /// The language for a code like "de", "de-DE" or "de_AT.UTF-8"; `None` if it is not offered.
    pub fn from_code(code: &str) -> Option<Lang> {
        let primary = code.split(['-', '_', '.', '@']).next()?.to_ascii_lowercase();
        Lang::ALL.into_iter().find(|lang| lang.code() == primary)
    }

    fn index(self) -> u8 {
        self as u8
    }

    fn from_index(index: u8) -> Lang {
        Lang::ALL.into_iter().find(|lang| lang.index() == index).unwrap_or(Lang::FALLBACK)
    }

    fn source(self) -> &'static str {
        match self {
            Lang::En => include_str!("../locales/en.txt"),
            Lang::De => include_str!("../locales/de.txt"),
        }
    }
}

/// The setting value for "follow the system language".
pub const AUTO: &str = "auto";

/// The first of the system's preferred languages that is offered, else English.
pub fn system_language() -> Lang {
    system_language_of(sys_locale::get_locales())
}

fn system_language_of(preferred: impl IntoIterator<Item = String>) -> Lang {
    preferred.into_iter().find_map(|code| Lang::from_code(&code)).unwrap_or(Lang::FALLBACK)
}

/// The language a setting stands for: [`AUTO`] = the system language, a code = that language; anything else
/// (e.g. a language removed in a later version) is treated like [`AUTO`].
pub fn resolve(setting: &str) -> Lang {
    Lang::from_code(setting).unwrap_or_else(system_language)
}

static CURRENT: AtomicU8 = AtomicU8::new(Lang::FALLBACK as u8);

/// The language texts are produced in right now.
pub fn language() -> Lang {
    Lang::from_index(CURRENT.load(Ordering::Relaxed))
}

pub fn set_language(lang: Lang) {
    CURRENT.store(lang.index(), Ordering::Relaxed);
}

type Catalog = HashMap<String, String>;

fn parse(source: &str) -> Catalog {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, text) = line.split_once(" = ")?;
            Some((key.trim().to_owned(), text.replace("\\n", "\n")))
        })
        .collect()
}

fn catalog(lang: Lang) -> &'static Catalog {
    static CATALOGS: OnceLock<Vec<Catalog>> = OnceLock::new();
    &CATALOGS.get_or_init(|| Lang::ALL.iter().map(|lang| parse(lang.source())).collect())
        [lang.index() as usize]
}

fn lookup(lang: Lang, key: &str) -> Option<&'static str> {
    catalog(lang).get(key).or_else(|| catalog(Lang::FALLBACK).get(key)).map(String::as_str)
}

/// The text of `key` in `lang` with the arguments filled in. See the module documentation.
pub fn translate_in(lang: Lang, key: &str, args: &[(&str, &dyn Display)]) -> String {
    let args: Vec<(&str, String)> = args.iter().map(|(name, value)| (*name, value.to_string())).collect();
    // English and German alike: the singular form for exactly 1.
    let plural = args.iter().find(|(name, _)| *name == "count").map(|(_, count)| {
        let form = if count == "1" { "one" } else { "other" };
        format!("{key}.{form}")
    });
    let template = plural.as_deref().and_then(|plural| lookup(lang, plural)).or_else(|| lookup(lang, key));
    let Some(template) = template else { return key.to_owned() };

    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push('{');
            rest = after;
            continue;
        };
        let name = &after[..close];
        match args.iter().find(|(arg, _)| *arg == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// The text of `key` in the current language.
pub fn translate(key: &str, args: &[(&str, &dyn Display)]) -> String {
    translate_in(language(), key, args)
}

/// `t!("key")` or `t!("key", name = value, …)`: the localised text for `key` in the current language. Use
/// `count = n` for a text with `.one`/`.other` forms.
#[macro_export]
macro_rules! t {
    ($key:literal $(,)?) => {
        $crate::translate($key, &[])
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::translate($key, &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+])
    };
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn codes() {
        assert_eq!(Lang::from_code("de"), Some(Lang::De));
        assert_eq!(Lang::from_code("de-AT"), Some(Lang::De));
        assert_eq!(Lang::from_code("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(Lang::from_code("fr-FR"), None);
        assert_eq!(Lang::from_code(""), None);
    }

    #[test]
    fn system_language_falls_back_to_english() {
        assert_eq!(system_language_of(["fr-FR".to_owned(), "de-DE".to_owned()]), Lang::De);
        assert_eq!(system_language_of(["fr-FR".to_owned()]), Lang::En);
        assert_eq!(system_language_of([]), Lang::En);
    }

    #[test]
    fn explicit_setting_wins_over_the_system() {
        assert_eq!(resolve("de"), Lang::De);
        assert_eq!(resolve("en"), Lang::En);
    }

    #[test]
    fn arguments_and_plurals() {
        assert_eq!(translate_in(Lang::En, "i18n.test", &[("name", &"x")]), "Hello x");
        assert_eq!(translate_in(Lang::De, "i18n.test", &[("name", &"x")]), "Hallo x");
        assert_eq!(translate_in(Lang::En, "i18n.files", &[("count", &1)]), "1 file");
        assert_eq!(translate_in(Lang::En, "i18n.files", &[("count", &3)]), "3 files");
        assert_eq!(translate_in(Lang::De, "i18n.files", &[("count", &1)]), "1 Datei");
        assert_eq!(translate_in(Lang::En, "no.such.key", &[]), "no.such.key");
        assert_eq!(translate_in(Lang::En, "i18n.test", &[]), "Hello {name}");
    }

    #[test]
    fn both_catalogs_have_the_same_keys_and_placeholders() {
        let (en, de) = (catalog(Lang::En), catalog(Lang::De));
        let placeholders = |text: &str| {
            let mut names: Vec<String> = text
                .split('{')
                .skip(1)
                .filter_map(|part| part.split('}').next())
                .map(str::to_owned)
                .collect();
            names.sort();
            names
        };
        for (key, text) in en {
            let other = de.get(key).unwrap_or_else(|| panic!("`{key}` is missing in de.txt"));
            assert_eq!(placeholders(text), placeholders(other), "placeholders of `{key}` differ");
        }
        for key in de.keys() {
            assert!(en.contains_key(key), "`{key}` is missing in en.txt");
        }
    }

    #[test]
    fn every_key_used_in_the_sources_exists() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = Vec::new();
        for dir in ["crates", "app/src-tauri/src"] {
            collect(&root.join(dir), &mut files);
        }
        let en = catalog(Lang::En);
        let mut used = 0;
        for file in files.iter().filter(|file| !file.ends_with("jq-i18n/src/lib.rs")) {
            let source = std::fs::read_to_string(file).unwrap();
            for (at, _) in source.match_indices("t!(\"") {
                // Not the end of another macro's name (`format!("…`).
                if source[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let key = source[at + 4..].split('"').next().unwrap();
                used += 1;
                let known = en.contains_key(key) || en.contains_key(&format!("{key}.other"));
                assert!(known, "`{key}` ({}) is missing in en.txt", file.display());
            }
        }
        assert!(used > 0);
    }

    fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                collect(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
}
