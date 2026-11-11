//! MIDI control: incoming messages become triggers, learned bindings map
//! triggers to actions, cues have triggers of their own, and MIDI Show
//! Control addresses cues by number. The port itself is opened elsewhere
//! (`jq-midi`).

use jq_i18n::t;
use jq_show::{MidiMessage, MscCommand, Show};
use serde::{Deserialize, Serialize};

pub use jq_show::MidiTrigger;

use crate::keys::ControlAction;
use crate::runner::Action;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiBinding {
    pub trigger: MidiTrigger,
    pub action: ControlAction,
}

/// A control pressed or released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiEvent {
    pub trigger: MidiTrigger,
    pub pressed: bool,
}

/// Turns raw messages into presses and releases: Note On (velocity > 0) is
/// pressed, Note Off (or velocity 0) released; a CC is pressed when it rises
/// to 64 or above and released when it falls below, so a held button or a
/// fader fires only once; a program change is only pressed.
pub struct MidiDecoder {
    cc: [[u8; 128]; 16],
}

impl Default for MidiDecoder {
    fn default() -> Self {
        Self { cc: [[0; 128]; 16] }
    }
}

impl MidiDecoder {
    pub fn event(&mut self, message: &[u8]) -> Option<MidiEvent> {
        let (&status, data) = message.split_first()?;
        let channel = status & 0x0f;
        let (trigger, pressed) = match (status & 0xf0, data) {
            (0x90, &[note, velocity, ..]) => (MidiTrigger::Note { channel, note: note & 0x7f }, velocity > 0),
            (0x80, &[note, ..]) => (MidiTrigger::Note { channel, note: note & 0x7f }, false),
            (0xb0, &[controller, value, ..]) => {
                let controller = controller & 0x7f;
                let last = std::mem::replace(&mut self.cc[channel as usize][controller as usize], value);
                let trigger = MidiTrigger::Cc { channel, controller };
                match (last >= 64, value >= 64) {
                    (false, true) => (trigger, true),
                    (true, false) => (trigger, false),
                    _ => return None,
                }
            }
            (0xc0, &[program, ..]) => (MidiTrigger::Program { channel, program: program & 0x7f }, true),
            _ => return None,
        };
        Some(MidiEvent { trigger, pressed })
    }
}

/// The global action bound to `trigger`, if any.
pub fn bound(bindings: &[MidiBinding], trigger: MidiTrigger) -> Option<ControlAction> {
    bindings.iter().find(|b| b.trigger == trigger).map(|b| b.action)
}

/// What a MIDI event does: a globally bound control only its action (when
/// pressed), any other starts the main-list cues with that trigger, and
/// releasing it fires the release trigger of those that have one.
pub fn actions(show: &Show, bindings: &[MidiBinding], event: MidiEvent) -> Vec<Action> {
    if let Some(action) = bound(bindings, event.trigger) {
        return if event.pressed { action.action(None).into_iter().collect() } else { Vec::new() };
    }
    show.cues
        .iter()
        .filter(|c| c.midi_trigger == Some(event.trigger))
        .filter_map(|c| match event.pressed {
            true => Some(Action::StartCue(c.id)),
            false => c.release_trigger.then_some(Action::ReleaseCue(c.id)),
        })
        .collect()
}

/// Binds `trigger` to `action`, replacing the action's previous binding and
/// any other use of the trigger.
pub fn learn(bindings: &mut Vec<MidiBinding>, action: ControlAction, trigger: MidiTrigger) {
    bindings.retain(|b| b.action != action && b.trigger != trigger);
    bindings.push(MidiBinding { trigger, action });
}

/// A MIDI Show Control command that was received; cue numbers as sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MscIn {
    Go(Option<String>),
    Stop(Option<String>),
    Resume(Option<String>),
    AllOff,
}

/// The device id that addresses every device.
pub const MSC_ALL_CALL: u8 = 0x7f;

