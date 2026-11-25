//! Cue libraries (`.jqlibrary`, see `jq-library`): the bundled ones, built
//! into the program, and those the operator installed, copied into the
//! `libraries` folder in the app's config folder. What the operator chose
//! about them (disabled, favorites, recently used) is in `libraries.json`
//! there, apart from the show settings, so it works without a show open.
//!
//! Shows never depend on a library: a cue made from a preset holds all of
//! its data. Libraries are only read to insert, reset and update cues.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use jq_library::{Library, LibraryError};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::error::{AppError, Error};
use crate::writer::{Key, Writer};

/// Sent to all windows when the libraries or their preferences change; the
/// payload is a [`LibrariesView`].
pub const EVENT_LIBRARIES: &str = "libraries";

/// What the operator chose, in the libraries folder.
const PREFS_FILE: &str = "libraries.json";
/// Recently used presets kept.
const RECENT_MAX: usize = 20;

/// The libraries that come with the program.
const BUNDLED: [(&str, &[u8]); 2] = [
    ("x32.jqlibrary", include_bytes!("../libraries/x32.jqlibrary")),
    ("magicq.jqlibrary", include_bytes!("../libraries/magicq.jqlibrary")),
];

/// A preset by its library and id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetKey {
    pub library: Uuid,
    pub preset: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Libraries not offered when adding cues (they still reset and update
    /// cues made from them).
    pub disabled: Vec<Uuid>,
    /// Most recent first.
    pub recent: Vec<PresetKey>,
    pub favorites: Vec<PresetKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Bundled,
    Installed,
}

/// One library file, read or not.
#[derive(Clone, Debug)]
struct Entry {
    source: Source,
    /// The file (bundled: its name).
    path: PathBuf,
    library: Result<Arc<Library>, LibraryError>,
}

/// A library as the settings list it.
#[derive(Clone, Debug, Serialize)]
pub struct LibrarySummary {
    /// `None` for a file that cannot be read.
    pub id: Option<Uuid>,
    pub source: Source,
    /// File name.
    pub file: String,
    pub error: Option<LibraryError>,
    pub name: jq_library::Text,
    pub description: jq_library::Text,
    pub author: String,
    pub homepage: String,
    pub revision: u32,
    pub presets: usize,
    pub languages: Vec<String>,
    pub enabled: bool,
    /// The one in use among files with the same id (the newest revision,
    /// the installed one of equal ones).
    pub active: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LibrariesView {
    pub libraries: Vec<LibrarySummary>,
    pub prefs: Prefs,
    /// Where installed libraries go (`None`: no config folder).
    pub folder: Option<PathBuf>,
}

/// How a library file compares with what is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    New,
    Newer,
    Same,
    Older,
}

#[derive(Clone, Debug, Serialize)]
pub struct Inspection {
    pub library: Library,
    pub comparison: Comparison,
    /// Revision of the one in use with the same id.
    pub installed: Option<u32>,
}

#[derive(Default)]
struct Store {
    entries: Vec<Entry>,
    prefs: Prefs,
}

pub struct Libraries {
    /// Installed libraries and the preferences; `None` = none can be installed.
    dir: Option<PathBuf>,
    store: RwLock<Store>,
    writer: Option<Writer>,
    app: Option<AppHandle>,
}

impl Libraries {
    /// Reads the bundled libraries and those installed in `dir`.
    pub fn load(dir: Option<PathBuf>, writer: Option<Writer>, app: Option<AppHandle>) -> Self {
        let libraries = Self { dir, store: RwLock::default(), writer, app };
        let prefs = libraries.dir.as_ref().and_then(|d| read_prefs(&d.join(PREFS_FILE))).unwrap_or_default();
        libraries.rescan_with(prefs);
        libraries
    }

    /// Only the bundled libraries, nothing saved (tests).
    #[cfg(test)]
    pub fn bundled() -> Self {
        Self::load(None, None, None)
    }

