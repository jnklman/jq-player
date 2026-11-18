//! The server on a free port, against a fake show thread: what a device
//! sees and may do in each role.

use std::sync::mpsc as std_mpsc;

use futures_util::{SinkExt, StreamExt};
use jq_core::Action;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

use super::*;

/// A server with a fake show thread, which answers `get_show`, says a
/// preview plays the file at [`preview_file`] and passes on the actions it
/// gets.
struct Fixture {
    server: Server,
    hub: Hub,
    registry: Registry,
    actions: std_mpsc::Receiver<Action>,
}

/// Half a second of sound for the previews, once per test run.
fn preview_file() -> std::path::PathBuf {
    static FILE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    FILE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("jq-remote-server-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::remote::render::tests::write_wav(&dir, 0.5)
    })
    .clone()
}

/// Previews with their files in a folder of the test's own.
fn test_previews() -> Arc<Previews> {
    let dir =
        std::env::temp_dir().join(format!("jq-remote-previews-{}", super::super::clients::random_hex(4)));
    Arc::new(Previews::new(dir))
}

fn fixture() -> Fixture {
    let (tx, rx) = std_mpsc::channel::<Request>();
    let (seen, actions) = std_mpsc::channel();
    std::thread::spawn(move || {
        for request in rx {
            match request {
                Request::View(reply) => {
                    let _ = reply.send(Ok(serde_json::json!({ "revision": 1, "view": null })));
                }
                Request::Action(action) => {
                    let _ = seen.send(action);
                }
                Request::RemotePreview(_, reply) => {
                    let sound = jq_core::PreviewSound {
                        path: preview_file(),
                        start: 0.0,
                        end: None,
                        gain: 1.0,
                        pan: 0.0,
                        rate: 1.0,
                        pitch: 1.0,
                    };
                    let _ = reply.send(Ok((sound, false)));
                }
                _ => {}
            }
        }
    });
    let (hub, registry) = (Hub::default(), Registry::detached());
    let assets: Assets = Arc::new(|path: &str| match path {
        "remote.html" => Some(Asset { bytes: b"<!doctype html>remote".to_vec(), mime: "text/html".into() }),
        "index.html" => Some(Asset { bytes: b"main window".to_vec(), mime: "text/html".into() }),
        _ => None,
    });
    let shared = Shared {
        backend: Backend::new(tx),
        hub: hub.clone(),
        registry: registry.clone(),
        assets,
        instance: "instance".into(),
        language: Arc::new(|| serde_json::json!({ "setting": "auto", "resolved": "en" })),
        previews: test_previews(),
    };
    let server = Server::start(0, Arc::new(shared), |e| panic!("server failed: {e}")).unwrap();
    Fixture { server, hub, registry, actions }
}

struct HttpResponse {
    status: u16,
    head: String,
    body: String,
}

/// A plain HTTP/1.1 request, `Connection: close`.
async fn http(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> HttpResponse {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    // A preview's body is not text; only its header and ASCII parts are looked at.
    let response = String::from_utf8_lossy(&bytes).into_owned();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    HttpResponse { status, head: head.to_owned(), body: body.to_owned() }
}

fn origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Asks to join as `name`; the cookie (`jq_remote=…`) and the device's id.
async fn join(port: u16, name: &str) -> (String, String) {
    let body = serde_json::json!({ "name": name }).to_string();
    let origin = origin(port);
    let headers = [("Origin", origin.as_str()), ("Content-Type", "application/json")];
    let response = http(port, "POST", "/api/join", &headers, &body).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let cookie = response
        .head
        .lines()
        .find_map(|line| line.strip_prefix("set-cookie: "))
        .expect("a cookie")
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let me: serde_json::Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(me["role"], "pending");
    (cookie, me["id"].as_str().unwrap().to_owned())
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(port: u16, cookie: Option<&str>, origin: &str) -> Result<Socket, tungstenite::Error> {
    let mut request = format!("ws://127.0.0.1:{port}/ws").into_client_request().unwrap();
    request.headers_mut().insert("Origin", origin.parse().unwrap());
    if let Some(cookie) = cookie {
        request.headers_mut().insert("Cookie", cookie.parse().unwrap());
    }
    tokio_tungstenite::connect_async(request).await.map(|(socket, _)| socket)
}

/// The next frame other than a ping, as JSON.
async fn next(socket: &mut Socket) -> serde_json::Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("a frame in time")
            .expect("open")
            .expect("no error");
        if let tungstenite::Message::Text(text) = message {
            let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
            if frame["type"] != "ping" {
                return frame;
            }
        }
    }
}

