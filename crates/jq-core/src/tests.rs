use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jq_engine::{AudioBuffer, EngineConfig, Mixer, engine};
use jq_show::*;

use crate::*;

const RATE: u32 = 1000;

/// Loader that makes constant buffers; `len<N>.wav` has N frames, files
/// named `missing*` fail. Records every load.
#[derive(Default)]
struct FakeLoader {
    loads: Mutex<Vec<PathBuf>>,
    /// The streams handed out, so a test can play the reader thread.
    streams: Mutex<Vec<Arc<jq_engine::stream::StreamData>>>,
}

impl MediaLoader for FakeLoader {
    fn load(&self, path: &Path, rate: u32) -> Result<AudioBuffer, String> {
        self.loads.lock().unwrap().push(path.to_owned());
        let name = path.file_stem().unwrap().to_str().unwrap();
        if name.starts_with("missing") {
            return Err("nicht gefunden".into());
        }
        if name.starts_with("stream") {
            // Two chunks, only the first loaded (no reader thread).
            let data = Arc::new(jq_engine::stream::StreamData::new(2 * jq_engine::stream::CHUNK_FRAMES));
            data.store(0, vec![0.5; jq_engine::stream::CHUNK_FRAMES * 2]);
            self.streams.lock().unwrap().push(data.clone());
            return Ok(AudioBuffer::streamed(rate, data));
        }
        let frames: usize = name.trim_start_matches("len").parse().unwrap_or(1000);
        Ok(AudioBuffer::from_interleaved_stereo(rate, vec![0.5; frames * 2]))
    }
}

struct Rig {
    runner: Runner,
    mixer: Mixer,
    loader: Arc<FakeLoader>,
    t0: Instant,
    /// Simulated time, advanced together with the rendered audio.
    ms: u64,
}

impl Rig {
    fn new(cues: Vec<Cue>) -> Self {
        let show = Show { cues, ..Show::default() };
        let (ctl, mixer) = engine(EngineConfig { report_rate_hz: 100, ..EngineConfig::new(RATE, 2) });
        let loader = Arc::new(FakeLoader::default());
        // Tests that need the GO lockout turn it on.
        let settings = LocalSettings { go_lockout: 0.0, ..LocalSettings::default() };
        let runner = Runner::new(show, PathBuf::from("media"), settings, ctl, loader.clone());
        Self { runner, mixer, loader, t0: Instant::now(), ms: 0 }
    }

    fn now(&self) -> Instant {
        self.t0 + Duration::from_millis(self.ms)
    }

    fn act(&mut self, action: Action) {
        let now = self.now();
        self.runner.handle(action, now);
    }

    /// Renders `ms` milliseconds of audio in 10 ms blocks, ticking in between.
    fn run(&mut self, ms: u64) {
        let mut out = vec![0.0; 10 * 2];
        for _ in 0..ms / 10 {
            self.mixer.process(&mut out);
            self.ms += 10;
            let now = self.now();
            self.runner.tick(now);
        }
    }

    /// Waits (in real time) until the background loader has armed the cue.
    fn wait_armed(&mut self, id: CueId) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !self.runner.is_armed(id) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
            self.run(10);
        }
        assert!(self.runner.is_armed(id), "cue not armed in time");
    }

    fn active(&self) -> Vec<String> {
        let show = self.runner.show();
        self.runner.state(self.now()).active.iter().map(|a| show.cue(a.cue).unwrap().number.clone()).collect()
    }

    fn playhead(&self) -> Option<String> {
        self.runner.playhead_cue().map(|id| self.runner.show().cue(id).unwrap().number.clone())
    }
}

fn sound(number: &str, file: &str) -> Cue {
    let mut c = Cue::new(CueKind::Sound(SoundCue::new(file)));
    c.number = number.into();
    c
}

fn fade(number: &str, targets: Targets, action: VolumeAction, duration: Option<f64>) -> Cue {
    let mut c = Cue::new(CueKind::Volume(VolumeCue {
        targets,
        action,
        volume_db: None,
        duration,
        curve: FadeCurve::Linear,
    }));
    c.number = number.into();
    c
}

/// The next cue starts at `at`.
fn with_advance(mut c: Cue, at: AdvanceAt) -> Cue {
    c.advance = Advance::new(at);
    c
}

#[test]
fn go_plays_cue_and_advances_playhead() {
    let mut rig = Rig::new(vec![sound("1", "len200.wav"), sound("2", "len200.wav")]);
    assert_eq!(rig.playhead().as_deref(), Some("1"));
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("2"));
    rig.run(50);
    assert_eq!(rig.active(), ["1"]);
    rig.run(200);
    assert!(rig.active().is_empty());
    rig.act(Action::Go);
    assert_eq!(rig.playhead(), None, "past the last cue");
    rig.act(Action::Go);
}

#[test]
fn cues_layer_instead_of_replacing_each_other() {
    let mut rig = Rig::new(vec![sound("1", "len500.wav"), sound("2", "len500.wav")]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(50);
    assert_eq!(rig.active(), ["1", "2"]);
}

#[test]
fn auto_continue_starts_next_cue_together() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len500.wav"), AdvanceAt::Start),
        sound("2", "len500.wav"),
        sound("3", "len500.wav"),
    ]);
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("3"));
    rig.run(20);
    assert_eq!(rig.active(), ["1", "2"]);
}

#[test]
fn auto_follow_starts_next_cue_after_end() {
    let mut rig =
        Rig::new(vec![with_advance(sound("1", "len100.wav"), AdvanceAt::End), sound("2", "len500.wav")]);
    rig.act(Action::Go);
    assert_eq!(rig.playhead(), None);
    rig.run(50);
    assert_eq!(rig.active(), ["1"]);
    rig.run(100);
    assert_eq!(rig.active(), ["2"]);
}

#[test]
fn pre_wait_delays_start() {
    let mut c = sound("1", "len500.wav");
    c.pre_wait = 0.2;
    let mut rig = Rig::new(vec![c]);
    rig.act(Action::Go);
    rig.run(100);
    assert!(rig.active().is_empty());
    assert_eq!(rig.runner.state(rig.now()).waiting.len(), 1);
    rig.run(150);
    assert_eq!(rig.active(), ["1"]);
}

#[test]
fn fade_cue_fades_and_stops_target() {
    let target = sound("1", "len5000.wav");
    let other = sound("2", "len5000.wav");
    let f = fade("3", Targets::Cues(vec![target.id]), VolumeAction::FadeStop, Some(0.1));
    let mut rig = Rig::new(vec![target, other, f]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(200);
    assert_eq!(rig.active(), ["2"]);
}

#[test]
fn linked_fade_cues_each_fade_their_predecessor() {
    let link = Some(jq_show::LinkId::new());
    let previous =
        || fade("", Targets::Previous(PreviousKeyword::Previous), VolumeAction::FadeStop, Some(0.05));
    let (mut f1, mut f2) = (previous(), previous());
    (f1.number, f2.number, f1.link, f2.link) = ("2".into(), "4".into(), link, link);
    let mut rig = Rig::new(vec![
        sound("1", "len5000.wav"),
        f1,
        sound("3", "len5000.wav"),
        f2,
        sound("5", "len5000.wav"),
    ]);
    // 1, 3 and 5 run; GO on 2 fades out only 1.
    for i in [0, 2, 4] {
        let id = rig.runner.show().cues[i].id;
        rig.act(Action::StartCue(id));
    }
    rig.run(50);
    let second = rig.runner.show().cues[1].id;
    rig.act(Action::StartCue(second));
    rig.run(200);
    assert_eq!(rig.active(), ["3", "5"]);
    let fourth = rig.runner.show().cues[3].id;
    rig.act(Action::StartCue(fourth));
    rig.run(200);
    assert_eq!(rig.active(), ["5"]);
}

#[test]
fn fade_cue_uses_target_fade_time_and_fade_to_keeps_running() {
    let mut target = sound("1", "len5000.wav");
    if let CueKind::Sound(s) = &mut target.kind {
        s.fade_out = 0.1;
    }
    let mut f = fade("2", Targets::All(AllKeyword::All), VolumeAction::FadeTo, None);
    if let CueKind::Volume(fc) = &mut f.kind {
        fc.volume_db = Some(-20.0);
    }
    let mut rig = Rig::new(vec![target, f]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(200);
    let state = rig.runner.state(rig.now());
    assert_eq!(state.active.len(), 1);
    assert!((state.active[0].level_db + 20.0).abs() < 0.1, "{}", state.active[0].level_db);
}

#[test]
fn fade_cue_without_running_target_succeeds_and_follows() {
    let mut rig = Rig::new(vec![
        with_advance(stop_cue("1", Targets::Output { output: "main".into() }), AdvanceAt::End),
        sound("2", "len500.wav"),
    ]);
    let second = rig.runner.show().cues[1].id;
    rig.wait_armed(second);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["2"]);
    assert!(rig.runner.take_notices().is_empty());
}

#[test]
fn stop_all_cancels_pre_wait_and_follow() {
    let mut waiting = sound("2", "len500.wav");
    waiting.pre_wait = 0.1;
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len300.wav"), AdvanceAt::End),
        sound("x", "len500.wav"),
        waiting,
    ]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::StopAll);
    rig.run(400);
    assert!(rig.active().is_empty());
    assert!(rig.runner.state(rig.now()).waiting.is_empty());
}

#[test]
fn fade_all_fades_everything() {
    let mut rig = Rig::new(vec![sound("1", "len5000.wav"), sound("2", "len5000.wav")]);
    rig.runner.set_settings(LocalSettings { fade_all: 0.1, ..rig.runner.settings().clone() });
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::FadeAll);
    rig.run(50);
    assert_eq!(rig.active().len(), 2, "still fading");
    rig.run(100);
    assert!(rig.active().is_empty());
}

#[test]
fn panic_fades_and_a_second_panic_stops_at_once() {
    let cues = ["1", "2", "3"].map(|n| sound(n, "len5000.wav"));
    let mut rig = Rig::new(cues.into());
    let mut settings = rig.runner.settings().clone();
    settings.panic_fade = 0.2;
    rig.runner.set_settings(settings);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Panic);
    rig.run(50);
    assert_eq!(rig.active().len(), 1, "still fading");
    rig.act(Action::Panic);
    rig.run(20);
    assert!(rig.active().is_empty(), "second panic during the fade");

    // After the fade has ended, a panic fades again.
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Panic);
    rig.run(300);
    assert!(rig.active().is_empty());
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Panic);
    rig.run(50);
    assert_eq!(rig.active().len(), 1, "fades again");
}

#[test]
fn a_panic_after_an_emergency_stop_fades_again() {
    let cues = ["1", "2", "3"].map(|n| sound(n, "len5000.wav"));
    let mut rig = Rig::new(cues.into());
    let mut settings = rig.runner.settings().clone();
    settings.panic_fade = 0.2;
    rig.runner.set_settings(settings);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Panic);
    rig.act(Action::Panic);
    rig.run(20);
    assert!(rig.active().is_empty());
    for _ in 0..2 {
        rig.run(500);
        rig.act(Action::Go);
        rig.run(50);
        rig.act(Action::Panic);
        rig.run(50);
        assert_eq!(rig.active().len(), 1, "fades, does not stop at once");
        rig.run(300);
        assert!(rig.active().is_empty());
    }
}

#[test]
fn panic_without_fade_time_stops_at_once() {
    let mut rig = Rig::new(vec![sound("1", "len5000.wav")]);
    let mut settings = rig.runner.settings().clone();
    settings.panic_fade = 0.0;
    rig.runner.set_settings(settings);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Panic);
    rig.run(20);
    assert!(rig.active().is_empty());
}

#[test]
fn go_lockout_drops_a_second_go_too_soon() {
    let mut rig =
        Rig::new(vec![sound("1", "len5000.wav"), sound("2", "len5000.wav"), sound("3", "len5000.wav")]);
    let mut settings = rig.runner.settings().clone();
    settings.go_lockout = 0.3;
    rig.runner.set_settings(settings);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["1"]);
    assert_eq!(rig.playhead().as_deref(), Some("2"));
    assert_eq!(rig.runner.state(rig.now()).go_rejected, 1);
    // Counted from the last GO taken, not the dropped one.
    rig.run(200);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["1", "2"]);
    assert_eq!(rig.playhead().as_deref(), Some("3"));
    assert_eq!(rig.runner.state(rig.now()).go_rejected, 1);
}

#[test]
fn go_lockout_leaves_hotkeys_alone() {
    let first = sound("1", "len5000.wav");
    let second = sound("2", "len5000.wav");
    let id = second.id;
    let mut rig = Rig::new(vec![first, second]);
    let mut settings = rig.runner.settings().clone();
    settings.go_lockout = 0.3;
    rig.runner.set_settings(settings);
    rig.act(Action::Go);
    rig.act(Action::StartCue(id));
    rig.run(20);
    assert_eq!(rig.active(), ["1", "2"]);
}

#[test]
fn stop_cue_stops_only_that_cue() {
    let one = sound("1", "len5000.wav");
    let id = one.id;
    let mut rig = Rig::new(vec![one, sound("2", "len5000.wav")]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::StopCue(id));
    rig.run(20);
    assert_eq!(rig.active(), ["2"]);
}

#[test]
fn broken_cue_reports_error_and_keeps_chain() {
    let mut rig =
        Rig::new(vec![with_advance(sound("1", "missing.wav"), AdvanceAt::End), sound("2", "len500.wav")]);
    let second = rig.runner.show().cues[1].id;
    rig.wait_armed(second);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["2"]);
    let notices = rig.runner.take_notices();
    assert!(matches!(&notices[..], [Notice::Error(m)] if m.contains("Cue 1")), "{notices:?}");
}

#[test]
fn playhead_moves_and_arms_ahead() {
    let cues: Vec<Cue> = (1..=6).map(|i| sound(&i.to_string(), &format!("len{}.wav", 100 + i))).collect();
    let (third, sixth) = (cues[2].id, cues[5].id);
    let mut rig = Rig::new(cues);
    rig.act(Action::PlayheadNext);
    rig.act(Action::PlayheadNext);
    assert_eq!(rig.playhead().as_deref(), Some("3"));
    rig.act(Action::PlayheadPrev);
    rig.act(Action::SetPlayhead(third));
    rig.wait_armed(third);
    rig.act(Action::Go);
    assert!(rig.runner.take_notices().is_empty(), "GO used the armed buffer");
    rig.wait_armed(sixth);
    let loads = rig.loader.loads.lock().unwrap();
    assert_eq!(loads.iter().filter(|p| p.ends_with("len103.wav")).count(), 1);
}

#[test]
fn loop_times_reports_remaining_over_all_passes() {
    let mut c = sound("1", "len1000.wav");
    if let CueKind::Sound(s) = &mut c.kind {
        s.loop_mode = LoopMode::Times(3);
    }
    let mut rig = Rig::new(vec![c]);
    rig.act(Action::Go);
    rig.run(500);
    let a = &rig.runner.state(rig.now()).active[0];
    assert!((a.remaining.unwrap() - 2.5).abs() < 0.02, "three passes in all: {:?}", a.remaining);
    assert_eq!(a.pass, Some(PassState { pass: 1, count: Some(3), vamp: None }));
    rig.run(1000);
    assert_eq!(active_state(&rig, "1").unwrap().pass.map(|p| p.pass), Some(2));
    rig.run(1400);
    assert_eq!(rig.active(), ["1"], "still in the third pass");
    rig.run(200);
    assert!(rig.active().is_empty());
}

#[test]
fn vamp_pass_counts_up_without_an_end() {
    let mut c = sound("1", "len5000.wav");
    if let CueKind::Sound(s) = &mut c.kind {
        s.vamps = vec![Vamp { start: 1.0, end: 1.5 }, Vamp { start: 2.0, end: 3.0 }];
    }
    let mut rig = Rig::new(vec![c]);
    rig.act(Action::Go);
    rig.run(500);
    assert_eq!(active_state(&rig, "1").unwrap().pass, None, "not in a vamp yet");
    rig.run(1300);
    assert_eq!(
        active_state(&rig, "1").unwrap().pass,
        Some(PassState { pass: 2, count: None, vamp: Some(1) })
    );
}

#[test]
fn set_show_keeps_playhead_on_its_cue() {
    let mut rig = Rig::new(vec![sound("1", "len100.wav"), sound("2", "len100.wav")]);
    rig.act(Action::PlayheadNext);
    let mut show = rig.runner.show().clone();
    show.cues.insert(0, sound("0", "len100.wav"));
    rig.runner.set_show(show);
    assert_eq!(rig.playhead().as_deref(), Some("2"));
}

#[test]
fn fade_cue_action_uses_own_fade_time() {
    let mut one = sound("1", "len5000.wav");
    if let CueKind::Sound(s) = &mut one.kind {
        s.fade_out = 0.1;
    }
    let id = one.id;
    let mut rig = Rig::new(vec![one, sound("2", "len5000.wav")]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::FadeCue(id));
    rig.run(50);
    assert_eq!(rig.active(), ["1", "2"], "still fading");
    rig.run(100);
    assert_eq!(rig.active(), ["2"]);
}

#[test]
fn fade_cue_reports_progress_until_done() {
    let target = sound("1", "len5000.wav");
    let f = fade("2", Targets::Cues(vec![target.id]), VolumeAction::FadeTo, Some(0.2));
    let fade_id = f.id;
    let mut rig = Rig::new(vec![target, f]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(100);
    let state = rig.runner.state(rig.now());
    assert_eq!(state.fading.len(), 1);
    assert_eq!(state.fading[0].cue, fade_id);
    assert!((state.fading[0].progress - 0.5).abs() < 0.01, "{}", state.fading[0].progress);
    assert_eq!(rig.active(), ["1"], "fade cue is not an active cue");
    rig.run(110);
    assert!(rig.runner.state(rig.now()).fading.is_empty());
}

#[test]
fn stop_all_ends_fade_progress() {
    let target = sound("1", "len5000.wav");
    let f = fade("2", Targets::Cues(vec![target.id]), VolumeAction::FadeStop, Some(1.0));
    let mut rig = Rig::new(vec![target, f]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::StopAll);
    assert!(rig.runner.state(rig.now()).fading.is_empty());
}

#[test]
fn running_cue_is_not_triggered_twice() {
    let mut rig = Rig::new(vec![sound("1", "len500.wav")]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::SetPlayhead(rig.runner.show().cues[0].id));
    rig.act(Action::Go);
    rig.run(50);
    assert_eq!(rig.active(), ["1"]);
    assert!(
        !rig.runner
            .take_notices()
            .iter()
            .any(|n| matches!(n, Notice::Warning(w) if w.contains("is already running"))),
        "a GO on a running cue is swallowed silently"
    );
    assert_eq!(rig.playhead(), None, "GO still advances the playhead");
    rig.run(450);
    assert!(rig.active().is_empty());
    rig.act(Action::SetPlayhead(rig.runner.show().cues[0].id));
    rig.act(Action::Go);
    rig.run(50);
    assert_eq!(rig.active(), ["1"], "can be triggered again once it has ended");
}

#[test]
fn waiting_cue_is_not_triggered_twice() {
    let mut c = sound("1", "len500.wav");
    c.pre_wait = 0.2;
    let mut rig = Rig::new(vec![c]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::SetPlayhead(rig.runner.show().cues[0].id));
    rig.act(Action::Go);
    assert_eq!(rig.runner.state(rig.now()).waiting.len(), 1);
    rig.run(200);
    assert_eq!(rig.active(), ["1"]);
}

#[test]
fn fading_cue_is_not_triggered_twice() {
    let target = sound("1", "len5000.wav");
    let f = fade("2", Targets::Cues(vec![target.id]), VolumeAction::FadeTo, Some(0.2));
    let fade_id = f.id;
    let mut rig = Rig::new(vec![target, f]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::SetPlayhead(fade_id));
    rig.act(Action::Go);
    let state = rig.runner.state(rig.now());
    assert!((state.fading[0].progress - 0.5).abs() < 0.01, "fade was not restarted");
}

#[test]
fn blocked_cue_keeps_auto_continue_chain() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len100.wav"), AdvanceAt::Start),
        with_advance(sound("2", "len500.wav"), AdvanceAt::Start),
        sound("3", "len100.wav"),
    ]);
    let second = rig.runner.show().cues[1].id;
    rig.act(Action::StartCue(second));
    rig.run(50);
    assert_eq!(rig.active(), ["2", "3"]);
    rig.run(100);
    assert_eq!(rig.active(), ["2"]);
    rig.act(Action::Go);
    rig.run(50);
    let mut active = rig.active();
    active.sort();
    assert_eq!(active, ["1", "2", "3"]);
    assert!(rig.runner.take_notices().contains(&Notice::Warning("Cue 2 is already running".into())));
}

fn edit_sound(rig: &mut Rig, number: &str, f: impl FnOnce(&mut SoundCue)) {
    let mut show = rig.runner.show().clone();
    let cue = show.cues.iter_mut().find(|c| c.number == number).unwrap();
    let CueKind::Sound(s) = &mut cue.kind else { panic!("not a sound cue") };
    f(s);
    rig.runner.set_show(show);
}

fn level_db(rig: &Rig) -> f32 {
    rig.runner.state(rig.now()).active[0].level_db
}

#[test]
fn volume_edit_applies_to_running_cue() {
    let mut rig = Rig::new(vec![sound("1", "len5000.wav")]);
    rig.act(Action::Go);
    rig.run(50);
    edit_sound(&mut rig, "1", |s| s.volume_db = -12.0);
    rig.run(50);
    assert!((level_db(&rig) + 12.0).abs() < 0.01, "{}", level_db(&rig));
}