    /// No libraries at all, not even the bundled ones (tests: a machine
    /// without the library a show's cues came from).
    #[cfg(test)]
    pub fn none() -> Self {
        Self { dir: None, store: RwLock::default(), writer: None, app: None }
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Store> {
        self.store.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Store> {
        self.store.write().unwrap_or_else(|e| e.into_inner())
    }

    fn rescan_with(&self, prefs: Prefs) {
        let mut entries: Vec<Entry> = BUNDLED
            .iter()
            .map(|(name, bytes)| Entry {
                source: Source::Bundled,
                path: PathBuf::from(name),
                library: jq_library::parse(bytes).map(Arc::new),
            })
            .collect();
        if let Some(dir) = &self.dir
            && let Ok(list) = std::fs::read_dir(dir)
        {
            let mut files: Vec<PathBuf> = list
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == jq_library::EXTENSION))
                .collect();
            files.sort();
            entries.extend(files.into_iter().map(|path| Entry {
                source: Source::Installed,
                library: read_file(&path).map(Arc::new),
                path,
            }));
        }
        let mut store = self.write();
        store.entries = entries;
        store.prefs = prefs;
    }

    /// Reads the folder again (a file was copied in by hand).
    pub fn rescan(&self) {
        let prefs = self.read().prefs.clone();
        self.rescan_with(prefs);
        self.changed();
    }

    /// The library in use with this id, enabled or not.
    pub fn library(&self, id: Uuid) -> Option<Arc<Library>> {
        let store = self.read();
        active(&store.entries, id).and_then(|e| e.library.clone().ok())
    }

    pub fn view(&self) -> LibrariesView {
        let store = self.read();
        let libraries = store
            .entries
            .iter()
            .map(|entry| {
                let file =
                    entry.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                match &entry.library {
                    Ok(lib) => LibrarySummary {
                        id: Some(lib.id),
                        source: entry.source,
                        file,
                        error: None,
                        name: lib.name.clone(),
                        description: lib.description.clone(),
                        author: lib.author.clone(),
                        homepage: lib.homepage.clone(),
                        revision: lib.revision,
                        presets: lib.presets.len(),
                        languages: lib.languages(),
                        enabled: !store.prefs.disabled.contains(&lib.id),
                        active: active(&store.entries, lib.id).is_some_and(|a| std::ptr::eq(a, entry)),
                    },
                    Err(error) => LibrarySummary {
                        id: None,
                        source: entry.source,
                        file,
                        error: Some(error.clone()),
                        name: Default::default(),
                        description: Default::default(),
                        author: String::new(),
                        homepage: String::new(),
                        revision: 0,
                        presets: 0,
                        languages: Vec::new(),
                        enabled: false,
                        active: false,
                    },
                }
            })
            .collect();
        LibrariesView { libraries, prefs: store.prefs.clone(), folder: self.dir.clone() }
    }

    /// Reads a library file and compares it with what is installed.
    pub fn inspect(&self, path: &Path) -> Result<Inspection, Error> {
        let library = read_file(path).map_err(|error| file_error(path, error))?;
        let installed = self.library(library.id).map(|l| l.revision);
        let comparison = match installed {
            None => Comparison::New,
            Some(r) if library.revision > r => Comparison::Newer,
            Some(r) if library.revision == r => Comparison::Same,
            Some(_) => Comparison::Older,
        };
        Ok(Inspection { library, comparison, installed })
    }

