use std::sync::Arc;
use std::time::Duration;

use crate::*;

const RATE: u32 = 1000;

/// Buffer whose left sample at frame `i` is `i / 1000`, right is negated, so
/// the output reveals exactly which frame was played.
fn ramp_buffer(frames: usize) -> Arc<AudioBuffer> {
    let samples = (0..frames).flat_map(|i| [i as f32 / 1000.0, -(i as f32) / 1000.0]).collect();
    Arc::new(AudioBuffer::from_interleaved_stereo(RATE, samples))
}

fn constant_buffer(frames: usize, value: f32) -> Arc<AudioBuffer> {
    Arc::new(AudioBuffer::from_interleaved_stereo(RATE, vec![value; frames * 2]))
}

fn setup(channels: usize) -> (Controller, Mixer) {
    engine(EngineConfig { report_rate_hz: 1, ..EngineConfig::new(RATE, channels) })
}

/// Renders `frames` frames and returns the left channel as played frame indices.
fn played_frames(mixer: &mut Mixer, frames: usize) -> Vec<i32> {
    let mut out = vec![0.0; frames * mixer.channels()];
    mixer.process(&mut out);
    out.chunks(mixer.channels()).map(|f| (f[0] * 1000.0).round() as i32).collect()
}

fn finished(ctl: &mut Controller) -> Vec<VoiceId> {
    ctl.poll_events()
        .filter_map(|e| match e {
            Event::Finished { id, .. } => Some(id),
            _ => None,
        })
        .collect()
}

#[test]
fn plays_to_end_and_returns_buffer() {
    let (mut ctl, mut mix) = setup(2);
    let buf = ramp_buffer(5);
    let id = ctl.play(VoiceParams::new(buf.clone())).unwrap();
    assert_eq!(played_frames(&mut mix, 8), [0, 1, 2, 3, 4, 0, 0, 0]);
    let events: Vec<_> = ctl.poll_events().collect();
    assert!(matches!(events[0], Event::Started(i) if i == id));
    let Event::Finished { id: fid, buffer, .. } = &events[1] else { panic!("{events:?}") };
    assert_eq!(*fid, id);
    assert!(Arc::ptr_eq(buffer, &buf));
    assert_eq!(mix.active_voices(), 0);
}

#[test]
fn start_and_end_position() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.start = 3;
    p.end = Some(6);
    ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 5), [3, 4, 5, 0, 0]);
}

#[test]
fn loop_jump_is_sample_exact() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.loops = [LoopRegion { start: 2, end: 5, repeats: Some(2) }].into();
    ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 18), [0, 1, 2, 3, 4, 2, 3, 4, 2, 3, 4, 5, 6, 7, 8, 9, 0, 0]);
}

#[test]
fn vamp_runs_until_released_at_loop_end() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(8));
    p.loops = [LoopRegion { start: 2, end: 5, repeats: None }].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 10), [0, 1, 2, 3, 4, 2, 3, 4, 2, 3]);
    ctl.exit_loop(id, LoopExit::AtLoopEnd, false, false).unwrap();
    assert_eq!(played_frames(&mut mix, 6), [4, 5, 6, 7, 0, 0]);
}

#[test]
fn vamp_exit_immediate_jumps_to_loop_end() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.loops = [LoopRegion { start: 2, end: 6, repeats: None }].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 3), [0, 1, 2]);
    ctl.exit_loop(id, LoopExit::Immediate { crossfade_frames: 0 }, false, false).unwrap();
    assert_eq!(played_frames(&mut mix, 5), [6, 7, 8, 9, 0]);
}

#[test]
fn loop_crossfade_keeps_level_constant() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(100, 0.5));
    p.loops = [LoopRegion { start: 10, end: 40, repeats: None }].into();
    p.loop_crossfade_frames = 8;
    ctl.play(p).unwrap();
    let mut out = vec![0.0; 200 * 2];
    mix.process(&mut out);
    assert!(out.chunks(2).all(|f| (f[0] - 0.5).abs() < 1e-6), "{out:?}");
}

#[test]
fn loop_crossfade_blends_tail_into_loop_start() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(20));
    p.loops = [LoopRegion { start: 2, end: 6, repeats: Some(1) }].into();
    p.loop_crossfade_frames = 2;
    ctl.play(p).unwrap();
    let mut out = vec![0.0; 9 * 2];
    mix.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|f| f[0] * 1000.0).collect();
    // Frame 6 jumps to 2: weight 1.0 on tail frame 6, then 0.5/0.5 (7 and 3).
    assert_eq!(left[..6], [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    assert!((left[6] - 6.0).abs() < 1e-3);
    assert!((left[7] - 5.0).abs() < 1e-3);
    assert!((left[8] - 4.0).abs() < 1e-3);
}

#[test]
fn layered_voices_are_summed() {
    let (mut ctl, mut mix) = setup(2);
    ctl.play(VoiceParams::new(constant_buffer(10, 0.25))).unwrap();
    ctl.play(VoiceParams::new(constant_buffer(5, 0.5))).unwrap();
    let mut out = vec![0.0; 8 * 2];
    mix.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
    assert_eq!(left, [0.75, 0.75, 0.75, 0.75, 0.75, 0.25, 0.25, 0.25]);
    assert_eq!(mix.active_voices(), 1);
}

#[test]
fn fade_out_stops_voice() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(100, 1.0))).unwrap();
    played_frames(&mut mix, 1);
    ctl.fade_out(id, Duration::from_millis(4), Curve::Linear).unwrap();
    let mut out = vec![0.0; 6 * 2];
    mix.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
    assert_eq!(left, [1.0, 0.75, 0.5, 0.25, 0.0, 0.0]);
    assert_eq!(finished(&mut ctl), [id]);
}

#[test]
fn fade_in_reaches_gain() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(10, 1.0));
    p.gain = 0.5;
    p.fade_in_frames = 2;
    ctl.play(p).unwrap();
    let mut out = vec![0.0; 4 * 2];
    mix.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
    assert_eq!(left, [0.0, 0.25, 0.5, 0.5]);
}

#[test]
fn stop_all_and_fade_all() {
    let (mut ctl, mut mix) = setup(2);
    let a = ctl.play(VoiceParams::new(constant_buffer(100, 0.1))).unwrap();
    let b = ctl.play(VoiceParams::new(constant_buffer(100, 0.1))).unwrap();
    played_frames(&mut mix, 1);
    ctl.fade_all(Duration::from_millis(10), Curve::SCurve).unwrap();
    played_frames(&mut mix, 20);
    let mut done = finished(&mut ctl);
    done.sort();
    assert_eq!(done, [a, b]);

    ctl.play(VoiceParams::new(constant_buffer(100, 0.1))).unwrap();
    played_frames(&mut mix, 1);
    ctl.stop_all().unwrap();
    // STOP_FADE is 10 frames at 1 kHz.
    played_frames(&mut mix, 10);
    assert!(played_frames(&mut mix, 3).iter().all(|&s| s == 0));
    assert_eq!(mix.active_voices(), 0);
}

#[test]
fn stop_blends_out_quickly_instead_of_cutting() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(100, 1.0))).unwrap();
    left(&mut mix, 2);
    ctl.stop(id).unwrap();
    let out = left(&mut mix, 12);
    assert!(close(out[0], 1.0) && close(out[5], 0.5) && close(out[9], 0.1), "{out:?}");
    assert_eq!(out[10..], [0.0, 0.0]);
    assert_eq!(finished(&mut ctl), [id]);
    // A fade of 0 s stops the same way.
    let id = ctl.play(VoiceParams::new(constant_buffer(100, 1.0))).unwrap();
    left(&mut mix, 1);
    ctl.fade_out(id, Duration::ZERO, Curve::Linear).unwrap();
    assert!(close(left(&mut mix, 6)[5], 0.5));
    left(&mut mix, 5);
    assert_eq!(finished(&mut ctl), [id]);
}

#[test]
fn routes_to_output_channel_pair() {
    let (mut ctl, mut mix) = setup(8);
    let mut p = VoiceParams::new(constant_buffer(1, 0.5));
    p.output_channel = 2;
    ctl.play(p).unwrap();
    let mut out = vec![0.0; 8];
    mix.process(&mut out);
    assert_eq!(out, [0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn rejects_invalid_params() {
    let (mut ctl, _mix) = setup(2);
    let other_rate = Arc::new(AudioBuffer::from_interleaved_stereo(44_100, vec![0.0; 4]));
    assert!(matches!(ctl.play(VoiceParams::new(other_rate)), Err(EngineError::SampleRateMismatch { .. })));
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.output_channel = 1;
    assert!(matches!(ctl.play(p), Err(EngineError::OutputChannel { .. })));
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.loops = [LoopRegion { start: 5, end: 5, repeats: None }].into();
    assert!(matches!(ctl.play(p), Err(EngineError::InvalidRegion(_))));
    let mut p = VoiceParams::new(ramp_buffer(10));
    let vamp = |start, end| LoopRegion { start, end, repeats: None };
    p.loops = [vamp(1, 5), vamp(4, 8)].into();
    assert!(matches!(ctl.play(p), Err(EngineError::InvalidRegion(_))), "overlapping loops");
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.marks = [5, 3].into();
    assert!(matches!(ctl.play(p), Err(EngineError::InvalidRegion(_))), "unsorted marks");
}

#[test]
fn voice_limit_rejects_and_returns_buffer() {
    let (mut ctl, mut mix) = engine(EngineConfig { max_voices: 1, ..EngineConfig::new(RATE, 2) });
    ctl.play(VoiceParams::new(constant_buffer(10, 0.1))).unwrap();
    let second = ctl.play(VoiceParams::new(constant_buffer(10, 0.1))).unwrap();
    played_frames(&mut mix, 1);
    assert!(ctl.poll_events().any(|e| matches!(e, Event::Rejected { id, .. } if id == second)));
}

#[test]
fn reports_positions() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(5000));
    p.loops = [LoopRegion { start: 0, end: 4000, repeats: None }].into();
    let id = ctl.play(p).unwrap();
    played_frames(&mut mix, 1000);
    let pos = ctl.poll_events().find_map(|e| match e {
        Event::Position { id: i, frame, looping, .. } if i == id => Some((frame, looping)),
        _ => None,
    });
    assert_eq!(pos, Some((1000, true)));
}

#[test]
fn reports_the_pass_through_each_loop() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(5000));
    // Vamp-like: frames 1000–2000 three times (two jumps back), then 3000–4000 until released.
    p.loops = [
        LoopRegion { start: 1000, end: 2000, repeats: Some(2) },
        LoopRegion { start: 3000, end: 4000, repeats: None },
    ]
    .into();
    let id = ctl.play(p).unwrap();
    // Reports come once per 1000 frames (RATE 1000, report rate 1 Hz): the
    // first after 1500 frames, in the middle of the first pass, then one per
    // pass.
    let mut passes = Vec::new();
    for frames in [500, 1000, 1000, 1000, 1000, 1000, 1000] {
        played_frames(&mut mix, frames);
        passes.extend(ctl.poll_events().filter_map(|e| match e {
            Event::Position { id: i, pass, .. } if i == id => Some(pass.map(|p| (p.index, p.pass))),
            _ => None,
        }));
    }
    assert_eq!(
        passes,
        [Some((0, 1)), Some((0, 2)), Some((0, 3)), None, Some((1, 1)), Some((1, 2))],
        "through the first loop, between the loops, then the open loop"
    );
}

