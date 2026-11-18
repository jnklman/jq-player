//! The remote server: the remote page (`remote.html` from the app's own
//! files), a join request over HTTP and a WebSocket per open page. It runs
//! on a thread with a runtime of its own, apart from Tauri's and from the
//! show thread, which it only reaches through the request queue; whatever
//! goes wrong in here ends at most the server.

use std::net::SocketAddr;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};

use super::clients::{Lookup, Registry};
use super::hub::{Frame, Hub};
use super::protocol::{self, Call, ClientMessage, Role};
use crate::backend::Request;
use crate::channel::{Backend, REPLY_TIMEOUT};

/// Name of the cookie holding a device's token.
const COOKIE: &str = "jq_remote";
/// A year: the device stays known until the host removes it.
const COOKIE_AGE: u64 = 365 * 24 * 60 * 60;
/// The page counts the connection as lost when pings stop for 3 s.
const PING_INTERVAL: Duration = Duration::from_secs(1);
/// A client that takes longer than this for one frame is cut off.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// Close code telling the page it was removed (not to reconnect).
const CLOSE_REMOVED: u16 = 4001;
/// How long stopping waits for the server's thread to end.
const STOP_WAIT: Duration = Duration::from_secs(1);

/// A file of the remote page.
pub struct Asset {
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// Looks up a file of the app's web files by its path (`remote.html`,
/// `assets/…`).
pub type Assets = Arc<dyn Fn(&str) -> Option<Asset> + Send + Sync>;

/// What every connection needs.
pub struct Shared {
    pub backend: Backend,
    pub hub: Hub,
    pub registry: Registry,
    pub assets: Assets,
    /// Random per program start: a page that reconnects to a new one knows
    /// the host restarted (its view revisions start again).
    pub instance: String,
    /// The language setting and what it resolves to (`get_language`).
    pub language: Arc<dyn Fn() -> serde_json::Value + Send + Sync>,
}

/// A running server; dropping it stops it.
pub struct Server {
    port: u16,
    stop: Option<oneshot::Sender<()>>,
    done: std_mpsc::Receiver<()>,
}

impl Server {
    /// Listens on `port` of every interface (0: any free port). `failed` is
    /// told if the server breaks down later.
    pub fn start(
        port: u16,
        shared: Arc<Shared>,
        failed: impl FnOnce(String) + Send + 'static,
    ) -> Result<Self, String> {
        let listener = std::net::TcpListener::bind(("0.0.0.0", port)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let (stop, stopped) = oneshot::channel();
        let (finished, done) = std_mpsc::channel();
        std::thread::Builder::new()
            .name("jq-remote".into())
            .spawn(move || {
                let run = catch_unwind(AssertUnwindSafe(|| serve(listener, shared, stopped)));
                match run {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => failed(e),
                    Err(_) => failed(jq_i18n::t!("remote.crashed")),
                }
                let _ = finished.send(());
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { port, stop: Some(stop), done })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        // So the port is free again for a server started right after.
        let _ = self.done.recv_timeout(STOP_WAIT);
    }
}

fn serve(
    listener: std::net::TcpListener,
    shared: Arc<Shared>,
    stopped: oneshot::Receiver<()>,
) -> Result<(), String> {
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::from_std(listener).map_err(|e| e.to_string())?;
        let app = router(shared).into_make_service_with_connect_info::<SocketAddr>();
        tokio::select! {
            served = axum::serve(listener, app) => served.map_err(|e| e.to_string()),
            _ = stopped => Ok(()),
        }
    })
    // Dropping the runtime ends the connections still open.
}

fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/api/me", get(me))
        .route("/api/join", post(join))
        .route("/ws", get(socket))
        .fallback(get(asset))
        .with_state(shared)
}

/// The page itself; whoever opens it is asked for a name first.
async fn page(State(shared): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    file(&shared, "remote.html", &headers).await
}

async fn asset(State(shared): State<Arc<Shared>>, uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    // Only the remote page's own files: never the app's main window. A
    // development build takes them from the Vite dev server, which serves the
    // sources under their own paths (`src/…`, `@vite/client`, `node_modules/…`)
    // and needs the query (`?v=…`, `?import`).
    let dev = cfg!(debug_assertions) && path != "index.html";
    let allowed = dev || path.starts_with("assets/") || REMOTE_ROOT_FILES.contains(&path);
    if !allowed || path.contains("..") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let request = match uri.path_and_query() {
        Some(full) if dev => full.as_str().trim_start_matches('/'),
        _ => path,
    };
    file(&shared, request, &headers).await
}