#[test]
fn volume_edit_keeps_fade_out_running() {
    let mut target = sound("1", "len5000.wav");
    if let CueKind::Sound(s) = &mut target.kind {
        s.volume_db = -6.0;
    }
    let f = fade("2", Targets::Cues(vec![target.id]), VolumeAction::FadeStop, Some(0.2));
    let mut rig = Rig::new(vec![target, f]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(50);
    edit_sound(&mut rig, "1", |s| s.volume_db = 0.0);
    rig.run(250);
    assert!(rig.active().is_empty());
}

#[test]
fn fade_to_target_stays_absolute_after_volume_edit() {
    let target = sound("1", "len5000.wav");
    let mut f = fade("2", Targets::Cues(vec![target.id]), VolumeAction::FadeTo, Some(0.1));
    if let CueKind::Volume(fc) = &mut f.kind {
        fc.volume_db = Some(-20.0);
    }
    let mut rig = Rig::new(vec![target, f]);
    rig.act(Action::Go);
    rig.run(50);
    edit_sound(&mut rig, "1", |s| s.volume_db = -6.0);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(200);
    assert!((level_db(&rig) + 20.0).abs() < 0.01, "{}", level_db(&rig));
}

#[test]
fn level_limits_cap_and_silence_cue_volumes() {
    let mut loud = sound("1", "len5000.wav");
    let mut quiet = sound("2", "len5000.wav");
    if let (CueKind::Sound(l), CueKind::Sound(q)) = (&mut loud.kind, &mut quiet.kind) {
        l.volume_db = 20.0;
        // At the minimum: silent, whatever the normalization adds.
        (q.volume_db, q.normalize_db) = (-60.0, Some(30.0));
    }
    let mut rig = Rig::new(vec![loud, quiet]);
    rig.act(Action::Go);
    rig.run(50);
    assert!((level_db(&rig) - 6.0).abs() < 0.01, "capped at +6: {}", level_db(&rig));
    edit_sound(&mut rig, "1", |s| s.volume_db = -70.0);
    rig.act(Action::Go);
    rig.run(50);
    let state = rig.runner.state(rig.now());
    assert!(state.active[1].level_db < -100.0, "{}", state.active[1].level_db);
    let mut out = vec![0.0; 2];
    rig.mixer.process(&mut out);
    assert!(out.iter().all(|s| s.abs() < 1e-5), "both silent: {out:?}");
}

#[test]
fn decibel_end_fade_falls_evenly_to_the_show_minimum() {
    let mut cue = sound("1", "len3000.wav");
    if let CueKind::Sound(s) = &mut cue.kind {
        (s.fade_out, s.end_fade, s.curve) = (2.0, true, FadeCurve::Decibel);
    }
    let mut rig = Rig::new(vec![cue]);
    let mut show = rig.runner.show().clone();
    show.level_limits.min_db = -40.0;
    rig.runner.set_show(show);
    rig.act(Action::Go);
    // Positions come every 10 ms: the level seen lags by up to one report.
    rig.run(2000);
    assert!((level_db(&rig) + 20.0).abs() < 0.5, "halfway to −40 dB: {}", level_db(&rig));
    rig.run(500);
    assert!((level_db(&rig) + 30.0).abs() < 0.5, "{}", level_db(&rig));
}

#[test]
fn normalization_offset_adds_to_the_volume() {
    let mut cue = sound("1", "len5000.wav");
    let mut f = fade("2", Targets::Cues(vec![cue.id]), VolumeAction::FadeTo, Some(0.05));
    if let (CueKind::Sound(s), CueKind::Volume(fc)) = (&mut cue.kind, &mut f.kind) {
        (s.volume_db, s.normalize_db) = (-3.0, Some(-5.0));
        fc.volume_db = Some(-10.0);
    }
    let mut rig = Rig::new(vec![cue, f]);
    rig.act(Action::Go);
    rig.run(50);
    assert!((level_db(&rig) + 8.0).abs() < 0.01, "{}", level_db(&rig));
    rig.act(Action::Go);
    rig.run(100);
    assert!((level_db(&rig) + 15.0).abs() < 0.01, "fade target plus offset: {}", level_db(&rig));
}

#[test]
fn normalizing_a_running_cue_changes_its_level() {
    let mut cue = sound("1", "len5000.wav");
    if let CueKind::Sound(s) = &mut cue.kind {
        s.volume_db = -3.0;
    }
    let id = cue.id;
    let mut rig = Rig::new(vec![cue]);
    rig.act(Action::Go);
    rig.run(50);
    assert!((level_db(&rig) + 3.0).abs() < 0.01, "{}", level_db(&rig));
    let mut show = rig.runner.show().clone();
    if let Some(CueKind::Sound(s)) = show.cue_mut(id).map(|c| &mut c.kind) {
        s.normalize_db = Some(-5.0);
    }
    rig.runner.set_show(show);
    rig.run(50);
    assert!((level_db(&rig) + 8.0).abs() < 0.01, "{}", level_db(&rig));
    let mut show = rig.runner.show().clone();
    if let Some(CueKind::Sound(s)) = show.cue_mut(id).map(|c| &mut c.kind) {
        s.normalize_db = None;
    }
    rig.runner.set_show(show);
    rig.run(50);
    assert!((level_db(&rig) + 3.0).abs() < 0.01, "offset removed: {}", level_db(&rig));
}

#[test]
fn fade_to_below_the_minimum_is_silence() {
    let target = sound("1", "len5000.wav");
    let mut f = fade("2", Targets::Cues(vec![target.id]), VolumeAction::FadeTo, Some(0.05));
    if let CueKind::Volume(fc) = &mut f.kind {
        fc.volume_db = Some(-65.0);
    }
    let mut rig = Rig::new(vec![target, f]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(100);
    assert_eq!(rig.active(), ["1"], "fade to keeps it running");
    assert!(level_db(&rig) < -100.0, "{}", level_db(&rig));
}

#[test]
fn pan_edit_applies_to_running_cue() {
    let mut rig = Rig::new(vec![sound("1", "len5000.wav")]);
    rig.act(Action::Go);
    rig.run(50);
    edit_sound(&mut rig, "1", |s| s.pan = -1.0);
    rig.run(50);
    let mut out = vec![0.0; 2];
    rig.mixer.process(&mut out);
    assert_eq!(out, [0.5, 0.0]);
}

#[test]
fn live_edit_applies_to_running_cue_until_set_show() {
    let mut rig = Rig::new(vec![sound("1", "len5000.wav")]);
    let saved = rig.runner.show().clone();
    rig.act(Action::Go);
    rig.run(50);
    let mut cue = saved.cues[0].clone();
    if let CueKind::Sound(s) = &mut cue.kind {
        s.volume_db = -12.0;
    }
    assert!(rig.runner.live_edit(cue));
    rig.run(50);
    assert!((level_db(&rig) + 12.0).abs() < 0.01, "{}", level_db(&rig));
    // The committed show (here: the edit given up) replaces the live one.
    rig.runner.set_show(saved);
    rig.run(50);
    assert!(level_db(&rig).abs() < 0.01, "{}", level_db(&rig));
    assert!(!rig.runner.live_edit(sound("2", "len5000.wav")), "no such cue");
}

#[test]
fn live_edit_reaches_running_linked_cues() {
    let link = Some(jq_show::LinkId::new());
    let (mut a, mut b) = (sound("1", "len5000.wav"), sound("2", "len5000.wav"));
    (a.link, b.link) = (link, link);
    let mut rig = Rig::new(vec![a, b]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(50);
    let mut cue = rig.runner.show().cues[0].clone();
    if let CueKind::Sound(s) = &mut cue.kind {
        s.volume_db = -12.0;
    }
    assert!(rig.runner.live_edit(cue));
    rig.run(50);
    let levels: Vec<f32> = rig.runner.state(rig.now()).active.iter().map(|a| a.level_db).collect();
    assert_eq!(levels.len(), 2);
    assert!(levels.iter().all(|l| (l + 12.0).abs() < 0.01), "{levels:?}");
}

#[test]
fn idle_only_without_running_or_waiting_cues() {
    let mut waiting = sound("2", "len200.wav");
    waiting.pre_wait = 0.1;
    let mut rig = Rig::new(vec![sound("1", "len200.wav"), waiting]);
    assert!(rig.runner.is_idle());
    rig.act(Action::Go);
    rig.run(10);
    assert!(!rig.runner.is_idle());
    rig.run(300);
    assert!(rig.runner.is_idle());
    rig.act(Action::Go);
    assert!(!rig.runner.is_idle(), "pre-wait counts as busy");
    rig.act(Action::StopAll);
    assert!(rig.runner.is_idle());
}

#[test]
fn fade_cue_ends_with_its_targets() {
    let mut f = fade("2", Targets::All(AllKeyword::All), VolumeAction::FadeTo, Some(1.0));
    if let CueKind::Volume(fc) = &mut f.kind {
        fc.volume_db = Some(-20.0);
    }
    let mut rig = Rig::new(vec![sound("1", "len200.wav"), f]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(100);
    assert_eq!(rig.runner.state(rig.now()).fading.len(), 1);
    rig.run(300);
    assert!(rig.runner.state(rig.now()).fading.is_empty(), "nothing left to fade");
    assert!(rig.runner.is_quiet() && !rig.runner.is_lively());
}

#[test]
fn running_wait_cue_keeps_runner_lively() {
    let mut rig = Rig::new(vec![wait("1", 0.5)]);
    rig.act(Action::Go);
    rig.run(100);
    assert!(rig.runner.is_quiet());
    assert!(rig.runner.is_lively(), "the wait cue still runs");
    rig.run(500);
    assert!(!rig.runner.is_lively());
}

#[test]
fn moved_media_dir_arms_from_new_place() {
    let cue = sound("1", "len100.wav");
    let id = cue.id;
    let mut rig = Rig::new(vec![cue]);
    rig.wait_armed(id);
    rig.runner.set_media_dir(PathBuf::from("renamed/media"));
    rig.wait_armed(id);
    let loads = rig.loader.loads.lock().unwrap();
    assert_eq!(loads.last().unwrap(), &Path::new("renamed/media").join("len100.wav"));
}

fn vamped(number: &str, file: &str, vamps: &[(f64, f64)], marks: &[f64]) -> Cue {
    let mut c = sound(number, file);
    if let CueKind::Sound(s) = &mut c.kind {
        s.vamps = vamps.iter().map(|&(start, end)| Vamp { start, end }).collect();
        s.marks = marks.to_vec();
    }
    c
}

/// A start/stop cue.
fn control(number: &str, kind: CueKind) -> Cue {
    let mut c = Cue::new(kind);
    c.number = number.into();
    c
}

fn stop_cue(number: &str, targets: Targets) -> Cue {
    control(number, CueKind::StartStop(StartStopCue { targets, action: StartStopAction::Stop, from: None }))
}

fn devamp(number: &str, target: CueId, mode: DevampMode) -> Cue {
    let mut c = Cue::new(CueKind::ExitLoop(ExitLoopCue {
        targets: Targets::Cues(vec![target]),
        exit: ExitLoop { mode, ..Default::default() },
    }));
    c.number = number.into();
    c
}

fn remaining(rig: &Rig) -> Option<f64> {
    rig.runner.state(rig.now()).active[0].remaining
}

#[test]
fn devamp_cue_releases_vamps_one_after_the_other() {
    let song = vamped("1", "len500.wav", &[(0.1, 0.2), (0.3, 0.4)], &[]);
    let id = song.id;
    let mut rig =
        Rig::new(vec![song, devamp("2", id, DevampMode::LoopEnd), devamp("3", id, DevampMode::LoopEnd)]);
    rig.wait_armed(id);
    rig.act(Action::Go);
    rig.run(1000);
    assert_eq!(rig.active(), ["1"], "vamps until devamped");
    assert!(rig.runner.state(rig.now()).active[0].looping);
    assert_eq!(remaining(&rig), None);

    rig.act(Action::Go);
    rig.run(1000);
    assert_eq!(rig.active(), ["1"], "held in vamp 2");
    assert_eq!(remaining(&rig), None);

    rig.act(Action::Go);
    rig.run(20);
    let left = remaining(&rig).expect("known once all vamps are released");
    assert!(left > 0.0 && left <= 0.2, "{left}");
    rig.run(200);
    assert!(rig.active().is_empty());
    assert!(rig.runner.take_notices().is_empty());
}

#[test]
fn devamp_immediate_and_at_mark_leave_early() {
    for (mode, max_ms) in [(DevampMode::Immediate, 120), (DevampMode::NextMark, 220)] {
        // Vamp 0.1–0.4 s, marks every 0.1 s; the rest after the vamp is 0.1 s.
        let song = vamped("1", "len500.wav", &[(0.1, 0.4)], &[0.1, 0.2, 0.3]);
        let id = song.id;
        let mut rig = Rig::new(vec![song, devamp("2", id, mode)]);
        rig.act(Action::Go);
        rig.run(150);
        rig.act(Action::Go);
        rig.run(max_ms);
        assert!(rig.active().is_empty(), "{mode:?} still running");
    }
}

#[test]
fn devamp_without_running_target_succeeds_and_follows() {
    let song = vamped("1", "len500.wav", &[(0.1, 0.2)], &[]);
    let id = song.id;
    let mut rig = Rig::new(vec![
        song,
        with_advance(devamp("2", id, DevampMode::LoopEnd), AdvanceAt::End),
        sound("3", "len300.wav"),
    ]);
    rig.act(Action::SetPlayhead(rig.runner.show().cues[1].id));
    rig.wait_armed(rig.runner.show().cues[2].id);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["3"]);
    assert!(rig.runner.take_notices().is_empty());
}

#[test]
fn endless_loop_can_be_devamped() {
    let mut song = sound("1", "len200.wav");
    if let CueKind::Sound(s) = &mut song.kind {
        s.loop_mode = LoopMode::Forever;
    }
    let id = song.id;
    let mut rig = Rig::new(vec![song, devamp("2", id, DevampMode::LoopEnd)]);
    rig.act(Action::Go);
    rig.run(1000);
    assert_eq!(rig.active(), ["1"]);
    rig.act(Action::Go);
    rig.run(300);
    assert!(rig.active().is_empty());
}

fn wait(number: &str, duration: f64) -> Cue {
    let mut c = Cue::new(CueKind::Wait(WaitCue { duration }));
    c.number = number.into();
    c
}

#[test]
fn wait_cue_delays_the_next_cue_and_shows_progress() {
    let mut rig = Rig::new(vec![with_advance(wait("1", 0.2), AdvanceAt::End), sound("2", "len300.wav")]);
    rig.act(Action::Go);
    rig.run(100);
    assert!(rig.active().is_empty());
    let fading = rig.runner.state(rig.now()).fading;
    assert_eq!(fading.len(), 1);
    assert!((fading[0].progress - 0.5).abs() < 0.06, "{}", fading[0].progress);
    rig.run(120);
    assert_eq!(rig.active(), ["2"]);
    assert!(rig.runner.state(rig.now()).fading.is_empty());
}

#[test]
fn exclusive_mode_fades_earlier_cues_but_layers_its_own_chain() {
    let mut rig = Rig::new(vec![
        sound("1", "len2000.wav"),
        with_advance(sound("2", "len2000.wav"), AdvanceAt::Start),
        sound("3", "len2000.wav"),
    ]);
    let mut show = rig.runner.show().clone();
    show.others = OthersEffect { effect: OthersAction::FadeStop, fade: 0.1, ..Default::default() };
    rig.runner.set_show(show);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(200);
    let mut active = rig.active();
    active.sort();
    assert_eq!(active, ["2", "3"], "cue 1 faded out, 2 and 3 started together");
}

#[test]
fn jumps_move_within_the_running_cue() {
    let cue = sound("1", "len2000.wav");
    let id = cue.id;
    let mut rig = Rig::new(vec![cue]);
    let mut settings = rig.runner.settings().clone();
    (settings.jump_back, settings.jump_forward, settings.jump_end) = (0.25, 0.5, 0.5);
    rig.runner.set_settings(settings);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::JumpEnd(id));
    rig.run(20);
    let left = remaining(&rig).unwrap();
    assert!((left - 0.48).abs() < 0.05, "{left}");
    rig.act(Action::JumpBack(id));
    rig.run(20);
    let left = remaining(&rig).unwrap();
    assert!((left - 0.71).abs() < 0.06, "{left}");
    rig.act(Action::JumpForward(id));
    rig.act(Action::JumpForward(id));
    rig.run(20);
    assert!(rig.active().is_empty(), "jumped past the end");
}

#[test]
fn jump_over_a_vamp_counts_as_devamped() {
    let song = vamped("1", "len3000.wav", &[(0.2, 0.4)], &[]);
    let id = song.id;
    let mut rig = Rig::new(vec![song]);
    let mut settings = rig.runner.settings().clone();
    settings.jump_end = 0.5;
    rig.runner.set_settings(settings);
    rig.act(Action::Go);
    rig.run(500);
    assert_eq!(remaining(&rig), None);
    rig.act(Action::JumpEnd(id));
    rig.run(20);
    assert!(remaining(&rig).is_some());
    rig.run(10_000);
    assert!(rig.active().is_empty());
}

fn preview_on_main_pair(rig: &mut Rig) {
    let mut settings = rig.runner.settings().clone();
    settings.preview_channel = Some(1);
    rig.runner.set_settings(settings);
}

/// Waits (in real time) until the preview has loaded and started.
fn wait_preview_playing(rig: &mut Rig) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !rig.runner.state(rig.now()).preview.is_some_and(|p| p.playing) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
        rig.run(10);
    }
    assert!(rig.runner.state(rig.now()).preview.is_some_and(|p| p.playing), "preview not started");
}

#[test]
fn preview_plays_without_touching_the_show() {
    let first = sound("1", "len300.wav");
    let id = first.id;
    let mut rig = Rig::new(vec![first, sound("2", "len300.wav")]);
    preview_on_main_pair(&mut rig);
    rig.act(Action::Preview(id));
    wait_preview_playing(&mut rig);
    assert!(rig.active().is_empty());
    assert!(rig.runner.is_idle());
    assert_eq!(rig.playhead().as_deref(), Some("1"));
    rig.run(400);
    assert!(rig.runner.state(rig.now()).preview.is_none(), "ends by itself");
    assert!(rig.runner.take_notices().is_empty());
}

#[test]
fn preview_toggles_and_switches_cues() {
    let (a, b) = (sound("1", "len2000.wav"), sound("2", "len2000.wav"));
    let (ida, idb) = (a.id, b.id);
    let mut rig = Rig::new(vec![a, b]);
    preview_on_main_pair(&mut rig);
    rig.act(Action::Preview(ida));
    wait_preview_playing(&mut rig);
    rig.act(Action::Preview(idb));
    wait_preview_playing(&mut rig);
    assert_eq!(rig.runner.state(rig.now()).preview.unwrap().cue, idb);
    rig.act(Action::Preview(idb));
    assert!(rig.runner.state(rig.now()).preview.is_none());
    rig.run(20);
    assert_eq!(rig.mixer.active_voices(), 0, "voices stopped");
}

fn preview_position(rig: &Rig) -> f64 {
    rig.runner.state(rig.now()).preview.unwrap().position
}

#[test]
fn preview_at_plays_from_the_position_to_the_file_end() {
    let mut cue = sound("1", "len2000.wav");
    if let CueKind::Sound(s) = &mut cue.kind {
        (s.start, s.end) = (0.2, Some(0.5));
    }
    let id = cue.id;
    let mut rig = Rig::new(vec![cue]);
    preview_on_main_pair(&mut rig);
    rig.runner.preview_at(id, 1.0, None);
    wait_preview_playing(&mut rig);
    let at = preview_position(&rig);
    assert!((1.0..1.1).contains(&at), "{at}");
    rig.run(500);
    let at = preview_position(&rig);
    assert!(at > 1.45, "plays past the cue's end: {at}");
    assert!(rig.active().is_empty(), "the show stays alone");
    rig.run(700);
    assert!(rig.runner.state(rig.now()).preview.is_none(), "ends at the file's end");
    assert!(rig.runner.take_notices().is_empty());
}

#[test]
fn preview_at_stops_at_the_given_end() {
    let cue = sound("1", "len5000.wav");
    let id = cue.id;
    let mut rig = Rig::new(vec![cue]);
    preview_on_main_pair(&mut rig);
    rig.runner.preview_at(id, 1.0, Some(1.3));
    wait_preview_playing(&mut rig);
    rig.run(400);
    assert!(rig.runner.state(rig.now()).preview.is_none(), "ended at 1.3 s");
}

#[test]
fn preview_at_moves_a_playing_preview() {
    let cue = sound("1", "len5000.wav");
    let id = cue.id;
    let mut rig = Rig::new(vec![cue]);
    preview_on_main_pair(&mut rig);
    rig.runner.preview_at(id, 3.0, None);
    wait_preview_playing(&mut rig);
    rig.runner.preview_at(id, 0.5, None);
    rig.run(100);
    let at = preview_position(&rig);
    assert!((0.55..0.7).contains(&at), "{at}");
    assert_eq!(rig.mixer.active_voices(), 1, "the same voice moved");
    rig.act(Action::Preview(id));
    wait_preview_playing(&mut rig);
    let at = preview_position(&rig);
    assert!(at < 0.1, "the cue's own preview from its start: {at}");
}

#[test]
fn preview_at_waits_for_the_streamed_part() {
    let cue = sound("1", "stream.wav");
    let id = cue.id;
    let mut rig = Rig::new(vec![cue]);
    preview_on_main_pair(&mut rig);
    // Second chunk, not loaded.
    rig.runner.preview_at(id, 70.0, None);
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(2));
        rig.run(10);
    }
    assert!(rig.runner.state(rig.now()).preview.is_some_and(|p| !p.playing), "waits instead of silence");
    assert_eq!(rig.mixer.active_voices(), 0);
    rig.runner.preview_at(id, 1.0, None);
    wait_preview_playing(&mut rig);
    assert!(rig.runner.take_notices().is_empty());
}

#[test]
fn preview_needs_an_output() {
    let cue = sound("1", "len300.wav");
    let id = cue.id;
    let mut rig = Rig::new(vec![cue]);
    rig.act(Action::Preview(id));
    assert!(rig.runner.state(rig.now()).preview.is_none());
    let notices = rig.runner.take_notices();
    assert!(matches!(&notices[..], [Notice::Warning(m)] if m.contains("preview")), "{notices:?}");
}

