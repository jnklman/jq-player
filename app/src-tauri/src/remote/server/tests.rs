//! The server on a free port, against a fake show thread: what a device
//! sees and may do in each role.

use std::sync::mpsc as std_mpsc;

use futures_util::{SinkExt, StreamExt};
use jq_core::Action;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

use super::*;

/// A server with a fake show thread, which answers `get_show` and passes
/// on the actions it gets.
struct Fixture {
    server: Server,
    hub: Hub,
    registry: Registry,
    actions: std_mpsc::Receiver<Action>,
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
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
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
    };
    // Windows lets a second socket bind to 0.0.0.0 next to a specific one, but not twice to the same.
    assert!(Server::start(taken, Arc::new(shared), |_| {}).is_err());
}