#[test]
fn end_fade_reaches_silence_at_end() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(8, 1.0));
    p.end_fade_frames = 4;
    ctl.play(p).unwrap();
    let mut out = vec![0.0; 9 * 2];
    mix.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
    assert_eq!(left, [1.0, 1.0, 1.0, 1.0, 1.0, 0.75, 0.5, 0.25, 0.0]);
}

#[test]
fn decibel_end_fade_falls_evenly_in_db() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(8, 1.0));
    p.end_fade_frames = 4;
    p.fade_curve = Curve::decibel(-60.0, Shape::Linear);
    ctl.play(p).unwrap();
    let mut out = vec![0.0; 9 * 2];
    mix.process(&mut out);
    let db: Vec<f32> = out.chunks(2).skip(4).map(|f| gain_to_db(f[0])).collect();
    for (got, want) in db.iter().zip([0.0, -15.0, -30.0, -45.0, f32::NEG_INFINITY]) {
        assert!(got == &want || (got - want).abs() < 1e-3, "{db:?}");
    }
}

#[test]
fn end_fade_waits_for_last_loop_pass() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(4, 1.0));
    p.loops = [LoopRegion { start: 0, end: 4, repeats: Some(1) }].into();
    p.end_fade_frames = 2;
    ctl.play(p).unwrap();
    let mut out = vec![0.0; 9 * 2];
    mix.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
    assert_eq!(left, [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.0]);
}

#[test]
fn end_blend_ramps_out_at_an_end_mark() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(10, 1.0));
    p.end = Some(6);
    p.end_blend_frames = 4;
    ctl.play(p).unwrap();
    assert_eq!(left(&mut mix, 8), [1.0, 1.0, 1.0, 0.75, 0.5, 0.25, 0.0, 0.0]);
}

#[test]
fn end_blend_waits_for_last_loop_pass() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(4, 1.0));
    p.loops = [LoopRegion { start: 0, end: 4, repeats: Some(1) }].into();
    p.end_blend_frames = 2;
    ctl.play(p).unwrap();
    assert_eq!(left(&mut mix, 9), [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.0]);
}

#[test]
fn end_blend_is_independent_of_the_envelope() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(20, 1.0));
    p.end_blend_frames = 4;
    let id = ctl.play(p).unwrap();
    left(&mut mix, 2);
    ctl.ramp(id, 0.5, Duration::from_millis(100), Curve::Linear, false).unwrap();
    left(&mut mix, 2);
    ctl.hold_fade(id, true).unwrap();
    let out = left(&mut mix, 17);
    let held = out[0];
    assert!(held < 1.0 && out[..12].iter().all(|&s| s == held), "{out:?}");
    assert!(close(out[13], held * 0.75) && close(out[15], held * 0.25), "{out:?}");
    assert_eq!(out[16], 0.0);
}

#[test]
fn end_blend_follows_a_seek_back() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(100, 0.5));
    p.end_blend_frames = 10;
    let id = ctl.play(p).unwrap();
    assert!(left(&mut mix, 95)[94] < 0.5, "end blend running");
    ctl.seek(id, 20, 0).unwrap();
    assert_eq!(left(&mut mix, 2), [0.5, 0.5], "full level again");
}

#[test]
fn end_blend_ramps_out_at_another_rate() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(40, 1.0));
    p.end = Some(20);
    (p.rate, p.pitch) = (0.5, 0.5);
    p.end_blend_frames = 8;
    ctl.play(p).unwrap();
    let out = left(&mut mix, 60);
    let last = out.iter().rposition(|&s| s != 0.0).unwrap();
    // 8 output frames are 4 file frames at half speed.
    assert!(out[last] < 0.2 && out[last - 5..=last].iter().all(|&s| s <= 0.75), "{out:?}");
    assert!(out[last - 12..last - 6].iter().all(|&s| s > 0.9), "{out:?}");
}

#[test]
fn pan_attenuates_opposite_side() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(1, 1.0));
    p.pan = 0.5;
    ctl.play(p).unwrap();
    let mut out = vec![0.0; 2];
    mix.process(&mut out);
    assert_eq!(out, [0.5, 1.0]);
}

#[test]
fn trim_ramps_without_disturbing_fade_out() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(100, 1.0))).unwrap();
    played_frames(&mut mix, 1);
    ctl.fade_out(id, Duration::from_millis(40), Curve::Linear).unwrap();
    // 20 ms smoothing = 20 frames at the test rate.
    ctl.set_trim(id, 0.5).unwrap();
    let mut out = vec![0.0; 50 * 2];
    mix.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
    let expect = |i: usize| (1.0 - i as f32 / 40.0).max(0.0) * (1.0 - 0.5 * (i.min(20) as f32 / 20.0));
    for (i, &v) in left.iter().enumerate() {
        assert!((v - expect(i)).abs() < 1e-6, "frame {i}: {v} != {}", expect(i));
    }
    assert_eq!(finished(&mut ctl), [id]);
}

#[test]
fn trim_is_reported_in_position_gain() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(5000, 1.0));
    p.gain = 0.5;
    let id = ctl.play(p).unwrap();
    ctl.set_trim(id, 0.5).unwrap();
    played_frames(&mut mix, 1000);
    let gain = ctl.poll_events().find_map(|e| match e {
        Event::Position { gain, .. } => Some(gain),
        _ => None,
    });
    assert_eq!(gain, Some(0.25));
}

#[test]
fn trim_rejects_nonsense() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(100, 0.01))).unwrap();
    ctl.set_trim(id, f32::NAN).unwrap();
    played_frames(&mut mix, 30);
    assert!(played_frames(&mut mix, 1).iter().all(|&s| s == 0));
    ctl.set_trim(id, f32::INFINITY).unwrap();
    let mut out = vec![0.0; 30 * 2];
    mix.process(&mut out);
    assert!((out[58] - 0.16).abs() < 1e-6, "{}", out[58]);
}

#[test]
fn pan_moves_smoothly_while_playing() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(100, 1.0))).unwrap();
    played_frames(&mut mix, 1);
    ctl.set_pan(id, -1.0).unwrap();
    let mut out = vec![0.0; 22 * 2];
    mix.process(&mut out);
    let right: Vec<f32> = out.chunks(2).map(|f| f[1]).collect();
    for (i, &v) in right.iter().enumerate() {
        let expect = 1.0 - i.min(20) as f32 / 20.0;
        assert!((v - expect).abs() < 1e-6, "frame {i}: {v} != {expect}");
    }
    assert!(out.chunks(2).all(|f| f[0] == 1.0));
}

fn vamp(start: usize, end: usize) -> LoopRegion {
    LoopRegion { start, end, repeats: None }
}

#[test]
fn several_vamps_are_left_one_after_the_other() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(12));
    p.loops = [vamp(1, 3), vamp(6, 8)].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 5), [0, 1, 2, 1, 2]);
    ctl.exit_loop(id, LoopExit::AtLoopEnd, false, false).unwrap();
    assert_eq!(played_frames(&mut mix, 9), [3, 4, 5, 6, 7, 6, 7, 6, 7]);
    ctl.exit_loop(id, LoopExit::AtLoopEnd, false, false).unwrap();
    assert_eq!(played_frames(&mut mix, 6), [8, 9, 10, 11, 0, 0]);
}

#[test]
fn devamp_ahead_releases_the_next_vamp_too() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.loops = [vamp(1, 3), vamp(5, 7)].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 3), [0, 1, 2]);
    // Two devamps while still in vamp 1: both vamps are played through once.
    ctl.exit_loop(id, LoopExit::AtLoopEnd, false, false).unwrap();
    ctl.exit_loop(id, LoopExit::Immediate { crossfade_frames: 0 }, false, false).unwrap();
    assert_eq!(played_frames(&mut mix, 8), [3, 4, 5, 6, 7, 8, 9, 0]);
}