async fn call(socket: &mut Socket, id: u64, cmd: &str, args: serde_json::Value) -> serde_json::Value {
    let frame = serde_json::json!({ "type": "call", "id": id, "cmd": cmd, "args": args });
    socket.send(tungstenite::Message::Text(frame.to_string().into())).await.unwrap();
    loop {
        let frame = next(socket).await;
        if frame["type"] == "reply" {
            assert_eq!(frame["id"], id);
            return frame;
        }
    }
}

#[tokio::test]
async fn a_device_is_let_in_to_watch_then_to_operate_then_removed() {
    let f = fixture();
    let port = f.server.port();
    let (cookie, id) = join(port, "FOH tablet").await;
    let mut socket = connect(port, Some(&cookie), &origin(port)).await.unwrap();
    let hello = next(&mut socket).await;
    assert_eq!(
        (&hello["type"], &hello["role"], &hello["instance"], &hello["client"]),
        (&"hello".into(), &"pending".into(), &"instance".into(), &id.clone().into())
    );
    let reply = call(&mut socket, 1, "get_show", serde_json::json!({})).await;
    assert!(reply["err"].is_string(), "a request sees nothing");

    f.registry.set_role(&id, Role::View).unwrap();
    assert_eq!(next(&mut socket).await, serde_json::json!({ "type": "role", "role": "view" }));
    let reply = call(&mut socket, 2, "get_show", serde_json::json!({})).await;
    assert_eq!(reply["ok"], serde_json::json!({ "revision": 1, "view": null }));
    f.hub.state(&serde_json::json!({ "playhead": null }));
    let event = next(&mut socket).await;
    assert_eq!((&event["name"], &event["payload"]["playhead"]), (&"state".into(), &serde_json::Value::Null));
    let go = serde_json::json!({ "action": { "action": "go" } });
    let reply = call(&mut socket, 3, "action", go.clone()).await;
    assert!(reply["err"].is_string(), "watching only");

    f.registry.set_role(&id, Role::Control).unwrap();
    assert_eq!(next(&mut socket).await["role"], "control");
    let reply = call(&mut socket, 4, "action", go).await;
    assert_eq!(reply["ok"], serde_json::Value::Null);
    assert!(matches!(f.actions.recv_timeout(Duration::from_secs(5)), Ok(Action::Go)));
    assert!(f.actions.try_recv().is_err(), "the refused GO never reached the show thread");
    let edit = call(&mut socket, 5, "edit", serde_json::json!({ "op": { "op": "undo" } })).await;
    assert!(edit["err"].is_string(), "never editing");
    let f1 = serde_json::json!({ "keys": [{ "chord": { "code": "F1" }, "action": "go" }] });
    let reply = call(&mut socket, 6, "set_own_keys", f1).await;
    assert_eq!(reply["ok"], serde_json::Value::Null);
    let keys = next(&mut socket).await;
    assert_eq!((&keys["type"], &keys["keys"][0]["chord"]["code"]), (&"options".into(), &"F1".into()));
    assert_eq!(f.registry.status().clients[0].keys.as_ref().map(Vec::len), Some(1));

    f.registry.remove(&id).unwrap();
    let closed = loop {
        match tokio::time::timeout(Duration::from_secs(5), socket.next()).await.unwrap() {
            Some(Ok(tungstenite::Message::Close(frame))) => break frame,
            Some(Ok(_)) => continue,
            other => panic!("closed without a frame: {other:?}"),
        }
    };
    assert_eq!(u16::from(closed.unwrap().code), CLOSE_REMOVED);
    assert!(connect(port, Some(&cookie), &origin(port)).await.is_err(), "its token is worth nothing now");
}

