//! MIDI ports via midir. Incoming messages go straight to a callback (the
//! show thread's queue), so MIDI control never depends on the UI; MIDI cues
//! send through an output port.

use jq_i18n::t;
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

const CLIENT_NAME: &str = "jQ Player";

/// An open input port; dropping it closes the port.
pub struct MidiIn {
    _connection: MidiInputConnection<()>,
    name: String,
}

impl MidiIn {
    /// Opens the first input port whose name contains `wanted` (ignoring
    /// case); every message is passed to `on_message`.
    pub fn connect(wanted: &str, mut on_message: impl FnMut(&[u8]) + Send + 'static) -> Result<Self, String> {
        let mut input = MidiInput::new(CLIENT_NAME).map_err(|e| t!("midi.unavailable", error = e))?;
        // Clock and active sensing would only wake the show thread; SysEx
        // carries MIDI Show Control.
        input.ignore(Ignore::TimeAndActiveSense);
        let needle = wanted.to_lowercase();
        let (port, name) = input
            .ports()
            .into_iter()
            .find_map(|p| {
                let name = input.port_name(&p).ok()?;
                name.to_lowercase().contains(&needle).then_some((p, name))
            })
            .ok_or_else(|| t!("midi.no_input", name = wanted))?;
        let connection = input
            .connect(&port, "jq-in", move |_, message, _| on_message(message), ())
            .map_err(|e| t!("midi.input_error", name = name, error = e))?;
        Ok(Self { _connection: connection, name })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// An open output port; dropping it closes the port.
pub struct MidiOut {
    connection: MidiOutputConnection,
    name: String,
}

impl MidiOut {
    /// Opens the first output port whose name contains `wanted` (ignoring case).
    pub fn connect(wanted: &str) -> Result<Self, String> {
        let output = MidiOutput::new(CLIENT_NAME).map_err(|e| t!("midi.unavailable", error = e))?;
        let needle = wanted.to_lowercase();
        let (port, name) = output
            .ports()
            .into_iter()
            .find_map(|p| {
                let name = output.port_name(&p).ok()?;
                name.to_lowercase().contains(&needle).then_some((p, name))
            })
            .ok_or_else(|| t!("midi.no_output", name = wanted))?;
        let connection =
            output.connect(&port, "jq-out").map_err(|e| t!("midi.output_error", name = name, error = e))?;
        Ok(Self { connection, name })
    }

    pub fn send(&mut self, message: &[u8]) -> Result<(), String> {
        self.connection.send(message).map_err(|e| t!("midi.output_error", name = self.name, error = e))
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Names of all MIDI output ports.
pub fn output_ports() -> Result<Vec<String>, String> {
    let output = MidiOutput::new(CLIENT_NAME).map_err(|e| t!("midi.unavailable", error = e))?;
    Ok(output.ports().iter().filter_map(|p| output.port_name(p).ok()).collect())
}

/// Names of all MIDI input ports.
pub fn input_ports() -> Result<Vec<String>, String> {
    let input = MidiInput::new(CLIENT_NAME).map_err(|e| t!("midi.unavailable", error = e))?;
    Ok(input.ports().iter().filter_map(|p| input.port_name(p).ok()).collect())
}
