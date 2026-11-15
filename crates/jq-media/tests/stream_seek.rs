//! A seek must give the same samples as decoding from the start: a streamed
//! buffer stitches chunks read after seeks to chunks read straight through,
//! and any difference clicks at the seam.

use std::path::Path;

use jq_media::stream::StreamDecoder;

/// Frames compared after each seek: more than one codec frame at either rate.
const COMPARED: usize = 4096;

/// 3 s of a sine plus pink noise at 44.1 kHz stereo, encoded by ffmpeg in
/// `tests/fixtures`: a lossy decoder that starts cold after a seek gets its
/// first frames wrong.
fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn decode(file: &str, rate: u32, seek: Option<usize>, frames: usize) -> Vec<f32> {
    let mut decoder = StreamDecoder::open(Path::new(&fixture(file)), Some(rate)).unwrap();
    if let Some(frame) = seek {
        decoder.seek(frame).unwrap();
    }
    let mut samples = Vec::new();
    decoder.read(&mut samples, frames).unwrap();
    samples
}

fn assert_seeks_match_straight_decode(file: &str, rate: u32) {
    let straight = decode(file, rate, None, usize::MAX);
    let frames = straight.len() / 2;
    assert!(frames > rate as usize * 2, "fixture too short: {frames} frames");
    // Targets spread over the file and over the positions within a codec frame.
    for target in (1_000..frames - COMPARED).step_by(9_973) {
        let seeked = decode(file, rate, Some(target), COMPARED);
        let reference = &straight[target * 2..(target + COMPARED) * 2];
        let worst = reference
            .iter()
            .zip(&seeked)
            .enumerate()
            .map(|(i, (a, b))| (i / 2, (a - b).abs()))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        assert!(
            worst.1 < 1e-4,
            "{file} at {rate} Hz, seek to {target}: off by {} at frame {}",
            worst.1,
            worst.0
        );
    }
}

/// LAME 96 kbps; MP3 also needs the bit reservoir of the frames before.
#[test]
fn mp3_seek_matches_straight_decode() {
    assert_seeks_match_straight_decode("seek.mp3", 44_100);
    assert_seeks_match_straight_decode("seek.mp3", 48_000);
}

/// AAC-LC 96 kbps in MP4 (ffmpeg's encoder, `-aac_pns 0`: noise substitution
/// draws random noise in the decoder, which no seek can reproduce).
#[test]
fn aac_seek_matches_straight_decode() {
    assert_seeks_match_straight_decode("seek.m4a", 44_100);
    assert_seeks_match_straight_decode("seek.m4a", 48_000);
}

/// Vorbis quality 3 in Ogg (libvorbis). The decoder puts out nothing for the
/// first packet after a reset, so the seek must not count from where it landed.
#[test]
fn vorbis_seek_matches_straight_decode() {
    assert_seeks_match_straight_decode("seek.ogg", 44_100);
    assert_seeks_match_straight_decode("seek.ogg", 48_000);
}