#[test]
fn devamp_before_reaching_the_vamp_plays_it_once() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(8));
    p.loops = [vamp(4, 6)].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 2), [0, 1]);
    // Immediate does not skip material that was never played.
    ctl.exit_loop(id, LoopExit::Immediate { crossfade_frames: 0 }, false, false).unwrap();
    assert_eq!(played_frames(&mut mix, 8), [2, 3, 4, 5, 6, 7, 0, 0]);
}

#[test]
fn devamp_at_next_mark_jumps_to_loop_end() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(14));
    p.loops = [vamp(2, 10)].into();
    p.marks = [0, 4, 6, 8, 12].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 5), [0, 1, 2, 3, 4]);
    ctl.exit_loop(id, LoopExit::AtNextMark { crossfade_frames: 0 }, false, false).unwrap();
    // Position 5: the next mark is 6, from there on the exit at 10.
    assert_eq!(played_frames(&mut mix, 6), [5, 10, 11, 12, 13, 0]);
}

#[test]
fn devamp_at_next_mark_without_mark_waits_for_loop_end() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(8));
    p.loops = [vamp(2, 5)].into();
    p.marks = [1, 6].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 4), [0, 1, 2, 3]);
    ctl.exit_loop(id, LoopExit::AtNextMark { crossfade_frames: 0 }, false, false).unwrap();
    assert_eq!(played_frames(&mut mix, 5), [4, 5, 6, 7, 0]);
}

#[test]
fn reports_looping_only_inside_an_unreleased_vamp() {
    let (mut ctl, mut mix) = engine(EngineConfig { report_rate_hz: RATE, ..EngineConfig::new(RATE, 2) });
    let mut p = VoiceParams::new(ramp_buffer(8));
    p.loops = [vamp(2, 4)].into();
    ctl.play(p).unwrap();
    let looping: Vec<bool> = (0..6)
        .map(|_| {
            played_frames(&mut mix, 1);
            ctl.poll_events()
                .filter_map(|e| match e {
                    Event::Position { looping, .. } => Some(looping),
                    _ => None,
                })
                .last()
                .unwrap()
        })
        .collect();
    // Positions after each frame: 1, 2, 3, 2 (jump), 3, 2.
    assert_eq!(looping, [false, true, true, true, true, true]);
}

#[test]
fn seek_jumps_and_passes_vamps_behind() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(12));
    p.loops = [vamp(1, 3), vamp(6, 8)].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 4), [0, 1, 2, 1]);
    // Over vamp 1 into vamp 2: vamp 1 is passed, vamp 2 still holds.
    ctl.seek(id, 5, 0).unwrap();
    assert_eq!(played_frames(&mut mix, 6), [5, 6, 7, 6, 7, 6]);
    // Back before vamp 1: it was passed, so it plays through once.
    ctl.seek(id, 0, 0).unwrap();
    assert_eq!(played_frames(&mut mix, 9), [0, 1, 2, 3, 4, 5, 6, 7, 6]);
    ctl.seek(id, 100, 0).unwrap();
    assert_eq!(played_frames(&mut mix, 2), [0, 0]);
    assert_eq!(mix.active_voices(), 0, "seek past the end finishes");
}

#[test]
fn seek_back_undoes_a_started_end_fade() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(100, 0.5));
    p.end_fade_frames = 10;
    let id = ctl.play(p).unwrap();
    let mut out = vec![0.0; 95 * 2];
    mix.process(&mut out);
    assert!(out[94 * 2] < 0.5, "end fade running");
    ctl.seek(id, 20, 0).unwrap();
    let mut out = vec![0.0; 2 * 2];
    mix.process(&mut out);
    assert_eq!(out[2], 0.5, "level restored");
}

/// A stream of `chunks` chunks whose left sample at frame `i` is `i / 1e6`;
/// only the chunks in `loaded` are present.
fn stream_buffer(chunks: usize, loaded: &[usize]) -> (Arc<AudioBuffer>, Arc<stream::StreamData>) {
    let frames = chunks * stream::CHUNK_FRAMES;
    let data = Arc::new(stream::StreamData::new(frames));
    for &c in loaded {
        let from = c * stream::CHUNK_FRAMES;
        let samples = (from..from + stream::CHUNK_FRAMES).flat_map(|i| [i as f32 / 1e6, 0.0]).collect();
        assert!(data.store(c, samples).is_none());
    }
    (Arc::new(AudioBuffer::streamed(RATE, data.clone())), data)
}

#[test]
fn stream_plays_loaded_chunks_and_fills_gaps_with_silence() {
    let (mut ctl, mut mix) = setup(2);
    let (buf, data) = stream_buffer(3, &[0, 2]);
    let mut p = VoiceParams::new(buf.clone());
    p.start = stream::CHUNK_FRAMES - 2;
    ctl.play(p).unwrap();
    let mut out = vec![0.0; 4 * 2];
    mix.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
    let last = (stream::CHUNK_FRAMES - 1) as f32 / 1e6;
    assert_eq!(left, [last - 1e-6, last, 0.0, 0.0], "chunk 1 is missing");
    assert_eq!(data.underruns(), 2);
    assert_eq!(data.positions().collect::<Vec<_>>(), [stream::CHUNK_FRAMES + 2], "reader can follow");
    assert!(buf.is_ready_at(0) && !buf.is_ready_at(stream::CHUNK_FRAMES));
}

#[test]
fn stream_waits_for_its_start_chunk_instead_of_playing_silence() {
    let (mut ctl, mut mix) = setup(2);
    let (buf, data) = stream_buffer(2, &[0]);
    let start = stream::CHUNK_FRAMES + 5;
    let mut p = VoiceParams::new(buf);
    p.start = start;
    p.wait_frames = 100;
    let id = ctl.play(p).unwrap();
    let mut out = vec![0.0; 10 * 2];
    mix.process(&mut out);
    assert!(out.iter().all(|&s| s == 0.0));
    assert_eq!(data.underruns(), 0, "waits, does not play silence");
    assert_eq!(data.positions().collect::<Vec<_>>(), [start], "stands still where the reader should load");
    assert!(!ctl.poll_events().any(|e| matches!(e, Event::Started(_))), "not started yet");

    let samples =
        (stream::CHUNK_FRAMES..2 * stream::CHUNK_FRAMES).flat_map(|i| [i as f32 / 1e6, 0.0]).collect();
    data.store(1, samples);
    mix.process(&mut out);
    assert_eq!(out[0], start as f32 / 1e6, "starts at its first frame once loaded");
    assert!(ctl.poll_events().any(|e| matches!(e, Event::Started(i) if i == id)));
}

#[test]
fn stream_wait_is_bounded_and_ends_on_a_stop() {
    let (mut ctl, mut mix) = setup(2);
    let (buf, data) = stream_buffer(2, &[0]);
    let mut p = VoiceParams::new(buf.clone());
    p.start = stream::CHUNK_FRAMES;
    p.wait_frames = 20;
    let late = ctl.play(p.clone()).unwrap();
    let mut out = vec![0.0; 10 * 2];
    mix.process(&mut out);
    mix.process(&mut out);
    assert_eq!(data.underruns(), 0);
    mix.process(&mut out);
    assert_eq!(data.underruns(), 10, "plays (silence) once the wait is over");
    assert!(ctl.poll_events().any(|e| matches!(e, Event::Started(i) if i == late)));

    ctl.stop(late).unwrap();
    let waiting = ctl.play(VoiceParams { wait_frames: 1000, ..p }).unwrap();
    mix.process(&mut out);
    ctl.poll_events().for_each(drop);
    ctl.stop(waiting).unwrap();
    mix.process(&mut out);
    let events: Vec<_> = ctl.poll_events().collect();
    assert!(
        matches!(events.as_slice(), [Event::Started(a), Event::Finished { id: b, .. }]
        if *a == waiting && *b == waiting),
        "a stop while waiting ends it at once: {events:?}"
    );
}

#[test]
fn stream_holds_loops_and_ends_where_the_file_really_ends() {
    let (mut ctl, mut mix) = setup(2);
    let (buf, data) = stream_buffer(2, &[0, 1]);
    let mut p = VoiceParams::new(buf);
    p.loops = [LoopRegion { start: 10, end: 20, repeats: Some(1) }].into();
    ctl.play(p).unwrap();
    assert_eq!(data.pins(), vec![(10..20 + 500)], "loop pinned with margin (RATE 1000 → 500 frames)");
    // The reader found the end early.
    data.set_frames(100);
    let mut out = vec![0.0; 200 * 2];
    mix.process(&mut out);
    assert_eq!(mix.active_voices(), 0);
    let finished: Vec<_> = ctl.poll_events().filter(|e| matches!(e, Event::Finished { .. })).collect();
    assert_eq!(finished.len(), 1);
    assert!(data.pins().is_empty(), "pins released with the voice");
    assert_eq!(data.positions().count(), 0);
}

#[test]
fn voices_on_one_stream_keep_their_own_position_and_pins() {
    let (mut ctl, mut mix) = setup(2);
    let (buf, data) = stream_buffer(2, &[0, 1]);
    // More voices than slots, so ids that once shared a slot meet as well.
    let mut ids = Vec::new();
    for i in 0..stream::POSITION_SLOTS + 1 {
        let mut p = VoiceParams::new(buf.clone());
        p.start = i * 100;
        p.end = Some(i * 100 + 50);
        p.loops = [LoopRegion { start: i * 100, end: i * 100 + 10, repeats: Some(1) }].into();
        ids.push(ctl.play(p).unwrap());
    }
    assert_eq!(data.pins().len(), stream::POSITION_SLOTS, "one voice got no slot");
    let mut out = vec![0.0; 10 * 2];
    mix.process(&mut out);
    let positions: Vec<usize> = data.positions().collect();
    assert_eq!(positions.len(), stream::POSITION_SLOTS);

    // The first voice ends; nobody else's position or pins go with it.
    ctl.stop(ids[0]).unwrap();
    mix.process(&mut out);
    assert_eq!(finished(&mut ctl), [ids[0]]);
    assert_eq!(data.pins().len(), stream::POSITION_SLOTS - 1);
    assert_eq!(data.positions().count(), stream::POSITION_SLOTS - 1);
    assert!(!data.positions().any(|p| p < 100), "only the first voice played there");
    // Its slot is free again.
    ctl.play(VoiceParams::new(buf.clone())).unwrap();
    assert_eq!(data.pins().len(), stream::POSITION_SLOTS - 1, "no loops, no pins");
    assert_eq!(data.positions().filter(|&p| p == 0).count(), 1, "start position reported");
}