/// Files outside `assets/` the remote page uses.
const REMOTE_ROOT_FILES: &[&str] = &["remote.webmanifest", "favicon.svg"];

async fn file(shared: &Arc<Shared>, path: &str, headers: &HeaderMap) -> Response {
    let assets = shared.assets.clone();
    let owned = path.to_owned();
    let Ok(Some(found)) = tokio::task::spawn_blocking(move || assets(&owned)).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut response = (StatusCode::OK, found.bytes).into_response();
    let set = response.headers_mut();
    if let Ok(mime) = HeaderValue::from_str(&found.mime) {
        set.insert(header::CONTENT_TYPE, mime);
    }
    // Built files carry a hash in their name; the page itself must be fresh
    // after an update.
    let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    set.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    set.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    if path.ends_with(".html") && !cfg!(debug_assertions) {
        let host = host(headers).unwrap_or_default();
        let csp = format!(
            "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
             media-src 'self'; connect-src 'self' ws://{host}; frame-ancestors 'none'"
        );
        if let Ok(csp) = HeaderValue::from_str(&csp) {
            set.insert(header::CONTENT_SECURITY_POLICY, csp);
        }
    }
    response
}

/// The device the cookie names: `{id, name, role}`, or `null` if it has to
/// ask to join (first visit, removed, or the host's address changed).
async fn me(State(shared): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    let found = token(&headers).and_then(|token| shared.registry.lookup(&token));
    axum::Json(found.map(|found| identity(&found))).into_response()
}

#[derive(Deserialize)]
struct JoinRequest {
    name: String,
}

/// A new request to join; sets the cookie with the device's token.
async fn join(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<JoinRequest>,
) -> Response {
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match shared.registry.join(&request.name, Some(address.to_string())) {
        Ok((token, found)) => {
            let cookie = format!("{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={COOKIE_AGE}");
            let mut response = axum::Json(identity(&found)).into_response();
            if let Ok(cookie) = HeaderValue::from_str(&cookie) {
                response.headers_mut().insert(header::SET_COOKIE, cookie);
            }
            response
        }
        Err(e) => (StatusCode::TOO_MANY_REQUESTS, e).into_response(),
    }
}

fn identity(found: &Lookup) -> serde_json::Value {
    serde_json::json!({ "id": found.id, "name": found.name, "role": found.role.borrow().unwrap_or(Role::Pending) })
}

async fn socket(
    State(shared): State<Arc<Shared>>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    // A page from elsewhere must not use the device's cookie to connect
    // (SameSite is not kept by every browser for WebSockets).
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(found) = token(&headers).and_then(|token| shared.registry.lookup(&token)) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    upgrade.on_upgrade(move |socket| client(socket, shared, found, address))
}

