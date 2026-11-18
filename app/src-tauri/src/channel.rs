//! The way into the show thread's queue, for the Tauri commands and the
//! remote server alike: both send the same [`Request`]s, so a GO from a
//! tablet takes the same path as one from the keyboard.

use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use crate::backend::{self, Request};

/// Requests that take longer than this are reported as failed.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// A handle on the show thread's queue; clones send into the same queue.
#[derive(Clone)]
pub struct Backend(Sender<Request>);

impl Backend {
    pub fn new(tx: Sender<Request>) -> Self {
        Self(tx)
    }

    /// Queues a request without waiting (GO is fire-and-forget).
    pub fn send(&self, request: Request) -> Result<(), String> {
        self.0.send(request).map_err(|_| jq_i18n::t!("app.thread_ended"))
    }

    /// Sends a request and blocks until its reply, at most `timeout`; never
    /// call it on an async runtime's own threads.
    pub fn ask_blocking<T>(
        &self,
        make: impl FnOnce(backend::Reply<T>) -> Request,
        timeout: Duration,
    ) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        self.send(make(tx))?;
        rx.recv_timeout(timeout).map_err(|_| jq_i18n::t!("app.no_reply"))?
    }

    /// Sends a request and waits for its reply off Tauri's async runtime.
    pub async fn ask<T: Send + 'static>(
        &self,
        make: impl FnOnce(backend::Reply<T>) -> Request,
    ) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        self.send(make(tx))?;
        tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(REPLY_TIMEOUT))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|_| jq_i18n::t!("app.no_reply"))?
    }
}
