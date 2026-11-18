//! Previews for remote devices: a sound cue rendered offline by the app's
//! own engine, as the host's preview plays it ([`PreviewSound`]: part,
//! level, balance, speed and pitch), into a 16-bit stereo WAV the device's
//! browser plays with a plain `<audio>` element.
//!
//! The length is known before the first sample (the part of the file at
//! the cue's rate), so the WAV is served while it is rendered; a request
//! for a part not rendered yet waits for it; the device hears how far it got
//! ([`Report`]). Rendering runs on a thread of its own, one preview at a
//! time, as fast as it goes while no cue runs and else at most [`PACE`]
//! times faster than real time, and reads the file itself: the show's
//! buffers, its streams and its audio thread are never touched. A rendering goes into a file of its own
//! in a folder of the app's cache (about 11 MB a minute), not into memory;
//! there is one per device: a new one replaces it, and it is dropped after
//! [`IDLE`] without a request. A dropped rendering's file is deleted, the
//! whole folder when the program ends ([`Previews::clear`]) and, after a
//! crash, when it starts again ([`Previews::new`]).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use jq_core::PreviewSound;
use jq_engine::{AudioBuffer, END_BLEND, EngineConfig, Event};
use serde::Serialize;
use tokio::sync::watch;

/// Sample rate of the WAV.
pub const RATE: u32 = 48_000;
/// Longest preview, seconds of output; a longer cue is cut there.
pub const MAX_SECS: f64 = 20.0 * 60.0;
/// While cues run, renders at most this much faster than real time: well
/// ahead of the device's playback, and gentle on the CPU the show needs.
const PACE: f64 = 4.0;
/// The device hears of the progress in steps of this many seconds.
const REPORT_STEP: f64 = 1.0;
/// Output frames per block.
const BLOCK: usize = 1024;
/// A streamed file whose next part takes longer than this to load fails.
const LOAD_WAIT: Duration = Duration::from_secs(10);
/// A rendering nobody asked for for this long is dropped.
pub const IDLE: Duration = Duration::from_secs(120);
/// Bytes of the WAV header.
const HEADER: usize = 44;
/// Bytes per frame: two 16-bit channels.
const FRAME_BYTES: usize = 4;
/// How long [`Previews::clear`] tries to delete the folder: the rendering
/// thread may still be closing a file.
const CLEAR_WAIT: Duration = Duration::from_millis(500);

/// How far a rendering got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Bytes ready, from the start (the header first).
    Ready(usize),
    /// Failed or replaced: the rest never comes.
    Ended,
}

/// How far a device's preview is rendered, for its player (the
/// `preview_progress` event): it can only move to what is there.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Report {
    /// Of the preview ([`Render::id`]).
    pub id: String,
    /// Seconds rendered from the start.
    pub rendered: f64,
    pub done: bool,
    /// It stopped with an error: the rest never comes.
    pub failed: bool,
}

type Reports = Arc<watch::Sender<Option<Report>>>;

/// One device's preview, being rendered or done.
pub struct Render {
    /// Random, the last part of its URL.
    pub id: String,
    /// The device it is for.
    pub client: String,
    /// Bytes of the whole WAV.
    pub len: usize,
    /// Seconds.
    pub duration: f64,
    /// The WAV as far as it is rendered; deleted with the rendering.
    path: PathBuf,
    progress: watch::Sender<Progress>,
    /// The device's progress reports.
    reports: Reports,
    cancelled: AtomicBool,
    last_used: Mutex<Instant>,
}

