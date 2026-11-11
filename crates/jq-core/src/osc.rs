//! OSC over UDP: sending OSC cues and turning incoming messages into the
//! same actions as keyboard and MIDI (`/go`, `/cue/{Q#}/start`, …).

use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use jq_i18n::t;
use jq_show::{OscArg, OscMessage, PositionTo, Show, StopwatchAction};
use rosc::{OscPacket, OscType};

use crate::runner::{Action, Control};
use crate::settings::LocalSettings;

/// How often the listener checks whether it should stop.
const LISTEN_POLL: Duration = Duration::from_millis(200);
/// Largest UDP datagram.
const MAX_PACKET: usize = 65_536;
/// How long a looked-up host name is used before it is looked up again.
const RESOLVE_TTL: Duration = Duration::from_secs(60);

/// Where OSC packets go; a trait so tests can record them.
pub trait OscOut: Send {
    /// `target` is `host:port`.
    fn send(&self, target: &str, packet: &[u8]) -> Result<(), String>;
}

/// Host name → address (or why there is none), and when it was looked up.
type Lookups = Mutex<HashMap<String, (Result<SocketAddr, String>, Instant)>>;

/// Sends from one unbound UDP socket; never waits for the network. Host
/// names are looked up on a thread of their own, as a lookup can take
/// seconds; messages to a name not looked up yet are sent from there, so a
/// failed lookup is reported by the next message to that name.
pub struct UdpOut {
    socket: Arc<UdpSocket>,
    lookups: Arc<Lookups>,
    /// Messages handed to the lookup thread and not sent yet; while there
    /// are any, the next ones queue behind them, keeping their order.
    queued: Arc<AtomicUsize>,
    resolver: Sender<(String, Vec<u8>)>,
}

impl UdpOut {
    pub fn new() -> std::io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        // Some desks listen on the broadcast address.
        socket.set_broadcast(true)?;
        let socket = Arc::new(socket);
        let lookups = Arc::new(Lookups::default());
        let queued = Arc::new(AtomicUsize::new(0));
        let (resolver, requests) = mpsc::channel::<(String, Vec<u8>)>();
        let (out, cache, waiting) = (socket.clone(), lookups.clone(), queued.clone());
        std::thread::Builder::new().name("jq-osc-out".into()).spawn(move || {
            for (target, packet) in requests {
                let addr = lookup(&cache, &target, Instant::now()).unwrap_or_else(|| {
                    let found = resolve(&target);
                    cache
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(target, (found.clone(), Instant::now()));
                    found
                });
                if let Ok(addr) = addr {
                    let _ = out.send_to(&packet, addr);
                }
                waiting.fetch_sub(1, Ordering::AcqRel);
            }
        })?;
        Ok(Self { socket, lookups, queued, resolver })
    }
}

impl OscOut for UdpOut {
    fn send(&self, target: &str, packet: &[u8]) -> Result<(), String> {
        let send = |addr| self.socket.send_to(packet, addr).map(|_| ()).map_err(|e| format!("{target}: {e}"));
        if let Ok(addr) = target.parse::<SocketAddr>() {
            return send(addr);
        }
        if self.queued.load(Ordering::Acquire) == 0
            && let Some(addr) = lookup(&self.lookups, target, Instant::now())
        {
            return send(addr?);
        }
        self.queued.fetch_add(1, Ordering::AcqRel);
        self.resolver.send((target.to_owned(), packet.to_vec())).map_err(|_| {
            self.queued.fetch_sub(1, Ordering::AcqRel);
            t!("osc.sender_stopped", target = target)
        })
    }
}

/// A recent lookup of `target`, if there is one.
fn lookup(lookups: &Lookups, target: &str, now: Instant) -> Option<Result<SocketAddr, String>> {
    let lookups = lookups.lock().unwrap_or_else(PoisonError::into_inner);
    let (addr, at) = lookups.get(target)?;
    (now.duration_since(*at) < RESOLVE_TTL).then(|| addr.clone())
}

/// Looks up `host:port`; the socket sends over IPv4, so that address wins.
fn resolve(target: &str) -> Result<SocketAddr, String> {
    let addrs: Vec<SocketAddr> = target.to_socket_addrs().map_err(|e| format!("{target}: {e}"))?.collect();
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or(addrs.first())
        .copied()
        .ok_or_else(|| t!("osc.address_not_found", target = target))
}

