//! Global shortcuts: the actions keyboard and MIDI bindings trigger, the
//! default keys and the rules for binding them. Esc is always panic (fixed,
//! see [`PANIC_KEY`]); further keys and MIDI can be bound to it. The UI resolves keys itself
//! (it has the selection and must stop the browser's default at once); the
//! bindings and their checks live here.

use jq_i18n::t;
use jq_show::{Chord, CueId};
use serde::{Deserialize, Serialize};

use crate::runner::Action;

/// Actions that can be bound to keys and (those not on the selected cue)
/// to MIDI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlAction {
    Go,
    StopAll,
    Panic,
    FadeAll,
    PauseAll,
    ResumeAll,
    TogglePauseAll,
    PlayheadNext,
    PlayheadPrev,
    StopPreview,
    // On the selected cue (keys only):
    Preview,
    TogglePauseCue,
    StopCue,
    FadeCue,
    RestartCue,
    ExitLoopCue,
    NextItem,
    JumpBack,
    JumpForward,
    JumpEnd,
    VolumeUp,
    VolumeDown,
    PanLeft,
    PanRight,
    RateUp,
    RateDown,
}

impl ControlAction {
    /// Works on the selected cue, which only the UI knows: keys only.
    pub fn on_selected(self) -> bool {
        self.action(None).is_none()
    }

    /// The runner's action; `None` if it needs a cue and `selected` is none.
    pub fn action(self, selected: Option<CueId>) -> Option<Action> {
        use ControlAction as C;
        Some(match self {
            C::Go => Action::Go,
            C::StopAll => Action::StopAll,
            C::Panic => Action::Panic,
            C::FadeAll => Action::FadeAll,
            C::PauseAll => Action::PauseAll,
            C::ResumeAll => Action::ResumeAll,
            C::TogglePauseAll => Action::TogglePauseAll,
            C::PlayheadNext => Action::PlayheadNext,
            C::PlayheadPrev => Action::PlayheadPrev,
            C::StopPreview => Action::StopPreview,
            on_cue => {
                let cue = selected?;
                match on_cue {
                    C::Preview => Action::Preview(cue),
                    C::TogglePauseCue => Action::TogglePauseCue(cue),
                    C::StopCue => Action::StopCue(cue),
                    C::FadeCue => Action::FadeCue(cue),
                    C::RestartCue => Action::RestartCue(cue),
                    C::ExitLoopCue => Action::ExitLoopCue(cue),
                    C::NextItem => Action::NextItem(cue),
                    C::JumpBack => Action::JumpBack(cue),
                    C::JumpForward => Action::JumpForward(cue),
                    C::JumpEnd => Action::JumpEnd(cue),
                    C::VolumeUp => Action::VolumeUp(cue),
                    C::VolumeDown => Action::VolumeDown(cue),
                    C::PanLeft => Action::PanLeft(cue),
                    C::PanRight => Action::PanRight(cue),
                    C::RateUp => Action::RateUp(cue),
                    C::RateDown => Action::RateDown(cue),
                    _ => unreachable!("handled above"),
                }
            }
        })
    }

    /// Name for messages, in the current language.
    pub fn name(self) -> String {
        use ControlAction as C;
        match self {
            C::Go => t!("action.go"),
            C::StopAll => t!("action.stop_all"),
            C::Panic => t!("action.panic"),
            C::FadeAll => t!("action.fade_all"),
            C::PauseAll => t!("action.pause_all"),
            C::ResumeAll => t!("action.resume_all"),
            C::TogglePauseAll => t!("action.toggle_pause_all"),
            C::PlayheadNext => t!("action.playhead_next"),
            C::PlayheadPrev => t!("action.playhead_prev"),
            C::StopPreview => t!("action.stop_preview"),
            C::Preview => t!("action.preview"),
            C::TogglePauseCue => t!("action.toggle_pause_cue"),
            C::StopCue => t!("action.stop_cue"),
            C::FadeCue => t!("action.fade_cue"),
            C::RestartCue => t!("action.restart_cue"),
            C::ExitLoopCue => t!("action.exit_loop_cue"),
            C::NextItem => t!("action.next_item"),
            C::JumpBack => t!("action.jump_back"),
            C::JumpForward => t!("action.jump_forward"),
            C::JumpEnd => t!("action.jump_end"),
            C::VolumeUp => t!("action.volume_up"),
            C::VolumeDown => t!("action.volume_down"),
            C::PanLeft => t!("action.pan_left"),
            C::PanRight => t!("action.pan_right"),
            C::RateUp => t!("action.rate_up"),
            C::RateDown => t!("action.rate_down"),
        }
    }
}

/// A key bound to a global action.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub chord: Chord,
    pub action: ControlAction,
}

/// The key that always panics (also with Shift, like all fixed keys), besides the keys bound to [`ControlAction::Panic`].
pub const PANIC_KEY: &str = "Escape";

/// Keys per action at most (a fixed key such as [`PANIC_KEY`] does not count).
pub const KEYS_PER_ACTION: usize = 2;

/// The keys of a new machine (the transport keys from before they could be
/// changed).
pub fn default_keys() -> Vec<KeyBinding> {
    let bind = |code: &str, label: &str, action| KeyBinding { chord: Chord::new(code, label), action };
    vec![
        bind("Space", " ", ControlAction::Go),
        bind("KeyF", "f", ControlAction::FadeAll),
        bind("Pause", "Pause", ControlAction::TogglePauseAll),
        bind("KeyK", "k", ControlAction::TogglePauseCue),
        bind("KeyP", "p", ControlAction::Preview),
    ]
}