impl Render {
    /// A rendering of `frames` frames, its file in `dir` with the header.
    fn new(client: &str, frames: usize, dir: &Path, reports: Reports) -> Result<Self, String> {
        let id = super::clients::random_hex(16);
        let path = dir.join(format!("{id}.wav"));
        let failed = |e: std::io::Error| format!("{}: {e}", path.display());
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&path, wav_header(frames)).map_err(failed)?;
        Ok(Self {
            id,
            client: client.to_owned(),
            len: HEADER + frames * FRAME_BYTES,
            duration: frames as f64 / RATE as f64,
            path,
            progress: watch::Sender::new(Progress::Ready(HEADER)),
            reports,
            cancelled: AtomicBool::new(false),
            last_used: Mutex::new(Instant::now()),
        })
    }

    pub fn progress(&self) -> watch::Receiver<Progress> {
        self.progress.subscribe()
    }

    /// The bytes of `range`, as far as they are rendered. Reads the disk:
    /// call it off the async runtime.
    pub fn read(&self, range: Range<usize>) -> Result<Vec<u8>, String> {
        let ready = match *self.progress.borrow() {
            Progress::Ready(ready) => ready,
            Progress::Ended => self.len,
        };
        let range = range.start.min(ready)..range.end.min(ready);
        let failed = |e: std::io::Error| format!("{}: {e}", self.path.display());
        let mut file = File::open(&self.path).map_err(failed)?;
        file.seek(SeekFrom::Start(range.start as u64)).map_err(failed)?;
        let mut bytes = vec![0; range.len()];
        file.read_exact(&mut bytes).map_err(failed)?;
        Ok(bytes)
    }

    /// Someone asked for it: it is kept another [`IDLE`].
    pub fn touch(&self) {
        *lock(&self.last_used) = Instant::now();
    }

    /// Stops rendering it; requests waiting for the rest end.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.progress.send_replace(Progress::Ended);
    }

    /// It failed: as [`Render::cancel`], and the device's player hears of it.
    fn fail(&self) {
        self.cancel();
        self.report(|report| report.failed = true);
    }

    /// Changes the device's report, if it is about this preview (not about
    /// the one that replaced it).
    fn report(&self, change: impl FnOnce(&mut Report)) {
        self.reports.send_if_modified(|report| match report {
            Some(report) if report.id == self.id => {
                let was = report.clone();
                change(report);
                *report != was
            }
            _ => false,
        });
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn idle(&self, now: Instant) -> bool {
        now.duration_since(*lock(&self.last_used)) > IDLE
    }

    /// `ready` bytes are in the file now.
    fn rendered(&self, ready: usize) {
        self.progress.send_if_modified(|p| {
            let more = *p != Progress::Ended;
            if more {
                *p = Progress::Ready(ready);
            }
            more
        });
        let secs = (ready - HEADER) as f64 / FRAME_BYTES as f64 / RATE as f64;
        let done = ready >= self.len;
        self.report(|report| {
            if done || secs - report.rendered >= REPORT_STEP {
                (report.rendered, report.done) = (if done { self.duration } else { secs }, done);
            }
        });
    }
}

impl Drop for Render {
    fn drop(&mut self) {
        // Nobody reads it any more (each read opens the file for itself).
        let _ = std::fs::remove_file(&self.path);
    }
}

struct Job {
    render: Arc<Render>,
    sound: PreviewSound,
    buffer: Arc<AudioBuffer>,
    /// Frames of the file.
    part: Range<usize>,
    /// At most [`PACE`] times real time (cues run).
    paced: bool,
}

/// The devices' previews and the thread rendering them.
pub struct Previews {
    /// Folder of the renderings' files; only theirs.
    dir: PathBuf,
    /// By device.
    renders: Mutex<HashMap<String, Arc<Render>>>,
    /// Progress of each device's preview, for its pages.
    reports: Mutex<HashMap<String, Reports>>,
    jobs: OnceLock<Sender<Job>>,
}