pub fn encode(message: &OscMessage) -> Result<Vec<u8>, String> {
    if !message.address.starts_with('/') {
        return Err(t!("osc.address_slash", address = message.address));
    }
    let args = message
        .args
        .iter()
        .map(|a| match a {
            OscArg::Int(i) => OscType::Int(*i),
            OscArg::Float(f) => OscType::Float(*f),
            OscArg::String(s) => OscType::String(s.clone()),
        })
        .collect();
    let packet = OscPacket::Message(rosc::OscMessage { addr: message.address.clone(), args });
    rosc::encoder::encode(&packet).map_err(|e| format!("OSC: {e}"))
}

/// `host:port` of a show device, with this machine's override from
/// `local.settings`. `None` if the show has no such device.
pub fn device_target(show: &Show, settings: &LocalSettings, device: &str) -> Option<String> {
    if let Some(target) = settings.osc_hosts.get(device) {
        return Some(target.clone());
    }
    let d = show.devices.iter().find(|d| d.id == device)?;
    // IPv6 addresses need brackets before the port.
    Some(if d.host.contains(':') {
        format!("[{}]:{}", d.host, d.port)
    } else {
        format!("{}:{}", d.host, d.port)
    })
}

/// All messages of a packet (bundles flattened) as address and arguments.
pub fn decode(packet: &[u8]) -> Vec<(String, Vec<OscType>)> {
    fn flatten(packet: OscPacket, out: &mut Vec<(String, Vec<OscType>)>) {
        match packet {
            OscPacket::Message(m) => out.push((m.addr, m.args)),
            OscPacket::Bundle(b) => b.content.into_iter().for_each(|p| flatten(p, out)),
        }
    }
    let mut out = Vec::new();
    if let Ok((_, packet)) = rosc::decoder::decode_udp(packet) {
        flatten(packet, &mut out);
    }
    out
}

/// What an incoming message asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    Action(Action),
    Control(Control),
}

/// A number argument (int or float).
fn number(arg: Option<&OscType>) -> Option<f64> {
    match arg? {
        OscType::Int(i) => Some(*i as f64),
        OscType::Long(i) => Some(*i as f64),
        OscType::Float(f) => Some(*f as f64),
        OscType::Double(d) => Some(*d),
        _ => None,
    }
    .filter(|n| n.is_finite())
}