    /// Copies a library file into the folder as `<id>.jqlibrary`, replacing
    /// an installed one with the same id.
    pub fn install(&self, path: &Path) -> Result<(), Error> {
        let dir = self.dir.as_ref().ok_or(AppError::NoLibraryFolder)?;
        let bytes = read_bytes(path).map_err(|error| file_error(path, error))?;
        let library = jq_library::parse(&bytes).map_err(|error| file_error(path, error))?;
        let io =
            |e: std::io::Error| AppError::LibraryIo { path: dir.display().to_string(), error: e.to_string() };
        std::fs::create_dir_all(dir).map_err(io)?;
        let target = dir.join(format!("{}.{}", library.id, jq_library::EXTENSION));
        // Other files with this id (copied in by hand) would compete with it.
        for entry in self.read().entries.iter() {
            if entry.source == Source::Installed
                && entry.path != target
                && entry.library.as_ref().is_ok_and(|l| l.id == library.id)
            {
                std::fs::remove_file(&entry.path).map_err(io)?;
            }
        }
        jq_show::write_atomic(&target, &bytes).map_err(io)?;
        let mut prefs = self.read().prefs.clone();
        prefs.disabled.retain(|id| *id != library.id);
        self.rescan_with(prefs);
        self.save_prefs();
        self.changed();
        Ok(())
    }

    /// Removes the installed files of library `id`; a bundled one cannot be.
    pub fn remove(&self, id: Uuid) -> Result<(), Error> {
        let files: Vec<PathBuf> = self
            .read()
            .entries
            .iter()
            .filter(|e| e.source == Source::Installed && e.library.as_ref().is_ok_and(|l| l.id == id))
            .map(|e| e.path.clone())
            .collect();
        if files.is_empty() {
            return Err(if self.library(id).is_some() {
                AppError::LibraryBuiltIn
            } else {
                AppError::LibraryNotFound
            }
            .into());
        }
        for file in files {
            std::fs::remove_file(&file).map_err(|e| AppError::LibraryIo {
                path: file.display().to_string(),
                error: e.to_string(),
            })?;
        }
        self.rescan();
        Ok(())
    }

    /// Removes an installed file that cannot be read.
    pub fn remove_file(&self, file: &str) -> Result<(), Error> {
        let path = self
            .read()
            .entries
            .iter()
            .find(|e| e.source == Source::Installed && e.path.file_name().is_some_and(|n| n == file))
            .map(|e| e.path.clone())
            .ok_or(AppError::LibraryNotFound)?;
        std::fs::remove_file(&path)
            .map_err(|e| AppError::LibraryIo { path: path.display().to_string(), error: e.to_string() })?;
        self.rescan();
        Ok(())
    }

    pub fn set_enabled(&self, id: Uuid, enabled: bool) {
        self.change_prefs(|prefs| {
            prefs.disabled.retain(|d| *d != id);
            if !enabled {
                prefs.disabled.push(id);
            }
        });
    }

    pub fn set_favorite(&self, key: PresetKey, favorite: bool) {
        self.change_prefs(|prefs| {
            prefs.favorites.retain(|k| *k != key);
            if favorite {
                prefs.favorites.push(key);
            }
        });
    }

    /// Puts a preset first among the recently used.
    pub fn used(&self, key: PresetKey) {
        self.change_prefs(|prefs| {
            prefs.recent.retain(|k| *k != key);
            prefs.recent.insert(0, key);
            prefs.recent.truncate(RECENT_MAX);
        });
    }

    fn change_prefs(&self, change: impl FnOnce(&mut Prefs)) {
        change(&mut self.write().prefs);
        self.save_prefs();
        self.changed();
    }

    fn save_prefs(&self) {
        let (Some(dir), Some(writer)) = (&self.dir, &self.writer) else {
            return;
        };
        let path = dir.join(PREFS_FILE);
        let prefs = self.read().prefs.clone();
        let key = Key::new(path.clone(), "libraries");
        writer.submit(Some(key), move || {
            let text = serde_json::to_vec_pretty(&prefs).expect("prefs serialize");
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            jq_show::write_atomic(&path, &text).map_err(|e| jq_core::Notice::SettingsNotSaved {
                error: format!("{}: {e}", path.display()).into(),
            })
        });
    }

    fn changed(&self) {
        if let Some(app) = &self.app {
            let _ = app.emit(EVENT_LIBRARIES, self.view());
        }
    }
}