impl Previews {
    /// Previews with their files in `dir`; what is left there (the program
    /// did not end normally) is deleted.
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::remove_dir_all(&dir);
        Self {
            dir,
            renders: Mutex::new(HashMap::new()),
            reports: Mutex::new(HashMap::new()),
            jobs: OnceLock::new(),
        }
    }

    /// Drops every preview and deletes the folder (the program ends).
    pub fn clear(&self) {
        let renders: Vec<_> = lock(&self.renders).drain().map(|(_, render)| render).collect();
        for render in &renders {
            render.cancel();
        }
        drop(renders);
        let deadline = Instant::now() + CLEAR_WAIT;
        while self.dir.exists() && std::fs::remove_dir_all(&self.dir).is_err() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The progress of device `client`'s preview, now and later.
    pub fn follow(&self, client: &str) -> watch::Receiver<Option<Report>> {
        self.reports_of(client).subscribe()
    }

    fn reports_of(&self, client: &str) -> Reports {
        lock(&self.reports)
            .entry(client.to_owned())
            .or_insert_with(|| Arc::new(watch::Sender::new(None)))
            .clone()
    }

    /// Opens the file of `sound` and queues its rendering for device
    /// `client`, replacing the device's last preview; `paced` (cues run): at
    /// most [`PACE`] times real time. Reads the disk: call it off the async
    /// runtime.
    pub fn start(&self, client: &str, sound: PreviewSound, paced: bool) -> Result<Arc<Render>, String> {
        let buffer = Arc::new(jq_media::load(&sound.path, RATE).map_err(|e| e.to_string())?);
        let part = part(&sound, buffer.frames());
        let reports = self.reports_of(client);
        let render =
            Arc::new(Render::new(client, output_frames(&part, sound.rate), &self.dir, reports.clone())?);
        if let Some(old) = lock(&self.renders).insert(client.to_owned(), render.clone()) {
            old.cancel();
        }
        reports.send_replace(Some(Report {
            id: render.id.clone(),
            rendered: 0.0,
            done: render.len == HEADER,
            failed: false,
        }));
        let job = Job { render: render.clone(), sound, buffer, part, paced };
        if self.jobs.get_or_init(spawn_worker).send(job).is_err() {
            render.cancel();
            return Err(jq_i18n::t!("remote.crashed"));
        }
        Ok(render)
    }

    /// The preview whose URL ends in `id`.
    pub fn get(&self, id: &str) -> Option<Arc<Render>> {
        lock(&self.renders).values().find(|r| r.id == id).cloned()
    }

    /// Drops the previews nobody asked for lately, and those of devices
    /// that may no longer preview.
    pub fn sweep(&self, may_preview: impl Fn(&str) -> bool) {
        let now = Instant::now();
        lock(&self.renders).retain(|client, render| {
            let keep = !render.idle(now) && may_preview(client);
            if !keep {
                render.cancel();
            }
            keep
        });
    }
}

impl Drop for Previews {
    fn drop(&mut self) {
        self.clear();
    }
}

fn spawn_worker() -> Sender<Job> {
    let (jobs, queue) = mpsc::channel::<Job>();
    let spawned = std::thread::Builder::new().name("jq-remote-render".into()).spawn(move || {
        for job in queue {
            if job.render.cancelled() {
                continue;
            }
            // A failure ends this preview (the device's player stops), never the thread.
            let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(&job)));
            if !matches!(rendered, Ok(Ok(()))) {
                job.render.fail();
            }
        }
    });
    // Without the thread the queue's receiver is gone: every job then fails
    // at `send`, which `start` reports.
    drop(spawned);
    jobs
}

/// Frames of the file the preview plays: the cue's part, cut at
/// [`MAX_SECS`] of output.
fn part(sound: &PreviewSound, frames: usize) -> Range<usize> {
    let at = |secs: f64| ((secs.max(0.0) * RATE as f64).round() as usize).min(frames);
    let start = at(sound.start);
    let end = sound.end.map_or(frames, at).max(start);
    let longest = (MAX_SECS * RATE as f64 * sound.rate.max(0.01) as f64) as usize;
    start..end.min(start.saturating_add(longest))
}

/// Output frames of `part` played at `rate` file frames per output frame.
fn output_frames(part: &Range<usize>, rate: f32) -> usize {
    (part.len() as f64 / rate.max(0.01) as f64).ceil() as usize
}