/// One open page: sends what its role lets it see and runs its calls.
async fn client(mut socket: WebSocket, shared: Arc<Shared>, found: Lookup, address: SocketAddr) {
    let _open = shared.registry.connect(&found.id, address.to_string());
    let (id, mut roles, mut keys) = (found.id, found.role, found.keys);
    let Some(mut role) = *roles.borrow_and_update() else { return };
    let mut feeds = shared.hub.subscribe();
    let (replies, mut replied) = mpsc::unbounded_channel::<String>();
    let mut ping = tokio::time::interval(PING_INTERVAL);

    let hello =
        protocol::hello(&shared.instance, &id, &found.name, role, keys.borrow_and_update().as_deref());
    if !send(&mut socket, hello).await {
        return;
    }
    if role != Role::Pending && !send_latest(&mut socket, &mut feeds).await {
        return;
    }
    loop {
        let frame: Option<String> = tokio::select! {
            message = socket.recv() => match message {
                Some(Ok(Message::Text(text))) => call(&shared, &id, role, &text, &replies),
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                Some(Ok(_)) => None,
            },
            Ok(()) = feeds.state.changed() => seen(role, &mut feeds.state),
            Ok(()) = feeds.show.changed() => seen(role, &mut feeds.show),
            Ok(()) = feeds.language.changed() => seen(role, &mut feeds.language),
            changed = roles.changed() => {
                let now = if changed.is_ok() { *roles.borrow_and_update() } else { None };
                let Some(now) = now else {
                    let close = CloseFrame { code: CLOSE_REMOVED, reason: "removed".into() };
                    let _ = tokio::time::timeout(SEND_TIMEOUT, socket.send(Message::Close(Some(close)))).await;
                    return;
                };
                let was = std::mem::replace(&mut role, now);
                if !send(&mut socket, protocol::role(now)).await {
                    return;
                }
                // Let in just now: what it may see from now on.
                if was == Role::Pending && now != Role::Pending && !send_latest(&mut socket, &mut feeds).await {
                    return;
                }
                None
            }
            Ok(()) = keys.changed() => Some(protocol::keys(keys.borrow_and_update().as_deref())),
            Some(reply) = replied.recv() => Some(reply),
            _ = ping.tick() => Some(protocol::PING.to_owned()),
        };
        if let Some(frame) = frame
            && !send(&mut socket, frame).await
        {
            return;
        }
    }
}

/// The newest frame of a feed, if the client may see it.
fn seen(role: Role, feed: &mut tokio::sync::watch::Receiver<Frame>) -> Option<String> {
    let frame = feed.borrow_and_update().clone();
    if role == Role::Pending {
        return None;
    }
    frame.map(|frame| frame.to_string())
}

/// What a client gets on connecting (or being let in): the latest show and
/// state, if any are about.
async fn send_latest(
    socket: &mut WebSocket,
    feeds: &mut super::hub::Feeds<tokio::sync::watch::Receiver<Frame>>,
) -> bool {
    for feed in [&mut feeds.show, &mut feeds.state] {
        let frame = feed.borrow_and_update().clone();
        if let Some(frame) = frame
            && !send(socket, frame.to_string()).await
        {
            return false;
        }
    }
    true
}

async fn send(socket: &mut WebSocket, frame: String) -> bool {
    matches!(tokio::time::timeout(SEND_TIMEOUT, socket.send(Message::Text(frame.into()))).await, Ok(Ok(())))
}

/// Runs a call; the reply at once, or later through `replies` for one the
/// show thread answers.
fn call(
    shared: &Arc<Shared>,
    client: &str,
    role: Role,
    text: &str,
    replies: &mpsc::UnboundedSender<String>,
) -> Option<String> {
    let ClientMessage::Call { id, cmd, args } = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(e) => return Some(protocol::reply(0, Err(jq_i18n::t!("remote.bad_call", error = e)))),
    };
    let call = match protocol::decode(role, &cmd, args) {
        Ok(call) => call,
        Err(e) => return Some(protocol::reply(id, Err(e))),
    };
    let done =
        |result: Result<(), String>| Some(protocol::reply(id, result.map(|()| serde_json::Value::Null)));
    match call {
        // Fire-and-forget, as from the app's own buttons.
        Call::Action(action) => done(shared.backend.send(Request::Action(action))),
        Call::Stopwatch(watch, action) => done(shared.backend.send(Request::Stopwatch(watch, action))),
        // Its open pages hear of it through the registry (a `keys` frame).
        Call::SetOwnKeys(keys) => done(shared.registry.set_keys(client, keys)),
        Call::GetLanguage => Some(protocol::reply(id, Ok((shared.language)()))),
        Call::GetShow => {
            let backend = shared.backend.clone();
            let replies = replies.clone();
            tokio::task::spawn_blocking(move || {
                let result = backend.ask_blocking(Request::View, REPLY_TIMEOUT);
                let _ = replies.send(protocol::reply(id, result));
            });
            None
        }
    }
}

/// The device's token from its cookie.
fn token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(COOKIE)?.strip_prefix('='))
        .map(str::to_owned)
}

fn host(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::HOST)?.to_str().ok()
}

/// The request comes from the remote page itself (its `Origin` is this
/// server).
fn same_origin(headers: &HeaderMap) -> bool {
    let origin = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok());
    match (origin, host(headers)) {
        (Some(origin), Some(host)) => origin == format!("http://{host}"),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