#[test]
fn preview_plays_on_a_separate_device() {
    let cue = sound("1", "len2000.wav");
    let id = cue.id;
    let mut rig = Rig::new(vec![cue]);
    let mut settings = rig.runner.settings().clone();
    settings.preview_device = Some("Kopfhörer".into());
    rig.runner.set_settings(settings);
    let (ctl, mut headphones) = engine(EngineConfig { report_rate_hz: 100, ..EngineConfig::new(RATE, 2) });
    rig.runner.set_preview_engine(Some(ctl));
    rig.act(Action::Preview(id));
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut out = vec![0.0; 20];
    while headphones.active_voices() == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
        rig.run(10);
        headphones.process(&mut out);
    }
    assert_eq!(headphones.active_voices(), 1, "plays on the preview device");
    assert_eq!(rig.mixer.active_voices(), 0, "nothing on the main device");
    assert!(out.iter().any(|&s| s != 0.0));
}

#[test]
fn start_cue_leaves_the_playhead_alone() {
    let mut rig =
        Rig::new(vec![sound("1", "len300.wav"), sound("2", "len300.wav"), sound("3", "len300.wav")]);
    let third = rig.runner.show().cues[2].id;
    rig.act(Action::StartCue(third));
    rig.run(20);
    assert_eq!(rig.active(), ["3"]);
    assert_eq!(rig.playhead().as_deref(), Some("1"));
}

/// Target and packet of each sent OSC message.
type Sent = Vec<(String, Vec<u8>)>;

/// Records OSC packets instead of sending them.
#[derive(Clone, Default)]
struct OscRecorder(Arc<Mutex<Sent>>);

impl osc::OscOut for OscRecorder {
    fn send(&self, target: &str, packet: &[u8]) -> Result<(), String> {
        self.0.lock().unwrap().push((target.into(), packet.to_vec()));
        Ok(())
    }
}

fn osc_cue(number: &str, device: Option<&str>, addresses: &[&str]) -> Cue {
    let mut c = Cue::new(CueKind::Osc(OscCue {
        device: device.map(Into::into),
        messages: addresses
            .iter()
            .map(|a| OscMessage { address: (*a).into(), args: vec![OscArg::Int(1)] })
            .collect(),
    }));
    c.number = number.into();
    c
}

#[test]
fn osc_cue_sends_to_its_device_and_continues() {
    let mut rig = Rig::new(vec![
        with_advance(osc_cue("1", Some("lx"), &["/pb/1/go", "/pb/2/go"]), AdvanceAt::Start),
        sound("2", "len300.wav"),
    ]);
    let mut show = rig.runner.show().clone();
    show.devices.push(NetDevice {
        id: "lx".into(),
        name: "Licht".into(),
        host: "10.0.0.5".into(),
        port: 8000,
    });
    rig.runner.set_show(show);
    let recorder = OscRecorder::default();
    rig.runner.set_osc_out(Box::new(recorder.clone()));
    rig.act(Action::Go);
    rig.run(20);
    let sent = recorder.0.lock().unwrap().clone();
    let addresses: Vec<_> = sent.iter().map(|(t, p)| (t.as_str(), osc::decode(p)[0].0.clone())).collect();
    assert_eq!(addresses, [("10.0.0.5:8000", "/pb/1/go".into()), ("10.0.0.5:8000", "/pb/2/go".into())]);
    assert_eq!(rig.active(), ["2"], "light and sound on one GO");
}

#[test]
fn broken_osc_cue_reports_and_keeps_the_chain() {
    let mut rig = Rig::new(vec![
        with_advance(osc_cue("1", Some("weg"), &["/go"]), AdvanceAt::End),
        sound("2", "len300.wav"),
    ]);
    rig.runner.set_osc_out(Box::new(OscRecorder::default()));
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["2"]);
    let notices = rig.runner.take_notices();
    assert!(notices.iter().any(|n| matches!(n, Notice::Error(m) if m.contains("weg"))), "{notices:?}");
}

#[derive(Clone, Default)]
struct MidiRecorder(Arc<Mutex<Vec<Vec<u8>>>>);

impl midi::MidiSink for MidiRecorder {
    fn send(&mut self, message: &[u8]) -> Result<(), String> {
        self.0.lock().unwrap().push(message.to_vec());
        Ok(())
    }
}

#[test]
fn midi_cue_sends_its_messages_and_continues() {
    let mut cue = Cue::new(CueKind::Midi(MidiCue {
        messages: vec![
            MidiMessage::ProgramChange { channel: 0, program: 3 },
            MidiMessage::Msc { device: 127, command: MscCommand::Go, cue: "5".into(), list: String::new() },
        ],
    }));
    cue.number = "1".into();
    let mut rig = Rig::new(vec![with_advance(cue, AdvanceAt::Start), sound("2", "len300.wav")]);
    let recorder = MidiRecorder::default();
    rig.runner.set_midi_out(Some(Box::new(recorder.clone())));
    rig.act(Action::Go);
    rig.run(20);
    let sent = recorder.0.lock().unwrap().clone();
    assert_eq!(sent, [vec![0xc0, 3], vec![0xf0, 0x7f, 127, 0x02, 0x7f, 0x01, b'5', 0xf7]]);
    assert_eq!(rig.active(), ["2"]);
}

#[test]
fn midi_cue_without_output_reports_and_continues() {
    let mut cue =
        Cue::new(CueKind::Midi(MidiCue { messages: vec![MidiMessage::NoteOff { channel: 0, note: 1 }] }));
    cue.number = "1".into();
    let mut rig = Rig::new(vec![with_advance(cue, AdvanceAt::Start), sound("2", "len300.wav")]);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["2"]);
    let notices = rig.runner.take_notices();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Error(m) if m.contains("MIDI output"))),
        "{notices:?}"
    );
}

fn playlist(number: &str, files: &[&str], gap: f64, repeat: bool) -> Cue {
    let mut c = Cue::new(CueKind::Playlist(PlaylistCue {
        gap,
        repeat,
        ..PlaylistCue::new(files.iter().map(|f| (*f).into()).collect())
    }));
    c.number = number.into();
    c
}

fn item(rig: &Rig) -> Option<(usize, usize)> {
    rig.runner.state(rig.now()).active.first().and_then(|a| a.item).map(|i| (i.index, i.count))
}

#[test]
fn playlist_plays_its_items_in_turn_then_follows() {
    let mut rig = Rig::new(vec![
        with_advance(playlist("1", &["len200.wav", "len300.wav"], 0.0, false), AdvanceAt::End),
        sound("2", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(item(&rig), Some((0, 2)));
    rig.run(250);
    assert_eq!(item(&rig), Some((1, 2)), "second item started without a gap");
    assert_eq!(rig.active(), ["1"]);
    rig.run(300);
    assert_eq!(rig.active(), ["2"], "auto-follow after the last item");
}

#[test]
fn playlist_gap_and_repeat() {
    let list = playlist("1", &["len100.wav", "len100.wav"], 0.1, true);
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(150);
    assert!(rig.active().is_empty(), "in the gap");
    rig.act(Action::StartCue(id));
    rig.run(10);
    // Swallowed silently (second trigger "ignore").
    assert!(
        !rig.runner
            .take_notices()
            .iter()
            .any(|n| matches!(n, Notice::Warning(m) if m.contains("is already running")))
    );
    rig.run(100);
    assert_eq!(item(&rig), Some((1, 2)));
    rig.run(200);
    assert_eq!(item(&rig), Some((0, 2)), "starts over");
}

#[test]
fn playlist_gap_is_reported_with_the_next_item() {
    let list = playlist("1", &["len100.wav", "len100.wav"], 0.2, true);
    let mut rig = Rig::new(vec![list]);
    let gap = |rig: &Rig| {
        let state = rig.runner.state(rig.now());
        state.gaps.first().map(|g| (g.next.index, g.next.count, g.remaining, g.paused))
    };
    rig.act(Action::Go);
    rig.run(50);
    assert_eq!(gap(&rig), None, "playing, not in a gap");
    rig.run(100);
    let (index, count, remaining, paused) = gap(&rig).expect("in the gap");
    assert_eq!((index, count, paused), (1, 2, false));
    assert!(remaining > 0.1 && remaining <= 0.2, "{remaining}");
    rig.act(Action::PauseAll);
    rig.run(500);
    let (_, _, held, paused) = gap(&rig).expect("still in the gap");
    assert!(paused && (held - remaining).abs() < 0.02, "{held} vs {remaining}");
    rig.act(Action::ResumeAll);
    rig.run(250);
    assert_eq!(gap(&rig), None);
    assert_eq!(item(&rig), Some((1, 2)));
    rig.run(150);
    assert_eq!(gap(&rig).map(|g| g.0), Some(0), "repeating: the first item is next");
}

#[test]
fn stopping_a_playlist_also_ends_its_gap() {
    let list = playlist("1", &["len100.wav", "len100.wav"], 0.2, false);
    let id = list.id;
    let mut rig = Rig::new(vec![list.clone()]);
    rig.act(Action::Go);
    rig.run(150);
    rig.act(Action::StopCue(id));
    rig.run(500);
    assert!(rig.active().is_empty());

    let mut rig = Rig::new(vec![list, stop_cue("2", Targets::All(AllKeyword::All))]);
    rig.act(Action::Go);
    rig.run(150);
    rig.act(Action::Go);
    rig.run(500);
    assert!(rig.active().is_empty(), "a stop cue ends the gap too");
}

fn has_next(rig: &Rig) -> bool {
    rig.runner.state(rig.now()).active.first().is_some_and(|a| a.has_next)
}

#[test]
fn next_item_fades_out_then_waits_the_gap() {
    let mut list = playlist("1", &["len2000.wav", "len2000.wav"], 0.1, false);
    if let CueKind::Playlist(p) = &mut list.kind {
        p.fade_out = 0.1;
    }
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(100);
    assert!(has_next(&rig));
    rig.act(Action::NextItem(id));
    rig.run(50);
    assert_eq!(item(&rig), Some((0, 2)), "fading out");
    assert!(!has_next(&rig), "already on its way");
    rig.run(100);
    assert!(rig.active().is_empty(), "in the gap");
    assert_eq!(rig.runner.state(rig.now()).gaps.len(), 1);
    rig.run(100);
    assert_eq!(item(&rig), Some((1, 2)));
    assert!(level_db(&rig) > -1.0, "the next item plays at its own level");
    assert!(!has_next(&rig), "the last item");
    rig.act(Action::NextItem(id));
    rig.run(100);
    assert_eq!(item(&rig), Some((1, 2)), "nothing after the last item");
}

#[test]
fn second_trigger_next_item_skips_to_the_next_item() {
    let list = with_second(
        playlist("1", &["len2000.wav", "len2000.wav", "len2000.wav"], 0.5, false),
        SecondTrigger::NextItem,
    );
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::StartCue(id));
    rig.run(100);
    rig.act(Action::StartCue(id));
    rig.run(50);
    assert!(rig.active().is_empty(), "in the gap");
    rig.act(Action::StartCue(id));
    rig.run(30);
    assert_eq!(item(&rig), Some((1, 3)), "a trigger in the gap starts the next item at once");
}

#[test]
fn next_item_without_fade_out_or_gap_goes_on_at_once() {
    let list = playlist("1", &["len2000.wav", "len2000.wav"], 0.0, true);
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::NextItem(id));
    rig.run(30);
    assert_eq!(item(&rig), Some((1, 2)));
    assert!(has_next(&rig), "repeating: the first item follows the last");
    rig.act(Action::NextItem(id));
    rig.run(30);
    assert_eq!(item(&rig), Some((0, 2)), "starts over");
}

#[test]
fn next_item_in_the_gap_starts_it_at_once() {
    let list = playlist("1", &["len100.wav", "len2000.wav", "len2000.wav"], 1.0, false);
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(200);
    assert!(rig.active().is_empty(), "in the gap");
    rig.act(Action::NextItem(id));
    rig.run(20);
    assert_eq!(item(&rig), Some((1, 3)));
    assert!(rig.runner.state(rig.now()).gaps.is_empty());

    // Paused in the gap: the item starts paused.
    rig.act(Action::NextItem(id));
    rig.run(500);
    assert!(rig.active().is_empty(), "in the second gap");
    rig.act(Action::PauseCue(id));
    rig.act(Action::NextItem(id));
    rig.run(200);
    let a = rig.runner.state(rig.now()).active[0].clone();
    assert_eq!(a.item.map(|i| i.index), Some(2));
    assert!(a.paused && a.elapsed == 0.0, "{a:?}");
}

#[test]
fn next_item_while_paused_goes_on_at_once_and_stays_paused_at_its_start() {
    let mut list = playlist("1", &["len2000.wav", "len2000.wav"], 0.5, false);
    if let CueKind::Playlist(p) = &mut list.kind {
        p.fade_out = 0.3;
    }
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(300);
    rig.act(Action::PauseCue(id));
    rig.run(50);
    assert!(has_next(&rig), "a paused item can go on too");
    rig.act(Action::NextItem(id));
    rig.run(200);
    let a = rig.runner.state(rig.now()).active[0].clone();
    assert_eq!(a.item.map(|i| i.index), Some(1), "no fade, no gap");
    assert!(a.paused && a.elapsed == 0.0 && a.progress == 0.0, "{a:?}");
    assert_eq!(rig.active(), ["1"], "the paused item is gone");
    rig.act(Action::ResumeCue(id));
    rig.run(200);
    let a = rig.runner.state(rig.now()).active[0].clone();
    assert!(!a.paused && (a.elapsed - 0.2).abs() < 0.03, "plays from its start: {a:?}");
}

#[test]
fn next_item_while_paused_during_its_fade_skips_the_rest() {
    let mut list = playlist("1", &["len2000.wav", "len2000.wav"], 0.5, false);
    if let CueKind::Playlist(p) = &mut list.kind {
        p.fade_out = 0.5;
    }
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::NextItem(id));
    rig.run(100);
    rig.act(Action::PauseCue(id));
    assert!(has_next(&rig));
    rig.act(Action::NextItem(id));
    rig.run(50);
    let a = rig.runner.state(rig.now()).active[0].clone();
    assert_eq!((a.item.map(|i| i.index), a.paused), (Some(1), true));
}

#[test]
fn fading_a_playlist_on_its_way_to_the_next_item_ends_it() {
    let mut list = playlist("1", &["len2000.wav", "len2000.wav"], 0.0, false);
    if let CueKind::Playlist(p) = &mut list.kind {
        p.fade_out = 0.2;
    }
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::NextItem(id));
    rig.run(50);
    rig.runner.take_notices();
    rig.act(Action::FadeCue(id));
    let notices = rig.runner.take_notices();
    assert!(notices.is_empty(), "no warning, the fade ends it: {notices:?}");
    rig.run(400);
    assert!(rig.active().is_empty());
    assert!(rig.runner.state(rig.now()).gaps.is_empty());
}

#[test]
fn next_item_keeps_a_faded_level() {
    let list = playlist("1", &["len200.wav", "len2000.wav"], 0.0, false);
    let mut down = fade("2", Targets::Cues(vec![list.id]), VolumeAction::FadeTo, Some(0.05));
    if let CueKind::Volume(f) = &mut down.kind {
        f.volume_db = Some(-20.0);
    }
    let mut rig = Rig::new(vec![list, down]);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::Go);
    rig.run(300);
    assert_eq!(item(&rig), Some((1, 2)));
    rig.run(100);
    assert!((level_db(&rig) + 20.0).abs() < 0.5, "{}", level_db(&rig));
}

#[test]
fn playlist_items_play_with_their_own_normalization() {
    let mut list = playlist("1", &["len200.wav", "len2000.wav"], 0.0, false);
    if let CueKind::Playlist(p) = &mut list.kind {
        (p.items[0].normalize_db, p.items[1].normalize_db) = (Some(-3.0), Some(-9.0));
    }
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(50);
    assert!((level_db(&rig) + 3.0).abs() < 0.01, "{}", level_db(&rig));
    rig.run(250);
    assert_eq!(item(&rig), Some((1, 2)));
    assert!((level_db(&rig) + 9.0).abs() < 0.01, "{}", level_db(&rig));
}

#[test]
fn normalizing_a_running_playlist_changes_the_item_playing() {
    let list = playlist("1", &["len200.wav", "len2000.wav"], 0.0, false);
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(50);
    let mut show = rig.runner.show().clone();
    if let Some(CueKind::Playlist(p)) = show.cue_mut(id).map(|c| &mut c.kind) {
        // Only the offset of the item playing counts.
        (p.items[0].normalize_db, p.items[1].normalize_db) = (Some(-3.0), Some(-9.0));
    }
    rig.runner.set_show(show);
    rig.run(50);
    assert!((level_db(&rig) + 3.0).abs() < 0.01, "{}", level_db(&rig));
    rig.run(200);
    assert_eq!(item(&rig), Some((1, 2)));
    assert!((level_db(&rig) + 9.0).abs() < 0.01, "{}", level_db(&rig));
}

#[test]
fn broken_playlist_item_is_skipped() {
    let mut rig = Rig::new(vec![playlist("1", &["missing.wav", "len300.wav"], 0.0, false)]);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(item(&rig), Some((1, 2)));
    let notices = rig.runner.take_notices();
    assert!(notices.iter().any(|n| matches!(n, Notice::Error(m) if m.contains("item 1"))), "{notices:?}");
}

fn shuffled(shuffle: Shuffle, repeat: bool) -> Cue {
    let mut list = playlist(
        "1",
        &["len50.wav", "len51.wav", "len52.wav", "len53.wav", "len54.wav", "len55.wav"],
        0.0,
        repeat,
    );
    if let CueKind::Playlist(p) = &mut list.kind {
        p.shuffle = shuffle;
    }
    list
}

/// The files the playlist plays (list index), one entry per item, for `ms`.
fn played_files(rig: &mut Rig, ms: u64) -> Vec<usize> {
    let mut files = Vec::new();
    let mut last = None;
    for _ in 0..ms / 10 {
        rig.run(10);
        let now = rig.runner.state(rig.now()).active.first().and_then(|a| a.item).map(|i| (i.index, i.file));
        if now.is_some() && now != last {
            files.extend(now.map(|(_, file)| file));
        }
        last = now;
    }
    files
}

#[test]
fn shuffled_playlist_keeps_its_order_for_all_passes_until_started_again() {
    let list = shuffled(Shuffle::PerStart, true);
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.runner.seed_shuffle(1);
    rig.act(Action::Go);
    let files = played_files(&mut rig, 3 * 6 * 80);
    assert!(files.len() >= 18, "{files:?}");
    let pass = files[..6].to_vec();
    assert_eq!(sorted(pass.clone()), [0, 1, 2, 3, 4, 5], "every item once a pass");
    assert_ne!(pass, [0, 1, 2, 3, 4, 5], "shuffled");
    assert_eq!(files[6..12], pass);
    assert_eq!(files[12..18], pass);

    rig.act(Action::StopCue(id));
    rig.run(50);
    rig.act(Action::StartCue(id));
    let again = played_files(&mut rig, 6 * 80);
    assert_eq!(sorted(again[..6].to_vec()), [0, 1, 2, 3, 4, 5]);
    assert_ne!(again[..6], pass, "a new start draws a new order");
}

#[test]
fn shuffled_playlist_reshuffles_every_pass() {
    let mut rig = Rig::new(vec![shuffled(Shuffle::PerPass, true)]);
    rig.runner.seed_shuffle(2);
    rig.act(Action::Go);
    let files = played_files(&mut rig, 4 * 6 * 80);
    assert!(files.len() >= 24, "{files:?}");
    let passes: Vec<Vec<usize>> = files[..24].chunks(6).map(<[usize]>::to_vec).collect();
    for pass in &passes {
        assert_eq!(sorted(pass.clone()), [0, 1, 2, 3, 4, 5]);
    }
    assert!(passes.windows(2).any(|pair| pair[0] != pair[1]), "{passes:?}");
    assert!(files.windows(2).all(|pair| pair[0] != pair[1]), "no item twice in a row: {files:?}");
}

#[test]
fn shuffled_playlist_without_repeat_ends_after_one_pass() {
    let mut rig = Rig::new(vec![
        with_advance(shuffled(Shuffle::PerPass, false), AdvanceAt::End),
        sound("2", "len500.wav"),
    ]);
    rig.runner.seed_shuffle(3);
    rig.act(Action::Go);
    let files = played_files(&mut rig, 6 * 80);
    assert_eq!(sorted(files), [0, 1, 2, 3, 4, 5]);
    rig.run(50);
    assert_eq!(rig.active(), ["2"], "auto-follow after the last item");
}

#[test]
fn shuffled_playlist_arms_the_item_it_starts_with() {
    let mut list = playlist("1", &["len1000.wav", "len1001.wav", "len1002.wav", "len1003.wav"], 0.0, false);
    if let CueKind::Playlist(p) = &mut list.kind {
        p.shuffle = Shuffle::PerStart;
    }
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.runner.seed_shuffle(4);
    rig.wait_armed(id);
    let loaded = rig.loader.loads.lock().unwrap().clone();
    rig.runner.take_notices();
    rig.act(Action::Go);
    rig.run(20);
    let item = rig.runner.state(rig.now()).active.first().and_then(|a| a.item).expect("plays");
    assert_eq!(item.index, 0);
    let file = format!("len{}.wav", 1000 + item.file);
    assert!(loaded.iter().any(|p| p.ends_with(&file)), "{file} armed: {loaded:?}");
    let notices = rig.runner.take_notices();
    assert!(
        notices.iter().all(|n| !matches!(n, Notice::Warning(m) if m.contains("preloaded"))),
        "{notices:?}"
    );
}

#[test]
fn streamed_cue_is_armed_only_where_loaded_and_reports_underruns() {
    let mut early = sound("1", "stream.wav");
    let mut late = sound("2", "stream.wav");
    if let (CueKind::Sound(a), CueKind::Sound(b)) = (&mut early.kind, &mut late.kind) {
        a.start = 1.0;
        b.start = 70.0; // chunk 1 at 1000 Hz
    }
    let (early_id, late_id) = (early.id, late.id);
    let mut rig = Rig::new(vec![early, late]);
    rig.wait_armed(early_id);
    assert!(!rig.runner.is_armed(late_id), "start chunk not loaded");
    rig.act(Action::StartCue(late_id));
    // The chunk never comes (no reader thread): after the start wait, silence
    // for as long as it plays.
    let silences = |rig: &mut Rig| {
        let notices = rig.runner.take_notices();
        notices.iter().filter(|n| matches!(n, Notice::Warning(m) if m.contains("silence"))).count()
    };
    rig.run(1000);
    assert!(active_state(&rig, "2").unwrap().loading);
    rig.run(1500);
    assert!(!active_state(&rig, "2").unwrap().loading, "plays after waiting 2 s");
    assert_eq!(silences(&mut rig), 0, "not one warning per tick while the silence lasts");
    rig.run(5000);
    assert_eq!(silences(&mut rig), 1, "a long stretch is reported anyway");
    rig.act(Action::StopCue(late_id));
    rig.run(1000);
    assert_eq!(silences(&mut rig), 1, "the rest once it is over");
    rig.run(1000);
    assert_eq!(silences(&mut rig), 0);
}