fn render(job: &Job) -> Result<(), String> {
    let (render, buffer, part) = (&job.render, &job.buffer, &job.part);
    let frames = (render.len - HEADER) / FRAME_BYTES;
    let config = EngineConfig {
        max_voices: 1,
        stretchers: 1,
        // A position per block: a streamed file's reader follows it.
        report_rate_hz: RATE / BLOCK as u32,
        ..EngineConfig::new(RATE, 2)
    };
    let (mut controller, mut mixer) = jq_engine::engine(config);
    let failed = |e: std::io::Error| format!("{}: {e}", render.path.display());
    let mut file = std::fs::OpenOptions::new().append(true).open(&render.path).map_err(failed)?;
    let params = job.sound.voice(buffer.clone(), part.start, part.end, 0, controller.frames(END_BLEND));
    controller.play(params).map_err(|e| e.to_string())?;
    let rate = job.sound.rate.max(0.01) as f64;
    let (began, mut done, mut finished) = (Instant::now(), 0, false);
    let mut block = vec![0f32; BLOCK * 2];
    let mut bytes = Vec::with_capacity(BLOCK * FRAME_BYTES);
    while done < frames {
        if render.cancelled() {
            return Ok(());
        }
        let n = BLOCK.min(frames - done);
        let out = &mut block[..n * 2];
        out.fill(0.0);
        if !finished {
            // The file frames this block reads, loaded first (a streamed file).
            let last = part.end.saturating_sub(1);
            let ahead = part.start + ((done + n) as f64 * rate).ceil() as usize;
            wait_loaded(render, buffer, (part.start + (done as f64 * rate) as usize).min(last))?;
            wait_loaded(render, buffer, ahead.min(last))?;
            mixer.process(out);
            finished = controller.poll_events().any(|e| matches!(e, Event::Finished { .. }));
        }
        bytes.clear();
        for sample in out.iter() {
            let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            bytes.extend_from_slice(&pcm.to_le_bytes());
        }
        file.write_all(&bytes).map_err(failed)?;
        done += n;
        render.rendered(HEADER + done * FRAME_BYTES);
        let due = began + Duration::from_secs_f64(done as f64 / RATE as f64 / PACE);
        if job.paced
            && let Some(wait) = due.checked_duration_since(Instant::now())
        {
            std::thread::sleep(wait);
        }
    }
    Ok(())
}