#[test]
fn positions_never_crowd_out_finished_buffers() {
    let (mut ctl, mut mix) = setup(2);
    let buf = constant_buffer(1000, 0.1);
    let looped = || {
        let mut p = VoiceParams::new(buf.clone());
        p.loops = [LoopRegion { start: 0, end: 1000, repeats: None }].into();
        p
    };
    let ids: Vec<VoiceId> = (0..4).map(|_| ctl.play(looped()).unwrap()).collect();
    // The control thread does not poll for a long time (report rate 1 Hz).
    let mut out = vec![0.0; 1000 * 2];
    for _ in 0..2000 {
        mix.process(&mut out);
    }
    ids.iter().for_each(|&id| ctl.stop(id).unwrap());
    mix.process(&mut out);
    let mut ended = finished(&mut ctl);
    ended.sort();
    assert_eq!(ended, ids, "every buffer came back");
    assert_eq!(Arc::strong_count(&buf), 1, "and freed on this thread");
}

#[test]
fn bad_levels_are_silenced_or_clamped() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(10, 0.01));
    p.gain = f32::NAN;
    p.pan = f32::NAN;
    let nan = ctl.play(p).unwrap();
    let mut out = vec![0.0; 2 * 2];
    mix.process(&mut out);
    assert_eq!(out, [0.0; 4], "NaN is silence");
    ctl.ramp(nan, 1e9, Duration::ZERO, Curve::Linear, false).unwrap();
    mix.process(&mut out);
    assert!(out.iter().all(|s| (s - 0.16).abs() < 1e-6), "at most +24 dB: {out:?}");
}

#[test]
fn dropped_controller_frees_its_stream_slots() {
    let (buf, data) = stream_buffer(2, &[0, 1]);
    let (mut ctl, _mix) = setup(2);
    let mut p = VoiceParams::new(buf.clone());
    p.loops = [LoopRegion { start: 10, end: 20, repeats: None }].into();
    ctl.play(p).unwrap();
    assert_eq!((data.positions().count(), data.pins().len()), (1, 1));
    // E.g. the preview device is switched while it plays.
    drop(ctl);
    assert_eq!((data.positions().count(), data.pins().len()), (0, 0));
}

fn left(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * mixer.channels()];
    mixer.process(&mut out);
    out.chunks(mixer.channels()).map(|f| f[0]).collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

#[test]
fn pause_blends_out_holds_the_position_and_resume_plays_on() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(ramp_buffer(100))).unwrap();
    assert_eq!(played_frames(&mut mix, 5), [0, 1, 2, 3, 4]);
    ctl.pause(id).unwrap();
    // PAUSE_FADE is 10 frames at 1 kHz: frame 5 at full level, then down.
    let blend = left(&mut mix, 10);
    assert!(
        close(blend[0], 0.005) && close(blend[1], 0.006 * 0.9) && close(blend[9], 0.014 * 0.1),
        "{blend:?}"
    );
    assert!(left(&mut mix, 50).iter().all(|&s| s == 0.0));
    assert!(left(&mut mix, 50).iter().all(|&s| s == 0.0));
    assert_eq!(mix.active_voices(), 1);
    ctl.resume(id).unwrap();
    let resumed = played_frames(&mut mix, 20);
    assert_eq!(resumed[10..], [25, 26, 27, 28, 29, 30, 31, 32, 33, 34]);
}

#[test]
fn a_voice_started_paused_is_silent_from_its_first_frame_until_resumed() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(100));
    (p.start, p.paused) = (20, true);
    let id = ctl.play(p).unwrap();
    assert!(left(&mut mix, 30).iter().all(|&s| s == 0.0), "no pause blend");
    assert_eq!(mix.active_voices(), 1);
    ctl.resume(id).unwrap();
    // Blends in over PAUSE_FADE (10 frames) from its start.
    let resumed = left(&mut mix, 15);
    assert!(close(resumed[0], 0.0) && close(resumed[5], 0.025 * 0.5), "{resumed:?}");
    assert!(close(resumed[14], 0.034), "{resumed:?}");
}

#[test]
fn a_voice_started_paused_stops_at_once() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(100));
    p.paused = true;
    let id = ctl.play(p).unwrap();
    left(&mut mix, 5);
    ctl.stop(id).unwrap();
    left(&mut mix, 1);
    assert_eq!(mix.active_voices(), 0);
}

#[test]
fn pause_keeps_the_position_and_the_reported_level() {
    let (mut ctl, mut mix) = engine(EngineConfig { report_rate_hz: 100, ..EngineConfig::new(RATE, 2) });
    let id = ctl.play(VoiceParams::new(ramp_buffer(1000))).unwrap();
    played_frames(&mut mix, 5);
    ctl.pause(id).unwrap();
    played_frames(&mut mix, 30);
    ctl.poll_events().for_each(drop);
    played_frames(&mut mix, 30);
    let pos = ctl.poll_events().find_map(|e| match e {
        Event::Position { frame, gain, .. } => Some((frame, gain)),
        _ => None,
    });
    assert_eq!(pos, Some((15, 1.0)));
}

#[test]
fn resume_during_the_pause_blend_turns_it_around() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(100, 1.0))).unwrap();
    ctl.pause(id).unwrap();
    let down = left(&mut mix, 4);
    assert!(close(down[3], 0.7), "{down:?}");
    ctl.resume(id).unwrap();
    let up = left(&mut mix, 6);
    assert!(close(up[0], 0.6) && up.windows(2).all(|w| w[1] >= w[0]), "{up:?}");
    left(&mut mix, 4);
    assert!(close(left(&mut mix, 1)[0], 1.0));
}

#[test]
fn pause_freezes_a_running_fade() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(1000, 1.0));
    p.fade_in_frames = 100;
    let id = ctl.play(p).unwrap();
    left(&mut mix, 5);
    ctl.pause(id).unwrap();
    left(&mut mix, 10);
    left(&mut mix, 300);
    ctl.resume(id).unwrap();
    left(&mut mix, 10);
    // 25 frames rendered into the fade-in, whatever the pause lasted.
    assert!(close(left(&mut mix, 1)[0], 0.25));
}

#[test]
fn loops_go_on_after_a_pause_at_the_same_sample() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.loops = [vamp(2, 5)].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 3), [0, 1, 2]);
    ctl.pause(id).unwrap();
    // Blend over 3 4 2 3 4 2 3 4 2 3, then nothing.
    played_frames(&mut mix, 10);
    assert!(played_frames(&mut mix, 7).iter().all(|&s| s == 0));
    ctl.resume(id).unwrap();
    // Blend over 4 2 3 4 2 3 4 2 3 4.
    played_frames(&mut mix, 10);
    assert_eq!(played_frames(&mut mix, 6), [2, 3, 4, 2, 3, 4]);
}

#[test]
fn stops_and_fades_end_a_paused_voice_at_once() {
    let (mut ctl, mut mix) = setup(2);
    let ids: Vec<VoiceId> =
        (0..3).map(|_| ctl.play(VoiceParams::new(constant_buffer(1000, 0.1))).unwrap()).collect();
    played_frames(&mut mix, 1);
    ids.iter().for_each(|&id| ctl.pause(id).unwrap());
    played_frames(&mut mix, 20);
    finished(&mut ctl);

    ctl.fade_out(ids[0], Duration::from_secs(5), Curve::Linear).unwrap();
    played_frames(&mut mix, 1);
    assert_eq!(finished(&mut ctl), [ids[0]]);
    ctl.fade_all(Duration::from_secs(5), Curve::SCurve).unwrap();
    played_frames(&mut mix, 1);
    let mut done = finished(&mut ctl);
    done.sort();
    assert_eq!(done, ids[1..]);
    assert_eq!(mix.active_voices(), 0);
}

#[test]
fn changes_to_a_paused_voice_apply_at_once() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(1000, 1.0))).unwrap();
    ctl.pause(id).unwrap();
    left(&mut mix, 20);
    ctl.ramp(id, 0.5, Duration::from_secs(5), Curve::Linear, false).unwrap();
    ctl.set_trim(id, 0.5).unwrap();
    ctl.resume(id).unwrap();
    left(&mut mix, 10);
    assert!(close(left(&mut mix, 1)[0], 0.25));
}

#[test]
fn duck_lowers_on_top_of_fades_and_trim() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(1000, 1.0))).unwrap();
    ctl.set_duck(id, 0.5, Duration::from_millis(4), Curve::Linear).unwrap();
    let ducked = left(&mut mix, 6);
    assert!(close(ducked[1], 0.875) && close(ducked[5], 0.5), "{ducked:?}");
    // A fade to a level and a trim keep the duck.
    ctl.ramp(id, 0.8, Duration::ZERO, Curve::Linear, false).unwrap();
    assert!(close(left(&mut mix, 1)[0], 0.4));
    ctl.set_duck(id, 1.0, Duration::ZERO, Curve::Linear).unwrap();
    assert!(close(left(&mut mix, 1)[0], 0.8));
}

