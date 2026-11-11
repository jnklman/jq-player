//! Device output: opens a cpal stream and runs the engine's mixer in it.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, DeviceDirection, DeviceId, FromSample, Host, HostId, SampleFormat, SizedSample,
    StreamConfig, SupportedBufferSize,
};
use jq_engine::{Controller, EngineConfig, Mixer};
use jq_i18n::t;
use serde::Serialize;

#[cfg(target_os = "linux")]
mod pipewire_default;
mod realtime;

/// Largest callback buffer the scratch space is sized for (frames).
const MAX_CALLBACK_FRAMES: usize = 8192;
/// Stream errors kept for display.
const MAX_ERRORS: usize = 32;
/// Callback buffer for ALSA and PulseAudio when none is set (frames, ~21 ms
/// at 48 kHz; cpal double-buffers it). Left to them, PulseAudio queues about
/// 2 s, and ALSA takes the card's largest buffer, which some cannot allocate
/// ("Out of memory" from `snd_pcm_hw_params`, e.g. 4 MB on a CA0132).
const DEFAULT_BUFFER_FRAMES: u32 = 1024;
/// How often the supervisor looks at the stream.
const CHECK_INTERVAL: Duration = Duration::from_millis(50);
/// A stream whose callback has not run for this long counts as lost (the
/// largest callback buffer lasts under 200 ms).
const STALL: Duration = Duration::from_secs(1);
/// A stream that has not called back at all after this long never will.
const START_TIMEOUT: Duration = Duration::from_secs(10);
/// Pause between attempts to open a lost device again.
const RETRY_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub enum AudioError {
    HostUnavailable { name: String, available: String },
    NoDefaultDevice,
    DeviceNotFound(String),
    UnsupportedFormat(String),
    Cpal(cpal::Error),
    Stopped,
    Channels,
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::HostUnavailable { name, available } => {
                t!("audio.host_unavailable", name = name, available = available)
            }
            Self::NoDefaultDevice => t!("audio.no_default_device"),
            Self::DeviceNotFound(name) => t!("audio.device_not_found", name = name),
            Self::UnsupportedFormat(format) => t!("audio.unsupported_format", format = format),
            Self::Cpal(e) => t!("audio.cpal", error = e),
            Self::Stopped => t!("audio.stopped"),
            Self::Channels => t!("audio.channels"),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for AudioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Cpal(e) => Some(e),
            _ => None,
        }
    }
}

impl From<cpal::Error> for AudioError {
    fn from(e: cpal::Error) -> Self {
        Self::Cpal(e)
    }
}

/// Which output to open; every field falls back to the system default.
#[derive(Clone, Debug, Default)]
pub struct OutputSpec {
    /// Host name, e.g. "ALSA", "JACK", "WASAPI", "ASIO".
    pub host: Option<String>,
    /// Part of the device's description, or its exact id.
    pub device: Option<String>,
    pub channels: Option<u16>,
    /// Sample rate to run at instead of the device default (e.g. to match
    /// another output whose buffers are shared).
    pub sample_rate: Option<u32>,
    /// Fixed callback buffer size in frames.
    pub buffer_frames: Option<u32>,
    /// Channels the outputs use. With `channels` unset, the stream gets at
    /// least that many if the device takes them (the default is often 2).
    pub min_channels: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OutputInfo {
    pub host: String,
    /// Short device name for display.
    pub device: String,
    /// Full description (manufacturer, type, connection) for tooltips.
    pub device_detail: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// Most channels the device takes; `None` if it cannot tell.
    pub max_channels: Option<u16>,
    pub sample_format: String,
    /// Fixed callback buffer in frames; `None` = the host's default.
    pub buffer_frames: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    /// Short name for display.
    pub name: String,
    /// Full description (manufacturer, type, connection).
    pub description: String,
    /// Default config, if the device can be opened for output.
    pub default_config: Option<String>,
    /// Most channels the device takes; `None` if it cannot tell.
    pub max_channels: Option<u16>,
    /// Smallest and largest callback buffer in frames, if the host tells.
    pub buffer_range: Option<(u32, u32)>,
}

/// A device as [`AudioOutput::open`] would pick it, without opening it.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceProbe {
    /// Short name for messages.
    pub name: String,
    /// Most channels the device takes; `None` if it cannot tell.
    pub max_channels: Option<u16>,
}