fn group_cues(group: &Cue) -> &[Cue] {
    match &group.kind {
        CueKind::Group(g) => &g.cues,
        _ => &[],
    }
}

fn group_of(number: &str, mode: GroupMode, cues: Vec<Cue>) -> Cue {
    let mut c = Cue::new(CueKind::Group(GroupCue { mode, cues }));
    c.number = number.into();
    c
}

#[test]
fn group_starts_all_its_cues_on_one_go() {
    let mut rig = Rig::new(vec![
        group_of(
            "1",
            GroupMode::All,
            vec![
                // Continue settings do not apply inside an all-at-once group.
                with_advance(sound("1a", "len300.wav"), AdvanceAt::Start),
                sound("1b", "len300.wav"),
            ],
        ),
        sound("2", "len300.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(20);
    let mut active = rig.active();
    active.sort();
    assert_eq!(active, ["1a", "1b"]);
    assert_eq!(rig.playhead().as_deref(), Some("2"));
    assert!(
        rig.runner.take_notices().iter().all(|n| !matches!(n, Notice::Warning(m) if m.contains("running")))
    );
}

#[test]
fn sequence_group_chains_its_cues_and_follows_when_done() {
    let mut rig = Rig::new(vec![
        with_advance(
            group_of(
                "1",
                GroupMode::Sequence,
                vec![with_advance(sound("1a", "len200.wav"), AdvanceAt::End), sound("1b", "len200.wav")],
            ),
            AdvanceAt::End,
        ),
        sound("2", "len300.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(100);
    assert_eq!(rig.active(), ["1a"]);
    rig.run(150);
    assert_eq!(rig.active(), ["1b"], "inside the group, auto-follow chains");
    rig.run(200);
    assert_eq!(rig.active(), ["2"], "the group's own auto-follow after its last cue");
}

#[test]
fn fade_stop_on_a_group_ends_its_cues_not_started_yet() {
    let mut waiting = sound("2b", "len2000.wav");
    waiting.pre_wait = 0.5;
    let sequence = group_of(
        "1",
        GroupMode::Sequence,
        vec![with_advance(sound("1a", "len300.wav"), AdvanceAt::End), sound("1b", "len2000.wav")],
    );
    let all = group_of("2", GroupMode::All, vec![sound("2a", "len2000.wav"), waiting]);
    let targets = Targets::Cues(vec![sequence.id, all.id]);
    let mut rig = Rig::new(vec![
        with_advance(sequence, AdvanceAt::Start),
        all,
        fade("3", targets, VolumeAction::FadeStop, Some(0.1)),
    ]);
    rig.act(Action::Go);
    rig.run(100);
    let mut active = rig.active();
    active.sort();
    assert_eq!(active, ["1a", "2a"]);
    rig.act(Action::Go);
    rig.run(600);
    assert!(rig.active().is_empty(), "1b and 2b must not start: {:?}", rig.active());
}

#[test]
fn stopping_or_fading_a_group_reaches_its_cues() {
    let group = group_of("1", GroupMode::All, vec![sound("1a", "len2000.wav"), sound("1b", "len2000.wav")]);
    let id = group.id;
    let mut rig = Rig::new(vec![group.clone()]);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::StartCue(id));
    // Swallowed silently (second trigger "ignore").
    assert!(
        !rig.runner
            .take_notices()
            .iter()
            .any(|n| matches!(n, Notice::Warning(m) if m.contains("is already running")))
    );
    rig.act(Action::StopCue(id));
    rig.run(20);
    assert!(rig.active().is_empty());

    let mut rig =
        Rig::new(vec![group, fade("2", Targets::Cues(vec![id]), VolumeAction::FadeStop, Some(0.05))]);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::Go);
    rig.run(100);
    assert!(rig.active().is_empty(), "a fade cue aimed at the group fades its cues");
}

#[test]
fn needed_channels_cover_outputs_and_main_preview() {
    let mut settings = LocalSettings::default();
    assert_eq!(settings.needed_channels(), 2);
    settings.outputs.insert("main".into(), 3);
    assert_eq!(settings.needed_channels(), 4);
    settings.preview_channel = Some(7);
    assert_eq!(settings.needed_channels(), 8, "a preview pair on the main device counts");
    settings.preview_device = Some("other".into());
    assert_eq!(settings.needed_channels(), 4, "a preview on its own device does not");
}

#[test]
fn changed_file_is_read_again_and_running_cue_reported() {
    let mut rig = Rig::new(vec![sound("1", "len500.wav"), sound("2", "len500.wav")]);
    let (first, second) = (rig.runner.show().cues[0].id, rig.runner.show().cues[1].id);
    rig.wait_armed(first);
    rig.act(Action::Go);
    rig.run(50);
    let path = PathBuf::from("media").join("len500.wav");
    let loads = |rig: &Rig| rig.loader.loads.lock().unwrap().iter().filter(|p| **p == path).count();
    assert_eq!(loads(&rig), 1);

    // Unchanged: the fake files do not exist, so the watcher sees `None`.
    let missing: Stamps = Arc::new([(path.clone(), None)].into());
    assert!(rig.runner.media_changed(missing.clone()).is_empty());

    let stamp = jq_show::paths::FileStamp { len: 1, modified: None, id: (0, 0, 0, 0) };
    let changed: Stamps = Arc::new([(path.clone(), Some(stamp))].into());
    assert_eq!(rig.runner.media_changed(changed), [first]);
    assert!(!rig.runner.is_armed(second), "the old buffer is dropped");
    // The reload reads a stamp (`None`) other than the watcher's: the watcher
    // is asked to look again, and the file is not read over and over
    // meanwhile; the running cue goes on.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut suspects = Vec::new();
    while suspects.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        rig.run(10);
        suspects = rig.runner.take_suspect_files();
    }
    assert_eq!(suspects, std::slice::from_ref(&path));
    std::thread::sleep(Duration::from_millis(20));
    rig.run(50);
    assert!(rig.runner.take_suspect_files().is_empty(), "asked once");
    assert!(!rig.runner.is_armed(second));
    assert_eq!(rig.active(), ["1"]);
    assert_eq!(loads(&rig), 2);

    // Once the watcher agrees with what was read, the cue is armed again.
    rig.runner.media_changed(missing);
    rig.wait_armed(second);
    assert_eq!(loads(&rig), 3);
}

fn not_armed_warnings(rig: &mut Rig) -> Vec<Notice> {
    rig.runner.take_notices().into_iter().filter(|n| format!("{n:?}").contains("not preloaded")).collect()
}

#[test]
fn whole_auto_follow_chain_is_armed() {
    let cues: Vec<Cue> = (1..=4)
        .map(|i| with_advance(sound(&i.to_string(), &format!("len{}.wav", 200 + i)), AdvanceAt::End))
        .collect();
    let fourth = cues[3].id;
    let mut rig = Rig::new(cues);
    rig.wait_armed(rig.runner.show().cues[2].id);
    rig.act(Action::Go);
    // Past the playhead, the chain still loads what comes in turn.
    rig.wait_armed(fourth);
    rig.run(1000);
    assert_eq!(rig.active(), Vec::<String>::new());
    assert_eq!(not_armed_warnings(&mut rig), []);
    let loads = rig.loader.loads.lock().unwrap();
    assert_eq!(loads.len(), 4, "every file read once: {loads:?}");
}

#[test]
fn direct_start_of_an_unarmed_cue_does_not_warn() {
    let mut cues: Vec<Cue> = (1..=4).map(|i| sound(&i.to_string(), &format!("len{}.wav", 200 + i))).collect();
    let mut hotkey = sound("5", "len600.wav");
    hotkey.hotkey = Some(Chord::new("KeyH", "h"));
    let id = hotkey.id;
    cues.push(hotkey);
    let mut rig = Rig::new(cues);
    rig.wait_armed(rig.runner.show().cues[2].id);
    assert!(!rig.runner.is_armed(id), "beyond what is armed");
    rig.act(Action::StartCue(id));
    rig.run(100);
    assert_eq!(rig.active(), ["5"]);
    assert_eq!(rig.runner.take_notices(), []);
}

#[test]
fn streamed_cue_waits_for_its_start_instead_of_playing_silence() {
    let mut cue = sound("1", "stream.wav");
    if let CueKind::Sound(s) = &mut cue.kind {
        s.start = 70.0; // chunk 1 at 1000 Hz, not read yet
    }
    let mut rig = Rig::new(vec![cue]);
    rig.act(Action::Go);
    rig.run(500);
    let state = active_state(&rig, "1").expect("running");
    assert!(state.loading);
    assert_eq!(state.elapsed, 0.0, "the clock waits too");
    // The file may have been read twice (armed in the background meanwhile).
    let streams = rig.loader.streams.lock().unwrap().clone();
    for data in &streams {
        data.store(1, vec![0.5; jq_engine::stream::CHUNK_FRAMES * 2]);
    }
    rig.run(300);
    let state = active_state(&rig, "1").expect("running");
    assert!(!state.loading && state.elapsed > 0.2, "{state:?}");
    assert!(streams.iter().all(|d| d.underruns() == 0), "no silence played");
    assert!(!rig.runner.take_notices().iter().any(|n| format!("{n:?}").contains("Stille")));
}

#[test]
fn cue_after_a_following_group_is_armed() {
    let mut rig = Rig::new(vec![
        with_advance(
            group_of(
                "1",
                GroupMode::Sequence,
                vec![with_advance(sound("1a", "len200.wav"), AdvanceAt::End), sound("1b", "len201.wav")],
            ),
            AdvanceAt::End,
        ),
        sound("2", "len300.wav"),
    ]);
    let (group, second) = (rig.runner.show().cues[0].id, rig.runner.show().cues[1].id);
    rig.wait_armed(group);
    rig.act(Action::Go);
    rig.wait_armed(second);
    rig.run(500);
    assert_eq!(rig.active(), ["2"]);
    assert_eq!(not_armed_warnings(&mut rig), []);
}

#[test]
fn absurd_times_neither_panic_nor_hang() {
    let mut early = sound("1", "len100.wav");
    early.pre_wait = -5.0;
    let mut late = with_advance(wait("2", 1e300), AdvanceAt::Start);
    late.pre_wait = f64::MAX;
    let mut rig = Rig::new(vec![
        with_advance(early, AdvanceAt::Start),
        late,
        fade("3", Targets::All(AllKeyword::All), VolumeAction::FadeTo, Some(1e300)),
        wait("4", 1e300),
    ]);
    rig.runner.set_settings(LocalSettings { fade_all: 1e300, ..rig.runner.settings().clone() });
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["1"], "a negative pre-wait is none");
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.act(Action::FadeAll);
    rig.run(20);
    let _ = rig.runner.state(rig.now());
}

#[test]
fn file_being_copied_is_neither_loaded_nor_played_until_done() {
    let mut rig = Rig::new(vec![sound("1", "len100.wav")]);
    let id = rig.runner.show().cues[0].id;
    let path = PathBuf::from("media").join("len100.wav");
    rig.runner.set_importing([path].into());
    rig.run(50);
    rig.act(Action::Go);
    let notices = rig.runner.take_notices();
    assert!(matches!(&notices[..], [Notice::Error(m)] if m.contains("copied")), "{notices:?}");

    rig.runner.set_importing(Default::default());
    rig.act(Action::SetPlayhead(id));
    rig.wait_armed(id);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["1"]);
}

fn active_state(rig: &Rig, number: &str) -> Option<ActiveState> {
    let show = rig.runner.show();
    rig.runner.state(rig.now()).active.into_iter().find(|a| show.cue(a.cue).unwrap().number == number)
}

fn elapsed(rig: &Rig, number: &str) -> f64 {
    active_state(rig, number).expect("cue not active").elapsed
}

fn id_of(rig: &Rig, number: &str) -> CueId {
    rig.runner.show().all().into_iter().find(|c| c.number == number).unwrap().id
}

fn sorted<T: Ord>(mut items: Vec<T>) -> Vec<T> {
    items.sort();
    items
}

#[test]
fn pause_holds_the_cue_and_resume_plays_on() {
    let mut rig = Rig::new(vec![sound("1", "len1000.wav")]);
    let id = id_of(&rig, "1");
    rig.act(Action::Go);
    rig.run(200);
    let level = level_of(&rig, "1");
    rig.act(Action::PauseCue(id));
    let (at, remaining) = (elapsed(&rig, "1"), active_state(&rig, "1").unwrap().remaining.unwrap());
    rig.run(2000);
    let paused = active_state(&rig, "1").expect("a paused cue stays in the list");
    assert!(paused.paused && rig.runner.state(rig.now()).any_paused);
    assert_eq!((paused.elapsed, paused.remaining), (at, Some(remaining)), "time stands still");
    assert_eq!(paused.level_db, level, "keeps its level");
    rig.act(Action::ResumeCue(id));
    rig.run(100);
    assert!((elapsed(&rig, "1") - at - 0.1).abs() < 0.015);
    assert!(!rig.runner.state(rig.now()).any_paused);
    rig.run(650);
    assert_eq!(rig.active(), ["1"], "plays the rest after the pause");
    rig.run(150);
    assert!(rig.active().is_empty());
}

#[test]
fn pause_holds_a_pre_wait() {
    let mut c = sound("1", "len500.wav");
    c.pre_wait = 0.2;
    let mut rig = Rig::new(vec![c]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::PauseAll);
    rig.run(1000);
    let waiting = &rig.runner.state(rig.now()).waiting[0];
    assert!(waiting.paused && (waiting.remaining - 0.1).abs() < 0.015, "{waiting:?}");
    assert!(rig.active().is_empty());
    rig.act(Action::ResumeAll);
    rig.run(60);
    assert!(rig.active().is_empty());
    rig.run(60);
    assert_eq!(rig.active(), ["1"]);
}

#[test]
fn pause_holds_a_wait_in_a_sequence_and_a_playlist_gap() {
    let mut rig = Rig::new(vec![
        group_of(
            "G",
            GroupMode::Sequence,
            vec![with_advance(wait("w", 0.2), AdvanceAt::End), sound("a", "len500.wav")],
        ),
        playlist("P", &["len100.wav", "len100.wav"], 0.1, false),
    ]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(150);
    assert_eq!(rig.active(), [] as [&str; 0], "wait running, playlist in its gap");
    rig.act(Action::PauseAll);
    assert!(rig.runner.state(rig.now()).fading[0].paused);
    rig.run(1000);
    assert!(rig.active().is_empty());
    rig.act(Action::ResumeAll);
    rig.run(30);
    assert!(rig.active().is_empty());
    rig.run(100);
    assert_eq!(sorted(rig.active()), ["P", "a"]);
}

#[test]
fn auto_follow_waits_for_the_resume() {
    let mut rig =
        Rig::new(vec![with_advance(sound("1", "len300.wav"), AdvanceAt::End), sound("2", "len300.wav")]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::TogglePauseAll);
    rig.run(1000);
    assert_eq!(rig.active(), ["1"]);
    rig.act(Action::TogglePauseAll);
    rig.run(250);
    assert_eq!(rig.active(), ["2"]);
}

#[test]
fn toggling_a_group_pauses_all_of_it_while_anything_plays() {
    let mut rig = Rig::new(vec![group_of(
        "G",
        GroupMode::All,
        vec![sound("a", "len1000.wav"), sound("b", "len1000.wav")],
    )]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::PauseCue(id_of(&rig, "a")));
    rig.act(Action::TogglePauseCue(id_of(&rig, "G")));
    let paused = |rig: &Rig| rig.runner.state(rig.now()).active.iter().map(|a| a.paused).collect::<Vec<_>>();
    assert_eq!(paused(&rig), [true, true]);
    rig.act(Action::TogglePauseCue(id_of(&rig, "G")));
    assert_eq!(paused(&rig), [false, false]);
}

#[test]
fn stop_fade_and_panic_end_paused_cues_at_once() {
    for action in [Action::StopAll, Action::FadeAll, Action::Panic] {
        let mut c = sound("2", "len1000.wav");
        c.pre_wait = 0.5;
        let mut rig = Rig::new(vec![sound("1", "len1000.wav"), c]);
        rig.act(Action::Go);
        rig.act(Action::Go);
        rig.run(50);
        rig.act(Action::PauseAll);
        rig.run(50);
        rig.act(action);
        rig.run(20);
        let state = rig.runner.state(rig.now());
        assert!(state.active.is_empty() && state.waiting.is_empty(), "{action:?}");
    }
}

#[test]
fn fade_cue_on_a_paused_cue_applies_at_once() {
    let target = sound("1", "len5000.wav");
    let mut to = fade("2", Targets::Cues(vec![target.id]), VolumeAction::FadeTo, Some(2.0));
    if let CueKind::Volume(f) = &mut to.kind {
        f.volume_db = Some(-6.0);
    }
    let stop = fade("3", Targets::Cues(vec![target.id]), VolumeAction::FadeStop, Some(2.0));
    let mut rig = Rig::new(vec![target, to, stop]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::PauseAll);
    rig.act(Action::Go);
    rig.act(Action::ResumeCue(id_of(&rig, "1")));
    rig.run(50);
    assert!((level_db(&rig) + 6.0).abs() < 0.1, "level set at once: {}", level_db(&rig));
    rig.act(Action::PauseCue(id_of(&rig, "1")));
    rig.act(Action::Go);
    rig.run(20);
    assert!(rig.active().is_empty(), "fade & stop stops at once");
}

fn with_second(mut c: Cue, mode: SecondTrigger) -> Cue {
    c.second_trigger = mode;
    c
}

#[test]
fn second_trigger_modes() {
    let start = |rig: &mut Rig| {
        let id = id_of(rig, "1");
        rig.act(Action::StartCue(id));
    };
    // Ignore: nothing happens, no warning.
    let mut rig = Rig::new(vec![sound("1", "len1000.wav")]);
    start(&mut rig);
    rig.run(100);
    start(&mut rig);
    rig.run(100);
    assert!((elapsed(&rig, "1") - 0.2).abs() < 0.015);
    assert!(
        rig.runner.take_notices().iter().all(|n| !matches!(n, Notice::Warning(w) if w.contains("running")))
    );

    let mut rig = Rig::new(vec![with_second(sound("1", "len1000.wav"), SecondTrigger::Pause)]);
    start(&mut rig);
    rig.run(100);
    start(&mut rig);
    assert!(active_state(&rig, "1").unwrap().paused);
    start(&mut rig);
    assert!(!active_state(&rig, "1").unwrap().paused);

    let mut rig = Rig::new(vec![with_second(sound("1", "len1000.wav"), SecondTrigger::Stop)]);
    start(&mut rig);
    rig.run(100);
    start(&mut rig);
    rig.run(20);
    assert!(rig.active().is_empty());

    let mut c = with_second(sound("1", "len1000.wav"), SecondTrigger::FadeOut);
    if let CueKind::Sound(s) = &mut c.kind {
        s.fade_out = 0.2;
    }
    let mut rig = Rig::new(vec![c]);
    start(&mut rig);
    rig.run(100);
    start(&mut rig);
    rig.run(100);
    assert_eq!(rig.active(), ["1"], "still fading");
    // A third trigger breaks the fade off and starts the cue from the top.
    rig.runner.take_notices();
    start(&mut rig);
    rig.run(20);
    assert!(elapsed(&rig, "1") < 0.05, "from the top");
    rig.run(200);
    assert_eq!(rig.active(), ["1"], "the fade is gone");
    assert!((level_db(&rig) - 0.0).abs() < 0.1, "at full level: {}", level_db(&rig));
    assert_eq!(rig.runner.take_notices(), []);

    let mut rig = Rig::new(vec![with_second(
        vamped("1", "len2000.wav", &[(0.1, 0.3)], &[]),
        SecondTrigger::ExitLoop { exit: ExitLoop::default() },
    )]);
    start(&mut rig);
    rig.run(200);
    assert_eq!(remaining(&rig), None);
    start(&mut rig);
    rig.run(20);
    assert!(remaining(&rig).is_some(), "left the vamp");
}

#[test]
fn release_trigger_fires_the_second_trigger_on_release() {
    let held = |mode| {
        let mut c = with_second(sound("1", "len2000.wav"), mode);
        c.release_trigger = true;
        if let CueKind::Sound(s) = &mut c.kind {
            s.fade_out = 0.2;
        }
        let id = c.id;
        (Rig::new(vec![c]), id)
    };
    // Stop: plays while the key is held.
    let (mut rig, id) = held(SecondTrigger::Stop);
    rig.act(Action::StartCue(id));
    rig.run(100);
    assert_eq!(rig.active(), ["1"]);
    rig.act(Action::ReleaseCue(id));
    rig.run(20);
    assert!(rig.active().is_empty());
    // Releasing a cue that does not run does nothing.
    rig.act(Action::ReleaseCue(id));
    rig.run(20);
    assert!(rig.active().is_empty());

    // Fade out: ends after its fade-out time; a press meanwhile breaks the
    // fade off and starts again.
    let (mut rig, id) = held(SecondTrigger::FadeOut);
    rig.act(Action::StartCue(id));
    rig.run(100);
    rig.act(Action::ReleaseCue(id));
    rig.run(300);
    assert!(rig.active().is_empty(), "faded out");
    rig.act(Action::StartCue(id));
    rig.run(100);
    rig.act(Action::ReleaseCue(id));
    rig.run(100);
    assert_eq!(rig.active(), ["1"], "still fading");
    rig.act(Action::StartCue(id));
    rig.run(300);
    assert_eq!(rig.active(), ["1"], "started again, no fade");
    assert!(elapsed(&rig, "1") < 0.35);

    // Pause: release pauses, the next press (its second trigger) resumes.
    let (mut rig, id) = held(SecondTrigger::Pause);
    rig.act(Action::StartCue(id));
    rig.run(100);
    rig.act(Action::ReleaseCue(id));
    assert!(active_state(&rig, "1").unwrap().paused);
    rig.act(Action::StartCue(id));
    assert!(!active_state(&rig, "1").unwrap().paused);

    // Released during its pre-wait: it never starts.
    let mut c = with_second(sound("1", "len2000.wav"), SecondTrigger::Stop);
    (c.release_trigger, c.pre_wait) = (true, 0.2);
    let id = c.id;
    let mut rig = Rig::new(vec![c]);
    rig.act(Action::StartCue(id));
    rig.run(50);
    rig.act(Action::ReleaseCue(id));
    rig.run(300);
    assert!(rig.active().is_empty(), "the pre-wait was broken off");

    // Without the setting a release does nothing.
    let c = with_second(sound("1", "len2000.wav"), SecondTrigger::Stop);
    let id = c.id;
    let mut rig = Rig::new(vec![c]);
    rig.act(Action::StartCue(id));
    rig.run(100);
    rig.act(Action::ReleaseCue(id));
    rig.run(20);
    assert_eq!(rig.active(), ["1"]);
}

#[test]
fn a_go_on_a_cue_fading_out_starts_it_again() {
    // GO on the playhead after FADE ALL.
    let mut rig = Rig::new(vec![sound("1", "len5000.wav"), sound("2", "len5000.wav")]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::FadeAll);
    rig.run(100);
    rig.act(Action::SetPlayhead(id_of(&rig, "1")));
    rig.act(Action::Go);
    rig.run(50);
    assert_eq!(rig.active(), ["1"]);
    assert!(elapsed(&rig, "1") < 0.1, "from the top");
    rig.run(1000);
    assert!(level_db(&rig).abs() < 0.1, "the FADE ALL no longer applies: {}", level_db(&rig));

    // The Fade button on a fading cue is no GO: it still only warns.
    let mut c = sound("1", "len5000.wav");
    if let CueKind::Sound(s) = &mut c.kind {
        s.fade_out = 0.5;
    }
    let id = c.id;
    let mut rig = Rig::new(vec![c]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::FadeCue(id));
    rig.run(100);
    rig.act(Action::FadeCue(id));
    assert!(rig.runner.take_notices().contains(&Notice::Warning("Cue 1 is already fading out".into())));
    rig.run(500);
    assert!(rig.active().is_empty());

    // A repeat cue fading out: the GO starts it anew, as a pass.
    let mut c = with_second(sound("1", "len5000.wav"), SecondTrigger::FadeOut);
    c.passes = Repeat::Times(3);
    if let CueKind::Sound(s) = &mut c.kind {
        s.fade_out = 0.5;
    }
    let mut rig = Rig::new(vec![c, sound("2", "len200.wav")]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::Go); // second trigger: fade out
    rig.run(100);
    rig.act(Action::Go); // fading: starts again (pass 2)
    rig.run(50);
    assert_eq!(rig.active(), ["1"]);
    assert!(elapsed(&rig, "1") < 0.1);
    assert_eq!(rig.playhead().as_deref(), Some("1"), "pass 2 of 3");
}

#[test]
fn second_trigger_restart_starts_over_without_the_old_follow() {
    let mut rig = Rig::new(vec![
        with_advance(with_second(sound("1", "len300.wav"), SecondTrigger::Restart), AdvanceAt::End),
        sound("2", "len300.wav"),
    ]);
    let id = id_of(&rig, "1");
    rig.act(Action::StartCue(id));
    rig.run(200);
    rig.act(Action::StartCue(id));
    rig.run(50);
    assert!(elapsed(&rig, "1") < 0.07, "started again");
    rig.run(150);
    assert_eq!(rig.active(), ["1"], "the old run's follow is gone");
    rig.run(150);
    assert_eq!(rig.active(), ["2"], "the new run follows");
}

#[test]
fn second_trigger_by_go_on_the_playhead_and_not_by_auto_continue() {
    let mut rig = Rig::new(vec![
        with_advance(sound("0", "len100.wav"), AdvanceAt::Start),
        with_second(sound("1", "len1000.wav"), SecondTrigger::Stop),
        sound("2", "len1000.wav"),
    ]);
    let id = id_of(&rig, "1");
    rig.act(Action::StartCue(id));
    rig.run(50);
    rig.act(Action::Go);
    rig.run(50);
    assert_eq!(sorted(rig.active()), ["0", "1"], "auto-continue does not stop it");
    assert!(rig.runner.take_notices().contains(&Notice::Warning("Cue 1 is already running".into())));
    rig.act(Action::SetPlayhead(id));
    rig.act(Action::Go);
    rig.run(100);
    assert!(rig.active().is_empty(), "GO on the playhead stops it");
    assert_eq!(rig.playhead().as_deref(), Some("2"), "and the playhead moves on");
}

#[test]
fn devamp_cue_can_leave_all_vamps_at_once() {
    let song = vamped("1", "len3000.wav", &[(0.1, 0.3), (0.5, 0.7)], &[]);
    let id = song.id;
    let mut d = devamp("2", id, DevampMode::LoopEnd);
    if let CueKind::ExitLoop(d) = &mut d.kind {
        d.exit.level = LoopLevel::All;
    }
    let mut rig = Rig::new(vec![song, d]);
    rig.act(Action::Go);
    rig.run(200);
    rig.act(Action::Go);
    rig.run(500);
    assert!(remaining(&rig).is_some(), "out of both vamps");
    rig.run(2400);
    assert!(rig.active().is_empty());
}

fn with_others(mut c: Cue, others: OthersEffect) -> Cue {
    c.others = Some(others);
    c
}

fn effect(effect: OthersAction, fade: f64) -> OthersEffect {
    OthersEffect { effect, fade, ..Default::default() }
}

#[test]
fn a_cue_of_its_own_overrides_the_shows_exclusive_mode() {
    let mut rig = Rig::new(vec![
        sound("1", "len2000.wav"),
        with_others(sound("2", "len2000.wav"), effect(OthersAction::None, 0.1)),
        sound("3", "len2000.wav"),
    ]);
    let mut show = rig.runner.show().clone();
    show.others = effect(OthersAction::FadeStop, 0.1);
    rig.runner.set_show(show);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(200);
    assert_eq!(sorted(rig.active()), ["1", "2"], "cue 2 layers");
    rig.act(Action::Go);
    rig.run(200);
    assert_eq!(rig.active(), ["3"]);
}

#[test]
fn exclusive_cue_acts_when_it_starts_and_on_its_output_only() {
    let mut late = with_others(
        sound("3", "len2000.wav"),
        OthersEffect { scope: OthersScope::Output, ..effect(OthersAction::FadeStop, 0.1) },
    );
    late.pre_wait = 0.3;
    let mut aux = sound("2", "len2000.wav");
    if let CueKind::Sound(s) = &mut aux.kind {
        s.output = "aux".into();
    }
    let mut rig = Rig::new(vec![sound("1", "len2000.wav"), aux, late]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(250);
    assert_eq!(sorted(rig.active()), ["1", "2"], "nothing before the pre-wait");
    rig.run(250);
    assert_eq!(sorted(rig.active()), ["2", "3"], "cue 1 on the same output faded out");
}

fn level_of(rig: &Rig, number: &str) -> f32 {
    active_state(rig, number).unwrap().level_db
}

#[test]
fn ducking_lowers_the_others_strongest_wins_and_comes_back() {
    let duck = |n: &str, file: &str, db: f64| {
        with_others(sound(n, file), OthersEffect { duck_db: db, ..effect(OthersAction::Duck, 0.05) })
    };
    let mut rig = Rig::new(vec![
        sound("1", "len3000.wav"),
        duck("2", "len1000.wav", -6.0),
        duck("3", "len400.wav", -12.0),
        sound("4", "len1000.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(100);
    assert!((level_of(&rig, "1") + 6.0).abs() < 0.1, "{}", level_of(&rig, "1"));
    assert!(level_of(&rig, "2").abs() < 0.1, "the ducking cue itself plays on");
    rig.act(Action::Go);
    rig.run(100);
    assert!((level_of(&rig, "1") + 12.0).abs() < 0.1, "strongest wins");
    assert!((level_of(&rig, "2") + 12.0).abs() < 0.1, "2 runs, so 3 ducks it too");
    rig.act(Action::Go);
    rig.run(100);
    assert!(level_of(&rig, "4").abs() < 0.1, "a cue started later is not ducked");
    rig.run(300);
    assert!((level_of(&rig, "1") + 6.0).abs() < 0.1, "3 ended: back to 2's duck");
    rig.run(700);
    assert!(level_of(&rig, "1").abs() < 0.1, "all ducks ended");
}

#[test]
fn a_fade_to_on_a_ducked_cue_keeps_the_duck() {
    let target = sound("1", "len3000.wav");
    let mut to = fade("3", Targets::Cues(vec![target.id]), VolumeAction::FadeTo, Some(0.05));
    if let CueKind::Volume(f) = &mut to.kind {
        f.volume_db = Some(-3.0);
    }
    let ducker = with_others(
        sound("2", "len2000.wav"),
        OthersEffect { duck_db: -10.0, ..effect(OthersAction::Duck, 0.05) },
    );
    let mut rig = Rig::new(vec![target, ducker, to]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::Go);
    rig.run(100);
    assert!((level_of(&rig, "1") + 13.0).abs() < 0.1, "{}", level_of(&rig, "1"));
}

#[test]
fn pausing_a_fade_cue_holds_its_fade() {
    let target = sound("1", "len5000.wav");
    let out = fade("2", Targets::Cues(vec![target.id]), VolumeAction::FadeStop, Some(0.4));
    let mut rig = Rig::new(vec![target, out]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(200);
    let fade_id = id_of(&rig, "2");
    rig.act(Action::PauseCue(fade_id));
    rig.run(20);
    let level = level_of(&rig, "1");
    rig.run(1000);
    assert_eq!(rig.active(), ["1"], "the target plays on");
    assert!(!active_state(&rig, "1").unwrap().paused);
    assert!((level_of(&rig, "1") - level).abs() < 0.01, "at the level the fade was held at");
    assert!(rig.runner.state(rig.now()).fading[0].paused);
    rig.act(Action::ResumeCue(fade_id));
    rig.run(250);
    assert!(rig.active().is_empty(), "the fade goes on and stops it");
}

// M3: advance points, loop exits, go-to cues, Repeat and the chain display.

fn with_sound(mut c: Cue, f: impl FnOnce(&mut SoundCue)) -> Cue {
    if let CueKind::Sound(s) = &mut c.kind {
        f(s);
    }
    c
}

fn go_to(number: &str, target: Option<CueId>, start: bool) -> Cue {
    let mut c = Cue::new(CueKind::GoTo(GoToCue { target, start }));
    c.number = number.into();
    c
}

fn with_passes(mut c: Cue, passes: Repeat, second: SecondTrigger) -> Cue {
    c.passes = passes;
    c.second_trigger = second;
    c
}

fn loaded(rig: &Rig, file: &str) -> usize {
    rig.loader.loads.lock().unwrap().iter().filter(|p| p.ends_with(file)).count()
}

#[test]
fn post_wait_starts_the_next_cue_after_its_time_even_past_the_end() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len100.wav"), AdvanceAt::After { secs: 0.3 }),
        sound("2", "len500.wav"),
    ]);
    rig.act(Action::Go);
    assert_eq!(rig.playhead(), None, "the chain is skipped");
    rig.run(280);
    assert!(rig.active().is_empty());
    rig.run(30);
    assert_eq!(rig.active(), ["2"]);
}

#[test]
fn post_wait_stands_still_while_its_cue_is_paused() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len5000.wav"), AdvanceAt::After { secs: 0.5 }),
        sound("2", "len500.wav"),
    ]);
    let first = id_of(&rig, "1");
    rig.act(Action::Go);
    rig.run(200);
    rig.act(Action::PauseCue(first));
    rig.run(1000);
    assert_eq!(rig.active(), ["1"]);
    rig.act(Action::ResumeCue(first));
    rig.run(290);
    assert_eq!(rig.active(), ["1"]);
    rig.run(20);
    assert_eq!(sorted(rig.active()), ["1", "2"]);
}

#[test]
fn stopping_a_cue_ends_its_post_wait() {
    for stop in [Action::StopCue, Action::FadeCue] {
        let mut rig = Rig::new(vec![
            with_advance(sound("1", "len2000.wav"), AdvanceAt::After { secs: 0.3 }),
            sound("2", "len500.wav"),
        ]);
        rig.act(Action::Go);
        rig.run(100);
        rig.act(stop(id_of(&rig, "1")));
        rig.run(500);
        assert!(!rig.active().contains(&"2".to_string()));
    }
}

#[test]
fn after_the_fade_in_waits_for_it_and_its_pause() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len2000.wav"), AdvanceAt::AfterFadeIn),
        sound("2", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(10);
    assert_eq!(sorted(rig.active()), ["1", "2"], "without a fade-in at once");

    let mut rig = Rig::new(vec![
        with_advance(with_sound(sound("1", "len2000.wav"), |s| s.fade_in = 0.2), AdvanceAt::AfterFadeIn),
        sound("2", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::PauseAll);
    rig.run(500);
    rig.act(Action::ResumeAll);
    rig.run(90);
    assert_eq!(rig.active(), ["1"]);
    rig.run(20);
    assert_eq!(sorted(rig.active()), ["1", "2"]);
}

#[test]
fn every_point_works_with_a_playlist_a_group_and_a_wait_cue() {
    let points = [AdvanceAt::Start, AdvanceAt::After { secs: 0.1 }, AdvanceAt::End];
    // Playlist and group both last 400 ms, the wait 400 ms.
    let cues = || {
        [
            playlist("1", &["len200.wav", "len200.wav"], 0.0, false),
            group_of("1", GroupMode::All, vec![sound("1a", "len400.wav")]),
            wait("1", 0.4),
        ]
    };
    for (point, at_ms) in points.into_iter().zip([0, 100, 400]) {
        for first in cues() {
            let mut rig = Rig::new(vec![with_advance(first, point), sound("2", "len1000.wav")]);
            let next = id_of(&rig, "2");
            rig.act(Action::Go);
            let started = |rig: &Rig| rig.runner.state(rig.now()).active.iter().any(|a| a.cue == next);
            if at_ms > 0 {
                rig.run(at_ms - 20);
                assert!(!started(&rig), "{point:?}");
            }
            rig.run(40);
            assert!(started(&rig), "{point:?}");
        }
    }
}

#[test]
fn the_playhead_skips_the_whole_chain_before_it_starts() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len200.wav"), AdvanceAt::Start),
        with_advance(sound("2", "len200.wav"), AdvanceAt::After { secs: 0.1 }),
        sound("3", "len200.wav"),
        sound("4", "len200.wav"),
    ]);
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("4"));
    rig.run(120);
    assert_eq!(sorted(rig.active()), ["1", "2", "3"]);
}

