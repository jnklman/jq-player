//! Peaks of media files kept on disk (like Reaper's peak files), so a
//! waveform is drawn without decoding the file again.
//!
//! The cache lives in a folder of the machine, never in the show folder:
//! `content/` holds the peaks by a hash of the file's content, `paths/` a
//! small record per file path with the [`FileStamp`] it had and its content
//! hash. An unchanged stamp finds the peaks without reading the file; a
//! changed one has the file hashed: the same content (a copy, a file only
//! touched) reuses its peaks, other content is decoded anew. Every write is
//! atomic, and anything unreadable counts as not cached; the cache never
//! makes a waveform fail.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{self, Read};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jq_media::loudness::Loudness;
use jq_media::peaks::{BLOCK, Peaks};
use jq_media::{MediaError, Waveform};
use jq_show::paths::{self, FileStamp};
use xxhash_rust::xxh3::{Xxh3, xxh3_128};

/// Above this the least recently used peaks are deleted. About 2.9 MB per
/// hour of material at 48 kHz, so it holds some 170 hours.
pub const CACHE_LIMIT: u64 = 500 << 20;

const PEAKS_MAGIC: &[u8; 4] = b"JQPK";
const LOUDNESS_MAGIC: &[u8; 4] = b"JQLD";
const RECORD_MAGIC: &[u8; 4] = b"JQPR";
/// Raised whenever the layout of either file changes; older files then
/// count as not cached.
const FORMAT_VERSION: u32 = 1;
/// Magic, version, hash, loudness, true peak.
const LOUDNESS_LEN: usize = 4 + 4 + 16 + 8 + 8;
/// Magic, version, hash, rate, frames, block, block count.
const PEAKS_HEADER: usize = 4 + 4 + 16 + 4 + 8 + 4 + 8;
/// Peaks used within this long are not marked as used again.
const TOUCH_AFTER: Duration = Duration::from_secs(3600);
/// Temporary files this old are left over from a crash.
const STALE_TEMP: Duration = Duration::from_secs(3600);

pub struct PeakCache {
    /// `None`: no cache folder; peaks are decoded every time.
    dir: Option<PathBuf>,
    limit: u64,
    /// One build per file at a time; a second request waits for the first
    /// and then finds its result stored.
    building: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
    temp_names: AtomicU64,
}

impl PeakCache {
    pub fn new(dir: PathBuf) -> Self {
        Self::with_limit(Some(dir), CACHE_LIMIT)
    }

    /// Without a cache folder (none could be found).
    pub fn disabled() -> Self {
        Self::with_limit(None, 0)
    }

    fn with_limit(dir: Option<PathBuf>, limit: u64) -> Self {
        Self { dir, limit, building: Mutex::new(HashMap::new()), temp_names: AtomicU64::new(0) }
    }

    /// `buckets` peaks of the whole file; see [`Peaks::overview`].
    pub fn waveform(&self, path: &Path, buckets: usize) -> Result<Waveform, MediaError> {
        Ok(self.peaks(path, &|| true)?.overview(buckets))
    }

    /// `from`–`to` seconds: from the stored peaks where they are fine
    /// enough, else decoded (see [`jq_media::waveform_range`]). Never
    /// builds peaks; the overview does that.
    pub fn waveform_range(
        &self,
        path: &Path,
        from: f64,
        to: f64,
        buckets: usize,
    ) -> Result<Waveform, MediaError> {
        let path = canonical(path);
        let stored = self.stored_hash(&path).and_then(|hash| self.read_peaks(hash));
        match stored.and_then(|peaks| peaks.range(from, to, buckets)) {
            Some(range) => Ok(range),
            None => jq_media::waveform_range(&path, from, to, buckets),
        }
    }

    /// The peaks of `path`, from the cache or decoded (and then stored).
    /// Gives up with [`MediaError::Cancelled`] once `keep_going` returns
    /// false; nothing is stored then.
    pub fn peaks(&self, path: &Path, keep_going: &dyn Fn() -> bool) -> Result<Peaks, MediaError> {
        if self.dir.is_none() {
            return Peaks::build(path, keep_going);
        }
        let path = canonical(path);
        if let Some(peaks) = self.stored_hash(&path).and_then(|hash| self.read_peaks(hash)) {
            return Ok(peaks);
        }
        Ok(self.exclusive(&path, keep_going)?.0)
    }

