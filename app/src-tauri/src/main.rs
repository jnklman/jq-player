// Prevents an additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Linux: WebKitGTK renders on the graphics card (DMA-BUF). Where that
    // shows a blank window (once seen with NVIDIA under Wayland), starting
    // with WEBKIT_DISABLE_DMABUF_RENDERER=1 falls back to the software path,
    // which costs noticeably more CPU per frame (see README).
    #[cfg(windows)]
    if tauri::webview_version().is_err() {
        missing_webview2();
        return;
    }
    jq_player_lib::run()
}

/// Without WebView2 (only possible with the bare .exe; the installer brings
/// it along) Tauri would show an English error and quit without a word.
#[cfg(windows)]
fn missing_webview2() {
    // Before Tauri and the settings: the system language decides.
    jq_i18n::set_language(jq_i18n::system_language());
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(jq_i18n::t!("startup.webview2_title"))
        .set_description(jq_i18n::t!("startup.webview2_text"))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