#[test]
fn a_sequence_group_chains_by_mixed_points() {
    let mut rig = Rig::new(vec![group_of(
        "1",
        GroupMode::Sequence,
        vec![
            with_advance(sound("1a", "len1000.wav"), AdvanceAt::After { secs: 0.2 }),
            with_advance(sound("1b", "len300.wav"), AdvanceAt::BeforeEnd { secs: 0.1 }),
            sound("1c", "len300.wav"),
        ],
    )]);
    rig.act(Action::Go);
    rig.run(190);
    assert_eq!(rig.active(), ["1a"]);
    rig.run(20);
    assert_eq!(sorted(rig.active()), ["1a", "1b"]);
    rig.run(200);
    assert_eq!(sorted(rig.active()), ["1a", "1b", "1c"], "100 ms before 1b ends");
}

#[test]
fn pause_all_holds_a_chain_of_three_points() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len300.wav"), AdvanceAt::End),
        with_advance(sound("2", "len1000.wav"), AdvanceAt::After { secs: 0.2 }),
        with_advance(sound("3", "len500.wav"), AdvanceAt::BeforeEnd { secs: 0.1 }),
        sound("4", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(400);
    assert_eq!(rig.active(), ["2"]);
    rig.act(Action::PauseAll);
    rig.run(1000);
    rig.act(Action::ResumeAll);
    rig.run(80);
    assert_eq!(rig.active(), ["2"]);
    rig.run(40);
    assert_eq!(sorted(rig.active()), ["2", "3"]);
    rig.act(Action::PauseAll);
    rig.run(1000);
    rig.act(Action::ResumeAll);
    rig.run(370);
    assert_eq!(sorted(rig.active()), ["2", "3"]);
    rig.run(40);
    assert_eq!(sorted(rig.active()), ["2", "3", "4"]);
}

#[test]
fn before_the_end_starts_the_next_cue_ahead_of_it() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len1000.wav"), AdvanceAt::BeforeEnd { secs: 0.2 }),
        sound("2", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(780);
    assert_eq!(rig.active(), ["1"]);
    rig.run(40);
    assert_eq!(sorted(rig.active()), ["1", "2"]);
}

#[test]
fn before_the_fade_out_starts_with_the_end_fade() {
    let song = with_sound(sound("1", "len1000.wav"), |s| (s.end_fade, s.fade_out) = (true, 0.3));
    let mut rig = Rig::new(vec![with_advance(song, AdvanceAt::BeforeFadeOut), sound("2", "len500.wav")]);
    rig.act(Action::Go);
    rig.run(680);
    assert_eq!(rig.active(), ["1"]);
    rig.run(40);
    assert_eq!(sorted(rig.active()), ["1", "2"]);

    // Without an end fade it means the end.
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len300.wav"), AdvanceAt::BeforeFadeOut),
        sound("2", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(280);
    assert_eq!(rig.active(), ["1"]);
    rig.run(40);
    assert_eq!(rig.active(), ["2"]);
}

#[test]
fn a_playlist_starts_before_the_end_of_its_last_item_only() {
    let mut rig = Rig::new(vec![
        with_advance(
            playlist("1", &["len300.wav", "len300.wav"], 0.0, false),
            AdvanceAt::BeforeEnd { secs: 0.1 },
        ),
        sound("2", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(480);
    assert_eq!(rig.active(), ["1"]);
    rig.run(40);
    assert_eq!(sorted(rig.active()), ["1", "2"]);
}

#[test]
fn stop_prevents_the_start_before_the_end() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len1000.wav"), AdvanceAt::BeforeEnd { secs: 0.2 }),
        sound("2", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(300);
    rig.act(Action::StopCue(id_of(&rig, "1")));
    rig.run(1000);
    assert!(rig.active().is_empty());
}

#[test]
fn a_wait_cue_starts_before_its_end_and_holds_while_paused() {
    let mut rig = Rig::new(vec![
        with_advance(wait("1", 0.5), AdvanceAt::BeforeEnd { secs: 0.2 }),
        sound("2", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::PauseCue(id_of(&rig, "1")));
    rig.run(500);
    rig.act(Action::ResumeCue(id_of(&rig, "1")));
    rig.run(180);
    assert!(rig.active().is_empty());
    rig.run(40);
    assert_eq!(rig.active(), ["2"]);
}

#[test]
fn a_jump_past_the_point_while_paused_starts_on_resume() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len3000.wav"), AdvanceAt::BeforeEnd { secs: 0.5 }),
        sound("2", "len500.wav"),
    ]);
    rig.runner.set_settings(LocalSettings { go_lockout: 0.0, jump_end: 0.2, ..LocalSettings::default() });
    let first = id_of(&rig, "1");
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::PauseCue(first));
    rig.run(50);
    rig.act(Action::JumpEnd(first));
    rig.run(500);
    assert_eq!(rig.active(), ["1"]);
    rig.act(Action::ResumeCue(first));
    rig.run(20);
    assert_eq!(sorted(rig.active()), ["1", "2"]);
}

fn with_loop_exit(mut c: Cue, of: Option<CueId>) -> Cue {
    c.advance = Advance::new(AdvanceAt::LoopExit { of });
    c
}

#[test]
fn the_loop_exit_starts_the_next_cue_when_the_vamp_is_left() {
    let song = vamped("1", "len1000.wav", &[(0.1, 0.4)], &[]);
    let id = song.id;
    let mut rig = Rig::new(vec![
        song,
        with_loop_exit(devamp("2", id, DevampMode::LoopEnd), None),
        sound("3", "len500.wav"),
        sound("4", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(150);
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("4"));
    rig.run(200);
    assert_eq!(rig.active(), ["1"], "not at the devamp");
    rig.run(70);
    assert_eq!(sorted(rig.active()), ["1", "3"], "at the end of the vamp's pass");
    let notices = rig.runner.take_notices();
    assert!(
        notices.iter().all(|n| matches!(n, Notice::Warning(m) if m.contains("preloaded"))),
        "{notices:?}"
    );
}

#[test]
fn the_loop_exit_without_a_running_target_goes_on_at_once() {
    let song = vamped("1", "len1000.wav", &[(0.1, 0.4)], &[]);
    let id = song.id;
    let mut rig = Rig::new(vec![
        song,
        with_loop_exit(devamp("2", id, DevampMode::LoopEnd), None),
        sound("3", "len500.wav"),
    ]);
    rig.act(Action::SetPlayhead(id_of(&rig, "2")));
    rig.act(Action::Go);
    rig.run(10);
    assert_eq!(rig.active(), ["3"]);
}

#[test]
fn the_loop_exit_waits_for_the_chosen_cue_or_the_first() {
    // Two vamps in the target group: 1a is out at 200 ms, 1b at 500 ms.
    for (of, at_ms) in [(Some("1b"), 500), (None, 200), (Some("2"), 200)] {
        let group = group_of(
            "1",
            GroupMode::All,
            vec![
                vamped("1a", "len1000.wav", &[(0.1, 0.2)], &[]),
                vamped("1b", "len1000.wav", &[(0.1, 0.5)], &[]),
            ],
        );
        let target = group.id;
        let mut rig =
            Rig::new(vec![group, devamp("2", target, DevampMode::LoopEnd), sound("3", "len500.wav")]);
        let devamp_cue = id_of(&rig, "2");
        let of = of.map(|number| id_of(&rig, number));
        let mut show = rig.runner.show().clone();
        if let Some(c) = show.cue_mut(devamp_cue) {
            c.advance = Advance::new(AdvanceAt::LoopExit { of });
        }
        rig.runner.set_show(show);
        let next = id_of(&rig, "3");
        let started = |rig: &Rig| rig.runner.state(rig.now()).active.iter().any(|a| a.cue == next);
        rig.act(Action::Go);
        rig.run(150);
        rig.act(Action::Go);
        rig.run(at_ms - 170);
        assert!(!started(&rig), "{of:?}");
        rig.run(40);
        assert!(started(&rig), "{of:?}");
    }
}

#[test]
fn a_loop_exit_with_stop_ends_the_cue_there() {
    let mut song = vamped("1", "len1000.wav", &[(0.1, 0.3)], &[]);
    song.second_trigger = SecondTrigger::ExitLoop {
        exit: ExitLoop { mode: DevampMode::LoopEnd, stop: true, ..Default::default() },
    };
    let id = song.id;
    let mut rig = Rig::new(vec![song]);
    rig.act(Action::Go);
    rig.run(150);
    rig.act(Action::StartCue(id));
    rig.run(140);
    assert_eq!(rig.active(), ["1"]);
    rig.run(40);
    assert!(rig.active().is_empty(), "ended at the exit (300 ms) plus the stop blend");
}

#[test]
fn a_go_to_cue_only_selects_its_target() {
    let mut rig = Rig::new(vec![go_to("1", None, false), sound("2", "len300.wav"), sound("3", "len300.wav")]);
    let target = id_of(&rig, "3");
    let mut show = rig.runner.show().clone();
    show.cues[0].kind = CueKind::GoTo(GoToCue { target: Some(target), start: false });
    rig.runner.set_show(show);
    rig.act(Action::Go);
    rig.run(20);
    assert!(rig.active().is_empty());
    assert_eq!(rig.playhead().as_deref(), Some("3"));
}

#[test]
fn a_go_to_cue_starts_its_target_and_skips_the_targets_chain() {
    let mut rig = Rig::new(vec![
        go_to("1", None, true),
        sound("2", "len300.wav"),
        with_advance(sound("3", "len300.wav"), AdvanceAt::Start),
        sound("4", "len300.wav"),
        sound("5", "len300.wav"),
    ]);
    let mut show = rig.runner.show().clone();
    show.cues[0].kind = CueKind::GoTo(GoToCue { target: Some(show.cues[2].id), start: true });
    rig.runner.set_show(show);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(sorted(rig.active()), ["3", "4"]);
    assert_eq!(rig.playhead().as_deref(), Some("5"));
}

#[test]
fn a_go_to_cue_late_in_a_chain_overrides_the_playhead() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len100.wav"), AdvanceAt::End),
        go_to("2", None, false),
        sound("3", "len300.wav"),
    ]);
    let mut show = rig.runner.show().clone();
    show.cues[1].kind = CueKind::GoTo(GoToCue { target: Some(show.cues[0].id), start: false });
    rig.runner.set_show(show);
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("3"));
    rig.run(150);
    assert_eq!(rig.playhead().as_deref(), Some("1"));
}