/// A running output. Dropping it stops the audio.
///
/// A device output is run by a supervisor thread that owns the cpal stream.
/// If the stream fails (device unplugged, driver reset, sample rate or
/// buffer size changed, callbacks stalled), the mixer moves to a silent
/// stand-in, so cues go on and commands are taken, and the device is opened
/// again with the same sample rate and channels as soon as it can be; the
/// mixer then moves back.
pub struct AudioOutput {
    _backend: Backend,
    info: OutputInfo,
    shared: Arc<Shared>,
    /// Opened with a set channel count rather than one fitted to the outputs.
    fixed_channels: bool,
}

/// Held only to keep the supervisor or thread alive.
#[allow(dead_code)]
enum Backend {
    Device(Supervisor),
    Null(NullThread),
}

/// What the supervisor shares with the [`AudioOutput`] handle.
#[derive(Default)]
struct Shared {
    errors: Mutex<Vec<String>>,
    /// Why the device is lost, while it is.
    lost: Mutex<Option<String>>,
    /// Frames of the last callback; 0 before the first.
    callback_frames: AtomicU32,
}

/// The mixer, wherever it runs: a stream callback or the null thread lends
/// it, and dropping them (with the stream or thread) puts it back home.
type Home = Arc<Mutex<Option<Mixer>>>;

struct Lent {
    mixer: Option<Mixer>,
    home: Home,
}

impl Lent {
    fn take(home: &Home) -> Option<Self> {
        let mixer = home.lock().unwrap_or_else(PoisonError::into_inner).take()?;
        Some(Self { mixer: Some(mixer), home: home.clone() })
    }

    fn process(&mut self, out: &mut [f32]) {
        if let Some(mixer) = &mut self.mixer {
            mixer.process(out);
        }
    }
}

impl Drop for Lent {
    // Runs when the stream is torn down, never inside a callback.
    fn drop(&mut self) {
        if let Some(mixer) = self.mixer.take() {
            *self.home.lock().unwrap_or_else(PoisonError::into_inner) = Some(mixer);
        }
    }
}

/// What a stream's callbacks tell the supervisor; atomics only.
#[derive(Default)]
struct Pulse {
    /// Data callbacks so far.
    beats: AtomicU64,
    /// Fatal error seen, see [`failure_code`]; 0 while fine.
    failure: AtomicU8,
}

/// Errors after which a stream plays no more and must be built again.
fn failure_code(kind: cpal::ErrorKind) -> u8 {
    match kind {
        cpal::ErrorKind::DeviceNotAvailable => 1,
        cpal::ErrorKind::StreamInvalidated => 2,
        cpal::ErrorKind::HostUnavailable => 3,
        _ => 0,
    }
}

fn failure_text(code: u8) -> String {
    match code {
        1 => t!("audio.failure.device_lost"),
        2 => t!("audio.failure.stream_invalidated"),
        3 => t!("audio.failure.host_lost"),
        _ => t!("audio.failure.silent"),
    }
}

/// Handle of the supervisor thread; dropping it stops the thread and the
/// stream with it.
struct Supervisor {
    stop: Option<Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// A device resolved and its stream config chosen, not opened yet.
struct Prepared {
    device: Device,
    config: StreamConfig,
    format: SampleFormat,
    info: OutputInfo,
    listing: DeviceInfo,
}

/// A playing stream.
struct Up {
    /// Held only to keep it playing.
    _stream: cpal::Stream,
    watch: Watch,
    _held: Held,
}

/// Devices our streams have open. ASIO loads one driver at a time, so while
/// a stream holds one, cpal cannot list it reliably (it stops at the first
/// other driver it fails to load); [`list_devices`] adds them from here.
static HELD: Mutex<Vec<DeviceInfo>> = Mutex::new(Vec::new());

/// Keeps a device in [`HELD`] while its stream lives.
struct Held(String);

impl Held {
    fn new(info: DeviceInfo) -> Self {
        let id = info.id.clone();
        HELD.lock().unwrap_or_else(PoisonError::into_inner).push(info);
        Self(id)
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let mut held = HELD.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(i) = held.iter().position(|info| info.id == self.0) {
            held.remove(i);
        }
    }
}

/// What the supervisor last saw of a stream.
struct Watch {
    pulse: Arc<Pulse>,
    beats: u64,
    since: Instant,
}

impl Watch {
    fn new(pulse: Arc<Pulse>, now: Instant) -> Self {
        Self { pulse, beats: 0, since: now }
    }

