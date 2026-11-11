//! Prints what a MIDI input sends, e.g. to check a controller:
//! `cargo run -p jq-midi --example monitor -- "AG08"` (Ctrl+C ends).

use std::sync::mpsc;

fn main() {
    let wanted = std::env::args().nth(1).unwrap_or_default();
    let (tx, rx) = mpsc::channel();
    let input = match jq_midi::MidiIn::connect(&wanted, move |m| {
        let _ = tx.send(m.to_vec());
    }) {
        Ok(input) => input,
        Err(e) => return eprintln!("{e}"),
    };
    println!("höre auf {}", input.name());
    for message in rx {
        println!("{message:02x?}");
    }
}
