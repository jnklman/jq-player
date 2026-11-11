//! Portable media paths: stored relative to the media folder with `/`
//! separators, translated to native paths on load.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use jq_i18n::t;

use crate::model::{Cue, CueId, CueKind, Show};

/// Joins a stored `/`-separated path onto `base`. Absolute paths (from shows
/// built by hand) are taken as they are.
pub fn resolve(base: &Path, stored: &str) -> PathBuf {
    let p = Path::new(stored);
    if p.is_absolute() || stored.starts_with('/') {
        return p.to_path_buf();
    }
    stored
        .split(['/', '\\'])
        .filter(|s| !s.is_empty() && *s != ".")
        .fold(base.to_path_buf(), |acc, s| acc.join(s))
}

/// Turns `file` into a stored path relative to `media_dir`. Returns `None`
/// when the file lies outside, so the caller can offer to copy it in.
pub fn relativize(media_dir: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(media_dir).ok()?;
    let parts: Option<Vec<&str>> = rel
        .components()
        .map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    parts.filter(|p| !p.is_empty()).map(|p| p.join("/"))
}

/// The first name from `name` on (`donner.wav`, `donner-2.wav`, …) that no
/// file in `media_dir` has and `taken` does not claim (copies under way).
pub fn free_name(media_dir: &Path, name: &str, taken: impl Fn(&str) -> bool) -> String {
    let path = Path::new(name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut n = 1;
    loop {
        let candidate = if n == 1 { name.to_owned() } else { format!("{stem}-{n}{ext}") };
        if !taken(&candidate) && !media_dir.join(&candidate).exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Copies `from` to `to` without ever leaving a partial `to`: the data goes
/// to a hidden file next to it, renamed once complete. `progress` gets the
/// bytes copied so far and stops the copy by returning false. Never
/// overwrites: fails if `to` appeared meanwhile.
pub fn copy_file(from: &Path, to: &Path, mut progress: impl FnMut(u64) -> bool) -> io::Result<()> {
    let name = to.file_name().ok_or_else(|| io::Error::other(t!("paths.no_file_name")))?;
    let mut part = OsString::from(".");
    part.push(name);
    part.push(".part");
    let part = to.with_file_name(part);
    let result = (|| {
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut source = File::open(from)?;
        let mut out = File::create(&part)?;
        let mut buf = vec![0; 1 << 20];
        let mut done = 0;
        loop {
            let len = match source.read(&mut buf) {
                Ok(0) => break,
                Ok(len) => len,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            out.write_all(&buf[..len])?;
            done += len as u64;
            if !progress(done) {
                return Err(io::Error::new(io::ErrorKind::Interrupted, t!("paths.cancelled")));
            }
        }
        out.sync_all()?;
        drop(out);
        if to.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                t!("paths.exists", path = to.display()),
            ));
        }
        fs::rename(&part, to)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

/// A picked file whose name in the media folder is taken by a different file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameClash {
    /// Stored path of the file that has the name.
    pub existing: String,
    /// The name goes to an earlier file of the same pick, not to one already
    /// in the folder.
    pub picked: bool,
}

/// How a picked file's name fares in the media folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameCheck {
    /// Nobody has the name.
    Free,
    /// The file with the name (in the folder, or picked before) has the
    /// same content: it can be used instead of a copy.
    Identical,
    Clash(NameClash),
}

/// What copying each of `files` (all outside the media folder) into the top
/// of the media folder runs into, copied in this order. Reads the files.
pub fn name_clashes(media_dir: &Path, files: &[PathBuf]) -> io::Result<Vec<NameCheck>> {
    let mut out = Vec::with_capacity(files.len());
    for (i, file) in files.iter().enumerate() {
        let Some(name) = file.file_name() else {
            out.push(NameCheck::Free);
            continue;
        };
        let existing = relativize(media_dir, &media_dir.join(name)).unwrap_or_default();
        let in_folder = media_dir.join(name);
        let (other, picked) = if in_folder.exists() {
            (Some(in_folder), false)
        } else {
            (files[..i].iter().find(|f| f.file_name() == Some(name)).cloned(), true)
        };
        out.push(match other {
            None => NameCheck::Free,
            Some(other) if same_content(&other, file)? => NameCheck::Identical,
            Some(_) => NameCheck::Clash(NameClash { existing, picked }),
        });
    }
    Ok(out)
}

/// Byte-wise comparison; the size alone says little (equally long WAVs).
fn same_content(a: &Path, b: &Path) -> io::Result<bool> {
    if a.metadata()?.len() != b.metadata()?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (File::open(a)?, File::open(b)?);
    let (mut buf_a, mut buf_b) = (vec![0; 64 * 1024], vec![0; 64 * 1024]);
    loop {
        let len = a.read(&mut buf_a)?;
        if len == 0 {
            return Ok(true);
        }
        b.read_exact(&mut buf_b[..len])?;
        if buf_a[..len] != buf_b[..len] {
            return Ok(false);
        }
    }
}

/// What a file on disk looked like when it was read: a different stamp
/// means its content may have changed. Beside size and modification time
/// (which copy tools can preserve) it holds, on Unix, the inode (a file
/// replaced by renaming gets a new one) and the change time (set by every
/// write, cannot be set back).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub len: u64,
    pub modified: Option<SystemTime>,
    /// Device, inode, change time (s, ns); zero where unavailable.
    pub id: (u64, u64, i64, i64),
}

/// The stamp of `path`; `None` if it is missing or not a file.
pub fn stamp(path: &Path) -> Option<FileStamp> {
    let meta = fs::metadata(path).ok().filter(fs::Metadata::is_file)?;
    #[cfg(unix)]
    let id = {
        use std::os::unix::fs::MetadataExt;
        (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec())
    };
    #[cfg(not(unix))]
    let id = (0, 0, 0, 0);
    Some(FileStamp { len: meta.len(), modified: meta.modified().ok(), id })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaIssue {
    Missing {
        cue: CueId,
        stored: String,
    },
    /// Found only when ignoring case: works on Windows, fails on Linux.
    CaseMismatch {
        cue: CueId,
        stored: String,
        actual: String,
    },
}

/// Checks every media file of every cue below `media_dir`.
pub fn check_media(show: &Show, media_dir: &Path) -> Vec<MediaIssue> {
    let mut issues = Vec::new();
    // Names in each folder, read once: shows keep hundreds of files in one.
    let mut listings = HashMap::new();
    // Groups report through their own cues.
    for cue in show.all().into_iter().filter(|c| !matches!(c.kind, CueKind::Group(_))) {
        for file in cue.media_files() {
            if is_exact_file(&resolve(media_dir, file), &mut listings) {
                continue;
            }
            match find_ignoring_case(media_dir, file) {
                Some(actual) => {
                    issues.push(MediaIssue::CaseMismatch { cue: cue.id, stored: file.clone(), actual })
                }
                None => issues.push(MediaIssue::Missing { cue: cue.id, stored: file.clone() }),
            }
        }
    }
    issues
}

/// True only if the file exists with exactly this spelling, also on
/// case-insensitive file systems. `listings` caches the names per folder.
fn is_exact_file(path: &Path, listings: &mut HashMap<PathBuf, HashSet<OsString>>) -> bool {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else { return false };
    path.is_file()
        && listings
            .entry(parent.to_owned())
            .or_insert_with(|| {
                fs::read_dir(parent)
                    .map(|d| d.filter_map(Result::ok).map(|e| e.file_name()).collect())
                    .unwrap_or_default()
            })
            .contains(name)
}

/// Resolves `stored` component by component, ignoring case. Returns the
/// actual spelling as a stored path.
fn find_ignoring_case(media_dir: &Path, stored: &str) -> Option<String> {
    let mut dir = media_dir.to_path_buf();
    let mut actual = Vec::new();
    for part in stored.split('/').filter(|s| !s.is_empty()) {
        let wanted = part.to_lowercase();
        let entry = fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .find(|e| e.file_name().to_str().is_some_and(|n| n.to_lowercase() == wanted))?;
        let name = entry.file_name().to_str()?.to_owned();
        dir = entry.path();
        actual.push(name);
    }
    dir.is_file().then(|| actual.join("/"))
}

/// A missing file found outside the media folder: cue, stored path, found file.
pub type FoundOutside = (CueId, String, PathBuf);
/// A missing file found inside the media folder: cue, old and new stored path.
pub type Relinked = (CueId, String, String);

/// Repairs missing files by searching `search_dir` recursively for files with
/// the same name. Found files inside `media_dir` are relinked; returns those
/// and the found files outside the media folder (to copy in).
pub fn relink(show: &mut Show, media_dir: &Path, search_dir: &Path) -> (Vec<Relinked>, Vec<FoundOutside>) {
    let mut by_name: HashMap<String, PathBuf> = HashMap::new();
    collect_files(search_dir, &mut by_name, 0);

    let missing: Vec<(CueId, String)> = check_media(show, media_dir)
        .into_iter()
        .filter_map(|i| match i {
            MediaIssue::Missing { cue, stored } => Some((cue, stored)),
            MediaIssue::CaseMismatch { .. } => None,
        })
        .collect();

    let (mut relinked, mut outside) = (Vec::new(), Vec::new());
    let mut fix = |cue: &mut Cue| {
        let id = cue.id;
        for file in cue.media_files_mut() {
            if !missing.iter().any(|(c, stored)| *c == id && stored == file) {
                continue;
            }
            let name = file.rsplit('/').next().unwrap_or_default().to_lowercase();
            let Some(found) = by_name.get(&name) else { continue };
            match relativize(media_dir, found) {
                Some(rel) => {
                    relinked.push((id, std::mem::replace(file, rel.clone()), rel));
                }
                None => outside.push((id, file.clone(), found.clone())),
            }
        }
    };
    fn each(cues: &mut [Cue], fix: &mut impl FnMut(&mut Cue)) {
        for c in cues {
            match &mut c.kind {
                CueKind::Group(g) => each(&mut g.cues, fix),
                _ => fix(c),
            }
        }
    }
    each(&mut show.cues, &mut fix);
    (relinked, outside)
}

/// Files below `media_dir` that none of the stored paths in `used` names,
/// as stored paths with their size in bytes, sorted. Names are compared
/// ignoring case: a file a cue names in another spelling still plays on
/// Windows. Hidden files (copies under way, system files) and linked
/// folders are left out.
pub fn unused_media<'a>(media_dir: &Path, used: impl IntoIterator<Item = &'a str>) -> Vec<(String, u64)> {
    let used: HashSet<String> = used.into_iter().filter_map(|s| media_key(media_dir, s)).collect();
    let mut out = Vec::new();
    collect_unused(media_dir, "", &used, &mut out, 0);
    out.sort();
    out
}

/// A stored path as it compares against the files of `media_dir`: relative,
/// `/`-separated, `.` and `..` applied, lower case. `None` outside the folder.
fn media_key(media_dir: &Path, stored: &str) -> Option<String> {
    let rel = if Path::new(stored).is_absolute() || stored.starts_with('/') {
        relativize(media_dir, Path::new(stored))?
    } else {
        stored.to_owned()
    };
    normalize(&rel).map(|key| key.to_lowercase())
}

/// A relative stored path with `.` and `..` applied; `None` if it leaves
/// its folder or names nothing.
fn normalize(rel: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in rel.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `to` as a stored path from the folder `from` (`../Alt/media`), for a
/// show pointing at another show's media folder. Both must be absolute and
/// free of `.`/`..`; `None` if they share no root (another drive).
pub fn relative_path(from: &Path, to: &Path) -> Option<String> {
    if !from.is_absolute() || !to.is_absolute() {
        return None;
    }
    let (from, to): (Vec<_>, Vec<_>) = (from.components().collect(), to.components().collect());
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    // Drive and root come first; both must be shared.
    if to.iter().skip(common).any(|c| matches!(c, Component::Prefix(_) | Component::RootDir)) {
        return None;
    }
    let mut parts = vec![".."; from.len() - common];
    for c in &to[common..] {
        parts.push(c.as_os_str().to_str()?);
    }
    Some(if parts.is_empty() { ".".into() } else { parts.join("/") })
}

/// Stores a media folder relative to the show folder only when it is inside
/// that folder; otherwise stores its absolute path.
pub fn media_root(show_dir: &Path, media_dir: &Path) -> Option<String> {
    let path = if let Ok(relative) = media_dir.strip_prefix(show_dir) {
        let parts: Option<Vec<&str>> = relative
            .components()
            .map(|component| match component {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect();
        let parts = parts?;
        if parts.is_empty() { ".".into() } else { parts.join("/") }
    } else {
        if !media_dir.is_absolute() {
            return None;
        }
        media_dir.to_str()?.replace('\\', "/")
    };
    Some(path)
}

/// What [`copy_media`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MediaCopy {
    pub copied: usize,
    /// Stored paths that are missing in the old media folder.
    pub missing: Vec<String>,
    /// Stored paths outside the media folder (absolute ones); not copied,
    /// they keep working as they are.
    pub outside: Vec<String>,
}

/// Copies the media files `stored` from the media folder `from` to the same
/// places below `to`, each once, never overwriting. Stops at the first
/// failed copy.
pub fn copy_media<'a>(
    from: &Path,
    to: &Path,
    stored: impl IntoIterator<Item = &'a str>,
) -> io::Result<MediaCopy> {
    let mut result = MediaCopy::default();
    let mut seen = HashSet::new();
    for file in stored {
        if !seen.insert(file) {
            continue;
        }
        let absolute = Path::new(file).is_absolute() || file.starts_with('/');
        let Some(rel) = normalize(file).filter(|_| !absolute) else {
            result.outside.push(file.to_owned());
            continue;
        };
        let source = resolve(from, &rel);
        if !source.is_file() {
            result.missing.push(file.to_owned());
            continue;
        }
        match copy_file(&source, &resolve(to, &rel), |_| true) {
            Ok(()) => result.copied += 1,
            // The same file in another spelling (`a.wav`, `./a.wav`, `A.wav` on Windows).
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io::Error::new(e.kind(), format!("{}: {e}", source.display()))),
        }
    }
    Ok(result)
}

fn collect_unused(
    dir: &Path,
    prefix: &str,
    used: &HashSet<String>,
    out: &mut Vec<(String, u64)>,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.filter_map(Result::ok) {
        let Some(name) = e.file_name().to_str().map(str::to_owned) else { continue };
        let Ok(kind) = e.file_type() else { continue };
        if name.starts_with('.') {
            continue;
        }
        let stored = format!("{prefix}{name}");
        if kind.is_dir() {
            collect_unused(&e.path(), &format!("{stored}/"), used, out, depth + 1);
        } else if kind.is_file() && !used.contains(&stored.to_lowercase()) {
            out.push((stored, e.metadata().map(|m| m.len()).unwrap_or(0)));
        }
    }
}

/// Deletes the media files `files` (stored paths) and the folders inside
/// `media_dir` that are left empty. Returns the files that could not be
/// deleted, with the reason.
pub fn remove_media(media_dir: &Path, files: &[String]) -> Vec<(String, io::Error)> {
    let mut failed = Vec::new();
    for stored in files {
        let path = resolve(media_dir, stored);
        if relativize(media_dir, &path).is_none() {
            failed.push((stored.clone(), io::Error::other(t!("paths.outside_media"))));
            continue;
        }
        if let Err(e) = fs::remove_file(&path) {
            failed.push((stored.clone(), e));
            continue;
        }
        // Fails, as wanted, while a folder still holds something.
        let mut dir = path.parent();
        while let Some(d) = dir.filter(|d| *d != media_dir && d.starts_with(media_dir)) {
            if fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
    }
    failed
}

fn collect_files(dir: &Path, out: &mut HashMap<String, PathBuf>, depth: usize) {
    if depth > 16 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.filter_map(Result::ok) {
        let path = e.path();
        // Linked folders are left out: a link back up would be searched
        // again on every level.
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            collect_files(&path, out, depth + 1);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            out.entry(name.to_lowercase()).or_insert(path);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{GroupCue, PlaylistCue, SoundCue};

    #[test]
    fn resolves_stored_paths_natively() {
        let p = resolve(Path::new("show/media"), "musik/ouvertuere.flac");
        assert_eq!(p, Path::new("show/media").join("musik").join("ouvertuere.flac"));
        assert_eq!(resolve(Path::new("m"), "a\\b.wav"), Path::new("m").join("a").join("b.wav"));
    }

    #[test]
    fn relativizes_inside_media_only() {
        let media = Path::new("/show/media");
        assert_eq!(
            relativize(media, Path::new("/show/media/sfx/donner.wav")).as_deref(),
            Some("sfx/donner.wav")
        );
        assert_eq!(relativize(media, Path::new("/elsewhere/x.wav")), None);
    }

    #[test]
    fn copies_in_without_overwriting_or_leaving_parts() {
        let dir = tempdir("copy");
        let media = dir.join("media");
        let source = dir.join("donner.wav");
        fs::write(&source, vec![7u8; 3 << 20]).unwrap();

        let target = media.join(free_name(&media, "donner.wav", |_| false));
        let mut seen = Vec::new();
        copy_file(&source, &target, |done| {
            seen.push(done);
            true
        })
        .unwrap();
        assert_eq!(seen, [1 << 20, 2 << 20, 3 << 20], "progress in bytes");
        assert_eq!(fs::read(&target).unwrap(), fs::read(&source).unwrap());

        // Taken on disk or by a copy under way: the next free name.
        assert_eq!(free_name(&media, "donner.wav", |_| false), "donner-2.wav");
        assert_eq!(free_name(&media, "donner.wav", |n| n == "donner-2.wav"), "donner-3.wav");
        assert!(copy_file(&source, &target, |_| true).is_err(), "never overwrites");

        // Stopped halfway: nothing is left behind.
        let other = media.join("regen.wav");
        let stopped = copy_file(&source, &other, |done| done < 2 << 20);
        assert_eq!(stopped.unwrap_err().kind(), io::ErrorKind::Interrupted);
        assert_eq!(fs::read_dir(&media).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    /// Absolute test paths need a drive letter on Windows.
    const ROOT: &str = if cfg!(windows) { "C:" } else { "" };

    #[test]
    fn relative_paths_between_folders() {
        let p = |s: &str| PathBuf::from(format!("{ROOT}{s}"));
        assert_eq!(relative_path(&p("/shows/Neu"), &p("/shows/Alt/media")).as_deref(), Some("../Alt/media"));
        assert_eq!(relative_path(&p("/shows/Neu"), &p("/shows/Neu/media")).as_deref(), Some("media"));
        assert_eq!(relative_path(&p("/a/b/c"), &p("/x")).as_deref(), Some("../../../x"));
        assert_eq!(relative_path(&p("/a"), &p("/a")).as_deref(), Some("."));
        assert_eq!(relative_path(&p("/a"), &PathBuf::from("rel/media")), None);
    }

    #[test]
    fn media_roots_are_relative_only_inside_the_show_folder() {
        let p = |s: &str| PathBuf::from(format!("{ROOT}{s}"));
        let show = p("/shows/one");
        assert_eq!(media_root(&show, &show), Some(".".into()));
        assert_eq!(media_root(&show, &p("/shows/one/media/nested")), Some("media/nested".into()));
        assert_eq!(media_root(&show, &p("/shows/two/media")), Some(format!("{ROOT}/shows/two/media")));
    }

    #[test]
    fn copies_used_media_once() {
        let dir = tempdir("copy-media");
        let (from, to) = (dir.join("alt"), dir.join("neu"));
        fs::create_dir_all(from.join("sfx")).unwrap();
        fs::write(from.join("a.wav"), b"a").unwrap();
        fs::write(from.join("sfx/b.wav"), b"b").unwrap();
        fs::write(from.join("unbenutzt.wav"), b"u").unwrap();
        let outside = dir.join("draussen.wav").to_string_lossy().replace('\\', "/");
        let files = ["a.wav", "sfx/b.wav", "./a.wav", "sfx/../a.wav", "weg.wav", "../x.wav", &outside];

        let copied = copy_media(&from, &to, files).unwrap();
        assert_eq!(copied.copied, 2);
        assert_eq!(copied.missing, ["weg.wav"]);
        assert_eq!(copied.outside, ["../x.wav".to_string(), outside]);
        assert_eq!(fs::read(to.join("sfx/b.wav")).unwrap(), b"b");
        assert!(!to.join("unbenutzt.wav").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_name_clashes_in_folder_and_pick() {
        let dir = tempdir("clash");
        let media = dir.join("media");
        fs::create_dir_all(&media).unwrap();
        fs::write(media.join("donner.wav"), b"aaaa").unwrap();
        for (sub, content) in [("a", b"aaaa"), ("b", b"bbbb"), ("c", b"cccc")] {
            fs::create_dir_all(dir.join(sub)).unwrap();
            fs::write(dir.join(sub).join("donner.wav"), content).unwrap();
            fs::write(dir.join(sub).join("regen.wav"), content).unwrap();
        }
        let pick = |names: &[&str]| names.iter().map(|n| dir.join(n)).collect::<Vec<_>>();
        let folder = NameCheck::Clash(NameClash { existing: "donner.wav".into(), picked: false });
        assert_eq!(
            name_clashes(&media, &pick(&["a/donner.wav", "b/donner.wav", "c/donner.wav"])).unwrap(),
            [NameCheck::Identical, folder.clone(), folder],
            "the identical file can be reused"
        );
        let earlier = NameCheck::Clash(NameClash { existing: "regen.wav".into(), picked: true });
        assert_eq!(
            name_clashes(&media, &pick(&["a/regen.wav", "a/regen.wav", "b/regen.wav"])).unwrap(),
            [NameCheck::Free, NameCheck::Identical, earlier],
            "picked twice can be reused"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_case_mismatch_and_missing_files_and_relinks() {
        let dir = tempdir("check");
        let media = dir.join("media");
        fs::create_dir_all(media.join("Musik")).unwrap();
        fs::write(media.join("Musik/Ouvertuere.flac"), b"x").unwrap();
        fs::create_dir_all(media.join("neu")).unwrap();
        fs::write(media.join("neu/donner.wav"), b"x").unwrap();

        let mut show = Show::default();
        let ok = Cue::new(CueKind::Sound(SoundCue::new("Musik/Ouvertuere.flac")));
        let case = Cue::new(CueKind::Sound(SoundCue::new("musik/ouvertuere.flac")));
        let gone = Cue::new(CueKind::Sound(SoundCue::new("sfx/donner.wav")));
        let (case_id, gone_id) = (case.id, gone.id);
        show.cues = vec![ok, case, gone];

        let issues = check_media(&show, &media);
        assert_eq!(
            issues,
            [
                MediaIssue::CaseMismatch {
                    cue: case_id,
                    stored: "musik/ouvertuere.flac".into(),
                    actual: "Musik/Ouvertuere.flac".into()
                },
                MediaIssue::Missing { cue: gone_id, stored: "sfx/donner.wav".into() },
            ]
        );

        let (relinked, outside) = relink(&mut show, &media, &media);
        assert_eq!(relinked, [(gone_id, "sfx/donner.wav".into(), "neu/donner.wav".into())]);
        assert!(outside.is_empty());
        let CueKind::Sound(s) = &show.cues[2].kind else { unreachable!() };
        assert_eq!(s.file, "neu/donner.wav");

        // Playlist items are checked and relinked one by one.
        let list = Cue::new(CueKind::Playlist(PlaylistCue::new(vec![
            "Musik/Ouvertuere.flac".into(),
            "alt/donner.wav".into(),
        ])));
        let list_id = list.id;
        show.cues = vec![list];
        assert_eq!(
            check_media(&show, &media),
            [MediaIssue::Missing { cue: list_id, stored: "alt/donner.wav".into() }]
        );
        let (relinked, _) = relink(&mut show, &media, &media);
        assert_eq!(relinked, [(list_id, "alt/donner.wav".into(), "neu/donner.wav".into())]);
        let CueKind::Playlist(p) = &show.cues[0].kind else { unreachable!() };
        let files: Vec<&str> = p.items.iter().map(|i| i.file.as_str()).collect();
        assert_eq!(files, ["Musik/Ouvertuere.flac", "neu/donner.wav"]);

        // Inside a group the cue itself is named and repaired.
        let inner = Cue::new(CueKind::Sound(SoundCue::new("weg/donner.wav")));
        let inner_id = inner.id;
        show.cues = vec![Cue::new(CueKind::Group(GroupCue { cues: vec![inner], ..Default::default() }))];
        assert_eq!(
            check_media(&show, &media),
            [MediaIssue::Missing { cue: inner_id, stored: "weg/donner.wav".into() }]
        );
        assert_eq!(relink(&mut show, &media, &media).0[0].0, inner_id);
        assert!(check_media(&show, &media).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_and_removes_unused_media() {
        let dir = tempdir("unused");
        let media = dir.join("media");
        for file in
            ["Musik/Ouvertuere.flac", "musik/alt.wav", "sfx/weg/donner.wav", "regen.wav", ".donner.wav.part"]
        {
            let path = resolve(&media, file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"abc").unwrap();
        }
        fs::create_dir_all(media.join("leer")).unwrap();
        let absolute = media.join("regen.wav").to_string_lossy().into_owned();
        // Other spelling, absolute, with `..`, outside the folder.
        let used = ["musik/ouvertuere.flac", absolute.as_str(), "sfx/../Musik/./alt.wav", "../regen.wav"];
        let unused = unused_media(&media, used);
        assert_eq!(unused, [("sfx/weg/donner.wav".to_owned(), 3)]);

        let names: Vec<String> = unused.into_iter().map(|(f, _)| f).collect();
        assert!(remove_media(&media, &names).is_empty());
        assert!(!media.join("sfx").exists(), "emptied folders go too");
        assert!(media.join("leer").is_dir(), "folders that were empty stay");
        assert!(media.join("regen.wav").is_file());
        assert!(unused_media(&media, used).is_empty());

        let failed = remove_media(&media, &["../media/regen.wav".into(), "gibt-es-nicht.wav".into()]);
        assert_eq!(failed.len(), 2);
        assert!(media.join("regen.wav").is_file(), "nothing outside the plain folder path");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stamp_changes_with_the_file() {
        let dir = tempdir("stamp");
        let path = dir.join("a.wav");
        assert_eq!(stamp(&path), None);
        assert_eq!(stamp(&dir), None, "a folder has no stamp");
        fs::write(&path, b"one").unwrap();
        let first = stamp(&path).unwrap();
        assert_eq!(stamp(&path), Some(first));

        // Same size, replaced by renaming (as editors and exports do).
        let tmp = dir.join("a.tmp");
        fs::write(&tmp, b"two").unwrap();
        fs::rename(&tmp, &path).unwrap();
        assert_ne!(stamp(&path), Some(first));
        fs::remove_dir_all(dir).unwrap();
    }

    pub(crate) fn tempdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("jq-show-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }
}
