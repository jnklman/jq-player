//! What the remote page and the server say to each other: JSON text frames
//! over the WebSocket (see `app/src/remote/socket.ts` for the other side).
//!
//! Server → client: `hello {instance, client, name, role, keys}` once,
//! `role {role}` when the host changes it, `keys {keys}` when the device's
//! own keys change, `event {name, payload}` (`show`,
//! `state`, `language_changed`), `reply {id, ok | err}` and `ping` every
//! second. Client → server: `call {id, cmd, args}`. Which commands a client
//! may call depends on its role and is checked here, not in the page.

use jq_core::Action;
use jq_core::keys::{ControlAction, KeyBinding, check_keys};
use jq_show::StopwatchAction;
use serde::{Deserialize, Serialize};

/// What a device may do. Ordered: each role may do what the one before may.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Asked to join, not let in yet: sees nothing.
    Pending,
    /// Sees the show (cue list, running cues, stopwatches).
    View,
    /// Also operates it: GO, stops, playhead, stopwatches.
    Control,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Call {
        id: u64,
        cmd: String,
        #[serde(default)]
        args: serde_json::Value,
    },
}

/// A command a client may call, checked against its role.
#[derive(Debug, PartialEq)]
pub enum Call {
    GetShow,
    GetLanguage,
    Action(Action),
    Stopwatch(String, StopwatchAction),
    /// The device's own keys; `None` = the host's again.
    SetOwnKeys(Option<Vec<KeyBinding>>),
}

/// The global actions a device's keys may trigger: those of the whole show,
/// none on a selection, of the editor or of the host's preview (mirrors
/// `REMOTE_ACTIONS` in `app/src/remote/keys.ts`).
pub const REMOTE_ACTIONS: [ControlAction; 9] = [
    ControlAction::Go,
    ControlAction::StopAll,
    ControlAction::Panic,
    ControlAction::FadeAll,
    ControlAction::PauseAll,
    ControlAction::ResumeAll,
    ControlAction::TogglePauseAll,
    ControlAction::PlayheadNext,
    ControlAction::PlayheadPrev,
];

/// Refuses keys for other actions than [`REMOTE_ACTIONS`], and what the
/// host's own keys may not be either (fixed keys, a key used twice …).
pub fn check_remote_keys(keys: &[KeyBinding]) -> Result<(), String> {
    if let Some(other) = keys.iter().find(|b| !REMOTE_ACTIONS.contains(&b.action)) {
        return Err(jq_i18n::t!("remote.key_action", action = other.action.name()));
    }
    check_keys(keys)
}

#[derive(Deserialize)]
struct ActionArgs {
    action: Action,
}

#[derive(Deserialize)]
struct StopwatchArgs {
    id: String,
    action: StopwatchAction,
}

#[derive(Deserialize)]
struct KeysArgs {
    keys: Option<Vec<KeyBinding>>,
}

/// The least role that may call `cmd`; `None` = no remote client may.
pub fn required_role(cmd: &str) -> Option<Role> {
    match cmd {
        "get_show" | "get_language" => Some(Role::View),
        "action" | "stopwatch" | "set_own_keys" => Some(Role::Control),
        _ => None,
    }
}

/// Decodes the call `cmd` with `args` if `role` may make it.
pub fn decode(role: Role, cmd: &str, args: serde_json::Value) -> Result<Call, String> {
    let allowed = required_role(cmd).is_some_and(|needed| role >= needed);
    if !allowed {
        return Err(jq_i18n::t!("remote.not_allowed"));
    }
    let bad = |e: serde_json::Error| jq_i18n::t!("remote.bad_call", error = e);
    Ok(match cmd {
        "get_show" => Call::GetShow,
        "get_language" => Call::GetLanguage,
        "action" => {
            let ActionArgs { action } = serde_json::from_value(args).map_err(bad)?;
            // The preview output is the host's headphones, not the show.
            if matches!(action, Action::Preview(_) | Action::StopPreview) {
                return Err(jq_i18n::t!("remote.not_allowed"));
            }
            Call::Action(action)
        }
        "stopwatch" => {
            let StopwatchArgs { id, action } = serde_json::from_value(args).map_err(bad)?;
            Call::Stopwatch(id, action)
        }
        "set_own_keys" => {
            let KeysArgs { keys } = serde_json::from_value(args).map_err(bad)?;
            if let Some(keys) = &keys {
                check_remote_keys(keys)?;
            }
            Call::SetOwnKeys(keys)
        }
        _ => return Err(jq_i18n::t!("remote.not_allowed")),
    })
}