#[test]
fn a_go_to_cue_starts_a_group_whole() {
    let group = group_of("2", GroupMode::All, vec![sound("2a", "len300.wav"), sound("2b", "len300.wav")]);
    let mut rig = Rig::new(vec![go_to("1", Some(group.id), true), group, sound("3", "len300.wav")]);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(sorted(rig.active()), ["2a", "2b"]);
    assert_eq!(rig.playhead().as_deref(), Some("3"));
}

#[test]
fn a_missing_go_to_target_is_reported_and_keeps_the_chain() {
    let mut rig = Rig::new(vec![
        with_advance(go_to("1", Some(CueId::new()), true), AdvanceAt::Start),
        sound("2", "len300.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["2"]);
    let notices = rig.runner.take_notices();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Error(m) if m.contains("Target missing"))),
        "{notices:?}"
    );

    let mut rig = Rig::new(vec![go_to("1", None, true)]);
    rig.act(Action::Go);
    assert!(matches!(rig.runner.take_notices()[..], [Notice::Warning(_)]));

    // A cue inside a group is there, but no valid target.
    let group = group_of("2", GroupMode::All, vec![sound("2a", "len300.wav")]);
    let inner = group_cues(&group)[0].id;
    let mut rig = Rig::new(vec![go_to("1", Some(inner), true), group]);
    rig.act(Action::Go);
    rig.run(20);
    assert!(rig.active().is_empty());
    let notices = rig.runner.take_notices();
    assert!(
        notices
            .iter()
            .any(|n| matches!(n, Notice::Error(m) if m.contains("2a") && m.contains("inside a group"))),
        "{notices:?}"
    );
}

#[test]
fn an_exclusive_go_to_target_spares_the_cues_of_the_same_go() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len2000.wav"), AdvanceAt::Start),
        go_to("2", None, true),
        sound("3", "len2000.wav"),
        sound("4", "len2000.wav"),
    ]);
    let mut show = rig.runner.show().clone();
    show.others = OthersEffect { effect: OthersAction::FadeStop, fade: 0.1, ..Default::default() };
    show.cues[1].kind = CueKind::GoTo(GoToCue { target: Some(show.cues[3].id), start: true });
    rig.runner.set_show(show);
    rig.act(Action::Go);
    rig.run(300);
    assert_eq!(sorted(rig.active()), ["1", "4"]);
}

#[test]
fn a_go_to_loop_without_a_pause_is_broken_off() {
    let mut rig = Rig::new(vec![go_to("1", None, true), sound("2", "len300.wav")]);
    let mut show = rig.runner.show().clone();
    show.cues[0].kind = CueKind::GoTo(GoToCue { target: Some(show.cues[0].id), start: true });
    rig.runner.set_show(show);
    rig.act(Action::Go);
    let notices = rig.runner.take_notices();
    assert!(notices.iter().any(|n| matches!(n, Notice::Error(m) if m.contains("loop"))), "{notices:?}");
}

fn repeat_state(rig: &Rig) -> Option<RepeatState> {
    rig.runner.state(rig.now()).repeat
}

#[test]
fn repeat_holds_the_playhead_for_its_passes() {
    let mut rig = Rig::new(vec![
        with_passes(sound("1", "len100.wav"), Repeat::Times(3), SecondTrigger::Ignore),
        sound("2", "len100.wav"),
    ]);
    for done in 1..3 {
        rig.act(Action::Go);
        assert_eq!(rig.playhead().as_deref(), Some("1"));
        assert_eq!(repeat_state(&rig), Some(RepeatState { done, count: Some(3) }));
        rig.run(200);
    }
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("2"));
    assert_eq!(repeat_state(&rig), None);
}

#[test]
fn repeat_forever_stays_until_the_playhead_moves_and_a_move_resets_it() {
    let mut rig = Rig::new(vec![
        with_passes(sound("1", "len100.wav"), Repeat::Forever, SecondTrigger::Ignore),
        sound("2", "len100.wav"),
    ]);
    for _ in 0..5 {
        rig.act(Action::Go);
        rig.run(200);
    }
    assert_eq!(rig.playhead().as_deref(), Some("1"));
    assert_eq!(repeat_state(&rig), Some(RepeatState { done: 5, count: None }));
    rig.act(Action::PlayheadNext);
    rig.act(Action::PlayheadPrev);
    assert_eq!(repeat_state(&rig), Some(RepeatState { done: 0, count: None }));
}

#[test]
fn a_restarting_repeat_cue_counts_each_go() {
    let mut rig =
        Rig::new(vec![with_passes(sound("1", "len500.wav"), Repeat::Times(3), SecondTrigger::Restart)]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::Go);
    assert_eq!(repeat_state(&rig).map(|r| r.done), Some(2));
    rig.run(450);
    assert_eq!(rig.active(), ["1"], "started anew");
}

#[test]
fn an_ignored_go_is_no_pass() {
    let mut rig = Rig::new(vec![
        with_passes(sound("1", "len500.wav"), Repeat::Times(2), SecondTrigger::Ignore),
        sound("2", "len100.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::Go);
    assert_eq!(repeat_state(&rig).map(|r| r.done), Some(1));
    rig.run(500);
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("2"), "after its end the GO counts");
}

#[test]
fn a_second_trigger_on_a_repeat_cue_is_no_pass() {
    let mut cue = with_passes(sound("1", "len20000.wav"), Repeat::Times(2), SecondTrigger::FadeOut);
    if let CueKind::Sound(s) = &mut cue.kind {
        s.fade_out = 0.2;
    }
    let mut rig = Rig::new(vec![cue, sound("2", "len100.wav")]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::Go);
    assert_eq!(repeat_state(&rig).map(|r| r.done), Some(1), "the fade-out is no pass");
    assert_eq!(rig.playhead().as_deref(), Some("1"));
    rig.run(400);
    assert!(rig.active().is_empty(), "faded out");
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["1"], "the second pass starts it again");
    assert_eq!(rig.playhead().as_deref(), Some("2"));
}

#[test]
fn a_repeat_cue_starts_its_chain_on_every_pass() {
    let mut rig = Rig::new(vec![
        with_advance(
            with_passes(sound("1", "len100.wav"), Repeat::Times(2), SecondTrigger::Ignore),
            AdvanceAt::End,
        ),
        sound("2", "len100.wav"),
        sound("3", "len100.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(150);
    assert_eq!(rig.active(), ["2"]);
    rig.run(100);
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("3"));
    rig.run(150);
    assert_eq!(rig.active(), ["2"]);
}

fn following(rig: &Rig) -> Vec<FollowState> {
    rig.runner.state(rig.now()).following
}

#[test]
fn following_shows_a_post_wait_running_down_and_paused() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len5000.wav"), AdvanceAt::After { secs: 1.0 }),
        sound("2", "len500.wav"),
    ]);
    let (first, second) = (id_of(&rig, "1"), id_of(&rig, "2"));
    rig.act(Action::Go);
    rig.run(300);
    let f = following(&rig);
    assert_eq!((f.len(), f[0].cue, f[0].after, f[0].root, f[0].paused), (1, second, first, first, false));
    assert!((f[0].remaining.unwrap() - 0.7).abs() < 0.02, "{f:?}");
    rig.act(Action::PauseCue(first));
    rig.run(500);
    let f = following(&rig);
    assert!(f[0].paused && (f[0].remaining.unwrap() - 0.7).abs() < 0.02, "{f:?}");
}

#[test]
fn following_at_the_end_shows_the_rest_of_the_cue_before() {
    let mut rig = Rig::new(vec![
        sound("0", "len3000.wav"),
        with_advance(sound("1", "len1000.wav"), AdvanceAt::Start),
        with_advance(sound("2", "len1000.wav"), AdvanceAt::End),
        sound("3", "len500.wav"),
    ]);
    rig.act(Action::SetPlayhead(id_of(&rig, "1")));
    rig.act(Action::Go);
    rig.run(400);
    let f = following(&rig);
    assert_eq!(f.len(), 1);
    assert_eq!((f[0].cue, f[0].after, f[0].root), (id_of(&rig, "3"), id_of(&rig, "2"), id_of(&rig, "1")));
    let rest = rig.runner.state(rig.now()).active.iter().find(|a| a.cue == f[0].after).unwrap().remaining;
    assert_eq!(f[0].remaining, rest);
}

#[test]
fn following_is_unknown_at_an_open_vamp_and_empty_after_stop_all() {
    let mut rig = Rig::new(vec![
        with_advance(vamped("1", "len1000.wav", &[(0.1, 0.4)], &[]), AdvanceAt::End),
        sound("2", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(200);
    assert_eq!(following(&rig)[0].remaining, None);
    rig.act(Action::StopAll);
    assert!(following(&rig).is_empty());
}

#[test]
fn a_chain_with_the_start_is_armed_whole() {
    let cues = (1..=6)
        .map(|i| with_advance(sound(&i.to_string(), &format!("len{}.wav", 300 + i)), AdvanceAt::Start));
    let mut cues: Vec<Cue> = cues.collect();
    cues[5].advance = Advance::default();
    let mut rig = Rig::new(cues);
    rig.wait_armed(id_of(&rig, "6"));
    assert_eq!((1..=6).filter(|i| loaded(&rig, &format!("len{}.wav", 300 + i)) == 1).count(), 6);
}

#[test]
fn the_cue_before_the_end_stays_armed_while_its_predecessor_runs_and_pauses() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len3000.wav"), AdvanceAt::BeforeEnd { secs: 0.5 }),
        sound("2", "len311.wav"),
        sound("3", "len312.wav"),
        sound("4", "len313.wav"),
        sound("5", "len314.wav"),
    ]);
    let next = id_of(&rig, "2");
    rig.wait_armed(next);
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("3"));
    rig.act(Action::PauseAll);
    rig.wait_armed(id_of(&rig, "5"));
    assert!(rig.runner.is_armed(next));
    assert_eq!(loaded(&rig, "len311.wav"), 1);
}

#[test]
fn arming_follows_go_to_cues_and_stops_at_a_loop() {
    let mut rig = Rig::new(vec![
        sound("1", "len321.wav"),
        go_to("2", None, false),
        sound("3", "len323.wav"),
        sound("4", "len324.wav"),
        sound("5", "len325.wav"),
        sound("6", "len326.wav"),
        go_to("7", None, true),
    ]);
    let mut show = rig.runner.show().clone();
    show.cues[1].kind = CueKind::GoTo(GoToCue { target: Some(show.cues[4].id), start: false });
    show.cues[6].kind = CueKind::GoTo(GoToCue { target: Some(show.cues[4].id), start: true });
    rig.runner.set_show(show);
    rig.wait_armed(id_of(&rig, "6"));
    assert_eq!(loaded(&rig, "len323.wav"), 0, "the go-to leads past it");
    // From 5 on the walk comes back to 5 by the go-to cue 7, and ends.
    rig.act(Action::SetPlayhead(id_of(&rig, "5")));
    rig.wait_armed(id_of(&rig, "6"));
}

#[test]
fn the_cue_at_the_loop_exit_stays_armed_until_it_starts() {
    let song = vamped("1", "len3000.wav", &[(0.1, 2.0)], &[]);
    let id = song.id;
    let mut rig = Rig::new(vec![
        song,
        with_loop_exit(devamp("2", id, DevampMode::LoopEnd), None),
        sound("3", "len331.wav"),
        sound("4", "len332.wav"),
        sound("5", "len333.wav"),
        sound("6", "len334.wav"),
        sound("7", "len335.wav"),
    ]);
    let next = id_of(&rig, "3");
    rig.wait_armed(next);
    rig.act(Action::Go);
    rig.run(150);
    rig.act(Action::Go);
    rig.wait_armed(id_of(&rig, "6"));
    assert!(rig.runner.is_armed(next));
    assert_eq!(loaded(&rig, "len331.wav"), 1);
}

#[test]
fn a_repeat_cue_is_not_loaded_again() {
    let mut rig =
        Rig::new(vec![with_passes(sound("1", "len341.wav"), Repeat::Times(3), SecondTrigger::Ignore)]);
    rig.wait_armed(id_of(&rig, "1"));
    rig.act(Action::Go);
    rig.run(400);
    rig.act(Action::Go);
    rig.run(100);
    assert_eq!(loaded(&rig, "len341.wav"), 1);
    assert!(rig.runner.take_notices().is_empty());
}

#[test]
fn a_pre_wait_knows_the_chain_it_belongs_to() {
    let mut next = sound("2", "len500.wav");
    next.pre_wait = 0.5;
    let mut rig = Rig::new(vec![with_advance(sound("1", "len1000.wav"), AdvanceAt::Start), next]);
    rig.act(Action::Go);
    rig.run(100);
    let waiting = rig.runner.state(rig.now()).waiting;
    assert_eq!((waiting.len(), waiting[0].cue, waiting[0].root), (1, id_of(&rig, "2"), id_of(&rig, "1")));
}

#[test]
fn a_group_keeps_its_later_cues_armed_while_it_starts() {
    // The outer group starts 6.3.1 before the wait in 6.3.2 sets its timer;
    // arming in between dropped the inner group's files.
    let inner = group_of(
        "6.3.2.2",
        GroupMode::All,
        vec![sound("6.3.2.2.1", "len5000.wav"), sound("6.3.2.2.2", "len8000.wav")],
    );
    let sequence = group_of(
        "6.3.2",
        GroupMode::Sequence,
        vec![with_advance(wait("6.3.2.1", 0.5), AdvanceAt::End), inner],
    );
    let outer = group_of("6.3", GroupMode::All, vec![sound("6.3.1", "len20000.wav"), sequence]);
    let id = outer.id;
    let mut rig = Rig::new(vec![outer, sound("6.4", "len300.wav")]);
    rig.wait_armed(id);
    rig.act(Action::Go);
    rig.run(1000);
    assert_eq!(sorted(rig.active()), ["6.3.1", "6.3.2.2.1", "6.3.2.2.2"]);
    let notices = rig.runner.take_notices();
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(rig.loader.loads.lock().unwrap().len(), 4, "every file loaded once");
}

#[test]
fn an_empty_playlist_goes_on_to_its_follow_cue() {
    let mut rig = Rig::new(vec![
        with_advance(playlist("1", &[], 0.0, false), AdvanceAt::End),
        sound("2", "len300.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["2"], "{:?}", rig.runner.take_notices());
}

#[test]
fn a_playlist_arms_the_item_after_a_missing_one() {
    let list = playlist("1", &["len1000.wav", "missing.wav", "len300.wav"], 0.0, false);
    let id = list.id;
    let mut rig = Rig::new(vec![list]);
    rig.wait_armed(id);
    rig.act(Action::Go);
    // Time for the background loader to fail on the missing file and load the next.
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(2));
        rig.run(10);
    }
    rig.runner.take_notices();
    rig.run(600);
    assert_eq!(item(&rig), Some((2, 3)));
    let notices = rig.runner.take_notices();
    assert!(
        notices.iter().all(|n| !matches!(n, Notice::Warning(m) if m.contains("preloaded"))),
        "{notices:?}"
    );
}

// --- M4: control cues -------------------------------------------------------

fn start_cue(number: &str, targets: Targets, from: Option<f64>) -> Cue {
    control(number, CueKind::StartStop(StartStopCue { targets, action: StartStopAction::Start, from }))
}

fn pause_cue(number: &str, targets: Targets, action: PauseAction) -> Cue {
    control(number, CueKind::Pause(PauseCue { targets, action }))
}

fn pan_cue(number: &str, targets: Targets, pan: f64, duration: f64) -> Cue {
    control(number, CueKind::Pan(PanCue { targets, pan, duration, curve: PanCurve::Linear }))
}

fn position_cue(number: &str, target: CueId, to: PositionTo) -> Cue {
    control(
        number,
        CueKind::Position(PositionCue { targets: Targets::Cues(vec![target]), to, crossfade_ms: 0.0 }),
    )
}

fn cue_id(rig: &Rig, number: &str) -> CueId {
    rig.runner.show().cues.iter().find(|c| c.number == number).unwrap().id
}

/// GO on the cue `number`.
fn go_on(rig: &mut Rig, number: &str) {
    let id = cue_id(rig, number);
    rig.act(Action::SetPlayhead(id));
    rig.act(Action::Go);
}

/// One output frame (left, right) of what plays now.
fn frame(rig: &mut Rig) -> [f32; 2] {
    let mut out = vec![0.0; 2];
    rig.mixer.process(&mut out);
    [out[0], out[1]]
}

fn progress(rig: &Rig) -> f32 {
    rig.runner.state(rig.now()).active[0].progress
}

fn paused(rig: &Rig) -> Vec<bool> {
    rig.runner.state(rig.now()).active.iter().map(|a| a.paused).collect()
}

#[test]
fn start_cue_starts_its_target_with_pre_wait_and_chain_without_the_playhead() {
    let mut first = with_advance(sound("1", "len5000.wav"), AdvanceAt::Start);
    first.pre_wait = 0.1;
    let id = first.id;
    let mut rig =
        Rig::new(vec![first, sound("2", "len5000.wav"), start_cue("3", Targets::Cues(vec![id]), None)]);
    go_on(&mut rig, "3");
    assert_eq!(rig.playhead(), None, "on past the start cue, not to its target");
    rig.run(50);
    assert!(rig.active().is_empty(), "pre-wait");
    rig.run(100);
    assert_eq!(rig.active(), ["1", "2"]);
}

#[test]
fn start_cue_resumes_a_paused_target_and_leaves_a_running_one() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let mut rig = Rig::new(vec![target, start_cue("2", Targets::Cues(vec![id]), None)]);
    rig.act(Action::Go);
    rig.run(50);
    rig.runner.take_notices();
    rig.act(Action::PauseCue(id));
    rig.run(50);
    assert_eq!(paused(&rig), [true]);
    go_on(&mut rig, "2");
    rig.run(20);
    assert_eq!(paused(&rig), [false]);
    assert_eq!(rig.runner.take_notices(), []);
    let elapsed = rig.runner.state(rig.now()).active[0].elapsed;
    go_on(&mut rig, "2");
    rig.run(20);
    let state = rig.runner.state(rig.now());
    assert!(state.active[0].elapsed > elapsed, "plays on, no restart");
    assert_eq!(rig.runner.take_notices(), [Notice::Warning("Cue 1 is already running".into())]);
}

#[test]
fn start_cue_plays_a_sound_from_its_position() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let mut rig = Rig::new(vec![target, start_cue("2", Targets::Cues(vec![id]), Some(2.0))]);
    go_on(&mut rig, "2");
    rig.run(50);
    assert!((progress(&rig) - 0.41).abs() < 0.02, "{}", progress(&rig));
}

