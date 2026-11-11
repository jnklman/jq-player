//! Keeps the machine and display awake while a show is open. Runs in the
//! core, not in the UI, so it holds even when the UI hangs.

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "error", rename_all = "snake_case")]
pub enum KeepAliveState {
    Off,
    Active,
    /// The inhibitor could not be set (e.g. no D-Bus service); shown as a
    /// clear warning instead of failing silently.
    Failed(String),
}

pub struct KeepAlive {
    guard: Option<keepawake::KeepAwake>,
    state: KeepAliveState,
}

impl KeepAlive {
    pub fn new() -> Self {
        Self { guard: None, state: KeepAliveState::Off }
    }

    pub fn state(&self) -> &KeepAliveState {
        &self.state
    }

    pub fn set_enabled(&mut self, enabled: bool) -> &KeepAliveState {
        if !enabled {
            self.guard = None;
            self.state = KeepAliveState::Off;
        } else if self.guard.is_none() {
            let result = keepawake::Builder::default()
                .display(true)
                .idle(true)
                .reason(jq_i18n::t!("keepalive.reason"))
                .app_name("jQ Player")
                .app_reverse_domain("de.jqplayer.app")
                .create();
            match result {
                Ok(g) => {
                    self.guard = Some(g);
                    self.state = KeepAliveState::Active;
                }
                Err(e) => self.state = KeepAliveState::Failed(e.to_string()),
            }
        }
        &self.state
    }
}

impl Default for KeepAlive {
    fn default() -> Self {
        Self::new()
    }
}