/// Maps an incoming message to an action or a control:
/// `/go`, `/stop`, `/panic`, `/fade`, `/pause`, `/resume`, `/pausetoggle`,
/// `/playhead/next|previous`, `/select/next|previous` (the same),
/// `/cue/{Q#}/start|stop|fade|playhead|select|pause|resume|pausetoggle|restart|exitloop`,
/// `/cue/{Q#}/reset [secs]`, `/cue/{Q#}/volume dB [secs]`,
/// `/cue/{Q#}/volume/relative dB [secs]`, `/cue/{Q#}/pan -1…1 [secs]`,
/// `/cue/{Q#}/position secs`, `/cue/{Q#}/position/relative secs`,
/// `/cue/{Q#}/rate percent [secs]`, `/cue/{Q#}/rate/relative points [secs]`,
/// `/cue/{Q#}/transpose semitones [secs]`,
/// `/cue/{Q#}/mark n`, `/select/{Q#}`, `/stopwatch/{name}/start|stop|reset|restart`.
/// For addresses without a value a first argument of 0 is a button release
/// and is ignored, so a desk button does not fire twice.
pub fn incoming(show: &Show, address: &str, args: &[OscType]) -> Option<Incoming> {
    let parts: Vec<&str> = address.trim_matches('/').split('/').collect();
    let cue = |number: &str| show.cues.iter().find(|c| c.number == number).map(|c| c.id);
    let value = number(args.first());
    let fade = number(args.get(1)).unwrap_or(0.0).max(0.0);
    let control = |c: Control| Some(Incoming::Control(c));
    // Addresses that carry a value, where 0 is one.
    match parts[..] {
        ["cue", n, "volume"] => {
            return control(Control::Volume { cue: cue(n)?, db: value?, fade, relative: false });
        }
        ["cue", n, "volume", "relative"] => {
            return control(Control::Volume { cue: cue(n)?, db: value?, fade, relative: true });
        }
        ["cue", n, "pan"] => {
            return control(Control::Pan { cue: cue(n)?, pan: value?, fade, relative: false });
        }
        ["cue", n, "position"] => {
            return control(Control::Position { cue: cue(n)?, to: PositionTo::Absolute { secs: value? } });
        }
        ["cue", n, "position", "relative"] => {
            return control(Control::Position { cue: cue(n)?, to: PositionTo::Relative { secs: value? } });
        }
        ["cue", n, "mark"] => {
            let index = value.filter(|v| *v >= 1.0)? as usize;
            return control(Control::Position { cue: cue(n)?, to: PositionTo::Mark { index } });
        }
        ["cue", n, "reset"] => {
            return control(Control::Reset { cue: cue(n)?, fade: value.unwrap_or(0.0).max(0.0) });
        }
        ["cue", n, "rate"] => {
            let rate = Some(value?);
            return control(Control::Rate { cue: cue(n)?, rate, transpose: None, relative: false, fade });
        }
        ["cue", n, "rate", "relative"] => {
            let rate = Some(value?);
            return control(Control::Rate { cue: cue(n)?, rate, transpose: None, relative: true, fade });
        }
        ["cue", n, "transpose"] => {
            let transpose = Some(value?);
            return control(Control::Rate { cue: cue(n)?, rate: None, transpose, relative: false, fade });
        }
        _ => {}
    }
    let released = match args.first() {
        Some(OscType::Int(0) | OscType::Long(0) | OscType::Bool(false)) => true,
        Some(OscType::Float(f)) => *f == 0.0,
        Some(OscType::Double(d)) => *d == 0.0,
        _ => false,
    };
    if released {
        return None;
    }
    let action = match parts[..] {
        ["go"] => Action::Go,
        ["stop"] => Action::StopAll,
        ["panic"] => Action::Panic,
        ["fade"] => Action::FadeAll,
        ["pause"] => Action::PauseAll,
        ["resume"] => Action::ResumeAll,
        ["pausetoggle"] => Action::TogglePauseAll,
        ["playhead" | "select", "next"] => Action::PlayheadNext,
        ["playhead" | "select", "previous" | "prev"] => Action::PlayheadPrev,
        ["select", n] => Action::SetPlayhead(cue(n)?),
        ["stopwatch", name, verb] => {
            let action = match verb {
                "start" => StopwatchAction::Start,
                "stop" => StopwatchAction::Stop,
                "reset" => StopwatchAction::Reset,
                "restart" => StopwatchAction::Restart,
                _ => return None,
            };
            let known = show.stopwatches.iter().any(|w| w.id == name || w.name.eq_ignore_ascii_case(name));
            return known.then(|| Incoming::Control(Control::Stopwatch { watch: name.into(), action }));
        }
        ["cue", n, verb] => {
            let id = cue(n)?;
            match verb {
                "start" | "go" => Action::StartCue(id),
                "stop" => Action::StopCue(id),
                "fade" => Action::FadeCue(id),
                "playhead" | "select" => Action::SetPlayhead(id),
                "pause" => Action::PauseCue(id),
                "resume" => Action::ResumeCue(id),
                "pausetoggle" => Action::TogglePauseCue(id),
                "restart" => Action::RestartCue(id),
                "exitloop" | "devamp" => Action::ExitLoopCue(id),
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(Incoming::Action(action))
}

/// Receives OSC on a UDP port in its own thread; dropping it stops the thread.
pub struct OscListener {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    port: u16,
}

impl OscListener {
    pub fn start(port: u16, mut on_packet: impl FnMut(&[u8]) + Send + 'static) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", port))?;
        socket.set_read_timeout(Some(LISTEN_POLL))?;
        let port = socket.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::Builder::new().name("jq-osc-in".into()).spawn(move || {
            let mut buf = vec![0; MAX_PACKET];
            while !flag.load(Ordering::Relaxed) {
                if let Ok((len, _)) = socket.recv_from(&mut buf) {
                    on_packet(&buf[..len]);
                }
            }
        })?;
        Ok(Self { stop, handle: Some(handle), port })
    }

    /// The port actually bound (useful when started on port 0).
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for OscListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use jq_show::{Cue, CueKind, NetDevice, WaitCue};

    use super::*;

    fn show_with(numbers: &[&str]) -> Show {
        let mut show = Show::default();
        for n in numbers {
            let mut cue = Cue::new(CueKind::Wait(WaitCue { duration: 1.0 }));
            cue.number = (*n).into();
            show.cues.push(cue);
        }
        show
    }

    #[test]
    fn maps_incoming_addresses() {
        let show = show_with(&["1", "12.5"]);
        let id = show.cues[1].id;
        let action = |address: &str, args: &[OscType]| match incoming(&show, address, args) {
            Some(Incoming::Action(a)) => Some(a),
            None => None,
            other => panic!("{other:?}"),
        };
        assert_eq!(action("/go", &[]), Some(Action::Go));
        assert_eq!(action("/go", &[OscType::Float(1.0)]), Some(Action::Go));
        assert_eq!(action("/go", &[OscType::Float(0.0)]), None, "button release");
        assert_eq!(action("/go", &[OscType::Int(0)]), None);
        assert_eq!(action("/stop", &[]), Some(Action::StopAll));
        assert_eq!(action("/panic", &[]), Some(Action::Panic));
        assert_eq!(action("/fade", &[]), Some(Action::FadeAll));
        assert_eq!(action("/pause", &[]), Some(Action::PauseAll));
        assert_eq!(action("/resume", &[]), Some(Action::ResumeAll));
        assert_eq!(action("/pausetoggle", &[]), Some(Action::TogglePauseAll));
        assert_eq!(action("/cue/12.5/pause", &[]), Some(Action::PauseCue(id)));
        assert_eq!(action("/cue/12.5/resume", &[]), Some(Action::ResumeCue(id)));
        assert_eq!(action("/cue/12.5/pausetoggle", &[]), Some(Action::TogglePauseCue(id)));
        assert_eq!(action("/playhead/next", &[]), Some(Action::PlayheadNext));
        assert_eq!(action("/playhead/previous", &[]), Some(Action::PlayheadPrev));
        assert_eq!(action("/cue/12.5/start", &[]), Some(Action::StartCue(id)));
        assert_eq!(action("/cue/12.5/stop", &[]), Some(Action::StopCue(id)));
        assert_eq!(action("/cue/12.5/playhead", &[]), Some(Action::SetPlayhead(id)));
        assert_eq!(action("/cue/99/start", &[]), None, "unknown cue");
        assert_eq!(action("/unknown", &[]), None);
        assert_eq!(action("/cue/12.5/restart", &[]), Some(Action::RestartCue(id)));
        assert_eq!(action("/cue/12.5/exitloop", &[]), Some(Action::ExitLoopCue(id)));
        assert_eq!(action("/select/next", &[]), Some(Action::PlayheadNext));
        assert_eq!(action("/select/previous", &[]), Some(Action::PlayheadPrev));
        assert_eq!(action("/select/12.5", &[]), Some(Action::SetPlayhead(id)));
    }

    #[test]
    fn maps_addresses_with_values() {
        let mut show = show_with(&["1", "12.5"]);
        show.stopwatches.push(jq_show::Stopwatch {
            id: "w1".into(),
            name: "Vorstellung".into(),
            countdown: None,
        });
        let id = show.cues[1].id;
        let control = |address: &str, args: &[OscType]| match incoming(&show, address, args) {
            Some(Incoming::Control(c)) => Some(c),
            None => None,
            other => panic!("{other:?}"),
        };
        let (f, i) = (OscType::Float, OscType::Int);
        assert_eq!(
            control("/cue/12.5/volume", &[f(-6.0)]),
            Some(Control::Volume { cue: id, db: -6.0, fade: 0.0, relative: false })
        );
        assert_eq!(
            control("/cue/12.5/volume", &[i(0), f(2.5)]),
            Some(Control::Volume { cue: id, db: 0.0, fade: 2.5, relative: false }),
            "0 is a value here, not a release"
        );
        assert_eq!(
            control("/cue/12.5/volume/relative", &[i(-3)]),
            Some(Control::Volume { cue: id, db: -3.0, fade: 0.0, relative: true })
        );
        assert_eq!(control("/cue/12.5/volume", &[]), None, "no value");
        assert_eq!(
            control("/cue/12.5/pan", &[f(0.0), f(1.0)]),
            Some(Control::Pan { cue: id, pan: 0.0, fade: 1.0, relative: false })
        );
        assert_eq!(
            control("/cue/12.5/position", &[f(30.0)]),
            Some(Control::Position { cue: id, to: PositionTo::Absolute { secs: 30.0 } })
        );
        assert_eq!(
            control("/cue/12.5/position/relative", &[f(-5.0)]),
            Some(Control::Position { cue: id, to: PositionTo::Relative { secs: -5.0 } })
        );
        assert_eq!(
            control("/cue/12.5/mark", &[i(2)]),
            Some(Control::Position { cue: id, to: PositionTo::Mark { index: 2 } })
        );
        assert_eq!(control("/cue/12.5/mark", &[i(0)]), None);
        assert_eq!(control("/cue/12.5/reset", &[]), Some(Control::Reset { cue: id, fade: 0.0 }));
        assert_eq!(control("/cue/12.5/reset", &[f(2.0)]), Some(Control::Reset { cue: id, fade: 2.0 }));
        assert_eq!(
            control("/cue/12.5/rate", &[i(80)]),
            Some(Control::Rate { cue: id, rate: Some(80.0), transpose: None, relative: false, fade: 0.0 })
        );
        assert_eq!(
            control("/cue/12.5/rate/relative", &[f(-10.0), f(2.0)]),
            Some(Control::Rate { cue: id, rate: Some(-10.0), transpose: None, relative: true, fade: 2.0 })
        );
        assert_eq!(
            control("/cue/12.5/transpose", &[i(0), i(1)]),
            Some(Control::Rate { cue: id, rate: None, transpose: Some(0.0), relative: false, fade: 1.0 }),
            "0 is a value here"
        );
        assert_eq!(control("/cue/12.5/rate", &[]), None, "no value");
        assert_eq!(control("/cue/99/rate", &[f(80.0)]), None, "unknown cue");
        assert_eq!(
            control("/stopwatch/vorstellung/start", &[]),
            Some(Control::Stopwatch { watch: "vorstellung".into(), action: StopwatchAction::Start })
        );
        assert_eq!(control("/stopwatch/w1/restart", &[f(0.0)]), None, "button release");
        assert_eq!(control("/stopwatch/pause/start", &[]), None, "unknown stopwatch");
        assert_eq!(control("/cue/99/volume", &[f(-6.0)]), None, "unknown cue");
    }

    #[test]
    fn encodes_and_decodes_messages() {
        let message = OscMessage {
            address: "/pb/1/go".into(),
            args: vec![OscArg::Int(1), OscArg::Float(0.5), OscArg::String("x".into())],
        };
        let packet = encode(&message).unwrap();
        let decoded = decode(&packet);
        assert_eq!(
            decoded,
            [(
                "/pb/1/go".to_string(),
                vec![OscType::Int(1), OscType::Float(0.5), OscType::String("x".into())]
            )]
        );
        assert!(encode(&OscMessage { address: "pb".into(), args: vec![] }).is_err());
        assert!(decode(b"not osc").is_empty());
    }

    #[test]
    fn device_target_prefers_the_local_override() {
        let mut show = Show::default();
        show.devices.push(NetDevice {
            id: "lx".into(),
            name: "Licht".into(),
            host: "10.0.0.5".into(),
            port: 8000,
        });
        show.devices.push(NetDevice { id: "v6".into(), name: "V6".into(), host: "::1".into(), port: 9000 });
        let mut settings = LocalSettings::default();
        assert_eq!(device_target(&show, &settings, "lx").as_deref(), Some("10.0.0.5:8000"));
        assert_eq!(device_target(&show, &settings, "v6").as_deref(), Some("[::1]:9000"));
        assert_eq!(device_target(&show, &settings, "nope"), None);
        settings.osc_hosts.insert("lx".into(), "192.168.1.20:8000".into());
        assert_eq!(device_target(&show, &settings, "lx").as_deref(), Some("192.168.1.20:8000"));
    }

    #[test]
    fn udp_round_trip_through_the_listener() {
        let (tx, rx) = mpsc::channel();
        let listener = OscListener::start(0, move |p| tx.send(p.to_vec()).unwrap()).unwrap();
        let out = UdpOut::new().unwrap();
        let packet = encode(&OscMessage { address: "/go".into(), args: vec![] }).unwrap();
        out.send(&format!("127.0.0.1:{}", listener.port()), &packet).unwrap();
        let received = rx.recv_timeout(Duration::from_secs(2)).expect("packet arrives");
        assert_eq!(received, packet);
        // By name: looked up off the show thread, then sent from there.
        out.send(&format!("localhost:{}", listener.port()), &packet).unwrap();
        let received = rx.recv_timeout(Duration::from_secs(2)).expect("packet arrives by name");
        assert_eq!(decode(&received)[0].0, "/go");
        drop(listener);
    }
}