#[test]
fn start_cue_on_previous_entry_and_arms_it() {
    let mut rig = Rig::new(vec![
        sound("1", "len5000.wav"),
        pause_cue("2", Targets::Previous(PreviousKeyword::Previous), PauseAction::Pause),
        start_cue("3", Targets::Previous(PreviousKeyword::Previous), None),
    ]);
    let id = cue_id(&rig, "1");
    rig.act(Action::SetPlayhead(cue_id(&rig, "3")));
    rig.wait_armed(id);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["1"]);
    assert!(rig.runner.take_notices().is_empty(), "armed before");
}

#[test]
fn consecutive_control_cues_target_the_previous_sound() {
    let mut rig = Rig::new(vec![
        sound("1", "len5000.wav"),
        pause_cue("2", Targets::Previous(PreviousKeyword::Previous), PauseAction::Pause),
        pause_cue("3", Targets::Previous(PreviousKeyword::Previous), PauseAction::Resume),
    ]);

    go_on(&mut rig, "1");
    rig.run(20);
    go_on(&mut rig, "2");
    assert!(active_state(&rig, "1").unwrap().paused);
    go_on(&mut rig, "3");
    assert!(!active_state(&rig, "1").unwrap().paused);
}

#[test]
fn stop_cue_ends_its_targets_post_wait() {
    let first = with_advance(sound("1", "len5000.wav"), AdvanceAt::After { secs: 0.3 });
    let id = first.id;
    let mut rig = Rig::new(vec![first, sound("2", "len500.wav"), stop_cue("3", Targets::Cues(vec![id]))]);
    rig.act(Action::Go);
    rig.run(50);
    assert_eq!(rig.playhead().as_deref(), Some("3"));
    rig.act(Action::Go);
    rig.run(400);
    assert!(rig.active().is_empty(), "stopped, and its next cue never started");
}

#[test]
fn stop_cue_on_an_output_stops_only_that_output() {
    let mut other = sound("2", "len5000.wav");
    if let CueKind::Sound(s) = &mut other.kind {
        s.output = "b".into();
    }
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len5000.wav"), AdvanceAt::Start),
        other,
        stop_cue("3", Targets::Output { output: "main".into() }),
    ]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(50);
    assert_eq!(rig.active(), ["2"]);
}

#[test]
fn pause_cue_holds_a_group_and_toggle_decides_for_all_targets() {
    let a = sound("a", "len5000.wav");
    let b = sound("b", "len5000.wav");
    let (ida, idb) = (a.id, b.id);
    let group = control("1", CueKind::Group(GroupCue { mode: GroupMode::All, cues: vec![a, b] }));
    let gid = group.id;
    let mut rig = Rig::new(vec![
        group,
        pause_cue("2", Targets::Cues(vec![gid]), PauseAction::Pause),
        pause_cue("3", Targets::Cues(vec![ida, idb]), PauseAction::Toggle),
        pause_cue("4", Targets::All(AllKeyword::All), PauseAction::Resume),
    ]);
    rig.act(Action::Go);
    rig.run(50);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(paused(&rig), [true, true]);
    rig.act(Action::ResumeCue(ida));
    rig.run(20);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(paused(&rig), [true, true], "one played, so toggle pauses both");
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(paused(&rig), [false, false]);
}

#[test]
fn pan_cue_moves_over_its_time_and_follows_at_the_end() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let mut rig = Rig::new(vec![
        target,
        with_advance(pan_cue("2", Targets::Cues(vec![id]), -1.0, 0.2), AdvanceAt::End),
        sound("3", "len5000.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::Go);
    rig.run(100);
    let [_, r] = frame(&mut rig);
    assert!((r - 0.25).abs() < 0.02, "halfway: {r}");
    assert_eq!(rig.active(), ["1"]);
    rig.run(150);
    assert_eq!(rig.active(), ["1", "3"]);
    let state = rig.runner.state(rig.now());
    assert!(state.fading.is_empty());
}

#[test]
fn control_cues_without_playing_targets_end_at_once() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let reset = ResetCue { targets: Targets::Cues(vec![id]), duration: 2.0 };
    let mut rig = Rig::new(vec![
        target,
        with_advance(pan_cue("2", Targets::Cues(vec![id]), -1.0, 2.0), AdvanceAt::End),
        with_advance(rate_cue("3", Targets::Cues(vec![id]), Some(120.0), None, 2.0), AdvanceAt::End),
        control("4", CueKind::Reset(reset)),
    ]);
    rig.act(Action::SetPlayhead(cue_id(&rig, "2")));
    rig.act(Action::Go);
    rig.run(20);
    let state = rig.runner.state(rig.now());
    assert!(state.fading.is_empty(), "nothing plays, nothing to move");
    assert!(rig.runner.is_quiet() && !rig.runner.is_lively());
}

#[test]
fn control_cues_on_paused_targets_end_at_once() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let mut volume = fade("5", Targets::Cues(vec![id]), VolumeAction::FadeTo, Some(2.0));
    if let CueKind::Volume(v) = &mut volume.kind {
        v.volume_db = Some(-20.0);
    }
    let reset = ResetCue { targets: Targets::Cues(vec![id]), duration: 2.0 };
    let mut rig = Rig::new(vec![
        target,
        with_advance(pan_cue("2", Targets::Cues(vec![id]), -1.0, 2.0), AdvanceAt::End),
        with_advance(rate_cue("3", Targets::Cues(vec![id]), Some(120.0), None, 2.0), AdvanceAt::End),
        with_advance(control("4", CueKind::Reset(reset)), AdvanceAt::End),
        volume,
    ]);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::PauseCue(id));
    rig.act(Action::SetPlayhead(cue_id(&rig, "2")));
    rig.act(Action::Go);
    rig.run(20);
    let state = rig.runner.state(rig.now());
    assert!(state.fading.is_empty(), "a paused voice takes the end value at once");
    assert_eq!(rig.active(), ["1"]);
}

#[test]
fn a_set_pan_stays_through_live_edits_and_the_next_playlist_item() {
    let list = playlist("1", &["len200.wav", "len2000.wav"], 0.0, false);
    let id = list.id;
    let mut rig =
        Rig::new(vec![list, pan_cue("2", Targets::Cues(vec![id]), -1.0, 0.0), sound("3", "len5000.wav")]);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::Go);
    rig.run(300);
    assert_eq!(item(&rig), Some((1, 2)));
    assert_eq!(frame(&mut rig), [0.5, 0.0], "the second item keeps the pan");
    edit_sound(&mut rig, "3", |s| s.volume_db = -6.0);
    rig.run(50);
    assert_eq!(frame(&mut rig), [0.5, 0.0], "a live edit elsewhere leaves it");
}

#[test]
fn a_playlist_pans_every_item_and_follows_live_edits() {
    let mut list = playlist("1", &["len200.wav", "len200.wav", "len2000.wav"], 0.0, false);
    if let CueKind::Playlist(p) = &mut list.kind {
        p.pan = -1.0;
    }
    let id = list.id;
    let mut rig = Rig::new(vec![list, pan_cue("2", Targets::Cues(vec![id]), 1.0, 0.0)]);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(frame(&mut rig), [0.5, 0.0], "the first item starts at the playlist's pan");
    rig.run(200);
    assert_eq!(item(&rig), Some((1, 3)));
    assert_eq!(frame(&mut rig), [0.5, 0.0], "and the second");
    rig.act(Action::Go);
    rig.run(200);
    assert_eq!(item(&rig), Some((2, 3)));
    assert_eq!(frame(&mut rig), [0.0, 0.5], "the third keeps the pan cue's pan");
    edit_playlist(&mut rig, "1", |p| p.volume_db = -0.0);
    edit_playlist(&mut rig, "1", |p| p.fade_out = 1.0);
    rig.run(50);
    assert_eq!(frame(&mut rig), [0.0, 0.5], "an edit of something else leaves it");
    edit_playlist(&mut rig, "1", |p| p.pan = 0.0);
    rig.run(50);
    assert_eq!(frame(&mut rig), [0.5, 0.5], "an edited pan moves the running item");
}

#[test]
fn a_paused_pan_cue_holds_the_pan_move() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let pan = pan_cue("2", Targets::Cues(vec![id]), -1.0, 1.0);
    let pan_id = pan.id;
    let mut rig = Rig::new(vec![target, pan]);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::Go);
    rig.run(500);
    rig.act(Action::PauseCue(pan_id));
    rig.run(500);
    let [l, r] = frame(&mut rig);
    assert!(l == 0.5 && (r - 0.25).abs() < 0.03, "held halfway: {r}");
    rig.act(Action::ResumeCue(pan_id));
    rig.run(600);
    assert_eq!(frame(&mut rig), [0.5, 0.0]);
}

#[test]
fn reset_cue_brings_level_and_pan_back() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let mut down = fade("2", Targets::Cues(vec![id]), VolumeAction::FadeTo, Some(0.05));
    if let CueKind::Volume(f) = &mut down.kind {
        f.volume_db = Some(-20.0);
    }
    let mut rig = Rig::new(vec![
        target,
        down,
        pan_cue("3", Targets::Cues(vec![id]), -1.0, 0.0),
        control("4", CueKind::Reset(ResetCue { targets: Targets::Cues(vec![id]), duration: 0.1 })),
    ]);
    for _ in 0..3 {
        rig.act(Action::Go);
        rig.run(100);
    }
    assert!((level_db(&rig) + 20.0).abs() < 0.1, "{}", level_db(&rig));
    rig.act(Action::Go);
    rig.run(50);
    assert!(!rig.runner.state(rig.now()).fading.is_empty(), "shown while it moves");
    rig.run(100);
    assert!(level_db(&rig).abs() < 0.1, "{}", level_db(&rig));
    assert_eq!(frame(&mut rig), [0.5, 0.5]);
}

#[test]
fn reset_leaves_a_cue_fading_out_to_a_stop() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let mut rig = Rig::new(vec![target]);
    rig.act(Action::Go);
    rig.run(20);
    rig.runner.take_notices();
    rig.act(Action::FadeCue(id));
    rig.act(Action::ResetCue(id));
    assert_eq!(
        rig.runner.take_notices(),
        [Notice::Warning("Cue 1 is fading out right now – Reset has no effect".into())]
    );
    rig.run(5200);
    assert!(rig.active().is_empty());
}

#[test]
fn fade_to_on_a_cue_fading_out_warns_and_leaves_it() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let mut up = fade("2", Targets::Cues(vec![id]), VolumeAction::FadeTo, Some(0.1));
    if let CueKind::Volume(f) = &mut up.kind {
        f.volume_db = Some(0.0);
    }
    let mut everything = fade("3", Targets::All(AllKeyword::All), VolumeAction::FadeTo, Some(0.1));
    if let CueKind::Volume(f) = &mut everything.kind {
        f.volume_db = Some(0.0);
    }
    let mut rig = Rig::new(vec![target, up, everything]);
    rig.act(Action::Go);
    rig.run(20);
    rig.runner.take_notices();
    rig.act(Action::FadeCue(id));
    rig.act(Action::Go);
    assert_eq!(
        rig.runner.take_notices(),
        [Notice::Warning("Cue 1 is fading out right now – “Fade to level” has no effect".into())]
    );
    rig.act(Action::Go);
    assert_eq!(rig.runner.take_notices(), [], "all running cues: no warning");
    rig.run(5200);
    assert!(rig.active().is_empty(), "the fade out went on");
}

#[test]
fn position_cue_jumps_absolute_relative_and_to_marks() {
    let song = vamped("1", "len5000.wav", &[], &[1.0, 3.0]);
    let id = song.id;
    let mut rig = Rig::new(vec![
        song,
        position_cue("2", id, PositionTo::Absolute { secs: 2.0 }),
        position_cue("3", id, PositionTo::Relative { secs: -0.5 }),
        position_cue("4", id, PositionTo::NextMark),
        position_cue("5", id, PositionTo::PreviousMark),
        position_cue("6", id, PositionTo::Mark { index: 2 }),
        position_cue("7", id, PositionTo::Mark { index: 3 }),
        position_cue("8", id, PositionTo::Absolute { secs: 99.0 }),
    ]);
    rig.act(Action::Go);
    rig.run(20);
    rig.runner.take_notices();
    // The previous mark is the one before a mark just passed.
    for (expect, number) in [(0.4, "2"), (0.3, "3"), (0.6, "4"), (0.2, "5"), (0.6, "6")] {
        go_on(&mut rig, number);
        rig.run(20);
        assert!((progress(&rig) - expect).abs() < 0.01, "cue {number}: {}", progress(&rig));
    }
    go_on(&mut rig, "7");
    rig.run(20);
    assert!((progress(&rig) - 0.6).abs() < 0.01, "no mark 3: stays");
    assert_eq!(rig.runner.take_notices(), [Notice::Warning("Cue 1: mark missing, no jump".into())]);
    go_on(&mut rig, "8");
    rig.run(20);
    assert!(rig.active().is_empty(), "to the end, which ends it");
}

#[test]
fn a_jump_back_before_a_vamp_left_holds_it_again() {
    let song = vamped("1", "len3000.wav", &[(0.5, 0.7)], &[]);
    let id = song.id;
    let mut rig = Rig::new(vec![
        song,
        devamp("2", id, DevampMode::Immediate),
        position_cue("3", id, PositionTo::Absolute { secs: 0.2 }),
    ]);
    rig.act(Action::Go);
    rig.run(600);
    rig.act(Action::Go);
    rig.run(200);
    assert!(remaining(&rig).is_some(), "out of the vamp");
    rig.act(Action::Go);
    rig.run(1000);
    assert_eq!(remaining(&rig), None, "the vamp holds again");
    assert_eq!(rig.active(), ["1"]);
    // So do the rehearsal jumps.
    rig.act(Action::ExitLoopCue(id));
    rig.run(300);
    assert!(remaining(&rig).is_some());
    rig.act(Action::JumpBack(id));
    rig.run(1000);
    assert_eq!(remaining(&rig), None);
}

#[test]
fn position_cue_leaves_a_paused_cue_paused() {
    let song = sound("1", "len5000.wav");
    let id = song.id;
    let mut rig = Rig::new(vec![song, position_cue("2", id, PositionTo::Absolute { secs: 2.0 })]);
    rig.act(Action::Go);
    rig.run(500);
    rig.act(Action::PauseCue(id));
    rig.run(50);
    rig.act(Action::Go);
    rig.run(200);
    let state = rig.runner.state(rig.now());
    assert!(state.active[0].paused);
    assert!((state.active[0].elapsed - 2.0).abs() < 0.05, "elapsed follows: {}", state.active[0].elapsed);
}

#[test]
fn stopwatches_run_by_cue_and_command_regardless_of_stop_all() {
    let mut rig = Rig::new(vec![
        control(
            "1",
            CueKind::Stopwatch(StopwatchCue { watch: Some("w".into()), action: StopwatchAction::Start }),
        ),
        control(
            "2",
            CueKind::Stopwatch(StopwatchCue { watch: Some("w".into()), action: StopwatchAction::Stop }),
        ),
        control("3", CueKind::Stopwatch(StopwatchCue { watch: None, action: StopwatchAction::Start })),
    ]);
    let mut show = rig.runner.show().clone();
    show.stopwatches.push(Stopwatch { id: "w".into(), name: "Vorstellung".into(), countdown: None });
    rig.runner.set_show(show.clone());
    let watch = |rig: &Rig| rig.runner.state(rig.now()).stopwatches[0].clone();
    assert_eq!((watch(&rig).elapsed, watch(&rig).running), (0.0, false));
    rig.act(Action::Go);
    rig.run(300);
    rig.act(Action::StopAll);
    rig.act(Action::PauseAll);
    rig.run(200);
    assert!(watch(&rig).running && (watch(&rig).elapsed - 0.5).abs() < 1e-6);
    rig.act(Action::Go);
    rig.run(1000);
    assert!(!watch(&rig).running && (watch(&rig).elapsed - 0.5).abs() < 1e-6, "stood still");
    let now = rig.now();
    rig.runner.stopwatch("w", StopwatchAction::Start, now);
    rig.run(100);
    assert!((watch(&rig).elapsed - 0.6).abs() < 1e-6, "runs on from its time");
    rig.runner
        .control(Control::Stopwatch { watch: "VORSTELLUNG".into(), action: StopwatchAction::Restart }, now);
    assert!(watch(&rig).running && (watch(&rig).elapsed - 0.1).abs() < 1e-6);
    rig.runner.stopwatch("w", StopwatchAction::Reset, now);
    assert_eq!((watch(&rig).elapsed, watch(&rig).running), (0.0, false));
    rig.act(Action::Go);
    assert_eq!(rig.runner.take_notices(), [Notice::Warning("Cue 3: no stopwatch chosen".into())]);
    show.stopwatches.clear();
    rig.runner.set_show(show);
    assert!(rig.runner.state(rig.now()).stopwatches.is_empty());
}

#[test]
fn remote_volume_can_be_relative_to_the_current_level() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let mut rig = Rig::new(vec![target]);
    rig.act(Action::Go);
    rig.run(20);
    let now = rig.now();
    rig.runner.control(Control::Volume { cue: id, db: -10.0, fade: 0.0, relative: false }, now);
    rig.run(50);
    rig.runner.control(Control::Volume { cue: id, db: -6.0, fade: 0.0, relative: true }, now);
    rig.run(50);
    assert!((level_db(&rig) + 16.0).abs() < 0.1, "{}", level_db(&rig));
}

#[test]
fn step_actions_move_level_pan_and_rate_by_the_settings() {
    let target = sound("1", "len10000.wav");
    let id = target.id;
    let mut rig = Rig::new(vec![target]);
    let settings =
        LocalSettings { volume_step: 3.0, pan_step: 0.25, rate_step: 5.0, ..rig.runner.settings().clone() };
    rig.runner.set_settings(settings);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::VolumeDown(id));
    rig.act(Action::VolumeDown(id));
    rig.act(Action::VolumeUp(id));
    rig.run(100);
    assert!((level_db(&rig) + 3.0).abs() < 0.1, "{}", level_db(&rig));
    for _ in 0..5 {
        rig.act(Action::PanRight(id));
    }
    rig.act(Action::PanLeft(id));
    rig.run(100);
    let a = active_state(&rig, "1").unwrap();
    assert!((a.pan - 0.75).abs() < 0.01, "clamped at 1, then one step back: {a:?}");
    rig.act(Action::RateDown(id));
    rig.act(Action::RateDown(id));
    rig.run(500);
    assert!((active_state(&rig, "1").unwrap().rate - 90.0).abs() < 0.01);
}

#[test]
fn restart_starts_a_running_cue_from_the_top() {
    let target = sound("1", "len5000.wav");
    let id = target.id;
    let mut rig = Rig::new(vec![target]);
    rig.act(Action::Go);
    rig.run(500);
    rig.act(Action::RestartCue(id));
    rig.run(50);
    assert!(rig.runner.state(rig.now()).active[0].elapsed < 0.1);
}

#[test]
fn start_cues_starting_each_other_are_broken_off() {
    let mut a = start_cue("1", Targets::Cues(vec![]), None);
    let b = start_cue("2", Targets::Cues(vec![a.id]), None);
    if let CueKind::StartStop(s) = &mut a.kind {
        s.targets = Targets::Cues(vec![b.id]);
    }
    let own = start_cue("3", Targets::Cues(vec![]), None);
    let mut own = own;
    let own_id = own.id;
    if let CueKind::StartStop(s) = &mut own.kind {
        s.targets = Targets::Cues(vec![own_id]);
    }
    let mut rig = Rig::new(vec![a, b, own]);
    rig.act(Action::Go);
    rig.run(20);
    let notices = rig.runner.take_notices();
    let broken = |m: &String| m.ends_with("start loop without a pause aborted");
    assert!(matches!(&notices[..], [Notice::Error(m)] if broken(m)), "{notices:?}");
    go_on(&mut rig, "3");
    rig.run(20);
    assert_eq!(
        rig.runner.take_notices(),
        [Notice::Error("Cue 3: start loop without a pause aborted".into())]
    );
}

#[test]
fn the_state_follows_the_pan_as_it_moves() {
    let mut target = sound("1", "len5000.wav");
    if let CueKind::Sound(s) = &mut target.kind {
        s.pan = 0.3;
    }
    let id = target.id;
    let mut rig = Rig::new(vec![target, pan_cue("2", Targets::Cues(vec![id]), -1.0, 1.0)]);
    rig.act(Action::Go);
    rig.run(20);
    let pan = |rig: &Rig| rig.runner.state(rig.now()).active[0].pan;
    assert!((pan(&rig) - 0.3).abs() < 1e-6, "{}", pan(&rig));
    rig.act(Action::Go);
    rig.run(500);
    assert!((pan(&rig) + 0.35).abs() < 0.05, "halfway: {}", pan(&rig));
    rig.run(600);
    assert!((pan(&rig) + 1.0).abs() < 1e-6, "{}", pan(&rig));
}

// Rate, pitch and the rate cue (M6).

fn with_speed(mut c: Cue, rate: f64, preserve_pitch: bool, transpose: f64) -> Cue {
    let speed = Speed { rate, preserve_pitch, transpose };
    match &mut c.kind {
        CueKind::Sound(s) => s.speed = speed,
        CueKind::Playlist(p) => p.speed = speed,
        _ => panic!("no speed"),
    }
    c
}

fn rate_cue(number: &str, targets: Targets, rate: Option<f64>, transpose: Option<f64>, duration: f64) -> Cue {
    control(number, CueKind::Rate(RateCue { targets, rate, transpose, duration, curve: PanCurve::Linear }))
}

#[test]
fn remaining_time_is_real_time_at_the_rate() {
    let mut rig = Rig::new(vec![with_speed(sound("1", "len10000.wav"), 50.0, false, 0.0)]);
    rig.act(Action::Go);
    rig.run(2000);
    let a = active_state(&rig, "1").unwrap();
    // 10 s of file at 50 % last 20 s; 2 s are gone.
    assert!((a.remaining.unwrap() - 18.0).abs() < 0.05, "{:?}", a.remaining);
    assert!((a.elapsed - 2.0).abs() < 0.02);
    assert_eq!(a.rate, 50.0);
    assert_eq!(a.transpose, 0.0);
}

