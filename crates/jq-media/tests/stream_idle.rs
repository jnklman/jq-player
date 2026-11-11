//! A streamed buffer nobody plays must not keep its reader thread busy. A
//! test binary of its own, so no other test's reader threads count.
#![cfg(target_os = "linux")]

use std::path::Path;
use std::time::{Duration, Instant};

use jq_engine::stream::CHUNK_FRAMES;

/// Times the reader threads (`jq-stream`) of this process were woken.
fn reader_wakeups() -> u64 {
    let mut total = 0;
    for task in std::fs::read_dir("/proc/self/task").unwrap().flatten() {
        let comm = std::fs::read_to_string(task.path().join("comm")).unwrap_or_default();
        if comm.trim() != "jq-stream" {
            continue;
        }
        let status = std::fs::read_to_string(task.path().join("status")).unwrap_or_default();
        total += status
            .lines()
            .filter(|l| {
                l.starts_with("voluntary_ctxt_switches") || l.starts_with("nonvoluntary_ctxt_switches")
            })
            .filter_map(|l| l.split_whitespace().last()?.parse::<u64>().ok())
            .sum::<u64>();
    }
    total
}

fn write_wav(path: &Path, rate: u32, frames: usize) {
    let data_len = (frames * 2) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        b.extend_from_slice(&(((i as f32 * 0.06).sin() * 10_000.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

fn wait(what: &str, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn idle_stream_reader_sleeps() {
    let dir = std::env::temp_dir().join(format!("jq-media-idle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("long.wav");
    write_wav(&path, 8_000, CHUNK_FRAMES * 6);
    let buf = jq_media::stream::open_stream(&path, 8_000).unwrap();
    let data = buf.stream().unwrap().clone();
    wait("start loaded", || buf.is_ready_at(0));

    // Armed but not playing: the reader sleeps (it used to look every 10 ms).
    std::thread::sleep(Duration::from_millis(50));
    let before = reader_wakeups();
    std::thread::sleep(Duration::from_millis(500));
    assert!(reader_wakeups() - before <= 2, "reader woke while idle");

    // Yet a new start point is loaded right away …
    let late = CHUNK_FRAMES * 4 + 10;
    data.set_hints(vec![late]);
    wait("new start point loaded", || buf.is_ready_at(late));
    // … and a playing voice is followed.
    let slot = data.claim_slot(1).unwrap();
    data.report_position(slot, CHUNK_FRAMES + 10);
    wait("playing position followed", || data.is_loaded(2));
    data.release_slot(1);

    // Dropping the buffer ends the thread.
    drop((buf, data));
    wait("reader thread ended", || reader_wakeups() == 0);
    std::fs::remove_dir_all(dir).unwrap();
}