/// Reads `F0 7F <device> 02 <format> <command> [cue [00 list …]] F7` for
/// device `device` (or the all call), format "all" or sound (10–1F). The
/// list and path are left out: there is only one cue list.
pub fn msc(message: &[u8], device: u8) -> Option<MscIn> {
    let [0xf0, 0x7f, to, 0x02, format, command, rest @ ..] = message else { return None };
    let (&0xf7, data) = rest.split_last()? else { return None };
    if *to != device && *to != MSC_ALL_CALL {
        return None;
    }
    if !(*format == 0x7f || (0x10..=0x1f).contains(format)) {
        return None;
    }
    let number = data.split(|&b| b == 0).next().unwrap_or_default();
    let cue = match std::str::from_utf8(number) {
        Ok(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Ok(_) => None,
        Err(_) => return None,
    };
    Some(match command {
        0x01 => MscIn::Go(cue),
        0x02 => MscIn::Stop(cue),
        0x03 => MscIn::Resume(cue),
        0x08 => MscIn::AllOff,
        _ => return None,
    })
}

/// What an MSC command does: GO with a number moves the playhead to that
/// cue and goes; STOP pauses and RESUME resumes (one cue or all), as in the
/// standard; ALL_OFF stops everything. An unknown number is an error.
pub fn msc_actions(show: &Show, command: &MscIn) -> Result<Vec<Action>, String> {
    let find = |number: &str| {
        show.cues
            .iter()
            .find(|c| c.number.trim() == number)
            .map(|c| c.id)
            .ok_or_else(|| t!("midi.msc_no_cue", number = number))
    };
    Ok(match command {
        MscIn::Go(None) => vec![Action::Go],
        MscIn::Go(Some(n)) => vec![Action::SetPlayhead(find(n)?), Action::Go],
        MscIn::Stop(None) => vec![Action::PauseAll],
        MscIn::Stop(Some(n)) => vec![Action::PauseCue(find(n)?)],
        MscIn::Resume(None) => vec![Action::ResumeAll],
        MscIn::Resume(Some(n)) => vec![Action::ResumeCue(find(n)?)],
        MscIn::AllOff => vec![Action::StopAll],
    })
}

/// Where MIDI cues send to; a trait so tests can record.
pub trait MidiSink: Send {
    fn send(&mut self, message: &[u8]) -> Result<(), String>;
}

/// Raw bytes of an outgoing message; values out of range are refused.
pub fn encode(message: &MidiMessage) -> Result<Vec<u8>, String> {
    let data = |v: u8, what: String| {
        if v < 128 { Ok(v) } else { Err(t!("midi.out_of_range", what = what, value = v)) }
    };
    let channel = |c: u8| if c < 16 { Ok(c) } else { Err(t!("midi.bad_channel", channel = c as u16 + 1)) };
    Ok(match *message {
        MidiMessage::ProgramChange { channel: c, program } => {
            vec![0xc0 | channel(c)?, data(program, t!("midi.what.program"))?]
        }
        MidiMessage::ControlChange { channel: c, controller, value } => {
            vec![
                0xb0 | channel(c)?,
                data(controller, t!("midi.what.controller"))?,
                data(value, t!("midi.what.value"))?,
            ]
        }
        MidiMessage::NoteOn { channel: c, note, velocity } => {
            vec![
                0x90 | channel(c)?,
                data(note, t!("midi.what.note"))?,
                data(velocity, t!("midi.what.velocity"))?,
            ]
        }
        MidiMessage::NoteOff { channel: c, note } => {
            vec![0x80 | channel(c)?, data(note, t!("midi.what.note"))?, 0]
        }
        MidiMessage::Msc { device, command, ref cue, ref list } => {
            let number = |text: &str| {
                if text.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    Ok(text.as_bytes().to_vec())
                } else {
                    Err(t!("midi.msc_cue_digits", text = text))
                }
            };
            // F0 7F <device> 02 <format: all types> <command> [cue [00 list]] F7
            let mut bytes = vec![0xf0, 0x7f, data(device, t!("midi.what.msc_device"))?, 0x02, 0x7f];
            bytes.push(match command {
                MscCommand::Go => 0x01,
                MscCommand::Stop => 0x02,
                MscCommand::Resume => 0x03,
            });
            if !cue.is_empty() {
                bytes.extend(number(cue)?);
                if !list.is_empty() {
                    bytes.push(0x00);
                    bytes.extend(number(list)?);
                }
            }
            bytes.push(0xf7);
            bytes
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(trigger: MidiTrigger) -> Option<MidiEvent> {
        Some(MidiEvent { trigger, pressed: true })
    }

    fn release(trigger: MidiTrigger) -> Option<MidiEvent> {
        Some(MidiEvent { trigger, pressed: false })
    }

    #[test]
    fn notes_press_and_release() {
        let mut d = MidiDecoder::default();
        let note = MidiTrigger::Note { channel: 2, note: 60 };
        assert_eq!(d.event(&[0x92, 60, 100]), press(note));
        assert_eq!(d.event(&[0x92, 60, 0]), release(note), "Note On with velocity 0 is Note Off");
        assert_eq!(d.event(&[0x82, 60, 64]), release(note));
        assert_eq!(d.event(&[0x92]), None, "truncated");
        assert_eq!(d.event(&[]), None);
    }

    #[test]
    fn cc_presses_once_when_rising_past_64_and_releases_below() {
        let mut d = MidiDecoder::default();
        let cc = MidiTrigger::Cc { channel: 0, controller: 7 };
        let events: Vec<_> =
            [0, 50, 64, 90, 127, 10, 5, 127].iter().map(|&v| d.event(&[0xb0, 7, v])).collect();
        assert_eq!(events, [None, None, press(cc), None, None, release(cc), None, press(cc)]);
    }

    #[test]
    fn program_change_presses() {
        let mut d = MidiDecoder::default();
        assert_eq!(d.event(&[0xc3, 12]), press(MidiTrigger::Program { channel: 3, program: 12 }));
        assert_eq!(d.event(&[0xc3]), None);
    }

    #[test]
    fn learning_replaces_old_bindings() {
        let note = |n| MidiTrigger::Note { channel: 0, note: n };
        let mut bindings = Vec::new();
        learn(&mut bindings, ControlAction::Go, note(60));
        learn(&mut bindings, ControlAction::StopAll, note(61));
        assert_eq!(bound(&bindings, note(60)), Some(ControlAction::Go));
        // New key for GO: the old one is free again.
        learn(&mut bindings, ControlAction::Go, note(62));
        assert_eq!(bound(&bindings, note(60)), None);
        assert_eq!(bound(&bindings, note(62)), Some(ControlAction::Go));
        // A key used by another action moves to the learned one.
        learn(&mut bindings, ControlAction::FadeAll, note(61));
        assert_eq!(bound(&bindings, note(61)), Some(ControlAction::FadeAll));
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn cue_triggers_start_and_release_cues() {
        use jq_show::{Cue, CueKind, SoundCue};
        let note = MidiTrigger::Note { channel: 0, note: 60 };
        let mut show = Show::default();
        let mut cue = |release| {
            let mut c = Cue::new(CueKind::Sound(SoundCue::new("a.wav")));
            c.midi_trigger = Some(note);
            c.release_trigger = release;
            show.cues.push(c.clone());
            c.id
        };
        let (a, b) = (cue(false), cue(true));
        let event = |pressed| MidiEvent { trigger: note, pressed };
        assert_eq!(
            actions(&show, &[], event(true)),
            [Action::StartCue(a), Action::StartCue(b)],
            "both start"
        );
        assert_eq!(actions(&show, &[], event(false)), [Action::ReleaseCue(b)], "only the one with a release");
        // A global binding wins and starts no cue.
        let bindings = [MidiBinding { trigger: note, action: ControlAction::Go }];
        assert_eq!(actions(&show, &bindings, event(true)), [Action::Go]);
        assert_eq!(actions(&show, &bindings, event(false)), []);
    }

    #[test]
    fn reads_msc_as_it_is_sent() {
        let sent = |command, cue: &str| {
            encode(&MidiMessage::Msc { device: 1, command, cue: cue.into(), list: "2".into() }).unwrap()
        };
        assert_eq!(msc(&sent(MscCommand::Go, "12.5"), 1), Some(MscIn::Go(Some("12.5".into()))));
        assert_eq!(msc(&sent(MscCommand::Go, ""), 1), Some(MscIn::Go(None)));
        assert_eq!(msc(&sent(MscCommand::Stop, "3"), 1), Some(MscIn::Stop(Some("3".into()))));
        assert_eq!(msc(&sent(MscCommand::Resume, ""), 1), Some(MscIn::Resume(None)));
        assert_eq!(msc(&sent(MscCommand::Go, "1"), 2), None, "another device");
        let mut all_call = sent(MscCommand::Go, "1");
        all_call[2] = MSC_ALL_CALL;
        assert_eq!(msc(&all_call, 2), Some(MscIn::Go(Some("1".into()))));
        assert_eq!(msc(&[0xf0, 0x7f, 1, 0x02, 0x7f, 0x08, 0xf7], 1), Some(MscIn::AllOff));
        assert_eq!(msc(&[0xf0, 0x7f, 1, 0x02, 0x01, 0x01, 0xf7], 1), None, "lighting format");
        assert_eq!(msc(&[0xf0, 0x7f, 1, 0x02, 0x10, 0x01, 0xf7], 1), Some(MscIn::Go(None)), "sound format");
        assert_eq!(msc(&[0xf0, 0x7f, 1, 0x02, 0x7f, 0x01, b'1'], 1), None, "cut off");
        assert_eq!(msc(&[0xf0, 0x7f, 1, 0x02, 0x7f, 0x05, 0xf7], 1), None, "LOAD is ignored");
    }

    #[test]
    fn msc_finds_cues_by_number() {
        use jq_show::{Cue, CueKind, SoundCue};
        let mut show = Show::default();
        let mut cue = Cue::new(CueKind::Sound(SoundCue::new("a.wav")));
        cue.number = "3".into();
        let id = cue.id;
        show.cues.push(cue);
        let go = msc_actions(&show, &MscIn::Go(Some("3".into())));
        assert_eq!(go, Ok(vec![Action::SetPlayhead(id), Action::Go]));
        assert_eq!(msc_actions(&show, &MscIn::Stop(None)), Ok(vec![Action::PauseAll]));
        assert_eq!(msc_actions(&show, &MscIn::Resume(Some("3".into()))), Ok(vec![Action::ResumeCue(id)]));
        assert_eq!(msc_actions(&show, &MscIn::AllOff), Ok(vec![Action::StopAll]));
        assert_eq!(msc_actions(&show, &MscIn::Go(Some("4".into()))), Err("MSC: no cue 4".into()));
    }

    #[test]
    fn encodes_outgoing_messages() {
        let pc = MidiMessage::ProgramChange { channel: 2, program: 5 };
        assert_eq!(encode(&pc).unwrap(), [0xc2, 5]);
        let cc = MidiMessage::ControlChange { channel: 0, controller: 7, value: 100 };
        assert_eq!(encode(&cc).unwrap(), [0xb0, 7, 100]);
        assert_eq!(encode(&MidiMessage::NoteOff { channel: 15, note: 60 }).unwrap(), [0x8f, 60, 0]);
        let msc =
            MidiMessage::Msc { device: 1, command: MscCommand::Go, cue: "12.5".into(), list: "2".into() };
        assert_eq!(
            encode(&msc).unwrap(),
            [0xf0, 0x7f, 1, 0x02, 0x7f, 0x01, b'1', b'2', b'.', b'5', 0x00, b'2', 0xf7]
        );
        let next =
            MidiMessage::Msc { device: 127, command: MscCommand::Stop, cue: String::new(), list: "9".into() };
        assert_eq!(
            encode(&next).unwrap(),
            [0xf0, 0x7f, 127, 0x02, 0x7f, 0x02, 0xf7],
            "no list without a cue"
        );

        assert!(encode(&MidiMessage::ProgramChange { channel: 16, program: 0 }).is_err());
        assert!(encode(&MidiMessage::NoteOn { channel: 0, note: 128, velocity: 1 }).is_err());
        let bad =
            MidiMessage::Msc { device: 1, command: MscCommand::Go, cue: "1a".into(), list: String::new() };
        assert!(encode(&bad).is_err());
    }

    #[test]
    fn bindings_round_trip_as_json() {
        let b = MidiBinding {
            trigger: MidiTrigger::Cc { channel: 15, controller: 64 },
            action: ControlAction::Go,
        };
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(json, r#"{"trigger":{"kind":"cc","channel":15,"controller":64},"action":"go"}"#);
        assert_eq!(serde_json::from_str::<MidiBinding>(&json).unwrap(), b);
    }
}
