//! Latency and CPU of the stretcher: `cargo run --release -p jq-stretch --example measure`.
//!
//! Prints the latency of both presets and, per audio buffer size, the mean and
//! worst time one stretched stereo voice takes per callback, as a share of the
//! callback's real-time budget.

use std::time::Instant;

use jq_stretch::{CHANNELS, Stretcher};

const RATE: u32 = 48_000;

fn main() {
    let input: Vec<f32> =
        (0..RATE as usize * 4 * CHANNELS).map(|i| ((i as f32) * 0.013).sin() * 0.3).collect();
    let t = Instant::now();
    let pool: Vec<Stretcher> = (0..16).map(|_| Stretcher::new(RATE)).collect();
    println!("creating 16: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    drop(pool);
    println!("cost estimate: {:.2} % per voice", Stretcher::cost(RATE) * 100.0);
    for (name, make) in
        [("default", Stretcher::default_preset as fn(u32) -> Stretcher), ("cheaper", Stretcher::new)]
    {
        let s = make(RATE);
        println!(
            "{name}: input latency {:.1} ms, output latency {:.1} ms",
            s.input_latency() as f64 * 1e3 / RATE as f64,
            s.output_latency() as f64 * 1e3 / RATE as f64
        );
        for buffer in [128usize, 256, 512] {
            let mut s = make(RATE);
            s.set_pitch(1.1);
            let mut out = vec![0.0; 64 * CHANNELS];
            let (mut pos, mut acc) = (0usize, 0.0f64);
            let mut times = Vec::new();
            let budget = buffer as f64 / RATE as f64;
            while pos + 2 * buffer < input.len() / CHANNELS {
                let t = Instant::now();
                for _ in 0..buffer / 64 {
                    acc += 64.0 * 0.8;
                    let take = acc as usize;
                    acc -= take as f64;
                    s.process(&input[pos * CHANNELS..(pos + take) * CHANNELS], &mut out);
                    pos += take;
                }
                times.push(t.elapsed().as_secs_f64());
            }
            let mean = times.iter().sum::<f64>() / times.len() as f64;
            times.sort_by(f64::total_cmp);
            let p999 = times[times.len() * 999 / 1000];
            let worst = times[times.len() - 1];
            println!(
                "  {buffer:>3} frames: mean {:.1} %, 99.9 % {:.1} %, worst {:.1} % of the callback per voice",
                mean / budget * 100.0,
                p999 / budget * 100.0,
                worst / budget * 100.0
            );
        }
    }
}
