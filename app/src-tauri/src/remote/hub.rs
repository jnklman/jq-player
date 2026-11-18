//! Hands the show thread's events to the remote clients without ever making
//! it wait. Each event kind is a `watch` channel holding the latest frame: a
//! slow client skips states it could not send in time, and since a `show`
//! event carries the whole view (with its revision) nothing is lost by that.
//! Nothing is serialized while no client listens.

use std::sync::Arc;

use serde::Serialize;
use serde_json::value::RawValue;
use tokio::sync::watch;

use super::protocol;

/// The latest frame of one event kind; `None` until one came while a client
/// listened.
pub type Frame = Option<Arc<str>>;

#[derive(Clone)]
pub struct Hub(Arc<Feeds<watch::Sender<Frame>>>);

/// One of each event kind the clients get.
pub struct Feeds<T> {
    pub state: T,
    pub show: T,
    pub language: T,
}

impl Default for Hub {
    fn default() -> Self {
        let feed = || watch::Sender::new(None);
        Self(Arc::new(Feeds { state: feed(), show: feed(), language: feed() }))
    }
}

impl Hub {
    /// The runtime state, as sent to the windows (up to 20 times a second).
    pub fn state(&self, state: &impl Serialize) {
        feed(&self.0.state, "state", || serde_json::to_string(state).ok());
    }

    /// A show event, serialized once already for the windows.
    pub fn show(&self, event: &RawValue) {
        feed(&self.0.show, "show", || Some(event.get().to_owned()));
    }

    pub fn language(&self, info: &impl Serialize) {
        feed(&self.0.language, "language_changed", || serde_json::to_string(info).ok());
    }

    /// Receivers for a new client; they see the latest frames as unchanged.
    pub fn subscribe(&self) -> Feeds<watch::Receiver<Frame>> {
        Feeds {
            state: self.0.state.subscribe(),
            show: self.0.show.subscribe(),
            language: self.0.language.subscribe(),
        }
    }
}

/// Puts the event's frame into `sender` if anyone listens, else forgets the
/// last one: a client coming later asks for the show (and with it the state)
/// itself, and must not get a stale one first.
fn feed(sender: &watch::Sender<Frame>, name: &str, payload: impl FnOnce() -> Option<String>) {
    if sender.receiver_count() == 0 {
        if sender.borrow().is_some() {
            sender.send_replace(None);
        }
        return;
    }
    let frame = payload().map(|payload| Arc::from(protocol::event(name, &payload)));
    sender.send_replace(frame);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_serialized_without_listeners() {
        struct Panics;
        impl Serialize for Panics {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                panic!("serialized without a listener");
            }
        }
        let hub = Hub::default();
        hub.state(&Panics);
        assert!(hub.subscribe().state.borrow().is_none());
    }

    #[test]
    fn a_slow_receiver_gets_only_the_latest() {
        let hub = Hub::default();
        let mut feeds = hub.subscribe();
        for n in 0..100 {
            hub.state(&serde_json::json!({ "n": n }));
        }
        assert!(feeds.state.has_changed().unwrap());
        let frame = feeds.state.borrow_and_update().clone().unwrap();
        assert_eq!(&*frame, r#"{"type":"event","name":"state","payload":{"n":99}}"#);
        assert!(!feeds.state.has_changed().unwrap(), "the 99 before it are gone");
    }

    #[test]
    fn a_frame_from_before_the_last_listener_left_is_dropped() {
        let hub = Hub::default();
        let feeds = hub.subscribe();
        hub.state(&1);
        drop(feeds);
        hub.state(&2);
        assert!(hub.subscribe().state.borrow().is_none());
    }
}