    /// Why the stream no longer plays, if it does not. Stalled callbacks
    /// count once it has played; before that, as some devices take long to
    /// start, only after [`START_TIMEOUT`]. The answer is a [`failure_code`].
    fn failure(&mut self, now: Instant) -> Option<u8> {
        let code = self.pulse.failure.load(Ordering::Relaxed);
        if code != 0 {
            return Some(code);
        }
        let beats = self.pulse.beats.load(Ordering::Relaxed);
        let limit = if beats > 0 { STALL } else { START_TIMEOUT };
        if beats != self.beats {
            (self.beats, self.since) = (beats, now);
        } else if now.duration_since(self.since) > limit {
            return Some(0);
        }
        None
    }
}

enum Link {
    Up(Up),
    /// The mixer runs silently; the device is tried again at `retry`.
    Down {
        null: NullThread,
        reason: u8,
        retry: Instant,
    },
    /// The mixer has not come back from a stream yet: some hosts drop the
    /// callback (and the mixer with it) only after the stream is gone.
    Gone {
        reason: u8,
    },
}

/// Runs the mixer at real-time pace into nowhere.
struct NullThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl NullThread {
    fn start(mut lent: Lent, sample_rate: u32, channels: u16) -> Self {
        const BLOCK: usize = 480;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::Builder::new()
            .name("jq-null-output".into())
            .spawn(move || {
                let mut buf = vec![0.0f32; BLOCK * channels as usize];
                let block = Duration::from_secs_f64(BLOCK as f64 / sample_rate as f64);
                let mut next = Instant::now();
                while !flag.load(Ordering::Relaxed) {
                    lent.process(&mut buf);
                    next += block;
                    std::thread::sleep(next.saturating_duration_since(Instant::now()));
                }
            })
            .expect("spawn null output thread");
        Self { stop, handle: Some(handle) }
    }
}

impl Drop for NullThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl AudioOutput {
    /// Opens the device and starts the stream with a fresh engine; returns
    /// the output and the engine's controller.
    pub fn open(spec: &OutputSpec, max_voices: usize) -> Result<(Self, Controller), AudioError> {
        realtime::keep_full_speed();
        let shared = Arc::new(Shared::default());
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (stop_tx, stop_rx) = mpsc::channel();
        let (spec_owned, thread_shared) = (spec.clone(), shared.clone());
        let handle = std::thread::Builder::new()
            .name("jq-audio-output".into())
            .spawn(move || supervise(spec_owned, max_voices, thread_shared, ready_tx, stop_rx))
            .expect("spawn audio output thread");
        let supervisor = Supervisor { stop: Some(stop_tx), handle: Some(handle) };
        let (info, controller) = ready_rx.recv().unwrap_or(Err(AudioError::Stopped))?;
        let fixed_channels = spec.channels.is_some();
        Ok((Self { _backend: Backend::Device(supervisor), info, shared, fixed_channels }, controller))
    }

    /// The device `open` would use and how many channels it takes. Slow on
    /// ALSA (lists the cards), so only call when the device changes.
    pub fn probe(spec: &OutputSpec) -> Result<DeviceProbe, AudioError> {
        let (host, device) = resolve(spec)?;
        Ok(DeviceProbe { name: describe(&device).0, max_channels: capacity(&host, &device) })
    }

    /// A silent output that consumes audio in real time, so cues run and end
    /// as usual. Used when no device can be opened.
    pub fn null(sample_rate: u32, channels: u16, max_voices: usize) -> (Self, Controller) {
        let (controller, mixer) = jq_engine::engine(EngineConfig {
            max_voices,
            ..EngineConfig::measured(sample_rate, channels as usize)
        });
        let lent = Lent { mixer: Some(mixer), home: Home::default() };
        let info = OutputInfo {
            host: "-".into(),
            device: t!("audio.silent.device"),
            device_detail: t!("audio.silent.detail"),
            sample_rate,
            channels,
            max_channels: None,
            sample_format: "F32".into(),
            buffer_frames: None,
        };
        let backend = Backend::Null(NullThread::start(lent, sample_rate, channels));
        (Self { _backend: backend, info, shared: Arc::default(), fixed_channels: true }, controller)
    }