#[test]
fn before_end_counts_real_time_at_the_rate() {
    let first = with_advance(
        with_speed(sound("1", "len4000.wav"), 50.0, false, 0.0),
        AdvanceAt::BeforeEnd { secs: 2.0 },
    );
    let mut rig = Rig::new(vec![first, sound("2", "len200.wav")]);
    rig.act(Action::Go);
    // 4 s of file last 8 s: the next cue comes after 6 s.
    rig.run(5900);
    assert_eq!(rig.active(), ["1"]);
    rig.run(200);
    assert_eq!(rig.active(), ["1", "2"]);
}

#[test]
fn rate_cue_moves_the_rate_and_the_remaining_time_follows() {
    let song = sound("1", "len10000.wav");
    let id = song.id;
    let mut rig = Rig::new(vec![song, rate_cue("2", Targets::Cues(vec![id]), Some(50.0), None, 0.0)]);
    rig.act(Action::Go);
    rig.run(1000);
    rig.act(Action::Go);
    rig.run(1000);
    let a = active_state(&rig, "1").unwrap();
    assert!((a.rate - 50.0).abs() < 0.1, "{}", a.rate);
    // 1 s at 100 %, then 1 s at 50 %: 8.5 s of file left, 17 s at 50 %
    // (the pitch is kept: the switch to the stretcher delays the change a
    // little).
    assert!((a.remaining.unwrap() - 17.0).abs() < 0.2, "{:?}", a.remaining);
}

#[test]
fn rate_cue_with_transposition_only_keeps_the_rate() {
    let song = sound("1", "len10000.wav");
    let id = song.id;
    let mut rig = Rig::new(vec![song, rate_cue("2", Targets::Cues(vec![id]), None, Some(3.0), 0.5)]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::Go);
    rig.run(1000);
    let a = active_state(&rig, "1").unwrap();
    assert_eq!(a.rate, 100.0);
    assert!((a.transpose - 3.0).abs() < 0.01, "{}", a.transpose);
    let notices = rig.runner.take_notices();
    assert!(!notices.iter().any(|n| matches!(n, Notice::Warning(w) if w.contains("pitch"))), "{notices:?}");
}

#[test]
fn rate_cue_on_a_varispeed_cue_moves_the_pitch_along() {
    let song = with_speed(sound("1", "len10000.wav"), 100.0, false, 0.0);
    let id = song.id;
    let mut rig = Rig::new(vec![song, rate_cue("2", Targets::Cues(vec![id]), Some(80.0), None, 0.2)]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(500);
    let a = active_state(&rig, "1").unwrap();
    assert!((a.rate - 80.0).abs() < 0.01);
    // The pitch follows the rate: no transposition beyond it.
    assert_eq!(a.transpose, 0.0);
}

#[test]
fn rate_cue_follows_after_its_duration() {
    let song = sound("1", "len10000.wav");
    let id = song.id;
    let cue = with_advance(rate_cue("2", Targets::Cues(vec![id]), Some(80.0), None, 2.0), AdvanceAt::End);
    let mut rig = Rig::new(vec![song, cue, sound("3", "len200.wav")]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(1900);
    assert_eq!(rig.active(), ["1"]);
    rig.run(200);
    assert_eq!(rig.active(), ["1", "3"]);
}

#[test]
fn a_paused_rate_cue_holds_the_rate() {
    let song = with_speed(sound("1", "len10000.wav"), 100.0, false, 0.0);
    let id = song.id;
    let rate = rate_cue("2", Targets::Cues(vec![id]), Some(50.0), None, 1.0);
    let rate_id = rate.id;
    let mut rig = Rig::new(vec![song, rate]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(500);
    rig.act(Action::PauseCue(rate_id));
    rig.run(1000);
    let held = active_state(&rig, "1").unwrap().rate;
    assert!((held - 75.0).abs() < 2.0, "{held}");
    assert!(!active_state(&rig, "1").unwrap().paused, "the target plays on");
    rig.act(Action::ResumeCue(rate_id));
    rig.run(1000);
    assert_eq!(active_state(&rig, "1").unwrap().rate, 50.0);
}

#[test]
fn the_next_playlist_item_starts_at_the_rate_set() {
    let list = playlist("1", &["len500.wav", "len500.wav"], 0.0, false);
    let id = list.id;
    let mut rig = Rig::new(vec![list, rate_cue("2", Targets::Cues(vec![id]), Some(80.0), None, 0.0)]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(700);
    let a = active_state(&rig, "1").unwrap();
    assert_eq!(a.item.map(|i| i.index), Some(1));
    assert!((a.rate - 80.0).abs() < 0.01, "{}", a.rate);
}

#[test]
fn reset_brings_the_rate_back() {
    let song = sound("1", "len10000.wav");
    let id = song.id;
    let mut rig = Rig::new(vec![
        song,
        rate_cue("2", Targets::Cues(vec![id]), Some(70.0), Some(-2.0), 0.0),
        control("3", CueKind::Reset(ResetCue { targets: Targets::Cues(vec![id]), duration: 0.1 })),
    ]);
    rig.act(Action::Go);
    rig.act(Action::Go);
    rig.run(500);
    assert!((active_state(&rig, "1").unwrap().rate - 70.0).abs() < 0.01);
    rig.act(Action::Go);
    rig.run(500);
    let a = active_state(&rig, "1").unwrap();
    assert_eq!((a.rate, a.transpose), (100.0, 0.0));
}

#[test]
fn rate_edit_applies_to_the_running_cue_only_where_it_changed() {
    let song = sound("1", "len10000.wav");
    let id = song.id;
    let mut rig = Rig::new(vec![
        song,
        sound("2", "len10000.wav"),
        rate_cue("3", Targets::Cues(vec![id]), Some(60.0), None, 0.0),
    ]);
    rig.act(Action::Go);
    rig.run(100);
    let rate_id = rig.runner.show().cues[2].id;
    rig.act(Action::StartCue(rate_id));
    rig.run(100);
    // An edit at another cue leaves the rate cue's work alone.
    edit_sound(&mut rig, "2", |s| s.volume_db = -3.0);
    rig.run(200);
    assert!((active_state(&rig, "1").unwrap().rate - 60.0).abs() < 0.01);
    // Keeping the pitch on the cue itself keeps the rate the rate cue set.
    edit_sound(&mut rig, "1", |s| s.speed.transpose = 2.0);
    rig.run(200);
    let a = active_state(&rig, "1").unwrap();
    assert!((a.rate - 60.0).abs() < 0.01, "{}", a.rate);
    assert!((a.transpose - 2.0).abs() < 0.01, "{}", a.transpose);
    // Its own rate replaces the rate cue's.
    edit_sound(&mut rig, "1", |s| s.speed.rate = 90.0);
    rig.run(200);
    assert!((active_state(&rig, "1").unwrap().rate - 90.0).abs() < 0.01);
}

#[test]
fn too_many_stretched_cues_play_varispeed_and_warn() {
    let cues: Vec<Cue> =
        (1..=5).map(|n| with_speed(sound(&n.to_string(), "len5000.wav"), 90.0, true, 0.0)).collect();
    let mut rig = Rig::new(cues);
    for _ in 0..5 {
        rig.act(Action::Go);
    }
    rig.run(100);
    let warnings: Vec<String> = rig
        .runner
        .take_notices()
        .into_iter()
        .filter_map(|n| match n {
            Notice::Warning(w) if w.contains("pitch") => Some(w),
            _ => None,
        })
        .collect();
    assert_eq!(
        warnings,
        ["Cue 5: too many cues with their own pitch at once – plays without pitch correction"]
    );
}

#[test]
fn remote_rate_is_clamped_and_can_be_relative() {
    let song = sound("1", "len10000.wav");
    let id = song.id;
    let mut rig = Rig::new(vec![song]);
    rig.act(Action::Go);
    rig.run(100);
    let now = rig.now();
    let rate =
        |rate, relative| Control::Rate { cue: id, rate: Some(rate), transpose: None, relative, fade: 0.0 };
    rig.runner.control(rate(200.0, false), now);
    rig.run(500);
    assert!((active_state(&rig, "1").unwrap().rate - 150.0).abs() < 0.01);
    rig.runner.control(rate(90.0, false), rig.now());
    rig.runner.control(rate(-10.0, true), rig.now());
    rig.run(500);
    assert!((active_state(&rig, "1").unwrap().rate - 80.0).abs() < 0.01);
    let transpose = Control::Rate { cue: id, rate: None, transpose: Some(-5.0), relative: false, fade: 0.0 };
    rig.runner.control(transpose, rig.now());
    rig.run(500);
    let a = active_state(&rig, "1").unwrap();
    assert!((a.rate - 80.0).abs() < 0.01 && (a.transpose + 5.0).abs() < 0.01, "{a:?}");
}

#[test]
fn cancel_follow_breaks_off_the_chain_and_the_cue_plays_on() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len1000.wav"), AdvanceAt::After { secs: 0.5 }),
        with_advance(sound("2", "len1000.wav"), AdvanceAt::End),
        sound("3", "len500.wav"),
    ]);
    rig.act(Action::Go);
    rig.run(100);
    rig.act(Action::CancelFollow(id_of(&rig, "1")));
    assert!(following(&rig).is_empty());
    rig.run(600);
    assert_eq!(rig.active(), ["1"]);
    // At the end it starts nothing either.
    rig.act(Action::StartCue(id_of(&rig, "2")));
    rig.run(100);
    rig.act(Action::CancelFollow(id_of(&rig, "2")));
    rig.run(1100);
    assert!(rig.active().is_empty(), "{:?}", rig.active());
}

fn scene(title: &str) -> Cue {
    let mut c = Cue::new(CueKind::Scene(SceneCue {}));
    c.title = title.into();
    c
}

#[test]
fn the_playhead_passes_over_scene_markers() {
    let cues = vec![
        scene("Akt 1"),
        sound("1", "len200.wav"),
        scene("Akt 2"),
        scene("Szene 1"),
        sound("2", "len200.wav"),
    ];
    let (act2, last) = (cues[2].id, cues[3].id);
    let mut rig = Rig::new(cues);
    assert_eq!(rig.playhead().as_deref(), Some("1"), "not on the first marker");
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("2"), "past both markers");
    rig.act(Action::PlayheadPrev);
    assert_eq!(rig.playhead().as_deref(), Some("1"));
    rig.act(Action::PlayheadPrev);
    assert_eq!(rig.playhead().as_deref(), Some("1"), "nothing but a marker before it");
    rig.act(Action::PlayheadNext);
    assert_eq!(rig.playhead().as_deref(), Some("2"));
    rig.act(Action::SetPlayhead(act2));
    assert_eq!(rig.playhead().as_deref(), Some("2"), "a marker hands on to the cue after it");
    rig.runner.take_notices();
    rig.act(Action::StartCue(last));
    rig.run(20);
    assert_eq!(rig.active(), ["1"], "starting a marker does nothing");
    assert!(rig.runner.take_notices().is_empty());

    let mut rig = Rig::new(vec![sound("1", "len200.wav"), scene("Ende")]);
    rig.act(Action::Go);
    assert_eq!(rig.playhead(), None, "a marker at the end is not stood on");
}

#[test]
fn chains_run_on_past_scene_markers() {
    let mut rig = Rig::new(vec![
        with_advance(sound("1", "len500.wav"), AdvanceAt::Start),
        scene("Akt 2"),
        with_advance(sound("2", "len100.wav"), AdvanceAt::End),
        scene("Szene 2"),
        sound("3", "len500.wav"),
        sound("4", "len500.wav"),
    ]);
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("4"), "past the whole chain");
    rig.run(20);
    assert_eq!(rig.active(), ["1", "2"]);
    rig.run(150);
    assert_eq!(rig.active(), ["1", "3"]);
}

#[test]
fn a_go_to_on_a_scene_marker_goes_to_the_cue_after_it() {
    let act = scene("Akt 2");
    let target = act.id;
    let go_to = |start| {
        let mut c = control("1", CueKind::GoTo(GoToCue { target: Some(target), start }));
        c.title = "Gehe zu".into();
        c
    };
    let mut rig =
        Rig::new(vec![go_to(false), sound("2", "len500.wav"), act.clone(), sound("3", "len500.wav")]);
    rig.act(Action::Go);
    assert_eq!(rig.playhead().as_deref(), Some("3"));
    let mut rig = Rig::new(vec![go_to(true), sound("2", "len500.wav"), act, sound("3", "len500.wav")]);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["3"]);
    assert_eq!(rig.playhead(), None);
}

#[test]
fn a_placeholder_plays_nothing_and_keeps_the_chain() {
    let mut placeholder = with_advance(sound("1", ""), AdvanceAt::End);
    placeholder.title = "Gewitter".into();
    let mut rig = Rig::new(vec![placeholder, sound("2", "len500.wav")]);
    let (first, second) = (rig.runner.show().cues[0].id, rig.runner.show().cues[1].id);
    assert!(rig.runner.is_armed(first), "nothing to load");
    rig.wait_armed(second);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(rig.active(), ["2"]);
    let notices = rig.runner.take_notices();
    assert_eq!(notices, [Notice::Warning("Cue 1 “Gewitter”: placeholder without a file".into())]);
    assert!(rig.loader.loads.lock().unwrap().iter().all(|p| p.ends_with("len500.wav")), "loads no file");
}

#[test]
fn a_countdown_counts_down_into_overtime() {
    let mut rig = Rig::new(vec![control(
        "1",
        CueKind::Stopwatch(StopwatchCue { watch: Some("p".into()), action: StopwatchAction::Restart }),
    )]);
    let mut show = rig.runner.show().clone();
    show.stopwatches.push(Stopwatch { id: "p".into(), name: "Pause".into(), countdown: Some(1.0) });
    show.stopwatches.push(Stopwatch { id: "w".into(), name: "Vorstellung".into(), countdown: None });
    rig.runner.set_show(show);
    let watch = |rig: &Rig, i: usize| rig.runner.state(rig.now()).stopwatches[i].clone();
    assert_eq!(watch(&rig, 0).remaining, Some(1.0), "stands at its start time");
    assert_eq!(watch(&rig, 1).remaining, None, "counts up");
    rig.act(Action::Go);
    rig.run(400);
    assert!((watch(&rig, 0).remaining.unwrap() - 0.6).abs() < 1e-6);
    rig.run(1000);
    assert!((watch(&rig, 0).remaining.unwrap() + 0.4).abs() < 1e-6, "runs on past 0");
    assert!(watch(&rig, 0).running);
    let now = rig.now();
    rig.runner.stopwatch("p", StopwatchAction::Reset, now);
    assert_eq!(watch(&rig, 0).remaining, Some(1.0));
}

fn edit_playlist(rig: &mut Rig, number: &str, f: impl FnOnce(&mut PlaylistCue)) {
    let mut cue = rig.runner.show().cues.iter().find(|c| c.number == number).unwrap().clone();
    let CueKind::Playlist(p) = &mut cue.kind else { panic!("not a playlist cue") };
    f(p);
    assert!(rig.runner.live_edit(cue));
}

#[test]
fn live_edits_of_volume_and_rate_apply_to_a_running_playlist() {
    let mut rig = Rig::new(vec![playlist("1", &["len5000.wav"], 0.0, false)]);
    rig.act(Action::Go);
    rig.run(50);
    edit_playlist(&mut rig, "1", |p| p.volume_db = -12.0);
    rig.run(50);
    assert!((level_db(&rig) + 12.0).abs() < 0.01, "{}", level_db(&rig));
    edit_playlist(&mut rig, "1", |p| p.speed.rate = 80.0);
    rig.run(200);
    let a = active_state(&rig, "1").unwrap();
    assert!((a.rate - 80.0).abs() < 0.01, "{}", a.rate);
}

/// `list` (a playlist) with overrides for item `index`.
fn with_overrides(mut list: Cue, index: usize, o: ItemOverrides) -> Cue {
    if let CueKind::Playlist(p) = &mut list.kind {
        p.items[index].overrides = o;
    }
    list
}

#[test]
fn an_item_gain_plays_below_the_playlist_volume_through_edits_and_fades() {
    let mut list = playlist("1", &["len200.wav", "len300.wav", "len5000.wav"], 0.0, false);
    if let CueKind::Playlist(p) = &mut list.kind {
        p.volume_db = -3.0;
    }
    let gain = ItemOverrides { gain_db: -6.0, ..Default::default() };
    let list = with_overrides(with_overrides(list, 1, gain), 2, gain);
    let id = list.id;
    let mut down = fade("2", Targets::Cues(vec![id]), VolumeAction::FadeTo, Some(0.05));
    if let CueKind::Volume(f) = &mut down.kind {
        f.volume_db = Some(-10.0);
    }
    let mut rig = Rig::new(vec![list, down]);
    rig.act(Action::Go);
    rig.run(50);
    assert!((level_db(&rig) + 3.0).abs() < 0.01, "the first item plays the playlist's: {}", level_db(&rig));
    rig.run(200);
    assert_eq!(item(&rig), Some((1, 3)));
    assert!((level_db(&rig) + 9.0).abs() < 0.01, "the second 6 dB below: {}", level_db(&rig));
    edit_playlist(&mut rig, "1", |p| p.volume_db = -1.0);
    rig.run(50);
    assert!((level_db(&rig) + 7.0).abs() < 0.01, "an edit keeps it below: {}", level_db(&rig));
    rig.act(Action::Go);
    rig.run(100);
    assert!((level_db(&rig) + 16.0).abs() < 0.01, "and a fade: {}", level_db(&rig));
    rig.run(200);
    assert_eq!(item(&rig), Some((2, 3)));
    assert!((level_db(&rig) + 16.0).abs() < 0.01, "the next item starts at the fade's: {}", level_db(&rig));
    edit_playlist(&mut rig, "1", |p| p.items[2].overrides = ItemOverrides::default());
    rig.run(50);
    assert!((level_db(&rig) + 10.0).abs() < 0.01, "a removed gain applies live: {}", level_db(&rig));
}

#[test]
fn an_item_pan_replaces_the_playlists_until_a_pan_cue_moves_it() {
    let mut list = playlist("1", &["len200.wav", "len200.wav", "len2000.wav"], 0.0, false);
    if let CueKind::Playlist(p) = &mut list.kind {
        p.pan = -1.0;
    }
    // The same file twice: only the second entry has its own pan.
    let list = with_overrides(list, 1, ItemOverrides { pan: Some(1.0), ..Default::default() });
    let id = list.id;
    let mut rig = Rig::new(vec![list, pan_cue("2", Targets::Cues(vec![id]), 0.0, 0.0)]);
    rig.act(Action::Go);
    rig.run(20);
    assert_eq!(frame(&mut rig), [0.5, 0.0], "the first item at the playlist's pan");
    rig.run(200);
    assert_eq!(item(&rig), Some((1, 3)));
    assert_eq!(frame(&mut rig), [0.0, 0.5], "the second at its own");
    edit_playlist(&mut rig, "1", |p| p.items[1].overrides.pan = Some(0.0));
    rig.run(50);
    assert_eq!(frame(&mut rig), [0.5, 0.5], "an edited override moves the running item");
    edit_playlist(&mut rig, "1", |p| p.items[1].overrides.pan = Some(1.0));
    rig.run(50);
    rig.act(Action::Go);
    rig.run(300);
    assert_eq!(item(&rig), Some((2, 3)));
    assert_eq!(frame(&mut rig), [0.5, 0.5], "a pan cue's pan goes on to the next item");
}

#[test]
fn reset_brings_an_item_back_to_its_own_pan() {
    let list = playlist("1", &["len5000.wav"], 0.0, false);
    let id = list.id;
    let list = with_overrides(list, 0, ItemOverrides { pan: Some(1.0), ..Default::default() });
    let mut reset = Cue::new(CueKind::Reset(ResetCue { targets: Targets::Cues(vec![id]), duration: 0.0 }));
    reset.number = "3".into();
    let mut rig = Rig::new(vec![list, pan_cue("2", Targets::Cues(vec![id]), -1.0, 0.0), reset]);
    rig.act(Action::Go);
    rig.run(20);
    rig.act(Action::Go);
    rig.run(50);
    assert_eq!(frame(&mut rig), [0.5, 0.0]);
    rig.act(Action::Go);
    rig.run(200);
    assert_eq!(frame(&mut rig), [0.0, 0.5], "back to the item's pan, not the playlist's");
}

#[test]
fn an_item_plays_at_its_own_rate_and_fade_in() {
    let list = playlist("1", &["len200.wav", "len5000.wav"], 0.0, false);
    let slow = ItemOverrides { rate: Some(50.0), ..Default::default() };
    let list = with_overrides(list, 0, slow);
    let fade_in = ItemOverrides { fade_in: Some(1.0), ..Default::default() };
    let list = with_overrides(list, 1, fade_in);
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(300);
    assert_eq!(item(&rig), Some((0, 2)), "200 ms of file at 50 % last 400 ms");
    assert_eq!(active_state(&rig, "1").unwrap().rate, 50.0);
    edit_playlist(&mut rig, "1", |p| p.speed.rate = 120.0);
    rig.run(20);
    assert_eq!(active_state(&rig, "1").unwrap().rate, 50.0, "the playlist's rate does not apply to it");
    rig.run(150);
    assert_eq!(item(&rig), Some((1, 2)));
    rig.run(10);
    assert!((active_state(&rig, "1").unwrap().rate - 120.0).abs() < 0.01, "the next plays the playlist's");
    assert!(level_db(&rig) < -20.0, "fading in: {}", level_db(&rig));
}

#[test]
fn an_item_plays_from_its_own_start_to_its_own_end() {
    let list = playlist("1", &["len1000.wav", "len5000.wav"], 0.0, false);
    let list = with_overrides(list, 0, ItemOverrides { start: 0.3, end: Some(0.5), ..Default::default() });
    let list = with_overrides(list, 1, ItemOverrides { start: 4.0, ..Default::default() });
    let mut rig = Rig::new(vec![list]);
    rig.act(Action::Go);
    rig.run(150);
    assert_eq!(item(&rig), Some((0, 2)));
    rig.run(100);
    assert_eq!(item(&rig), Some((1, 2)), "the first item ends after its 200 ms");
    let remaining = active_state(&rig, "1").unwrap().remaining.unwrap();
    assert!(remaining < 1.0, "the second starts at 4 s: {remaining}");
    rig.run(1000);
    assert!(active_state(&rig, "1").is_none(), "and plays to its file's end");
}