/// Among the readable files with library `id`: the newest revision, the
/// installed one of equal ones.
fn active(entries: &[Entry], id: Uuid) -> Option<&Entry> {
    entries
        .iter()
        .filter(|e| e.library.as_ref().is_ok_and(|l| l.id == id))
        .max_by_key(|e| (e.library.as_ref().map_or(0, |l| l.revision), e.source == Source::Installed))
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, LibraryError> {
    let size = std::fs::metadata(path).map_err(|e| LibraryError::Invalid { error: e.to_string() })?.len();
    if size > jq_library::MAX_FILE_SIZE {
        return Err(LibraryError::TooLarge { max: jq_library::MAX_FILE_SIZE });
    }
    std::fs::read(path).map_err(|e| LibraryError::Invalid { error: e.to_string() })
}

fn read_file(path: &Path) -> Result<Library, LibraryError> {
    jq_library::parse(&read_bytes(path)?)
}

fn read_prefs(path: &Path) -> Option<Prefs> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn file_error(path: &Path, error: LibraryError) -> Error {
    AppError::LibraryFile { path: path.display().to_string(), error }.into()
}

pub type Shared = Arc<Libraries>;

#[tauri::command]
pub fn libraries(libraries: State<'_, Shared>) -> LibrariesView {
    libraries.view()
}

/// A library with all its texts (the UI picks the language).
#[tauri::command]
pub fn library(libraries: State<'_, Shared>, id: Uuid) -> Result<Library, Error> {
    libraries.library(id).map(|l| (*l).clone()).ok_or_else(|| AppError::LibraryNotFound.into())
}

/// Several libraries at once, keyed by id; unknown ids are left out.
#[tauri::command]
pub fn library_presets(libraries: State<'_, Shared>, ids: Vec<Uuid>) -> BTreeMap<Uuid, Library> {
    ids.into_iter().filter_map(|id| Some((id, (*libraries.library(id)?).clone()))).collect()
}

#[tauri::command]
pub async fn inspect_library(libraries: State<'_, Shared>, path: PathBuf) -> Result<Inspection, Error> {
    let libraries = libraries.inner().clone();
    tauri::async_runtime::spawn_blocking(move || libraries.inspect(&path))
        .await
        .map_err(|e| Error::from(e.to_string()))?
}

#[tauri::command]
pub async fn install_library(libraries: State<'_, Shared>, path: PathBuf) -> Result<LibrariesView, Error> {
    let libraries = libraries.inner().clone();
    tauri::async_runtime::spawn_blocking(move || libraries.install(&path).map(|()| libraries.view()))
        .await
        .map_err(|e| Error::from(e.to_string()))?
}

#[tauri::command]
pub fn remove_library(libraries: State<'_, Shared>, id: Uuid) -> Result<LibrariesView, Error> {
    libraries.remove(id).map(|()| libraries.view())
}

#[tauri::command]
pub fn remove_library_file(libraries: State<'_, Shared>, file: String) -> Result<LibrariesView, Error> {
    libraries.remove_file(&file).map(|()| libraries.view())
}

#[tauri::command]
pub fn rescan_libraries(libraries: State<'_, Shared>) -> LibrariesView {
    libraries.rescan();
    libraries.view()
}

#[tauri::command]
pub fn set_library_enabled(libraries: State<'_, Shared>, id: Uuid, enabled: bool) -> LibrariesView {
    libraries.set_enabled(id, enabled);
    libraries.view()
}

#[tauri::command]
pub fn set_preset_favorite(
    libraries: State<'_, Shared>,
    library: Uuid,
    preset: String,
    favorite: bool,
) -> LibrariesView {
    libraries.set_favorite(PresetKey { library, preset }, favorite);
    libraries.view()
}

#[tauri::command]
pub async fn open_library_folder(libraries: State<'_, Shared>) -> Result<(), Error> {
    let dir = libraries.dir.clone().ok_or(AppError::NoLibraryFolder)?;
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)
            .map_err(|e| AppError::LibraryIo { path: dir.display().to_string(), error: e.to_string() })?;
        crate::media::open_folder(&dir)
    })
    .await
    .map_err(|e| Error::from(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jq-libraries-{name}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(dir: &Path, name: &str, revision: u32, preset_title: &str) -> PathBuf {
        let mut lib: serde_json::Value = serde_json::from_slice(BUNDLED[0].1).unwrap();
        lib["revision"] = revision.into();
        lib["presets"][0]["title"]["en"] = preset_title.into();
        let path = dir.join(name);
        std::fs::write(&path, serde_json::to_vec(&lib).unwrap()).unwrap();
        path
    }

    #[test]
    fn bundled_libraries_load() {
        let libraries = Libraries::bundled();
        let view = libraries.view();
        assert_eq!(view.libraries.len(), BUNDLED.len());
        assert!(view.libraries.iter().all(|l| l.error.is_none() && l.active && l.enabled));
    }

    #[test]
    fn the_newest_revision_is_used_and_installing_replaces() {
        let (dir, downloads) = (tempdir("dir"), tempdir("downloads"));
        let libraries = Libraries::load(Some(dir.clone()), None, None);
        let x32 = libraries.view().libraries[0].id.unwrap();
        let newer = file(&downloads, "x32-new.jqlibrary", 2, "Newer");
        let inspected = libraries.inspect(&newer).unwrap();
        assert_eq!((inspected.comparison, inspected.installed), (Comparison::Newer, Some(1)));
        libraries.install(&newer).unwrap();
        assert_eq!(libraries.library(x32).unwrap().presets[0].title.get("en"), "Newer");
        assert!(dir.join(format!("{x32}.jqlibrary")).is_file());
        // An older installed file loses against the bundled one.
        let older = file(&downloads, "x32-old.jqlibrary", 0, "Older");
        assert_eq!(libraries.inspect(&older).unwrap().comparison, Comparison::Older);
        libraries.install(&older).unwrap();
        assert_eq!(libraries.library(x32).unwrap().revision, 1);
        let view = libraries.view();
        assert_eq!(view.libraries.iter().filter(|l| l.id == Some(x32)).count(), 2);
        assert_eq!(view.libraries.iter().filter(|l| l.id == Some(x32) && l.active).count(), 1);
        // Removing it leaves the bundled one.
        libraries.remove(x32).unwrap();
        assert_eq!(libraries.view().libraries.len(), BUNDLED.len());
        assert!(matches!(libraries.remove(x32), Err(Error::App(AppError::LibraryBuiltIn))));
    }

    #[test]
    fn broken_files_are_listed_and_refused() {
        let dir = tempdir("broken");
        std::fs::write(dir.join("bad.jqlibrary"), b"{ nope").unwrap();
        let libraries = Libraries::load(Some(dir.clone()), None, None);
        let view = libraries.view();
        let bad = view.libraries.iter().find(|l| l.file == "bad.jqlibrary").unwrap();
        assert!(matches!(bad.error, Some(LibraryError::Invalid { .. })));
        assert!(matches!(
            libraries.inspect(&dir.join("bad.jqlibrary")),
            Err(Error::App(AppError::LibraryFile { .. }))
        ));
        libraries.remove_file("bad.jqlibrary").unwrap();
        assert_eq!(libraries.view().libraries.len(), BUNDLED.len());
    }

    #[test]
    fn prefs_keep_recent_presets_first() {
        let libraries = Libraries::bundled();
        let key = |p: &str| PresetKey { library: Uuid::nil(), preset: p.into() };
        for p in ["a", "b", "a"] {
            libraries.used(key(p));
        }
        assert_eq!(libraries.view().prefs.recent, vec![key("a"), key("b")]);
        libraries.set_favorite(key("b"), true);
        libraries.set_enabled(Uuid::nil(), false);
        let prefs = libraries.view().prefs;
        assert_eq!((prefs.favorites, prefs.disabled), (vec![key("b")], vec![Uuid::nil()]));
    }
}