    /// The loudness of `path`, from the cache or measured (and then
    /// stored, with the peaks if they are not yet); like [`Self::peaks`].
    pub fn loudness(&self, path: &Path, keep_going: &dyn Fn() -> bool) -> Result<Loudness, MediaError> {
        if self.dir.is_none() {
            return Loudness::measure(path, keep_going);
        }
        let path = canonical(path);
        if let Some(loudness) = self.stored_hash(&path).and_then(|hash| self.read_loudness(hash)) {
            return Ok(loudness);
        }
        Ok(self.exclusive(&path, keep_going)?.1)
    }

    /// [`Self::load`], one request per file at a time.
    fn exclusive(&self, path: &Path, keep_going: &dyn Fn() -> bool) -> Result<(Peaks, Loudness), MediaError> {
        let lock = locked(&self.building).entry(path.to_owned()).or_default().clone();
        let result = {
            let _building = lock.lock().unwrap_or_else(PoisonError::into_inner);
            self.load(path, keep_going)
        };
        let mut building = locked(&self.building);
        // Only this request and the map hold it: nobody waits.
        if Arc::strong_count(&lock) == 2 {
            building.remove(path);
        }
        result
    }

    /// Peaks and loudness, from the cache where they are, the rest decoded
    /// and stored.
    fn load(&self, path: &Path, keep_going: &dyn Fn() -> bool) -> Result<(Peaks, Loudness), MediaError> {
        // Missing: decoding reports why.
        let Some(before) = paths::stamp(path) else { return Peaks::build_measured(path, keep_going) };
        // Found by its record (also when built meanwhile by the request this
        // one waited for), or else by its content.
        let (hash, recorded) = match self.record_hash(path, &before) {
            Some(hash) => (hash, true),
            None => (hash_file(path, keep_going)?, false),
        };
        // Stored only if the file stayed as it was hashed, so a file still
        // being written is never kept half done.
        let unchanged = || paths::stamp(path) == Some(before);
        let (peaks, loudness) = match (self.read_peaks(hash), self.read_loudness(hash)) {
            (Some(peaks), Some(loudness)) => (peaks, loudness),
            (Some(peaks), None) => {
                let loudness = Loudness::measure(path, keep_going)?;
                if unchanged() {
                    self.write_loudness(hash, &loudness);
                }
                (peaks, loudness)
            }
            (None, _) => {
                let (peaks, loudness) = Peaks::build_measured(path, keep_going)?;
                if !(unchanged() && self.write_peaks(hash, &peaks)) {
                    return Ok((peaks, loudness));
                }
                self.write_loudness(hash, &loudness);
                self.prune();
                (peaks, loudness)
            }
        };
        if !recorded && unchanged() {
            self.write_record(path, &before, hash);
        }
        Ok((peaks, loudness))
    }

    /// The content hash recorded for `path` as it is now.
    fn stored_hash(&self, path: &Path) -> Option<u128> {
        paths::stamp(path).and_then(|stamp| self.record_hash(path, &stamp))
    }

    /// The content hash recorded for `path` if it still has `stamp`.
    fn record_hash(&self, path: &Path, stamp: &FileStamp) -> Option<u128> {
        let bytes = fs::read(self.record_path(path)?).ok()?;
        let rest = bytes.strip_prefix(RECORD_MAGIC)?;
        let (version, rest) = rest.split_first_chunk::<4>()?;
        let rest = rest.strip_prefix(stamp_bytes(stamp).as_slice())?;
        let hash: [u8; 16] = rest.try_into().ok()?;
        (u32::from_le_bytes(*version) == FORMAT_VERSION).then_some(u128::from_le_bytes(hash))
    }

    fn read_peaks(&self, hash: u128) -> Option<Peaks> {
        let path = self.peaks_path(hash)?;
        let peaks = decode_peaks(&fs::read(&path).ok()?, hash)?;
        touch(&path);
        Some(peaks)
    }

    fn read_loudness(&self, hash: u128) -> Option<Loudness> {
        let path = self.loudness_path(hash)?;
        let loudness = decode_loudness(&fs::read(&path).ok()?, hash)?;
        touch(&path);
        Some(loudness)
    }

    fn write_loudness(&self, hash: u128, loudness: &Loudness) {
        if let Some(path) = self.loudness_path(hash) {
            let _ = self.write_atomic(&path, &encode_loudness(hash, loudness));
        }
    }