    pub fn info(&self) -> &OutputInfo {
        &self.info
    }

    /// Puts the texts in `info` that the program wrote itself (the silent output, PipeWire's default device)
    /// into the current language; returns whether anything changed. Device names from the system stay.
    pub fn relocalize(&mut self) -> bool {
        let mut changed = false;
        for (device, detail) in [
            ("audio.silent.device", "audio.silent.detail"),
            ("audio.default_device", "audio.default_device_detail"),
        ] {
            let is = |text: &str, key: &str| {
                jq_i18n::Lang::ALL.iter().any(|l| jq_i18n::translate_in(*l, key, &[]) == text)
            };
            if is(&self.info.device, device) {
                let (device, detail) = (jq_i18n::translate(device, &[]), jq_i18n::translate(detail, &[]));
                changed = device != self.info.device || detail != self.info.device_detail;
                (self.info.device, self.info.device_detail) = (device, detail);
                break;
            }
        }
        changed
    }

    /// Why the device is lost while the output runs silently and tries to
    /// open it again; `None` while it plays.
    /// Frames per callback as the device runs it; `None` before the first
    /// callback (and for the null output).
    pub fn callback_frames(&self) -> Option<u32> {
        Some(self.shared.callback_frames.load(Ordering::Relaxed)).filter(|&f| f > 0)
    }

    pub fn lost(&self) -> Option<String> {
        self.shared.lost.lock().ok()?.clone()
    }

    /// True if the outputs now need more channels than the stream has and
    /// opening it again would provide them.
    pub fn needs_channels(&self, min_channels: u16) -> bool {
        !self.fixed_channels
            && min_channels > self.info.channels
            && self.info.max_channels.is_none_or(|max| min_channels <= max)
    }

