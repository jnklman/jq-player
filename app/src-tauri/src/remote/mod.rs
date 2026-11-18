//! Remote control: other devices on the show network watch the show in a
//! browser and, if the host lets them, operate it (GO, stops, playhead,
//! stopwatches), never edit it. Off by default (`LocalSettings.remote`).
//!
//! - [`hub`]: the show thread's events for the clients, never blocking it.
//! - [`server`]: web page, join request and WebSocket, on a thread of its own.
//! - [`protocol`]: the frames, and which role may call what.
//! - [`clients`]: the devices and their roles; kept in `remote-clients.json`.

pub mod clients;
pub mod hub;
pub mod protocol;
pub mod server;

use std::sync::Arc;

use jq_core::{Notice, RemoteSettings};
use tauri::State;

pub use clients::{Registry, RemoteStatus};
pub use hub::Hub;
use protocol::Role;
use server::{Server, Shared};

/// Sent to the main window whenever a device asks, connects, leaves or gets
/// another role, and when the server starts or stops; a [`RemoteStatus`].
pub const EVENT_REMOTE: &str = "remote";

/// File of the let-in devices, in the app's config folder.
pub const CLIENTS_FILE: &str = "remote-clients.json";

/// Tells the host something (a notice in the main window).
pub type Notify = Arc<dyn Fn(Notice) + Send + Sync>;

/// The server as the settings want it; owned by the show thread.
pub struct Remote {
    shared: Arc<Shared>,
    notify: Notify,
    server: Option<Server>,
    /// The settings last applied: a server that failed to start is tried
    /// again only once they change.
    applied: Option<RemoteSettings>,
}

impl Remote {
    pub fn new(shared: Shared, notify: Notify) -> Self {
        Self { shared: Arc::new(shared), notify, server: None, applied: None }
    }

    /// Starts, stops or moves the server as `settings` say. Cheap when
    /// nothing changed, so it is called after every batch of requests.
    pub fn apply(&mut self, settings: &RemoteSettings, notices: &mut Vec<Notice>) {
        if self.applied.as_ref() == Some(settings) {
            return;
        }
        self.applied = Some(*settings);
        // The old one first, so a server on the same port can start.
        self.server = None;
        let registry = &self.shared.registry;
        if !settings.enabled {
            registry.set_server(None, None);
            return;
        }
        let failed = {
            let (registry, notify) = (registry.clone(), self.notify.clone());
            move |error: String| {
                let e = jq_i18n::t!("remote.stopped", error = error);
                registry.set_server(None, Some(e.clone()));
                notify(Notice::Error(e));
            }
        };
        match Server::start(settings.port, self.shared.clone(), failed) {
            Ok(server) => {
                registry.set_server(Some(server.port()), None);
                self.server = Some(server);
            }
            // A port taken by another program: the show goes on without.
            Err(e) => {
                let e = jq_i18n::t!("remote.listen_failed", port = settings.port, error = e);
                notices.push(Notice::Warning(e.clone()));
                registry.set_server(None, Some(e));
            }
        }
    }
}

/// The server and the devices (later changes come as `remote` events).
#[tauri::command]
pub fn get_remote(registry: State<'_, Registry>) -> RemoteStatus {
    registry.status()
}

/// Lets a device in as `role` ("view" or "control"), or changes its role.
/// Allowed in show mode too: it is part of operating, not of editing.
#[tauri::command]
pub fn set_remote_client(registry: State<'_, Registry>, id: String, role: Role) -> Result<(), String> {
    registry.set_role(&id, role)
}

/// Removes a device, or turns its request down.
#[tauri::command]
pub fn remove_remote_client(registry: State<'_, Registry>, id: String) -> Result<(), String> {
    registry.remove(&id)
}

/// One address the remote page is reached at.
#[derive(serde::Serialize)]
pub struct RemoteAddress {
    /// Name of the network interface (e.g. "Ethernet", "wlan0").
    pub interface: String,
    /// `http://<ip>:<port>/`, with the scheme so a browser does not try
    /// HTTPS first.
    pub url: String,
    /// The URL as a QR code, an SVG document.
    pub qr_svg: String,
}

/// The addresses of this machine's network interfaces (IPv4, up, not
/// loopback) at the port the server listens on; none while it is off.
#[tauri::command]
pub async fn remote_info(registry: State<'_, Registry>) -> Result<Vec<RemoteAddress>, String> {
    let Some(port) = registry.status().listening else { return Ok(Vec::new()) };
    tauri::async_runtime::spawn_blocking(move || addresses(port)).await.map_err(|e| e.to_string())?
}

fn addresses(port: u16) -> Result<Vec<RemoteAddress>, String> {
    let interfaces = if_addrs::get_if_addrs().map_err(|e| e.to_string())?;
    let mut found: Vec<RemoteAddress> = interfaces
        .into_iter()
        .filter(|i| i.ip().is_ipv4() && !i.is_loopback() && !i.is_link_local() && i.is_oper_up())
        .map(|i| {
            let url = format!("http://{}:{port}/", i.ip());
            let qr_svg = qr_svg(&url);
            RemoteAddress { interface: i.name, url, qr_svg }
        })
        .collect();
    found.dedup_by(|a, b| a.url == b.url);
    Ok(found)
}

fn qr_svg(text: &str) -> String {
    use qrcode::render::svg;
    qrcode::QrCode::new(text.as_bytes())
        .map(|code| {
            code.render::<svg::Color<'_>>()
                .min_dimensions(160, 160)
                .dark_color(svg::Color("#000000"))
                .light_color(svg::Color("#ffffff"))
                .build()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_qr_code_is_an_svg() {
        let svg = super::qr_svg("http://192.168.1.20:8642/");
        assert!(svg.contains("<svg") && svg.contains("</svg>"));
    }
}