#[tokio::test]
async fn a_device_previews_a_cue_only_if_the_host_lets_it() {
    let f = fixture();
    let port = f.server.port();
    let (cookie, id) = join(port, "Sound desk").await;
    f.registry.set_role(&id, Role::View).unwrap();
    let mut socket = connect(port, Some(&cookie), &origin(port)).await.unwrap();
    assert_eq!(next(&mut socket).await["preview"], false);
    let cue = serde_json::json!({ "cue": "00000000-0000-0000-0000-000000000000" });
    let reply = call(&mut socket, 1, "local_preview", cue.clone()).await;
    assert!(reply["err"].is_string(), "not without the host's leave");

    f.registry.set_preview(&id, true).unwrap();
    let options = next(&mut socket).await;
    assert_eq!((&options["type"], &options["preview"]), (&"options".into(), &true.into()));
    let reply = call(&mut socket, 2, "local_preview", cue).await;
    let url = reply["ok"]["url"].as_str().expect("a url").to_owned();
    let preview = reply["ok"]["id"].clone();
    let done = loop {
        let event = next(&mut socket).await;
        if event["name"] == "preview_progress" && event["payload"]["done"] == true {
            break event["payload"].clone();
        }
    };
    assert_eq!(done["id"], preview, "the device hears how far its preview got");
    assert!((reply["ok"]["duration"].as_f64().unwrap() - 0.5).abs() < 1e-6);
    // 0.5 s at 48 kHz, 4 bytes a frame, and the header.
    let len = 44 + 24_000 * 4;

    let whole = http(port, "GET", &url, &[("Cookie", &cookie)], "").await;
    assert_eq!(whole.status, 200);
    assert!(whole.head.to_lowercase().contains("content-type: audio/wav"));
    assert!(whole.head.to_lowercase().contains(&format!("content-length: {len}")));
    let part = http(port, "GET", &url, &[("Cookie", &cookie), ("Range", "bytes=0-3")], "").await;
    assert_eq!((part.status, part.body.as_str()), (206, "RIFF"));
    assert!(part.head.to_lowercase().contains(&format!("content-range: bytes 0-3/{len}")));
    let past = http(port, "GET", &url, &[("Cookie", &cookie), ("Range", &format!("bytes={len}-"))], "").await;
    assert_eq!(past.status, 416);

    let (other, _) = join(port, "Phone").await;
    assert_eq!(http(port, "GET", &url, &[("Cookie", &other)], "").await.status, 404, "not another device's");
    assert_eq!(http(port, "GET", &url, &[], "").await.status, 401);
    f.registry.set_preview(&id, false).unwrap();
    assert_eq!(http(port, "GET", &url, &[("Cookie", &cookie)], "").await.status, 404, "leave taken back");
}

#[test]
fn ranges_are_read_as_browsers_send_them() {
    assert_eq!(byte_range("bytes=0-", 100), Some(0..100));
    assert_eq!(byte_range("bytes=10-19", 100), Some(10..20));
    assert_eq!(byte_range("bytes=90-200", 100), Some(90..100));
    assert_eq!(byte_range("bytes=-10", 100), Some(90..100));
    assert_eq!(byte_range("bytes=0-1, 5-6", 100), Some(0..2));
    assert_eq!(byte_range("bytes=100-", 100), None);
    assert_eq!(byte_range("items=0-1", 100), None);
}

#[tokio::test]
async fn only_the_remote_page_with_a_token_may_connect() {
    let f = fixture();
    let port = f.server.port();
    let (cookie, _) = join(port, "Phone").await;
    let refused = |result: Result<Socket, tungstenite::Error>| match result {
        Err(tungstenite::Error::Http(response)) => response.status().as_u16(),
        other => panic!("not refused: {:?}", other.map(|_| ())),
    };
    assert_eq!(refused(connect(port, Some(&cookie), "http://evil.example").await), 403);
    assert_eq!(refused(connect(port, None, &origin(port)).await), 401);
    assert_eq!(refused(connect(port, Some("jq_remote=guessed"), &origin(port)).await), 401);
    let headers = [("Origin", "http://evil.example"), ("Content-Type", "application/json")];
    let foreign = http(port, "POST", "/api/join", &headers, r#"{"name":"x"}"#).await;
    assert_eq!(foreign.status, 403, "no requests from other sites");
}

#[tokio::test]
async fn the_page_is_served_but_not_the_main_window() {
    let f = fixture();
    let port = f.server.port();
    let page = http(port, "GET", "/", &[], "").await;
    assert_eq!((page.status, page.body.as_str()), (200, "<!doctype html>remote"));
    assert!(page.head.to_lowercase().contains("cache-control: no-cache"));
    assert_eq!(http(port, "GET", "/index.html", &[], "").await.status, 404);
    assert_eq!(http(port, "GET", "/assets/../index.html", &[], "").await.status, 404);
    assert_eq!(http(port, "GET", "/assets/missing.js", &[], "").await.status, 404);
    let me = http(port, "GET", "/api/me", &[], "").await;
    assert_eq!(me.body, "null", "unknown without a cookie");
}

#[test]
fn a_taken_port_is_an_error_not_a_crash() {
    let f = fixture();
    let taken = f.server.port();
    let shared = Shared {
        backend: Backend::new(std_mpsc::channel().0),
        hub: Hub::default(),
        registry: Registry::detached(),
        assets: Arc::new(|_: &str| None),
        instance: String::new(),
        language: Arc::new(|| serde_json::Value::Null),
        previews: test_previews(),
    };
    // Windows lets a second socket bind to 0.0.0.0 next to a specific one, but not twice to the same.
    assert!(Server::start(taken, Arc::new(shared), |_| {}).is_err());
}