/// Waits until frame `at` of a streamed buffer is loaded.
fn wait_loaded(render: &Render, buffer: &AudioBuffer, at: usize) -> Result<(), String> {
    let deadline = Instant::now() + LOAD_WAIT;
    while at < buffer.frames() && !buffer.is_ready_at(at) {
        if render.cancelled() || Instant::now() > deadline {
            return Err(jq_i18n::t!("remote.preview_slow"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

/// The header of a 16-bit stereo WAV of `frames` frames at [`RATE`].
fn wav_header(frames: usize) -> [u8; HEADER] {
    let data = (frames * FRAME_BYTES).min(u32::MAX as usize - HEADER) as u32;
    let mut h = [0u8; HEADER];
    let mut put = |at: usize, bytes: &[u8]| h[at..at + bytes.len()].copy_from_slice(bytes);
    put(0, b"RIFF");
    put(4, &(data + HEADER as u32 - 8).to_le_bytes());
    put(8, b"WAVEfmt ");
    put(16, &16u32.to_le_bytes());
    put(20, &1u16.to_le_bytes()); // PCM
    put(22, &2u16.to_le_bytes()); // channels
    put(24, &RATE.to_le_bytes());
    put(28, &(RATE * FRAME_BYTES as u32).to_le_bytes());
    put(32, &(FRAME_BYTES as u16).to_le_bytes());
    put(34, &16u16.to_le_bytes()); // bits
    put(36, b"data");
    put(40, &data.to_le_bytes());
    h
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn sound(path: std::path::PathBuf, rate: f32) -> PreviewSound {
        PreviewSound { path, start: 0.0, end: None, gain: 1.0, pan: 0.0, rate, pitch: rate }
    }

    /// A WAV file of `secs` seconds in `dir`: a ramp on the left channel.
    pub(crate) fn write_wav(dir: &std::path::Path, secs: f64) -> std::path::PathBuf {
        let frames = (secs * RATE as f64) as usize;
        let mut bytes = wav_header(frames).to_vec();
        for i in 0..frames {
            let left = ((i % 1000) as i16 - 500) * 20;
            bytes.extend_from_slice(&left.to_le_bytes());
            bytes.extend_from_slice(&0i16.to_le_bytes());
        }
        let path = dir.join(format!("ramp-{secs}.wav"));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn wait_done(render: &Render) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut progress = render.progress();
        while *progress.borrow_and_update() != Progress::Ready(render.len) {
            assert_ne!(*progress.borrow(), Progress::Ended, "ended early");
            assert!(Instant::now() < deadline, "not done in time");
            std::thread::sleep(Duration::from_millis(5));
        }
        render.read(0..render.len).unwrap()
    }

    #[test]
    fn a_cue_is_rendered_as_a_wav_of_the_known_length() {
        let dir = std::env::temp_dir().join(format!("jq-render-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_wav(&dir, 0.5);
        let previews = Previews::new(dir.join("previews"));

        let mut half = sound(path.clone(), 0.5);
        (half.start, half.end) = (0.1, Some(0.3));
        let render = previews.start("tablet", half, false).unwrap();
        // 0.2 s of file at half speed: 0.4 s.
        assert_eq!(render.len, HEADER + (0.4 * RATE as f64) as usize * FRAME_BYTES);
        assert!((render.duration - 0.4).abs() < 1e-6);
        let wav = wait_done(&render);
        let report = previews.follow("tablet").borrow().clone().unwrap();
        assert_eq!(
            (report.id.as_str(), report.rendered, report.done),
            (render.id.as_str(), render.duration, true)
        );
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize, render.len - HEADER);
        let samples: Vec<i16> = wav[HEADER..].chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        assert!(samples.iter().step_by(2).any(|&s| s.abs() > 1000), "the left channel plays");

        // Slower at the same pitch: through the stretcher.
        let mut stretched = sound(path.clone(), 0.8);
        stretched.pitch = 1.0;
        let wav = wait_done(&previews.start("tablet", stretched, true).unwrap());
        let loud = wav[HEADER..].chunks(4).filter(|f| i16::from_le_bytes([f[0], f[1]]).abs() > 1000).count();
        assert!(loud > RATE as usize / 4, "stretched, it plays too: {loud} frames");

        // A new preview of the device replaces the last one, and its file goes.
        let first = render.path.clone();
        assert!(first.exists());
        let next = previews.start("tablet", sound(path.clone(), 1.0), false).unwrap();
        assert!(previews.get(&render.id).is_none());
        drop(render);
        assert!(!first.exists(), "a dropped preview's file is deleted");
        assert_eq!(previews.get(&next.id).map(|r| r.client.clone()), Some("tablet".into()));
        wait_done(&next);
        previews.sweep(|_| false);
        assert!(previews.get(&next.id).is_none(), "dropped once the device may not preview");

        assert!(previews.start("tablet", sound(dir.join("missing.wav"), 1.0), false).is_err());

        let last = previews.start("phone", sound(path.clone(), 1.0), false).unwrap();
        wait_done(&last);
        drop(last);
        previews.clear();
        assert!(!dir.join("previews").exists(), "the folder goes when the program ends");
        std::fs::create_dir_all(dir.join("previews")).unwrap();
        std::fs::write(dir.join("previews").join("left.wav"), b"x").unwrap();
        let _again = Previews::new(dir.join("previews"));
        assert!(!dir.join("previews").exists(), "left over from a crash: deleted on the next start");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_long_cue_is_cut() {
        let s = sound("x".into(), 1.0);
        let frames = 2 * MAX_SECS as usize * RATE as usize;
        assert_eq!(part(&s, frames).len(), MAX_SECS as usize * RATE as usize);
        let slow = sound("x".into(), 0.5);
        assert_eq!(output_frames(&part(&slow, frames), 0.5), MAX_SECS as usize * RATE as usize);
    }
}