#[test]
fn duck_in_db_s_curve_eases_into_its_level() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(1000, 1.0))).unwrap();
    ctl.set_duck(id, 0.1, Duration::from_millis(4), Curve::decibel(-60.0, Shape::S)).unwrap();
    let db: Vec<f32> = left(&mut mix, 5).into_iter().map(gain_to_db).collect();
    // Smoothstep of −20 dB at 1/4, 1/2, 3/4.
    for (got, want) in db.iter().zip([0.0, -3.125, -10.0, -16.875, -20.0]) {
        assert!((got - want).abs() < 1e-3, "{db:?}");
    }
}

#[test]
fn exit_all_loops_leaves_every_vamp_at_once() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(12));
    p.loops = [vamp(1, 3), vamp(6, 8)].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 5), [0, 1, 2, 1, 2]);
    ctl.exit_loop(id, LoopExit::AtLoopEnd, true, false).unwrap();
    assert_eq!(played_frames(&mut mix, 10), [3, 4, 5, 6, 7, 8, 9, 10, 11, 0]);
}

#[test]
fn exit_all_loops_immediately_skips_the_rest_of_the_current_one() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(12));
    p.loops = [vamp(1, 4), vamp(6, 8)].into();
    let id = ctl.play(p).unwrap();
    assert_eq!(played_frames(&mut mix, 2), [0, 1]);
    ctl.exit_loop(id, LoopExit::Immediate { crossfade_frames: 0 }, true, false).unwrap();
    assert_eq!(played_frames(&mut mix, 9), [4, 5, 6, 7, 8, 9, 10, 11, 0]);
}

#[test]
fn a_held_fade_stands_still_while_the_voice_plays_on() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(ramp_buffer(100))).unwrap();
    ctl.fade_out(id, Duration::from_millis(10), Curve::Linear).unwrap();
    left(&mut mix, 4);
    ctl.hold_fade(id, true).unwrap();
    // Level held at 0.6 while playback goes on (frames 4, 5, …).
    let held = left(&mut mix, 20);
    assert!(close(held[0], 0.004 * 0.6) && close(held[19], 0.023 * 0.6), "{held:?}");
    assert!(finished(&mut ctl).is_empty());
    ctl.hold_fade(id, false).unwrap();
    left(&mut mix, 6);
    assert_eq!(finished(&mut ctl), [id], "the fade goes on and stops the voice");
}

#[test]
fn a_new_fade_releases_a_held_one() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(100, 1.0))).unwrap();
    ctl.fade_out(id, Duration::from_millis(10), Curve::Linear).unwrap();
    left(&mut mix, 5);
    ctl.hold_fade(id, true).unwrap();
    ctl.ramp(id, 1.0, Duration::from_millis(5), Curve::Linear, false).unwrap();
    left(&mut mix, 5);
    assert!(close(left(&mut mix, 1)[0], 1.0));
}

/// Kinds of the notifications among the pending events, in order: `N` for
/// `NearEnd`, `X` for a loop exit, `L` for the last one, `F` for `Finished`.
fn notifications(ctl: &mut Controller) -> String {
    ctl.poll_events()
        .filter_map(|e| match e {
            Event::NearEnd(_) => Some('N'),
            Event::LoopExited { last: false, .. } => Some('X'),
            Event::LoopExited { last: true, .. } => Some('L'),
            Event::Finished { .. } => Some('F'),
            _ => None,
        })
        .collect()
}

#[test]
fn near_end_comes_in_the_block_that_reaches_it() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(100));
    p.notify_before_end = Some(30);
    ctl.play(p).unwrap();
    played_frames(&mut mix, 69);
    assert_eq!(notifications(&mut ctl), "");
    played_frames(&mut mix, 1);
    assert_eq!(notifications(&mut ctl), "N");
    played_frames(&mut mix, 40);
    assert_eq!(notifications(&mut ctl), "F", "once, then the end");
}

#[test]
fn near_end_waits_for_the_last_pass_of_a_loop() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.loops = [LoopRegion { start: 0, end: 10, repeats: Some(2) }].into();
    p.notify_before_end = Some(3);
    ctl.play(p).unwrap();
    for _ in 0..2 {
        played_frames(&mut mix, 10);
        assert_eq!(notifications(&mut ctl), "");
    }
    played_frames(&mut mix, 6);
    assert_eq!(notifications(&mut ctl), "");
    played_frames(&mut mix, 1);
    assert_eq!(notifications(&mut ctl), "N");
}

#[test]
fn near_end_waits_for_the_devamp() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(20));
    p.loops = [vamp(12, 18)].into();
    p.notify_before_end = Some(10);
    let id = ctl.play(p).unwrap();
    played_frames(&mut mix, 50);
    assert_eq!(notifications(&mut ctl), "");
    ctl.exit_loop(id, LoopExit::Immediate { crossfade_frames: 0 }, false, false).unwrap();
    played_frames(&mut mix, 1);
    assert_eq!(notifications(&mut ctl), "LN");
}

#[test]
fn a_seek_past_near_end_reports_at_once_and_only_once() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(100));
    p.notify_before_end = Some(10);
    let id = ctl.play(p).unwrap();
    played_frames(&mut mix, 5);
    ctl.seek(id, 95, 0).unwrap();
    played_frames(&mut mix, 1);
    assert_eq!(notifications(&mut ctl), "N");
    ctl.seek(id, 10, 0).unwrap();
    played_frames(&mut mix, 85);
    assert_eq!(notifications(&mut ctl), "");
}

#[test]
fn a_lead_longer_than_the_cue_reports_at_the_start() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(100));
    p.notify_before_end = Some(500);
    ctl.play(p).unwrap();
    played_frames(&mut mix, 1);
    assert_eq!(notifications(&mut ctl), "N");
}

#[test]
fn no_near_end_while_paused_then_at_the_same_sample() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(100));
    p.notify_before_end = Some(30);
    let id = ctl.play(p).unwrap();
    played_frames(&mut mix, 50);
    ctl.pause(id).unwrap();
    // The blend plays frames 50–59, then the voice stands at 60.
    played_frames(&mut mix, 10);
    // A jump past the mark while paused waits for the resume.
    ctl.seek(id, 80, 0).unwrap();
    played_frames(&mut mix, 100);
    assert_eq!(notifications(&mut ctl), "");
    ctl.resume(id).unwrap();
    played_frames(&mut mix, 1);
    assert_eq!(notifications(&mut ctl), "N");

    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(100));
    p.notify_before_end = Some(30);
    let id = ctl.play(p).unwrap();
    played_frames(&mut mix, 50);
    ctl.pause(id).unwrap();
    played_frames(&mut mix, 300);
    ctl.resume(id).unwrap();
    // From 60 on, the mark at 70 is reached after ten frames as without the pause.
    played_frames(&mut mix, 9);
    assert_eq!(notifications(&mut ctl), "");
    played_frames(&mut mix, 1);
    assert_eq!(notifications(&mut ctl), "N");
}

#[test]
fn near_end_comes_before_a_natural_end_but_never_after_a_stop() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.notify_before_end = Some(0);
    ctl.play(p.clone()).unwrap();
    played_frames(&mut mix, 20);
    assert_eq!(notifications(&mut ctl), "NF");

    p.buffer = ramp_buffer(100);
    p.notify_before_end = Some(10);
    let id = ctl.play(p.clone()).unwrap();
    played_frames(&mut mix, 50);
    ctl.stop(id).unwrap();
    played_frames(&mut mix, 100);
    assert_eq!(notifications(&mut ctl), "F");
    let id = ctl.play(p).unwrap();
    played_frames(&mut mix, 50);
    ctl.fade_out(id, Duration::from_millis(100), Curve::Linear).unwrap();
    played_frames(&mut mix, 100);
    assert_eq!(notifications(&mut ctl), "F");
}

#[test]
fn loop_exits_are_reported_in_their_block_for_every_mode() {
    let modes = [
        LoopExit::AtLoopEnd,
        LoopExit::AtNextMark { crossfade_frames: 0 },
        LoopExit::Immediate { crossfade_frames: 0 },
    ];
    // Leaves the vamp 10–20 (mark at 15) after playing 0–11: at frame 20,
    // the mark or right away.
    for (mode, frames_to_exit) in modes.into_iter().zip([9, 4, 1]) {
        let (mut ctl, mut mix) = setup(2);
        let mut p = VoiceParams::new(ramp_buffer(40));
        p.loops = [vamp(10, 20)].into();
        p.marks = [15].into();
        let id = ctl.play(p).unwrap();
        played_frames(&mut mix, 12);
        ctl.exit_loop(id, mode, false, false).unwrap();
        played_frames(&mut mix, frames_to_exit - 1);
        assert_eq!(notifications(&mut ctl), "", "{mode:?}");
        played_frames(&mut mix, 1);
        assert_eq!(notifications(&mut ctl), "L", "{mode:?}");
    }
}

#[test]
fn repeats_running_out_are_no_loop_exit() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(10));
    p.loops = [LoopRegion { start: 2, end: 5, repeats: Some(2) }].into();
    ctl.play(p).unwrap();
    played_frames(&mut mix, 30);
    assert_eq!(notifications(&mut ctl), "F");
}

#[test]
fn leaving_all_loops_reports_each_and_marks_the_last() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(20));
    p.loops = [vamp(2, 5), vamp(8, 11)].into();
    let id = ctl.play(p).unwrap();
    played_frames(&mut mix, 3);
    ctl.exit_loop(id, LoopExit::AtLoopEnd, true, false).unwrap();
    played_frames(&mut mix, 30);
    assert_eq!(notifications(&mut ctl), "XLF");
}

