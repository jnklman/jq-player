//! Media-folder operations. File-system work happens in the background,
//! never on the show thread.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;

use jq_show::paths;
use serde::Serialize;
use tauri::State;

use crate::backend::Request;
use crate::{Backend, REPLY_TIMEOUT};

#[derive(Serialize)]
pub struct UnusedFile {
    /// Stored path, relative to the media folder.
    file: String,
    bytes: u64,
}

#[derive(Serialize)]
pub struct Cleaned {
    deleted: usize,
    /// Files that could not be deleted, with the reason.
    failed: Vec<String>,
}

/// The media folder and the stored paths the show may still need.
async fn used_media(backend: &Backend) -> Result<(PathBuf, Vec<String>), String> {
    let (tx, rx) = mpsc::channel();
    backend.send(Request::UsedMedia(tx))?;
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(REPLY_TIMEOUT))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|_| jq_i18n::t!("app.no_reply"))?
}

fn unused(media_dir: &std::path::Path, used: &[String]) -> Vec<(String, u64)> {
    paths::unused_media(media_dir, used.iter().map(String::as_str))
}

/// Files in the media folder that neither the show (as edited or as saved)
/// nor the clipboard uses. Fails in show mode.
#[tauri::command]
pub async fn unused_media(backend: State<'_, Backend>) -> Result<Vec<UnusedFile>, String> {
    let (media_dir, used) = used_media(&backend).await?;
    tauri::async_runtime::spawn_blocking(move || {
        unused(&media_dir, &used).into_iter().map(|(file, bytes)| UnusedFile { file, bytes }).collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Deletes `files` (from `unused_media`) for good, leaving out those the
/// show has started using meanwhile, and the folders left empty.
#[tauri::command]
pub async fn clean_media(backend: State<'_, Backend>, files: Vec<String>) -> Result<Cleaned, String> {
    let (media_dir, used) = used_media(&backend).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let unused: HashSet<String> = unused(&media_dir, &used).into_iter().map(|(file, _)| file).collect();
        let files: Vec<String> = files.into_iter().filter(|f| unused.contains(f)).collect();
        let failed = paths::remove_media(&media_dir, &files);
        Cleaned {
            deleted: files.len() - failed.len(),
            failed: failed.into_iter().map(|(file, e)| format!("{file}: {e}")).collect(),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Opens the selected media folder in the system file manager.
#[tauri::command]
pub async fn open_media_dir(backend: State<'_, Backend>) -> Result<(), String> {
    let dir = backend.ask(Request::MediaDir).await?;

    tauri::async_runtime::spawn_blocking(move || {
        if !dir.is_dir() {
            return Err(jq_i18n::t!("media.dir_not_found", path = dir.display()));
        }
        #[cfg(target_os = "windows")]
        let mut command = {
            let mut command = Command::new("explorer");
            command.arg(&dir);
            command
        };
        #[cfg(target_os = "linux")]
        let mut command = {
            let mut command = Command::new("xdg-open");
            command.arg(&dir);
            command
        };

        command.spawn().map(|_| ()).map_err(|e| jq_i18n::t!("media.file_manager_failed", error = e))
    })
    .await
    .map_err(|e| e.to_string())?
}
