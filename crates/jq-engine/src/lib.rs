//! Realtime audio engine of jQ Player.
//!
//! The engine knows nothing about cues: it plays voices — a shared buffer
//! with position, gain ramp, loop (vamp) regions and marks, read at a rate
//! (varispeed, or through a stretcher with its own pitch) — and sums them
//! per output. The [`Controller`] talks to the [`Mixer`] in the audio callback
//! only through lock-free queues.

mod buffer;
mod mixer;
mod pool;
mod ramp;
mod resample;
pub mod stream;
mod voice;

pub use buffer::AudioBuffer;
pub use mixer::{
    Command, Controller, END_BLEND, EngineConfig, EngineError, Event, Held, Mixer, PARAM_SMOOTHING,
    PAUSE_FADE, RegionProblem, SWITCH_FADE, engine,
};
pub use pool::{prime_ratio, stretcher_pool_size};
pub use ramp::{Curve, Ramp, Shape, db_to_gain, gain_to_db};
pub use voice::{LoopExit, LoopPass, LoopRegion, VoiceId, VoiceParams};

#[cfg(test)]
mod tests;