#[test]
fn a_loop_exit_with_stop_ends_after_the_stop_blend() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(constant_buffer(100, 1.0));
    p.loops = [vamp(10, 20)].into();
    let id = ctl.play(p).unwrap();
    left(&mut mix, 12);
    ctl.exit_loop(id, LoopExit::AtLoopEnd, false, true).unwrap();
    let out = left(&mut mix, 30);
    // Frames 12–20 at full level, from the exit at 20 STOP_FADE (10 frames).
    assert!(out[..9].iter().all(|&s| close(s, 1.0)), "{out:?}");
    assert!(out[9] < 1.0 && out[17] > 0.0, "{out:?}");
    assert!(out[18..].iter().all(|&s| s == 0.0), "{out:?}");
    assert_eq!(notifications(&mut ctl), "LF");
}

/// Right channel of a constant 1.0 buffer: 1 − pan (for pan ≤ 0).
fn right(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * mixer.channels()];
    mixer.process(&mut out);
    out.chunks(mixer.channels()).map(|f| f[1]).collect()
}

#[test]
fn a_pan_move_reaches_its_target_after_its_time() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(1000, 1.0))).unwrap();
    right(&mut mix, 1);
    ctl.pan_to(id, -1.0, Duration::from_millis(100), Curve::Linear).unwrap();
    let r = right(&mut mix, 101);
    assert!(close(r[0], 1.0) && close(r[50], 0.5) && close(r[100], 0.0), "{r:?}");
}

#[test]
fn an_s_curve_pan_move_is_halfway_in_the_middle_and_slow_at_the_ends() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(1000, 1.0))).unwrap();
    right(&mut mix, 1);
    ctl.pan_to(id, -1.0, Duration::from_millis(100), Curve::SCurve).unwrap();
    let r = right(&mut mix, 101);
    assert!(close(r[50], 0.5), "{}", r[50]);
    assert!(r[10] > 0.9 && r[90] < 0.1, "{} {}", r[10], r[90]);
}

#[test]
fn a_held_pan_move_stands_still_while_a_fade_goes_on() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(constant_buffer(1000, 1.0))).unwrap();
    ctl.pan_to(id, -1.0, Duration::from_millis(100), Curve::Linear).unwrap();
    ctl.ramp(id, 0.0, Duration::from_millis(100), Curve::Linear, false).unwrap();
    right(&mut mix, 50);
    ctl.hold_pan(id, true).unwrap();
    let mut out = vec![0.0; 2 * 2];
    mix.process(&mut out);
    // Pan held at −0.5 (right × 0.5), gain goes on from 0.5 down.
    assert!(close(out[1], 0.5 * 0.5) && close(out[3], 0.49 * 0.5), "{out:?}");
    right(&mut mix, 20);
    ctl.hold_pan(id, false).unwrap();
    let mut out = vec![0.0; 2];
    mix.process(&mut out);
    let gain = 0.28;
    assert!(close(out[1], gain * 0.5), "the move goes on from where it stood: {out:?}");
}

#[test]
fn a_rearming_seek_back_holds_a_vamp_left_already() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(20));
    p.loops = [vamp(4, 8)].into();
    let id = ctl.play(p).unwrap();
    played_frames(&mut mix, 6);
    ctl.exit_loop(id, LoopExit::AtLoopEnd, false, false).unwrap();
    assert_eq!(played_frames(&mut mix, 4), [6, 7, 8, 9]);
    ctl.seek_rearming(id, 2, 0).unwrap();
    assert_eq!(played_frames(&mut mix, 9), [2, 3, 4, 5, 6, 7, 4, 5, 6], "the vamp holds again");
    // A plain seek back does not.
    ctl.exit_loop(id, LoopExit::AtLoopEnd, false, false).unwrap();
    assert_eq!(played_frames(&mut mix, 3), [7, 8, 9]);
    ctl.seek(id, 2, 0).unwrap();
    assert_eq!(played_frames(&mut mix, 8), [2, 3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn a_rearmed_vamp_reports_its_exit_again() {
    let (mut ctl, mut mix) = setup(2);
    let mut p = VoiceParams::new(ramp_buffer(20));
    p.loops = [vamp(4, 8)].into();
    let id = ctl.play(p).unwrap();
    ctl.exit_loop(id, LoopExit::AtLoopEnd, false, false).unwrap();
    played_frames(&mut mix, 10);
    assert_eq!(notifications(&mut ctl), "L");
    ctl.seek_rearming(id, 0, 0).unwrap();
    played_frames(&mut mix, 10);
    ctl.exit_loop(id, LoopExit::AtLoopEnd, false, false).unwrap();
    played_frames(&mut mix, 6);
    assert_eq!(notifications(&mut ctl), "L");
}

#[test]
fn positions_report_the_pan_as_it_moves() {
    let (mut ctl, mut mix) = engine(EngineConfig { report_rate_hz: 100, ..EngineConfig::new(RATE, 2) });
    let id = ctl.play(VoiceParams::new(constant_buffer(1000, 1.0))).unwrap();
    ctl.pan_to(id, -1.0, Duration::from_millis(100), Curve::Linear).unwrap();
    played_frames(&mut mix, 50);
    let pans: Vec<f32> = ctl
        .poll_events()
        .filter_map(|e| match e {
            Event::Position { pan, .. } => Some(pan),
            _ => None,
        })
        .collect();
    assert!(close(*pans.last().unwrap(), -0.5), "{pans:?}");
}

/// FNV-1a over the bits of every sample.
fn checksum(samples: &[f32]) -> u64 {
    samples
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |h, s| (h ^ u64::from(s.to_bits())).wrapping_mul(0x0000_0100_0000_01b3))
}

/// A voice through most of what the engine does at 100 %: loops, vamps,
/// marks, crossfades, fades, pan, trim, duck, pause, seeks and the end fade.
/// Its output must stay bit-identical while the voice is reworked.
#[test]
fn direct_playback_stays_bit_identical() {
    let (mut ctl, mut mix) = setup(4);
    let samples = (0..3000)
        .flat_map(|i| {
            let t = i as f32;
            [(t * 0.031).sin() * 0.6 + (t * 0.17).sin() * 0.2, (t * 0.023).cos() * 0.5]
        })
        .collect();
    let mut p = VoiceParams::new(Arc::new(AudioBuffer::from_interleaved_stereo(RATE, samples)));
    p.start = 50;
    p.end = Some(2900);
    p.loops =
        [LoopRegion { start: 200, end: 400, repeats: Some(2) }, vamp(700, 1000), vamp(1500, 1800)].into();
    p.marks = [300, 800, 900, 1600, 1700].into();
    p.gain = 0.8;
    p.fade_in_frames = 120;
    p.fade_curve = Curve::decibel(-60.0, Shape::S);
    p.loop_crossfade_frames = 16;
    p.end_fade_frames = 300;
    p.pan = -0.3;
    p.output_channel = 2;
    p.notify_before_end = Some(250);
    let id = ctl.play(p).unwrap();
    let mut out = Vec::new();
    let mut render = |mix: &mut Mixer, frames: usize| {
        let mut block = vec![0.0; frames * 4];
        mix.process(&mut block);
        out.extend_from_slice(&block);
    };
    render(&mut mix, 333);
    ctl.set_trim(id, 0.7).unwrap();
    render(&mut mix, 517);
    ctl.set_duck(id, 0.5, Duration::from_millis(40), Curve::SCurve).unwrap();
    render(&mut mix, 211);
    ctl.exit_loop(id, LoopExit::AtNextMark { crossfade_frames: 12 }, false, false).unwrap();
    render(&mut mix, 300);
    ctl.pause(id).unwrap();
    render(&mut mix, 64);
    ctl.resume(id).unwrap();
    render(&mut mix, 129);
    ctl.pan_to(id, 0.6, Duration::from_millis(90), Curve::SCurve).unwrap();
    render(&mut mix, 250);
    ctl.seek_rearming(id, 1200, 20).unwrap();
    render(&mut mix, 701);
    ctl.ramp(id, 0.3, Duration::from_millis(100), Curve::Linear, false).unwrap();
    render(&mut mix, 77);
    ctl.hold_fade(id, true).unwrap();
    render(&mut mix, 50);
    ctl.hold_fade(id, false).unwrap();
    render(&mut mix, 400);
    ctl.exit_loop(id, LoopExit::Immediate { crossfade_frames: 8 }, true, false).unwrap();
    render(&mut mix, 1500);
    let events = notifications(&mut ctl);
    assert_eq!(events, "LLNF");
    assert_eq!(checksum(&out), 130_835_893_719_015_155);
}

// Rate and pitch: varispeed through the interpolation, everything else
// through a stretcher. At a real sample rate, with tolerances where the
// stretcher is involved.

const HZ: u32 = 48_000;

fn setup_hz(stretchers: usize) -> (Controller, Mixer) {
    engine(EngineConfig { stretchers, prime_ratio: 16, ..EngineConfig::new(HZ, 2) })
}

fn sine_buffer(freq: f32, secs: f32) -> Arc<AudioBuffer> {
    let frames = (secs * HZ as f32) as usize;
    let samples = (0..frames)
        .flat_map(|i| {
            let s = (std::f32::consts::TAU * freq * i as f32 / HZ as f32).sin() * 0.5;
            [s, s]
        })
        .collect();
    Arc::new(AudioBuffer::from_interleaved_stereo(HZ, samples))
}

/// Renders in callbacks of 256 frames until the voice ends (or `limit`
/// frames); the left channel and the frames until `Finished`.
fn render_until_finished(ctl: &mut Controller, mix: &mut Mixer, limit: usize) -> (Vec<f32>, Option<usize>) {
    let mut left = Vec::new();
    let mut end = None;
    while left.len() < limit && end.is_none() {
        let mut out = vec![0.0; 256 * 2];
        mix.process(&mut out);
        left.extend(out.chunks(2).map(|f| f[0]));
        if ctl.poll_events().any(|e| matches!(e, Event::Finished { .. })) {
            end = Some(left.len());
        }
    }
    (left, end)
}

/// Frequency from the rising zero crossings.
fn frequency_of(left: &[f32]) -> f32 {
    let crossings: Vec<usize> = (1..left.len()).filter(|&i| left[i - 1] < 0.0 && left[i] >= 0.0).collect();
    let (first, last) = (crossings[0], crossings[crossings.len() - 1]);
    (crossings.len() - 1) as f32 * HZ as f32 / (last - first) as f32
}

fn rms(s: &[f32]) -> f32 {
    (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}

#[test]
fn speed_of_one_keeps_direct_playback() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(ramp_buffer(10))).unwrap();
    assert_eq!(played_frames(&mut mix, 3), [0, 1, 2]);
    ctl.speed_to(id, 1.0, 1.0, Duration::ZERO, Curve::Linear).unwrap();
    assert_eq!(played_frames(&mut mix, 8), [3, 4, 5, 6, 7, 8, 9, 0]);
}