    /// Stream errors (xruns, device lost) since the last call.
    pub fn take_errors(&self) -> Vec<String> {
        self.shared.errors.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default()
    }
}

/// The supervisor thread: opens the stream, reports back through `ready`,
/// then watches it until `stop` is dropped.
fn supervise(
    spec: OutputSpec,
    max_voices: usize,
    shared: Arc<Shared>,
    ready: SyncSender<Result<(OutputInfo, Controller), AudioError>>,
    stop: Receiver<()>,
) {
    let opened = prepare(&spec).and_then(|prepared| {
        let config = &prepared.config;
        let (controller, mixer) = jq_engine::engine(EngineConfig {
            max_voices,
            ..EngineConfig::measured(config.sample_rate, config.channels as usize)
        });
        let home = Home::default();
        let up = start(&prepared, Lent { mixer: Some(mixer), home: home.clone() }, &shared)?;
        Ok((prepared.info, controller, home, up))
    });
    let (info, home, up) = match opened {
        Ok((info, controller, home, up)) => {
            if ready.send(Ok((info.clone(), controller))).is_err() {
                return;
            }
            (info, home, up)
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    // The engine keeps its sample rate and channels, so the stream must too.
    let pinned = OutputSpec {
        host: Some(info.host.clone()),
        channels: Some(info.channels),
        sample_rate: Some(info.sample_rate),
        ..spec
    };
    // The mixer goes to the null thread; with a failed attempt to open the
    // device again, the reason says why.
    let fall_back = |reason: u8, attempt: Option<AudioError>| {
        let text = failure_text(reason);
        let Some(lent) = Lent::take(&home) else {
            set_lost(&shared, Some(t!("audio.lost.recovering", reason = text)));
            return Link::Gone { reason };
        };
        set_lost(
            &shared,
            Some(match attempt {
                Some(e) => t!("audio.lost.last_attempt", reason = text, error = e),
                None => text,
            }),
        );
        let null = NullThread::start(lent, info.sample_rate, info.channels);
        Link::Down { null, reason, retry: Instant::now() + RETRY_INTERVAL }
    };
    let mut link = Link::Up(up);
    while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(CHECK_INTERVAL) {
        let now = Instant::now();
        link = match link {
            Link::Up(mut up) => match up.watch.failure(now) {
                None => Link::Up(up),
                Some(reason) => {
                    drop(up);
                    fall_back(reason, None)
                }
            },
            // Resolve first, so the mixer stays on the null thread while the
            // device is absent.
            Link::Down { null, reason, retry } if now >= retry => match prepare(&pinned) {
                Err(_) => Link::Down { null, reason, retry: now + RETRY_INTERVAL },
                Ok(prepared) => {
                    drop(null);
                    match Lent::take(&home).map(|lent| start(&prepared, lent, &shared)) {
                        Some(Ok(up)) => {
                            set_lost(&shared, None);
                            Link::Up(up)
                        }
                        // A failed start drops the lent mixer, so it is home again.
                        Some(Err(e)) => fall_back(reason, Some(e)),
                        None => fall_back(reason, None),
                    }
                }
            },
            Link::Gone { reason } => fall_back(reason, None),
            link => link,
        };
    }
}

fn set_lost(shared: &Shared, reason: Option<String>) {
    if let Ok(mut lost) = shared.lost.lock() {
        *lost = reason;
    }
}

/// Resolves the device and chooses the stream config.
fn prepare(spec: &OutputSpec) -> Result<Prepared, AudioError> {
    let (host, device) = resolve(spec)?;
    let default = device.default_output_config()?;
    let format = default.sample_format();
    if !matches!(format, SampleFormat::F32 | SampleFormat::I16 | SampleFormat::I32 | SampleFormat::U16) {
        return Err(AudioError::UnsupportedFormat(format!("{format:?}")));
    }
    let max_channels = capacity(&host, &device);
    if spec.channels.is_some_and(|c| c < 2) {
        return Err(AudioError::Channels);
    }
    let config = StreamConfig {
        channels: spec.channels.unwrap_or_else(|| {
            stream_channels(&host, &device, default.channels(), spec.min_channels, max_channels)
        }),
        sample_rate: spec.sample_rate.unwrap_or(default.sample_rate()),
        buffer_size: match spec.buffer_frames {
            // The JACK server sets the size for all clients; cpal refuses
            // any other, which would leave the show without a device.
            Some(_) if host.id().name() == "JACK" => BufferSize::Default,
            Some(frames) => BufferSize::Fixed(frames),
            None if matches!(host.id().name(), "ALSA" | "PulseAudio") => {
                BufferSize::Fixed(DEFAULT_BUFFER_FRAMES)
            }
            None => BufferSize::Default,
        },
    };
    // The callback splits the device buffer into frames of this many channels.
    if config.channels == 0 {
        return Err(AudioError::Channels);
    }
    let listing = device_info(&device, max_channels);
    let (name, detail) = describe(&device);
    let info = OutputInfo {
        host: host.id().name().to_string(),
        device: name,
        device_detail: detail,
        sample_rate: config.sample_rate,
        channels: config.channels,
        max_channels,
        sample_format: format!("{format:?}"),
        buffer_frames: match config.buffer_size {
            BufferSize::Fixed(frames) => Some(frames),
            BufferSize::Default => None,
        },
    };
    Ok(Prepared { device, config, format, info, listing })
}

/// Builds and plays the stream with the lent mixer. On failure the mixer
/// is home again.
fn start(prepared: &Prepared, lent: Lent, shared: &Arc<Shared>) -> Result<Up, AudioError> {
    let Prepared { device, config, .. } = prepared;
    let pulse = Arc::new(Pulse::default());
    let (shared, p) = (shared.clone(), pulse.clone());
    // ASIO drivers run their callback threads themselves.
    let pro_audio = prepared.info.host == "WASAPI";
    let stream = match prepared.format {
        SampleFormat::F32 => build_stream::<f32>(device, config, lent, shared, p, pro_audio)?,
        SampleFormat::I16 => build_stream::<i16>(device, config, lent, shared, p, pro_audio)?,
        SampleFormat::I32 => build_stream::<i32>(device, config, lent, shared, p, pro_audio)?,
        SampleFormat::U16 => build_stream::<u16>(device, config, lent, shared, p, pro_audio)?,
        f => return Err(AudioError::UnsupportedFormat(format!("{f:?}"))),
    };
    stream.play()?;
    let held = Held::new(prepared.listing.clone());
    Ok(Up { _stream: stream, watch: Watch::new(pulse, Instant::now()), _held: held })
}

pub fn host_names() -> Vec<String> {
    hosts().iter().map(|h| h.name().to_string()).collect()
}

pub fn default_host_name() -> String {
    cpal::default_host().id().name().to_string()
}

/// cpal's available hosts, without
/// - JACK when no server answers: cpal always reports it, and choosing it
///   then only leads to an empty device list;
/// - PulseAudio next to PipeWire: it is pipewire-pulse then, and cpal gives
///   its streams a fixed surround channel map, so on a card with `aux`
///   channels (pro profiles) the server mixes everything down to 1/2.
fn hosts() -> Vec<HostId> {
    let available = cpal::available_hosts();
    let pipewire = available.iter().any(|h| h.name() == "PipeWire");
    available
        .into_iter()
        .filter(|&id| match id.name() {
            "JACK" => cpal::host_from_id(id).is_ok_and(|h| h.devices().is_ok_and(|mut d| d.next().is_some())),
            "PulseAudio" => !pipewire,
            _ => true,
        })
        .collect()
}

pub fn list_devices(host: Option<&str>) -> Result<Vec<DeviceInfo>, AudioError> {
    let host = pick_host(host)?;
    let mut devices: Vec<DeviceInfo> = listed_devices(&host)?
        .into_iter()
        .filter(DeviceTrait::supports_output)
        .map(|d| {
            let max_channels = capacity(&host, &d);
            device_info(&d, max_channels)
        })
        .collect();
    let held = HELD.lock().unwrap_or_else(PoisonError::into_inner);
    for info in held.iter() {
        let ours = info.id.parse::<DeviceId>().is_ok_and(|id| id.host() == host.id());
        if ours && !devices.iter().any(|d| d.id == info.id) {
            devices.push(info.clone());
        }
    }
    Ok(devices)
}

fn device_info(device: &Device, max_channels: Option<u16>) -> DeviceInfo {
    let (name, description) = describe(device);
    DeviceInfo {
        id: device_id(device),
        name,
        description,
        default_config: device.default_output_config().ok().map(|c| {
            t!(
                "audio.default_config",
                rate = c.sample_rate(),
                channels = c.channels(),
                format = format!("{:?}", c.sample_format())
            )
        }),
        max_channels,
        buffer_range: device.default_output_config().ok().and_then(|c| match *c.buffer_size() {
            SupportedBufferSize::Range { min, max } => Some((min, max)),
            SupportedBufferSize::Unknown => None,
        }),
    }
}

/// The host's devices worth offering, before any of them is opened: probing
/// every ALSA PCM is slow, and alsa-lib prints to stderr for each one that
/// fails (OSS emulation, dmix on a card PipeWire holds, …).
fn listed_devices(host: &Host) -> Result<Vec<Device>, AudioError> {
    let devices: Vec<Device> = host.devices()?.collect();
    if host.id().name() != "ALSA" {
        // Inputs ("JACK Input") claim output configs too. PipeWire lists other
        // apps' playback streams (ours included) as outputs; its sinks are
        // the `Duplex` nodes. Its `*_default` nodes repeat "Standardgerät".
        let wanted = if host.id().name() == "PipeWire" {
            |direction| direction == DeviceDirection::Duplex
        } else {
            |direction| direction != DeviceDirection::Input
        };
        return Ok(devices
            .into_iter()
            .filter(|d| d.description().is_ok_and(|desc| wanted(desc.direction())))
            .filter(|d| !device_id(d).ends_with("_default"))
            .collect());
    }
    let ids: Vec<String> = devices.iter().map(device_id).collect();
    let named: Vec<String> = devices
        .iter()
        .zip(&ids)
        .filter(|(_, id)| alsa_card(id) == Some(true))
        .map(|(d, _)| describe(d).1)
        .collect();
    Ok(devices
        .into_iter()
        .zip(ids)
        .filter(|(d, id)| match alsa_card(id) {
            Some(true) => true,
            // cpal adds `hw:CARD=0,…` next to the hinted `hw:CARD=Name,…`.
            Some(false) => !named.contains(&describe(d).1),
            None => false,
        })
        .map(|(d, _)| d)
        .collect())
}

/// Which ALSA PCMs are listed: each card's devices as `hw:`; `Some(named)`,
/// true if addressed by card id rather than index. `default` is the UI's
/// "Standardgerät" already; `plughw:`, `dmix:`, `front:`, `surround*:` … only
/// repeat the same hardware through plugins; `pipewire`/`pulse` are better
/// served by their own hosts (through ALSA they mix multichannel streams down
/// and underrun); the rest (rate converters, upmix, OSS, usbstream) are no
/// outputs of their own.
fn alsa_card(id: &str) -> Option<bool> {
    let card = id.strip_prefix("alsa:hw:CARD=")?.split(',').next()?;
    Some(!card.chars().all(|c| c.is_ascii_digit()))
}

fn device_id(device: &Device) -> String {
    device.id().map_or_else(|_| "?".into(), |i| i.to_string())
}

/// Short name and full description. cpal's `Display` appends manufacturer,
/// type and connection ("… [Speaker] via S/PDIF"), too much for a label.
fn describe(device: &Device) -> (String, String) {
    // PipeWire's default node follows the system default sink.
    if device_id(device) == "pipewire:output_default" {
        return (t!("audio.default_device"), t!("audio.default_device_detail"));
    }
    match device.description() {
        Ok(desc) => (desc.name().to_string(), desc.to_string()),
        Err(_) => ("?".into(), "?".into()),
    }
}

fn build_stream<T>(
    device: &Device,
    config: &StreamConfig,
    mut lent: Lent,
    shared: Arc<Shared>,
    pulse: Arc<Pulse>,
    pro_audio: bool,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut scratch = vec![0.0f32; MAX_CALLBACK_FRAMES * channels];
    let beat = pulse.clone();
    let frames = shared.clone();
    // The callback thread joins MMCSS in its first callback (one system call
    // per stream, before any audio of it is due).
    let mut join_pro_audio = pro_audio;
    let stream = device.build_output_stream(
        *config,
        move |data: &mut [T], _| {
            if std::mem::take(&mut join_pro_audio) {
                realtime::join_pro_audio();
            }
            beat.beats.fetch_add(1, Ordering::Relaxed);
            frames.callback_frames.store((data.len() / channels) as u32, Ordering::Relaxed);
            for out in data.chunks_mut(scratch.len()) {
                let buf = &mut scratch[..out.len()];
                lent.process(buf);
                for (o, s) in out.iter_mut().zip(buf.iter()) {
                    *o = T::from_sample(*s);
                }
            }
        },
        // Never block here: if the list is busy, the error is dropped.
        move |err: cpal::Error| {
            let code = failure_code(err.kind());
            if code != 0 {
                pulse.failure.store(code, Ordering::Relaxed);
            }
            if let Ok(mut e) = shared.errors.try_lock()
                && e.len() < MAX_ERRORS
            {
                e.push(err.to_string());
            }
        },
        None,
    )?;
    Ok(stream)
}

fn pick_host(name: Option<&str>) -> Result<Host, AudioError> {
    let Some(name) = name else { return Ok(cpal::default_host()) };
    let id = hosts().into_iter().find(|h| h.name().eq_ignore_ascii_case(name)).ok_or_else(|| {
        AudioError::HostUnavailable { name: name.into(), available: host_names().join(", ") }
    })?;
    Ok(cpal::host_from_id(id)?)
}

/// Host and device as the spec names them; a device id ("jack:…") brings its
/// host along when none is set.
fn resolve(spec: &OutputSpec) -> Result<(Host, Device), AudioError> {
    let host = match (&spec.host, &spec.device) {
        (None, Some(device)) => host_of(device).unwrap_or_else(cpal::default_host),
        (host, _) => pick_host(host.as_deref())?,
    };
    let device = pick_device(&host, spec.device.as_deref())?;
    Ok((host, device))
}

/// Most channels the device takes, if it tells. PipeWire's "Standardgerät"
/// claims stereo, so the sink it points at is asked instead.
fn capacity(host: &Host, device: &Device) -> Option<u16> {
    #[cfg(not(target_os = "linux"))]
    let _ = host;
    #[cfg(target_os = "linux")]
    if device_id(device) == "pipewire:output_default" {
        let sink = format!("pipewire:{}", pipewire_default::default_sink()?);
        let sink = host.devices().ok()?.find(|d| device_id(d) == sink)?;
        return capacity(host, &sink);
    }
    device.supported_output_configs().ok()?.map(|c| c.channels()).max()
}

/// Channels to open when none are set: the default, or enough for the
/// outputs if the device takes that many. PipeWire maps any count onto the
/// sink; elsewhere the smallest supported count that fits is used.
fn stream_channels(host: &Host, device: &Device, default: u16, needed: u16, max: Option<u16>) -> u16 {
    if needed <= default || max.is_some_and(|max| needed > max) {
        return default;
    }
    if host.id().name() == "PipeWire" {
        return needed;
    }
    device
        .supported_output_configs()
        .ok()
        .and_then(|configs| configs.map(|c| c.channels()).filter(|&c| c >= needed).min())
        .unwrap_or(default)
}

/// The host a device id belongs to ("jack:…"), so a device chosen on another
/// host still opens when the host setting is left at the default.
fn host_of(device: &str) -> Option<Host> {
    let id: DeviceId = device.parse().ok()?;
    cpal::host_from_id(id.host()).ok()
}

fn pick_device(host: &Host, wanted: Option<&str>) -> Result<Device, AudioError> {
    let Some(wanted) = wanted else { return host.default_output_device().ok_or(AudioError::NoDefaultDevice) };
    // Any PCM by exact id, so a hand-written one in local.settings still works;
    // a description only among the listed ones, without opening the rest.
    if let Some(device) = host.devices()?.find(|d| device_id(d) == wanted) {
        return Ok(device);
    }
    let needle = wanted.to_lowercase();
    listed_devices(host)?
        .into_iter()
        .find(|d| describe(d).1.to_lowercase().contains(&needle))
        .ok_or_else(|| AudioError::DeviceNotFound(wanted.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixer_comes_home_from_the_null_thread() {
        let (_controller, mixer) = jq_engine::engine(EngineConfig::new(48_000, 2));
        let home = Home::new(Mutex::new(Some(mixer)));
        for _ in 0..2 {
            let lent = Lent::take(&home).expect("mixer at home");
            assert!(Lent::take(&home).is_none());
            drop(NullThread::start(lent, 48_000, 2));
            assert!(home.lock().unwrap().is_some());
        }
    }

    #[test]
    fn watch_reports_errors_and_stalls() {
        let t0 = Instant::now();
        let pulse = Arc::new(Pulse::default());
        let mut watch = Watch::new(pulse.clone(), t0);
        // Not started yet: no stall until the start timeout.
        assert_eq!(watch.failure(t0 + STALL * 5), None);
        assert_eq!(Watch::new(pulse.clone(), t0).failure(t0 + START_TIMEOUT * 2), Some(0));
        pulse.beats.store(1, Ordering::Relaxed);
        assert_eq!(watch.failure(t0 + STALL * 5), None);
        assert_eq!(watch.failure(t0 + STALL * 5 + STALL / 2), None);
        assert_eq!(watch.failure(t0 + STALL * 7), Some(0));
        pulse.beats.store(2, Ordering::Relaxed);
        assert_eq!(watch.failure(t0 + STALL * 7), None);

        assert_eq!(failure_code(cpal::ErrorKind::Xrun), 0);
        let code = failure_code(cpal::ErrorKind::StreamInvalidated);
        pulse.failure.store(code, Ordering::Relaxed);
        assert_eq!(watch.failure(t0 + STALL * 7), Some(code));
    }

    #[test]
    fn alsa_listing() {
        assert_eq!(alsa_card("alsa:hw:CARD=AG08,DEV=0"), Some(true));
        assert_eq!(alsa_card("alsa:hw:CARD=0,DEV=3"), Some(false));
        for id in [
            "alsa:default",
            "alsa:pipewire",
            "alsa:pulse",
            "alsa:oss",
            "alsa:lavrate",
            "alsa:jack",
            "alsa:plughw:CARD=AG08,DEV=0",
            "alsa:dmix:CARD=AG08,DEV=0",
            "alsa:front:CARD=Creative,DEV=0",
            "alsa:surround51:CARD=Creative,DEV=0",
            "alsa:usbstream:CARD=AG08",
            "wasapi:hw:CARD=0,DEV=0",
        ] {
            assert_eq!(alsa_card(id), None, "{id}");
        }
    }
}
