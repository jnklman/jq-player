//! Saving the show under a new name, in a new show folder. Its media are
//! either copies of the files the show uses or the old media folder,
//! linked. Copying happens in the background, never on the show thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use jq_core::Notice;
use jq_show::ShowFile;
use jq_show::paths;
use serde::Deserialize;
use tauri::State;

use crate::backend::Request;
use crate::error::{AppError, Error};
use crate::session::real_path;
use crate::{Backend, REPLY_TIMEOUT};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaChoice {
    /// The files the show uses go into the new folder's own media folder.
    Copy,
    /// The new show uses the old media folder.
    Link,
}

/// Saves the show as `<dir>/<name>/<name>.jqshow` and goes on there.
/// Fails in show mode, while cues run or files are being copied in.
#[tauri::command]
pub async fn save_show_as(
    backend: State<'_, Backend>,
    dir: PathBuf,
    name: String,
    media: MediaChoice,
) -> Result<serde_json::Value, Error> {
    let (tx, rx) = mpsc::channel();
    backend.send(Request::SaveAsPlan(tx))?;
    let (media_dir, files) = tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(REPLY_TIMEOUT))
        .await
        .map_err(|e| Error::from(e.to_string()))?
        .map_err(|_| Error::from(AppError::NoReply))??;
    // The show thread checks that nothing it uses now was left out.
    let copied = (media == MediaChoice::Copy).then(|| files.clone());
    let (path, media_root, warnings) =
        tauri::async_runtime::spawn_blocking(move || prepare(&dir, &name, &media_dir, &files, media))
            .await
            .map_err(|e| Error::from(e.to_string()))??;
    // From here on the show thread removes the folder if it fails.
    backend.ask(|reply| Request::SaveAs { path, media_root, copied, warnings, reply }).await
}

/// Creates the new show folder and fills its media folder. Returns the show
/// file's path, the media folder as stored and warnings for the operator.
/// On failure nothing is left behind.
fn prepare(
    dir: &Path,
    name: &str,
    media_dir: &Path,
    files: &[String],
    media: MediaChoice,
) -> Result<(PathBuf, String, Vec<Notice>), Error> {
    let path = ShowFile::create_folder(dir, name).map_err(Error::from)?;
    let folder = path.parent().expect("show file lies in its folder").to_owned();
    let filled = match media {
        MediaChoice::Copy => copy(media_dir, &folder.join("media"), files),
        MediaChoice::Link => Ok((linked_root(&folder, media_dir), Vec::new())),
    };
    match filled {
        Ok((media_root, warnings)) => Ok((path, media_root, warnings)),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&folder);
            Err(e)
        }
    }
}

fn copy(from: &Path, to: &Path, files: &[String]) -> Result<(String, Vec<Notice>), Error> {
    std::fs::create_dir(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let copied = paths::copy_media(from, to, files.iter().map(String::as_str))
        .map_err(|e| Error::from(AppError::MediaNotCopied { error: e }))?;
    let mut warnings = Vec::new();
    if !copied.missing.is_empty() {
        warnings.push(Notice::SaveAsMissing { count: copied.missing.len(), file: copied.missing[0].clone() });
    }
    if !copied.outside.is_empty() {
        warnings.push(Notice::SaveAsOutside { count: copied.outside.len(), file: copied.outside[0].clone() });
    }
    Ok(("media".into(), warnings))
}

/// The media folder relative to the new show folder when it is inside it;
/// otherwise its absolute path.
fn linked_root(folder: &Path, media_dir: &Path) -> String {
    let real = |p: &Path| real_path(p).unwrap_or_else(|_| p.to_path_buf());
    paths::media_root(&real(folder), &real(media_dir))
        .unwrap_or_else(|| media_dir.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jq-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn links_or_copies_the_media() {
        let dir = tempdir("save-as");
        let media = dir.join("Alt/media");
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("a.wav"), b"a").unwrap();
        let files = vec!["a.wav".to_string(), "weg.wav".to_string()];

        let (path, root, warnings) = prepare(&dir, "Verknüpft", &media, &files, MediaChoice::Link).unwrap();
        assert_eq!(path, dir.join("Verknüpft/Verknüpft.jqshow"));
        let media_root = real_path(&media).unwrap().to_str().unwrap().replace('\\', "/");
        assert_eq!((root.as_str(), warnings.len()), (media_root.as_str(), 0));
        assert!(!dir.join("Verknüpft/media").exists());
        assert!(paths::resolve(path.parent().unwrap(), &root).join("a.wav").is_file());

        let (path, root, warnings) = prepare(&dir, "Kopie", &media, &files, MediaChoice::Copy).unwrap();
        assert_eq!(root, "media");
        assert!(
            matches!(&warnings[0], Notice::SaveAsMissing { file, .. } if file == "weg.wav"),
            "{warnings:?}"
        );
        assert_eq!(std::fs::read(path.with_file_name("media").join("a.wav")).unwrap(), b"a");

        assert!(matches!(
            prepare(&dir, "Kopie", &media, &files, MediaChoice::Copy),
            Err(Error::Show(jq_show::ShowError::Exists { .. }))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