#[test]
fn varispeed_takes_over_without_a_seam() {
    let (mut ctl, mut mix) = setup(2);
    let id = ctl.play(VoiceParams::new(ramp_buffer(400))).unwrap();
    assert_eq!(played_frames(&mut mix, 100)[99], 99);
    ctl.speed_to(id, 0.5, 0.5, Duration::ZERO, Curve::Linear).unwrap();
    let mut out = vec![0.0; 6 * 2];
    mix.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|f| f[0] * 1000.0).collect();
    // The next frame exactly, then half steps (a ramp interpolates to a ramp).
    assert_eq!(left[0], 100.0);
    for (i, got) in left.iter().enumerate() {
        assert!((got - (100.0 + i as f32 * 0.5)).abs() < 0.01, "{left:?}");
    }
}

#[test]
fn varispeed_changes_duration_and_pitch_together() {
    for rate in [0.5f32, 1.5] {
        let (mut ctl, mut mix) = setup_hz(0);
        let mut p = VoiceParams::new(sine_buffer(440.0, 1.0));
        p.rate = rate;
        p.pitch = rate;
        ctl.play(p).unwrap();
        let (left, end) = render_until_finished(&mut ctl, &mut mix, 3 * HZ as usize);
        let want = HZ as f32 / rate;
        let end = end.unwrap() as f32;
        assert!((end - want).abs() <= 256.0 + 100.0, "rate {rate}: ended after {end}");
        let f = frequency_of(&left[1000..(want as usize - 1000)]);
        assert!((f - 440.0 * rate).abs() < 440.0 * rate * 0.002, "rate {rate}: {f} Hz");
        assert!(ctl.poll_events().all(|e| !matches!(e, Event::NoStretcher(_))));
    }
}

#[test]
fn varispeed_level_stays_even() {
    let (mut ctl, mut mix) = setup_hz(0);
    let mut p = VoiceParams::new(sine_buffer(1000.0, 1.0));
    p.rate = 0.7;
    p.pitch = 0.7;
    ctl.play(p).unwrap();
    let (left, _) = render_until_finished(&mut ctl, &mut mix, HZ as usize);
    let level = rms(&left[2000..20_000]);
    assert!((level - 0.5 / 2f32.sqrt()).abs() < 0.005, "{level}");
}

#[test]
fn stretched_keeps_pitch_or_shifts_it_alone() {
    // Rate 1.5 keeping the pitch: shorter, still 440 Hz.
    let (mut ctl, mut mix) = setup_hz(2);
    let mut p = VoiceParams::new(sine_buffer(440.0, 1.5));
    p.rate = 1.5;
    p.pitch = 1.0;
    ctl.play(p).unwrap();
    let (left, end) = render_until_finished(&mut ctl, &mut mix, 3 * HZ as usize);
    let end = end.unwrap() as f32;
    assert!((end - HZ as f32).abs() < 0.05 * HZ as f32, "ended after {end}");
    let f = frequency_of(&left[5000..40_000]);
    assert!((f - 440.0).abs() < 4.0, "{f} Hz");

    // An octave up at the original rate: as long, 880 Hz.
    let (mut ctl, mut mix) = setup_hz(2);
    let mut p = VoiceParams::new(sine_buffer(440.0, 1.0));
    p.pitch = 2.0;
    ctl.play(p).unwrap();
    let (left, end) = render_until_finished(&mut ctl, &mut mix, 3 * HZ as usize);
    let end = end.unwrap() as f32;
    assert!((end - HZ as f32).abs() < 0.05 * HZ as f32, "ended after {end}");
    let f = frequency_of(&left[5000..40_000]);
    assert!((f - 880.0).abs() < 8.0, "{f} Hz");
}

#[test]
fn a_stretched_start_is_on_time() {
    // Silence, then a tone from 0.5 s: at rate 0.5 it is heard from 1 s.
    let (mut ctl, mut mix) = setup_hz(2);
    let half = HZ as usize / 2;
    let samples = (0..HZ as usize)
        .flat_map(|i| {
            let s = if i < half { 0.0 } else { (i as f32 * 0.05).sin() * 0.5 };
            [s, s]
        })
        .collect();
    let mut p = VoiceParams::new(Arc::new(AudioBuffer::from_interleaved_stereo(HZ, samples)));
    p.rate = 0.5;
    p.pitch = 1.0;
    ctl.play(p).unwrap();
    let mut left = Vec::new();
    let mut started_at = None;
    while left.len() < 2 * HZ as usize {
        let mut out = vec![0.0; 256 * 2];
        mix.process(&mut out);
        left.extend(out.chunks(2).map(|f| f[0]));
        if started_at.is_none() && ctl.poll_events().any(|e| matches!(e, Event::Started(_))) {
            started_at = Some(left.len());
        }
    }
    let started_at = started_at.unwrap();
    // Pre-roll delays the start by a few callbacks at most.
    assert!(started_at <= 4 * 256, "started after {started_at}");
    let onset = left.iter().position(|s| s.abs() > 0.25).unwrap() - (started_at - 256);
    let want = HZ as usize;
    assert!(onset.abs_diff(want) < HZ as usize / 50, "onset at {onset}, want {want}");
}

#[test]
fn switching_to_the_stretcher_lines_up() {
    let (mut ctl, mut mix) = setup_hz(2);
    let id = ctl.play(VoiceParams::new(sine_buffer(300.0, 2.0))).unwrap();
    let mut left = Vec::new();
    let render = |mix: &mut Mixer, frames: usize, left: &mut Vec<f32>| {
        let mut out = vec![0.0; frames * 2];
        mix.process(&mut out);
        left.extend(out.chunks(2).map(|f| f[0]));
    };
    render(&mut mix, 4800, &mut left);
    // Pitch apart from the rate: over to the stretcher, still at rate 1.
    ctl.speed_to(id, 1.0, 1.0001, Duration::ZERO, Curve::Linear).unwrap();
    for _ in 0..60 {
        render(&mut mix, 256, &mut left);
    }
    // No dip or comb while blending: every 10 ms stays at the tone's level.
    let want = 0.5 / 2f32.sqrt();
    for (i, w) in left.chunks(480).enumerate() {
        let level = rms(w);
        assert!((level / want).log10().abs() * 20.0 < 1.0, "10 ms window {i}: {level}");
    }
    // Lined up: after the switch the tone goes on in phase with the file.
    let tail = left.len() - 2000;
    let expected = |n: usize| (std::f32::consts::TAU * 300.0 * n as f32 / HZ as f32).sin() * 0.5;
    let diff: Vec<f32> = (tail..left.len()).map(|n| left[n] - expected(n)).collect();
    assert!(rms(&diff) < 0.02, "{}", rms(&diff));
}

#[test]
fn near_end_and_end_fade_count_in_real_time() {
    let (mut ctl, mut mix) = setup_hz(0);
    let mut p = VoiceParams::new(sine_buffer(440.0, 1.0));
    p.rate = 0.5;
    p.pitch = 0.5;
    p.notify_before_end = Some(HZ as usize / 2);
    p.end_fade_frames = HZ / 4;
    ctl.play(p).unwrap();
    let mut left = Vec::new();
    let (mut near, mut end) = (None, None);
    while end.is_none() && left.len() < 3 * HZ as usize {
        let mut out = vec![0.0; 256 * 2];
        mix.process(&mut out);
        left.extend(out.chunks(2).map(|f| f[0]));
        for e in ctl.poll_events() {
            match e {
                Event::NearEnd(_) => near = Some(left.len()),
                Event::Finished { .. } => end = Some(left.len()),
                _ => {}
            }
        }
    }
    let (near, end) = (near.unwrap(), end.unwrap());
    assert!((end - near).abs_diff(HZ as usize / 2) <= 512, "near end {near}, end {end}");
    // The end fade starts a quarter second (real time) before the end.
    let peak = |from: usize| left[from..from + 480].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak(end - HZ as usize / 4 - 2000) > 0.45);
    assert!(peak(end - HZ as usize / 8 - 240) < 0.35);
}