/// Letters that with Ctrl are the app's own commands (save, open, undo,
/// clipboard …), by the letter on the key, whatever its position.
const FIXED_CTRL_LETTERS: [&str; 9] = ["s", "n", "o", "z", "y", "c", "x", "v", "a"];
/// Panic and the keys of the list and of focus, with or without Shift.
const FIXED_CODES: [&str; 9] =
    [PANIC_KEY, "ArrowUp", "ArrowDown", "Home", "End", "Delete", "Tab", "Enter", "NumpadEnter"];

/// Keys the app keeps for itself (Esc always panics); they cannot be bound.
pub fn fixed_keys() -> Vec<Chord> {
    let ctrl = FIXED_CTRL_LETTERS.iter().map(|l| Chord::new(&format!("Key{}", l.to_uppercase()), l).ctrl());
    let plain = FIXED_CODES.iter().map(|c| Chord::new(c, ""));
    ctrl.chain(plain).collect()
}

/// True if the app keeps `chord` for itself (see [`fixed_keys`]).
pub fn is_fixed(chord: &Chord) -> bool {
    if chord.ctrl && !chord.alt {
        // The letter as recorded on this machine's layout; the code if there is none.
        let letter = if chord.label.chars().count() == 1 {
            chord.label.to_lowercase()
        } else {
            chord.code.strip_prefix("Key").unwrap_or("").to_lowercase()
        };
        if FIXED_CTRL_LETTERS.contains(&letter.as_str()) {
            return true;
        }
    }
    !chord.ctrl && !chord.alt && FIXED_CODES.contains(&chord.code.as_str())
}

/// The global action on `chord`, if any.
pub fn bound(keys: &[KeyBinding], chord: &Chord) -> Option<ControlAction> {
    keys.iter().find(|b| b.chord.same(chord)).map(|b| b.action)
}

/// Refuses bindings with an empty or fixed key, a key used twice, or more
/// than [`KEYS_PER_ACTION`] keys for one action.
pub fn check_keys(keys: &[KeyBinding]) -> Result<(), String> {
    for (i, b) in keys.iter().enumerate() {
        if b.chord.code.trim().is_empty() {
            return Err(t!("keys.key_missing", action = b.action.name()));
        }
        if is_fixed(&b.chord) {
            return Err(t!("keys.fixed", key = b.chord.text()));
        }
        if let Some(other) = keys[..i].iter().find(|o| o.chord.same(&b.chord)) {
            return Err(t!("keys.taken", key = b.chord.text(), action = other.action.name()));
        }
        if keys[..=i].iter().filter(|o| o.action == b.action).count() > KEYS_PER_ACTION {
            return Err(t!("keys.too_many", action = b.action.name(), max = KEYS_PER_ACTION));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_the_old_transport_keys() {
        let keys = default_keys();
        assert_eq!(check_keys(&keys), Ok(()));
        assert_eq!(bound(&keys, &Chord::new("Space", "")), Some(ControlAction::Go));
        assert_eq!(
            bound(&keys, &Chord::new("KeyF", "F")),
            Some(ControlAction::FadeAll),
            "label does not count"
        );
        assert_eq!(bound(&keys, &Chord::new("KeyF", "f").shift()), None, "modifiers count");
        assert!(keys.iter().all(|b| b.action != ControlAction::Panic), "Esc is fixed");
    }

    #[test]
    fn checks_refuse_bad_bindings() {
        let go = |chord: Chord| KeyBinding { chord, action: ControlAction::Go };
        let three = [go(Chord::new("F1", "")), go(Chord::new("F2", "")), go(Chord::new("F3", ""))];
        assert!(check_keys(&three).is_err());
        assert!(check_keys(&three[..2]).is_ok());
        assert!(check_keys(&[go(Chord::new("KeyS", "s").ctrl())]).is_err(), "Ctrl+S saves");
        // QWERTZ: the key labelled Z sits where QWERTY has Y.
        assert!(check_keys(&[go(Chord::new("KeyY", "z").ctrl())]).is_err(), "Ctrl+Z undoes");
        assert!(check_keys(&[go(Chord::new("KeyZ", "y").ctrl().shift())]).is_err());
        assert!(check_keys(&[go(Chord::new("KeyB", "b").ctrl())]).is_ok());
        assert!(check_keys(&[go(Chord::new("ArrowUp", ""))]).is_err());
        assert!(check_keys(&[go(Chord::new("ArrowUp", "").ctrl())]).is_ok());
        let panic = |chord: Chord| KeyBinding { chord, action: ControlAction::Panic };
        assert!(check_keys(&[panic(Chord::new(PANIC_KEY, "Escape"))]).is_err(), "Esc is panic anyway");
        assert!(check_keys(&[panic(Chord::new(PANIC_KEY, "").shift())]).is_err());
        let two = [panic(Chord::new("F11", "")), panic(Chord::new("F12", ""))];
        assert!(check_keys(&two).is_ok(), "two keys besides Esc");
        assert!(check_keys(&[go(Chord::new("", ""))]).is_err());
        let twice = [
            go(Chord::new("F1", "")),
            KeyBinding { chord: Chord::new("F1", ""), action: ControlAction::StopAll },
        ];
        assert!(check_keys(&twice).is_err());
    }

    #[test]
    fn selected_cue_actions_need_a_cue() {
        assert!(ControlAction::Preview.on_selected());
        assert!(!ControlAction::Go.on_selected());
        assert_eq!(ControlAction::Preview.action(None), None);
        let id = CueId::new();
        assert_eq!(ControlAction::RateUp.action(Some(id)), Some(Action::RateUp(id)));
        assert_eq!(ControlAction::Go.action(Some(id)), Some(Action::Go));
    }
}