    /// Returns false if they could not be stored.
    fn write_peaks(&self, hash: u128, peaks: &Peaks) -> bool {
        self.peaks_path(hash).is_some_and(|path| self.write_atomic(&path, &encode_peaks(hash, peaks)).is_ok())
    }

    fn write_record(&self, path: &Path, stamp: &FileStamp, hash: u128) {
        let Some(record) = self.record_path(path) else { return };
        let mut bytes = RECORD_MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&stamp_bytes(stamp));
        bytes.extend_from_slice(&hash.to_le_bytes());
        let _ = self.write_atomic(&record, &bytes);
    }

    /// Written beside `target` and renamed, so readers (also of another
    /// instance of the app) never see half a file.
    fn write_atomic(&self, target: &Path, bytes: &[u8]) -> io::Result<()> {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let number = self.temp_names.fetch_add(1, Ordering::Relaxed);
        let temp = target.with_extension(format!("{}-{number}.tmp", std::process::id()));
        let written = fs::write(&temp, bytes).and_then(|()| fs::rename(&temp, target));
        if written.is_err() {
            let _ = fs::remove_file(&temp);
        }
        written
    }

    fn peaks_path(&self, hash: u128) -> Option<PathBuf> {
        let name = format!("{hash:032x}");
        Some(self.dir.as_ref()?.join("content").join(&name[..2]).join(name + ".jqpk"))
    }

    fn loudness_path(&self, hash: u128) -> Option<PathBuf> {
        Some(self.peaks_path(hash)?.with_extension("jqld"))
    }

    fn record_path(&self, path: &Path) -> Option<PathBuf> {
        let name = format!("{:032x}", xxh3_128(path.as_os_str().as_encoded_bytes()));
        Some(self.dir.as_ref()?.join("paths").join(&name[..2]).join(name))
    }

    /// Bytes on disk.
    pub fn size(&self) -> u64 {
        let Some(dir) = &self.dir else { return 0 };
        ["content", "paths"].iter().flat_map(|sub| files(&dir.join(sub))).map(|(_, meta)| meta.len()).sum()
    }

    /// Deletes all stored peaks; they are built again when needed.
    pub fn clear(&self) -> io::Result<()> {
        let Some(dir) = &self.dir else { return Ok(()) };
        for sub in ["content", "paths"] {
            match fs::remove_dir_all(dir.join(sub)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    /// Deletes the least recently used peaks while the cache is over its
    /// limit, down to three quarters of it (so not every new file prunes).
    pub fn prune(&self) {
        let Some(dir) = &self.dir else { return };
        let mut stored = files(&dir.join("content"));
        let mut total: u64 = stored.iter().map(|(_, meta)| meta.len()).sum();
        if total <= self.limit {
            return;
        }
        stored.sort_by_key(|(_, meta)| meta.modified().unwrap_or(UNIX_EPOCH));
        for (path, meta) in stored {
            if total <= self.limit / 4 * 3 {
                break;
            }
            if fs::remove_file(path).is_ok() {
                total -= meta.len();
            }
        }
    }

    /// For the start of the app: removes what a crash left behind and
    /// records whose peaks are gone, then prunes.
    pub fn clean_up(&self) {
        let Some(dir) = &self.dir else { return };
        for (path, meta) in ["content", "paths"].iter().flat_map(|sub| files(&dir.join(sub))) {
            let stale =
                meta.modified().ok().and_then(|t| t.elapsed().ok()).is_none_or(|age| age > STALE_TEMP);
            if path.extension().is_some_and(|ext| ext == "tmp") && stale {
                let _ = fs::remove_file(path);
            }
        }
        for (path, _) in files(&dir.join("paths")) {
            let hash = fs::read(&path).ok().and_then(|bytes| bytes.last_chunk::<16>().copied());
            let target = hash.and_then(|hash| self.peaks_path(u128::from_le_bytes(hash)));
            if path.extension().is_none() && !target.is_some_and(|target| target.exists()) {
                let _ = fs::remove_file(path);
            }
        }
        self.prune();
    }
}

/// Builds the peaks of a show's files and measures their loudness in the
/// background, one after the other, so waveforms are there when they are
/// looked at and cues can be normalized. It only reads
/// while the show is quiet ([`Prewarmer::set_busy`]): a build under way when
/// that changes is given up and taken up again later. Dropping it ends its
/// thread.
pub struct Prewarmer {
    shared: Arc<Shared>,
}

struct Shared {
    files: Mutex<VecDeque<PathBuf>>,
    /// Loudness of the files done, as requested; `None` if a file could
    /// not be read.
    measured: Mutex<Vec<(PathBuf, Option<Loudness>)>>,
    wake: Condvar,
    busy: AtomicBool,
    closed: AtomicBool,
}

/// How often a waiting prewarmer looks whether the show became quiet.
const PREWARM_POLL: Duration = Duration::from_millis(100);

impl Prewarmer {
    /// Starts building once the show was quiet for `quiet`.
    pub fn new(cache: Arc<PeakCache>, quiet: Duration) -> Self {
        let shared = Arc::new(Shared {
            files: Mutex::new(VecDeque::new()),
            measured: Mutex::new(Vec::new()),
            wake: Condvar::new(),
            busy: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("jq-peaks".into())
            .spawn(move || prewarm(&cache, &worker, quiet))
            .expect("spawn peaks thread");
        Self { shared }
    }

    /// Queues `path` unless it is queued already.
    pub fn request(&self, path: PathBuf) {
        let mut files = locked(&self.shared.files);
        if !files.contains(&path) {
            files.push_back(path);
            self.shared.wake.notify_one();
        }
    }

    /// The loudness of the files done since the last call.
    pub fn poll(&self) -> Vec<(PathBuf, Option<Loudness>)> {
        std::mem::take(&mut *locked(&self.shared.measured))
    }

    /// While `busy` (something plays, a GO may come), nothing is read.
    /// Cheap; meant for every tick of the show thread.
    pub fn set_busy(&self, busy: bool) {
        self.shared.busy.store(busy, Ordering::Relaxed);
    }
}

impl Drop for Prewarmer {
    fn drop(&mut self) {
        // Set under the queue's mutex: the worker checks the flag while holding
        // it, so it either sees the flag or is already waiting for the notify.
        let files = locked(&self.shared.files);
        self.shared.closed.store(true, Ordering::Relaxed);
        self.shared.wake.notify_all();
        drop(files);
    }
}

fn prewarm(cache: &PeakCache, shared: &Shared, quiet: Duration) {
    let closed = || shared.closed.load(Ordering::Relaxed);
    let busy = || shared.busy.load(Ordering::Relaxed);
    let mut quiet_since: Option<Instant> = None;
    loop {
        let path = {
            let mut files = locked(&shared.files);
            loop {
                if closed() {
                    return;
                }
                let Some(path) = files.front() else {
                    // Idle meanwhile says nothing about how busy the show was.
                    quiet_since = None;
                    files = shared.wake.wait(files).unwrap_or_else(PoisonError::into_inner);
                    continue;
                };
                if busy() {
                    quiet_since = None;
                } else if quiet_since.get_or_insert_with(Instant::now).elapsed() >= quiet {
                    break path.clone();
                }
                files =
                    shared.wake.wait_timeout(files, PREWARM_POLL).unwrap_or_else(PoisonError::into_inner).0;
            }
        };
        // A file that makes the decoder panic is skipped, not retried forever.
        let built = catch_unwind(AssertUnwindSafe(|| cache.loudness(&path, &|| !busy() && !closed())));
        if !matches!(built, Ok(Err(MediaError::Cancelled))) {
            let mut files = locked(&shared.files);
            if files.front() == Some(&path) {
                files.pop_front();
            }
            locked(&shared.measured).push((path, built.ok().and_then(Result::ok)));
        }
    }
}

/// Marks a stored file as used for pruning, at most once in a while.
fn touch(path: &Path) {
    let used = fs::metadata(path).and_then(|m| m.modified()).ok();
    if used.and_then(|t| t.elapsed().ok()).is_none_or(|age| age > TOUCH_AFTER)
        && let Ok(file) = File::options().write(true).open(path)
    {
        let _ = file.set_modified(SystemTime::now());
    }
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

fn hash_file(path: &Path, keep_going: &dyn Fn() -> bool) -> Result<u128, MediaError> {
    let io = |source| MediaError::Io { path: path.to_owned(), source };
    let mut file = File::open(path).map_err(io)?;
    let (mut hasher, mut buffer) = (Xxh3::new(), vec![0; 1 << 20]);
    loop {
        if !keep_going() {
            return Err(MediaError::Cancelled);
        }
        match file.read(&mut buffer) {
            Ok(0) => return Ok(hasher.digest128()),
            Ok(read) => hasher.update(&buffer[..read]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(io(e)),
        }
    }
}

/// `stamp` as stored in a record; equal bytes mean an equal stamp.
fn stamp_bytes(stamp: &FileStamp) -> Vec<u8> {
    let (kind, since) = match stamp.modified.map(|t| t.duration_since(UNIX_EPOCH)) {
        None => (0u8, Duration::ZERO),
        Some(Ok(after)) => (1, after),
        Some(Err(before)) => (2, before.duration()),
    };
    let mut bytes = stamp.len.to_le_bytes().to_vec();
    bytes.push(kind);
    bytes.extend_from_slice(&since.as_secs().to_le_bytes());
    bytes.extend_from_slice(&since.subsec_nanos().to_le_bytes());
    let (device, inode, changed, changed_nanos) = stamp.id;
    for part in
        [device.to_le_bytes(), inode.to_le_bytes(), changed.to_le_bytes(), changed_nanos.to_le_bytes()]
    {
        bytes.extend_from_slice(&part);
    }
    bytes
}

/// A silent file's missing values are stored as NaN.
fn encode_loudness(hash: u128, loudness: &Loudness) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(LOUDNESS_LEN);
    bytes.extend_from_slice(LOUDNESS_MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&hash.to_le_bytes());
    for value in [loudness.lufs, loudness.peak_db] {
        bytes.extend_from_slice(&value.unwrap_or(f64::NAN).to_le_bytes());
    }
    bytes
}

/// `None` unless `bytes` are a complete loudness of this version for `hash`.
fn decode_loudness(bytes: &[u8], hash: u128) -> Option<Loudness> {
    let rest = bytes.strip_prefix(LOUDNESS_MAGIC)?;
    let (version, rest) = rest.split_first_chunk::<4>()?;
    let (stored, rest) = rest.split_first_chunk::<16>()?;
    let (lufs, rest) = rest.split_first_chunk::<8>()?;
    let peak: &[u8; 8] = rest.try_into().ok()?;
    (u32::from_le_bytes(*version) == FORMAT_VERSION && u128::from_le_bytes(*stored) == hash).then_some(())?;
    let value = |bytes: &[u8; 8]| Some(f64::from_le_bytes(*bytes)).filter(|v| v.is_finite());
    Some(Loudness { lufs: value(lufs), peak_db: value(peak) })
}

fn encode_peaks(hash: u128, peaks: &Peaks) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(PEAKS_HEADER + peaks.blocks.len() * 4);
    bytes.extend_from_slice(PEAKS_MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&hash.to_le_bytes());
    bytes.extend_from_slice(&peaks.rate.to_le_bytes());
    bytes.extend_from_slice(&peaks.frames.to_le_bytes());
    bytes.extend_from_slice(&(BLOCK as u32).to_le_bytes());
    bytes.extend_from_slice(&(peaks.blocks.len() as u64).to_le_bytes());
    for [left, right] in &peaks.blocks {
        bytes.extend_from_slice(&left.to_le_bytes());
        bytes.extend_from_slice(&right.to_le_bytes());
    }
    bytes
}

/// `None` unless `bytes` are complete peaks of this version for `hash`.
fn decode_peaks(bytes: &[u8], hash: u128) -> Option<Peaks> {
    let rest = bytes.strip_prefix(PEAKS_MAGIC)?;
    let (version, rest) = rest.split_first_chunk::<4>()?;
    let (stored_hash, rest) = rest.split_first_chunk::<16>()?;
    let (rate, rest) = rest.split_first_chunk::<4>()?;
    let (frames, rest) = rest.split_first_chunk::<8>()?;
    let (block, rest) = rest.split_first_chunk::<4>()?;
    let (count, rest) = rest.split_first_chunk::<8>()?;
    let (rate, frames, count) =
        (u32::from_le_bytes(*rate), u64::from_le_bytes(*frames), u64::from_le_bytes(*count));
    let valid = u32::from_le_bytes(*version) == FORMAT_VERSION
        && u128::from_le_bytes(*stored_hash) == hash
        && u32::from_le_bytes(*block) as usize == BLOCK
        && rate > 0
        && count == frames.div_ceil(BLOCK as u64)
        && rest.len() as u64 == count * 4;
    valid.then(|| Peaks {
        rate,
        frames,
        blocks: rest
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&[l0, l1, r0, r1]| [u16::from_le_bytes([l0, l1]), u16::from_le_bytes([r0, r1])])
            .collect(),
    })
}

/// The files one folder below `dir` (`dir/ab/file`).
fn files(dir: &Path) -> Vec<(PathBuf, fs::Metadata)> {
    let entries = |dir: &Path| fs::read_dir(dir).into_iter().flatten().flatten().collect::<Vec<_>>();
    entries(dir)
        .iter()
        .flat_map(|sub| entries(&sub.path()))
        .filter_map(|entry| Some((entry.path(), entry.metadata().ok().filter(fs::Metadata::is_file)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jq-peaks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("media")).unwrap();
        dir
    }

    /// Mono 16-bit WAV of `frames` frames at 8 kHz, as loud as `level` (0…1).
    fn write_wav(path: &Path, frames: usize, level: f32) {
        let samples: Vec<i16> =
            (0..frames).map(|i| ((i as f32 * 0.05).sin() * level * i16::MAX as f32) as i16).collect();
        let data_len = (samples.len() * 2) as u32;
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        for field in
            [16u32.to_le_bytes(), [1, 0, 1, 0], 8000u32.to_le_bytes(), 16000u32.to_le_bytes(), [2, 0, 16, 0]]
        {
            bytes.extend_from_slice(&field);
        }
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        samples.iter().for_each(|sample| bytes.extend_from_slice(&sample.to_le_bytes()));
        fs::write(path, bytes).unwrap();
    }

    /// Peak files stored.
    fn stored(dir: &Path) -> usize {
        files(&dir.join("cache/content"))
            .iter()
            .filter(|(path, _)| path.extension().is_some_and(|e| e == "jqpk"))
            .count()
    }

    fn loudest(waveform: &Waveform) -> f32 {
        waveform.peaks.iter().copied().fold(0.0, f32::max)
    }

    #[test]
    fn peaks_are_kept_until_the_content_changes() {
        let dir = test_dir("keep");
        let cache = PeakCache::new(dir.join("cache"));
        let file = dir.join("media/a.wav");
        write_wav(&file, 20_000, 0.5);
        let first = cache.waveform(&file, 100).unwrap();
        assert!((loudest(&first) - 0.5).abs() < 0.01);
        assert_eq!(stored(&dir), 1);
        assert_eq!(cache.waveform(&file, 100).unwrap(), first);

        // Only touched: the content hash finds the same peaks.
        File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(9))
            .unwrap();
        assert_eq!(cache.waveform(&file, 100).unwrap(), first);
        // A copy elsewhere too.
        fs::copy(&file, dir.join("media/b.wav")).unwrap();
        assert_eq!(cache.waveform(&dir.join("media/b.wav"), 100).unwrap(), first);
        assert_eq!(stored(&dir), 1);

        // New content of the same length: new peaks.
        write_wav(&file, 20_000, 0.25);
        assert!((loudest(&cache.waveform(&file, 100).unwrap()) - 0.25).abs() < 0.01);
        assert_eq!(stored(&dir), 2);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn broken_cache_files_are_built_again() {
        let dir = test_dir("broken");
        let cache = PeakCache::new(dir.join("cache"));
        let file = dir.join("media/a.wav");
        write_wav(&file, 20_000, 0.5);
        let first = cache.waveform(&file, 50).unwrap();
        let (stored_peaks, _) = files(&dir.join("cache/content"))
            .into_iter()
            .find(|(p, _)| p.extension().unwrap() == "jqpk")
            .unwrap();
        let bytes = fs::read(&stored_peaks).unwrap();
        fs::write(&stored_peaks, &bytes[..bytes.len() - 3]).unwrap();
        assert_eq!(cache.waveform(&file, 50).unwrap(), first);
        assert_eq!(fs::read(&stored_peaks).unwrap(), bytes, "stored again");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn nothing_is_stored_when_cancelled_or_changed_meanwhile() {
        let dir = test_dir("changed");
        let cache = PeakCache::new(dir.join("cache"));
        let file = dir.join("media/a.wav");
        write_wav(&file, 200_000, 0.5);
        assert!(matches!(cache.peaks(&file, &|| false), Err(MediaError::Cancelled)));
        assert_eq!(cache.size(), 0);

        // Written again while being read: the result is not kept.
        let rewritten = AtomicBool::new(false);
        let rewrite = || {
            if !rewritten.swap(true, Ordering::Relaxed) {
                write_wav(&file, 200_000, 0.25);
            }
            true
        };
        cache.peaks(&file, &rewrite).unwrap();
        assert_eq!(cache.size(), 0);
        assert!((loudest(&cache.waveform(&file, 10).unwrap()) - 0.25).abs() < 0.01);
        assert_eq!(stored(&dir), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn least_recently_used_peaks_go_first() {
        let dir = test_dir("prune");
        let file = |name: &str| dir.join("media").join(name);
        // Each file's peaks and loudness take some 1.65 kB: pruning on the
        // third keeps two.
        let cache = PeakCache::with_limit(Some(dir.join("cache")), 4800);
        for (i, name) in ["a.wav", "b.wav", "c.wav"].iter().enumerate() {
            write_wav(&file(name), 100_000, 0.2 + i as f32 * 0.1);
            cache.waveform(&file(name), 10).unwrap();
            // Distinct use times, oldest first.
            for (path, _) in files(&dir.join("cache/content")) {
                let age = fs::metadata(&path).unwrap().modified().unwrap() - Duration::from_secs(10);
                File::options().write(true).open(&path).unwrap().set_modified(age).unwrap();
            }
        }
        assert_eq!(stored(&dir), 2, "a went");

        cache.clean_up();
        assert_eq!(files(&dir.join("cache/paths")).len(), 2, "a's record went too");
        cache.clear().unwrap();
        assert_eq!(cache.size(), 0);
        assert!(cache.waveform(&file("a.wav"), 10).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn works_without_a_cache_folder() {
        let dir = test_dir("disabled");
        let file = dir.join("media/a.wav");
        write_wav(&file, 20_000, 0.5);
        let cache = PeakCache::disabled();
        assert!((loudest(&cache.waveform(&file, 10).unwrap()) - 0.5).abs() < 0.01);
        assert!(cache.waveform_range(&file, 0.0, 1.0, 10).is_ok());
        assert!(cache.waveform(&dir.join("missing.wav"), 10).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prewarmer_waits_for_a_quiet_show() {
        let dir = test_dir("prewarm");
        let file = dir.join("media/a.wav");
        write_wav(&file, 20_000, 0.5);
        let cache = Arc::new(PeakCache::new(dir.join("cache")));
        let prewarmer = Prewarmer::new(cache.clone(), Duration::from_millis(50));
        prewarmer.set_busy(true);
        prewarmer.request(file.clone());
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(cache.size(), 0, "nothing read while busy");
        prewarmer.set_busy(false);
        let deadline = Instant::now() + Duration::from_secs(5);
        while stored(&dir) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(stored(&dir), 1);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut measured = Vec::new();
        while measured.is_empty() && Instant::now() < deadline {
            measured = prewarmer.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(measured.len(), 1);
        assert_eq!(measured[0].0, file);
        assert!(measured[0].1.is_some_and(|l| l.lufs.is_some()));
        drop(prewarmer);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loudness_is_kept_beside_the_peaks() {
        let dir = test_dir("loudness");
        let cache = PeakCache::new(dir.join("cache"));
        let file = dir.join("media/a.wav");
        write_wav(&file, 40_000, 0.5);
        let first = cache.loudness(&file, &|| true).unwrap();
        assert!(first.lufs.is_some_and(|l| l < -5.0 && l > -15.0), "{first:?}");
        assert_eq!(stored(&dir), 1, "the peaks came along");
        assert_eq!(cache.loudness(&file, &|| false).unwrap(), first, "from the cache, nothing decoded");

        // Peaks from before loudness was kept: only the loudness is measured.
        let (loudness, _) = files(&dir.join("cache/content"))
            .into_iter()
            .find(|(p, _)| p.extension().unwrap() == "jqld")
            .unwrap();
        fs::remove_file(&loudness).unwrap();
        assert!(cache.peaks(&file, &|| false).is_ok(), "peaks still there");
        assert_eq!(cache.loudness(&file, &|| true).unwrap(), first);
        assert!(loudness.exists());

        // A silent file has none.
        let silent = dir.join("media/silent.wav");
        write_wav(&silent, 20_000, 0.0);
        let none = cache.loudness(&silent, &|| true).unwrap();
        assert_eq!((none.lufs, none.peak_db), (None, None));
        assert_eq!(cache.loudness(&silent, &|| false).unwrap(), none);
        fs::remove_dir_all(dir).unwrap();
    }
}