#[test]
fn a_stretched_vamp_loops_and_is_left_when_heard() {
    let (mut ctl, mut mix) = setup_hz(2);
    let mut p = VoiceParams::new(sine_buffer(440.0, 2.0));
    p.loops = [vamp(HZ as usize / 2, HZ as usize)].into();
    p.rate = 0.8;
    p.pitch = 1.0;
    let id = ctl.play(p).unwrap();
    let mut frames = 0;
    let mut render = |mix: &mut Mixer, n: usize| {
        for _ in 0..n {
            let mut out = vec![0.0; 256 * 2];
            mix.process(&mut out);
            frames += 256;
        }
        frames
    };
    // Two seconds at 80 %: 1.6 s of file, looping in 0.5–1 s.
    render(&mut mix, 375);
    assert!(ctl.poll_events().all(|e| !matches!(e, Event::LoopExited { .. } | Event::Finished { .. })));
    ctl.exit_loop(id, LoopExit::AtLoopEnd, false, false).unwrap();
    let mut exited = None;
    let mut position_before = None;
    for _ in 0..200 {
        let now = render(&mut mix, 1);
        for e in ctl.poll_events() {
            match e {
                Event::Position { frame, .. } if exited.is_none() => position_before = Some(frame),
                Event::LoopExited { .. } => exited = Some(now),
                _ => {}
            }
        }
        if exited.is_some() {
            break;
        }
    }
    assert!(exited.is_some());
    // Heard just before the exit: near the loop end.
    let before = position_before.unwrap();
    assert!(before > HZ as usize * 3 / 4 && before <= HZ as usize, "{before}");
}

#[test]
fn without_a_free_stretcher_the_voice_plays_varispeed() {
    let (mut ctl, mut mix) = setup_hz(1);
    let mut p = VoiceParams::new(sine_buffer(440.0, 1.0));
    p.rate = 1.2;
    p.pitch = 1.0;
    let first = ctl.play(p.clone()).unwrap();
    let second = ctl.play(p).unwrap();
    let mut out = vec![0.0; 256 * 2];
    mix.process(&mut out);
    let lacking: Vec<VoiceId> = ctl
        .poll_events()
        .filter_map(|e| match e {
            Event::NoStretcher(id) => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(lacking, [second]);
    assert_ne!(first, second);
    // The first gives its stretcher back when it ends; the next one gets it.
    let _ = render_until_finished(&mut ctl, &mut mix, 2 * HZ as usize);
    let (_, _) = render_until_finished(&mut ctl, &mut mix, 2 * HZ as usize);
    let mut p = VoiceParams::new(sine_buffer(440.0, 0.1));
    p.pitch = 1.5;
    ctl.play(p).unwrap();
    mix.process(&mut out);
    assert!(ctl.poll_events().all(|e| !matches!(e, Event::NoStretcher(_))));
}

#[test]
fn a_held_speed_move_stands_still() {
    let (mut ctl, mut mix) = setup_hz(0);
    let id = ctl.play(VoiceParams::new(sine_buffer(440.0, 2.0))).unwrap();
    ctl.speed_to(id, 0.5, 0.5, Duration::from_millis(500), Curve::Linear).unwrap();
    let mut out = vec![0.0; 12_000 * 2];
    mix.process(&mut out);
    ctl.hold_speed(id, true).unwrap();
    mix.process(&mut out);
    mix.process(&mut out);
    let rates: Vec<f32> = ctl
        .poll_events()
        .filter_map(|e| match e {
            Event::Position { rate, .. } => Some(rate),
            _ => None,
        })
        .collect();
    let last = *rates.last().unwrap();
    // A quarter of the way down after 250 ms, and held there.
    assert!((last - 0.75).abs() < 0.01, "{rates:?}");
    ctl.hold_speed(id, false).unwrap();
    mix.process(&mut out);
    mix.process(&mut out);
    let rate = ctl
        .poll_events()
        .filter_map(|e| match e {
            Event::Position { rate, .. } => Some(rate),
            _ => None,
        })
        .last()
        .unwrap();
    assert_eq!(rate, 0.5);
}

#[test]
fn a_paused_stretched_voice_goes_on_where_it_stood() {
    let (mut ctl, mut mix) = setup_hz(1);
    let mut p = VoiceParams::new(sine_buffer(440.0, 2.0));
    p.rate = 0.9;
    p.pitch = 1.0;
    let id = ctl.play(p).unwrap();
    let mut out = vec![0.0; 9_000 * 2];
    mix.process(&mut out);
    mix.process(&mut out);
    let before = ctl
        .poll_events()
        .filter_map(|e| match e {
            Event::Position { frame, .. } => Some(frame),
            _ => None,
        })
        .last()
        .unwrap();
    ctl.pause(id).unwrap();
    for _ in 0..5 {
        mix.process(&mut out);
    }
    let during: Vec<usize> = ctl
        .poll_events()
        .filter_map(|e| match e {
            Event::Position { frame, .. } => Some(frame),
            _ => None,
        })
        .collect();
    // The pause blend lets a few hundred frames through, then it stands.
    assert!(during.windows(2).all(|w| w[0] == w[1]), "{during:?}");
    assert!(during[0] - before < 1000, "{before} → {during:?}");
    ctl.resume(id).unwrap();
    let mut out = vec![0.0; 2400 * 2];
    mix.process(&mut out);
    // Plays on at once, at full level.
    let level = rms(&out.chunks(2).map(|f| f[0]).skip(960).collect::<Vec<_>>());
    assert!(level > 0.3, "{level}");
}

/// Counts allocations and frees of the current thread while armed.
mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    pub struct Counting;

    thread_local! {
        static ARMED: Cell<bool> = const { Cell::new(false) };
        static COUNT: Cell<usize> = const { Cell::new(0) };
    }

    fn count() {
        if ARMED.with(Cell::get) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
    }

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            count();
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            count();
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            count();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    /// Allocations and frees `f` makes on this thread.
    pub fn allocations(f: impl FnOnce()) -> usize {
        COUNT.with(|c| c.set(0));
        ARMED.with(|a| a.set(true));
        f();
        ARMED.with(|a| a.set(false));
        COUNT.with(Cell::get)
    }
}

#[global_allocator]
static ALLOC: counting::Counting = counting::Counting;

#[test]
fn the_audio_thread_never_allocates() {
    let (mut ctl, mut mix) = setup_hz(2);
    let buffer = sine_buffer(440.0, 1.0);
    let mut out = vec![0.0; 256 * 2];
    let mut process = |mix: &mut Mixer, n: usize| {
        counting::allocations(|| {
            for _ in 0..n {
                mix.process(&mut out);
            }
        })
    };
    let mut p = VoiceParams::new(buffer.clone());
    p.loops = [vamp(10_000, 20_000)].into();
    p.end_fade_frames = 4800;
    p.notify_before_end = Some(4800);
    let direct = ctl.play(p.clone()).unwrap();
    let mut p2 = p.clone();
    p2.rate = 0.8;
    p2.pitch = 1.0;
    let stretched = ctl.play(p2).unwrap();
    let mut p3 = p.clone();
    p3.rate = 1.3;
    p3.pitch = 1.3;
    let varispeed = ctl.play(p3).unwrap();
    let mut total = process(&mut mix, 40);
    // Direct over to varispeed, varispeed over to the stretcher (no free
    // one left: plays on varispeed), moves, seeks, exits, pause.
    ctl.speed_to(direct, 0.9, 0.9, Duration::from_millis(100), Curve::SCurve).unwrap();
    ctl.speed_to(varispeed, 1.1, 1.0, Duration::from_millis(100), Curve::Linear).unwrap();
    ctl.speed_to(stretched, 1.2, 0.8, Duration::from_millis(300), Curve::Linear).unwrap();
    total += process(&mut mix, 40);
    ctl.seek(stretched, 5000, 480).unwrap();
    ctl.exit_loop(direct, LoopExit::AtLoopEnd, false, false).unwrap();
    ctl.pause(varispeed).unwrap();
    total += process(&mut mix, 40);
    ctl.resume(varispeed).unwrap();
    ctl.exit_loop(stretched, LoopExit::Immediate { crossfade_frames: 480 }, true, true).unwrap();
    ctl.exit_loop(varispeed, LoopExit::AtLoopEnd, true, false).unwrap();
    total += process(&mut mix, 400);
    // Every voice ended; the stretchers are back in the pool.
    let ended = ctl.poll_events().filter(|e| matches!(e, Event::Finished { .. })).count();
    assert_eq!(ended, 3);
    let mut again = VoiceParams::new(buffer);
    again.pitch = 1.5;
    ctl.play(again).unwrap();
    total += process(&mut mix, 10);
    assert!(ctl.poll_events().all(|e| !matches!(e, Event::NoStretcher(_))));
    assert_eq!(total, 0);
}

#[test]
fn idle_stream_reader_is_woken_by_the_control_thread() {
    let (mut ctl, mut mixer) = setup(2);
    let (buf, data) = stream_buffer(4, &[0]);
    // Nothing changes: the wait runs out.
    let seen = data.changes();
    let start = std::time::Instant::now();
    data.wait_for_change(seen, Some(Duration::from_millis(30)));
    assert!(start.elapsed() >= Duration::from_millis(30));
    assert!(!data.has_voices());

    // A reader waiting without a timeout is woken by a new start point …
    let waiter = {
        let data = data.clone();
        std::thread::spawn(move || data.wait_for_change(seen, None))
    };
    std::thread::sleep(Duration::from_millis(20));
    data.set_hints(vec![0, stream::CHUNK_FRAMES * 2]);
    waiter.join().unwrap();

    // … and by a voice taking a slot, after which it follows the positions
    // by itself; its end wakes the reader again.
    let seen = data.changes();
    let id = ctl.play(VoiceParams::new(buf)).unwrap();
    assert!(data.changes() != seen && data.has_voices());
    let seen = data.changes();
    ctl.stop(id).unwrap();
    let mut out = vec![0.0; 2 * 64];
    for _ in 0..4 {
        mixer.process(&mut out);
    }
    let _: Vec<_> = ctl.poll_events().collect();
    assert!(data.changes() != seen && !data.has_voices());

    // Stopping (dropping the buffer) ends the wait, so the thread ends.
    let seen = data.changes();
    data.stop();
    data.wait_for_change(seen, None);
}