/// A text frame of a server message.
pub fn hello(instance: &str, client: &str, name: &str, role: Role, keys: Option<&[KeyBinding]>) -> String {
    serde_json::json!({
        "type": "hello", "instance": instance, "client": client, "name": name, "role": role, "keys": keys,
    })
    .to_string()
}

/// The device's own keys changed (`null`: it uses the host's).
pub fn keys(keys: Option<&[KeyBinding]>) -> String {
    serde_json::json!({ "type": "keys", "keys": keys }).to_string()
}

pub fn role(role: Role) -> String {
    serde_json::json!({ "type": "role", "role": role }).to_string()
}

/// An event with its payload as JSON text, put in without parsing it again.
pub fn event(name: &str, payload: &str) -> String {
    format!(r#"{{"type":"event","name":{},"payload":{payload}}}"#, serde_json::Value::from(name))
}

pub fn reply(id: u64, result: Result<serde_json::Value, String>) -> String {
    match result {
        Ok(value) => serde_json::json!({ "type": "reply", "id": id, "ok": value }),
        Err(error) => serde_json::json!({ "type": "reply", "id": id, "err": error }),
    }
    .to_string()
}

pub const PING: &str = r#"{"type":"ping"}"#;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn each_role_may_call_only_its_commands() {
        let go = || json!({ "action": { "action": "go" } });
        let watch = || json!({ "id": "w", "action": "start" });
        // (command, args, pending, view, control)
        let table = [
            ("get_show", json!({}), false, true, true),
            ("get_language", json!({}), false, true, true),
            ("action", go(), false, false, true),
            ("stopwatch", watch(), false, false, true),
            ("set_own_keys", json!({ "keys": null }), false, false, true),
            ("set_settings", json!({}), false, false, false),
            ("edit", json!({ "op": { "op": "undo" } }), false, false, false),
            ("open_show", json!({ "path": "/x" }), false, false, false),
            ("live_cue", json!({}), false, false, false),
            ("preview_at", json!({}), false, false, false),
        ];
        for (cmd, args, pending, view, control) in table {
            for (role, allowed) in [(Role::Pending, pending), (Role::View, view), (Role::Control, control)] {
                assert_eq!(decode(role, cmd, args.clone()).is_ok(), allowed, "{cmd} as {role:?}");
            }
        }
    }

    #[test]
    fn calls_are_decoded() {
        let call = decode(Role::Control, "action", json!({ "action": { "action": "go" } }));
        assert_eq!(call, Ok(Call::Action(Action::Go)));
        let call = decode(Role::Control, "stopwatch", json!({ "id": "w", "action": "reset" }));
        assert_eq!(call, Ok(Call::Stopwatch("w".into(), StopwatchAction::Reset)));
        assert!(decode(Role::Control, "action", json!({ "action": { "action": "nonsense" } })).is_err());
    }

    #[test]
    fn the_hosts_preview_is_not_for_remote_clients() {
        let preview =
            json!({ "action": { "action": "preview", "cue": "00000000-0000-0000-0000-000000000000" } });
        assert!(decode(Role::Control, "action", preview).is_err());
        assert!(decode(Role::Control, "action", json!({ "action": { "action": "stop_preview" } })).is_err());
    }

    #[test]
    fn own_keys_only_for_the_shows_global_actions() {
        let keys =
            |action: &str, code: &str| json!({ "keys": [{ "chord": { "code": code }, "action": action }] });
        let call = decode(Role::Control, "set_own_keys", keys("go", "F1")).unwrap();
        let Call::SetOwnKeys(Some(bound)) = call else { panic!("{call:?}") };
        assert_eq!(bound[0].action, ControlAction::Go);
        assert!(decode(Role::Control, "set_own_keys", keys("playhead_prev", "F2")).is_ok());
        for action in ["preview", "stop_preview", "stop_cue", "focus_goto", "volume_up"] {
            assert!(decode(Role::Control, "set_own_keys", keys(action, "F1")).is_err(), "{action}");
        }
        assert!(decode(Role::Control, "set_own_keys", keys("go", "Enter")).is_err(), "a fixed key");
        assert_eq!(
            decode(Role::Control, "set_own_keys", json!({ "keys": null })),
            Ok(Call::SetOwnKeys(None))
        );
        assert!(decode(Role::View, "set_own_keys", keys("go", "F1")).is_err());
    }

    #[test]
    fn frames_are_json() {
        let parsed: serde_json::Value = serde_json::from_str(&event("state", r#"{"a":1}"#)).unwrap();
        assert_eq!(parsed, json!({ "type": "event", "name": "state", "payload": { "a": 1 } }));
        let parsed: serde_json::Value = serde_json::from_str(&reply(3, Err("no".into()))).unwrap();
        assert_eq!(parsed, json!({ "type": "reply", "id": 3, "err": "no" }));
    }
}
