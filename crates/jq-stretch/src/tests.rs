use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::f32::consts::TAU;

use super::*;

/// Counts allocations of the current thread while armed, so tests running
/// in parallel do not disturb each other.
struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(Cell::get) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ARMED.with(Cell::get) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.with(Cell::get) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// Allocations (and frees) `f` makes on this thread.
fn allocations(f: impl FnOnce()) -> usize {
    COUNT.with(|c| c.set(0));
    ARMED.with(|a| a.set(true));
    f();
    ARMED.with(|a| a.set(false));
    COUNT.with(Cell::get)
}

const RATE: u32 = 48_000;

fn sine(freq: f32, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let s = (TAU * freq * i as f32 / RATE as f32).sin() * 0.5;
            [s, s]
        })
        .collect()
}

/// Frequency of the left channel from its rising zero crossings.
fn frequency(samples: &[f32]) -> f32 {
    let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let crossings: Vec<usize> = (1..left.len()).filter(|&i| left[i - 1] < 0.0 && left[i] >= 0.0).collect();
    let (first, last) = (crossings[0], crossings[crossings.len() - 1]);
    (crossings.len() - 1) as f32 * RATE as f32 / (last - first) as f32
}

/// Runs `input` through in blocks of `block` output frames at time ratio `rate`.
fn run(s: &mut Stretcher, input: &[f32], rate: f64, block: usize) -> Vec<f32> {
    let frames = input.len() / CHANNELS;
    let mut out = Vec::new();
    let mut buf = vec![0.0; block * CHANNELS];
    let (mut pos, mut acc) = (0usize, 0.0f64);
    while pos < frames {
        acc += block as f64 * rate;
        let take = (acc as usize).min(frames - pos);
        acc -= take as f64;
        s.process(&input[pos * CHANNELS..(pos + take) * CHANNELS], &mut buf);
        out.extend_from_slice(&buf);
        pos += take;
    }
    out
}

#[test]
fn band_count_is_even_for_usual_rates() {
    // Mirrors the FFT size choice in signalsmith-linear (stft.h, fft.h).
    fn complex_fast_size_above(size: usize) -> usize {
        let mut pow2 = 1;
        while pow2 < 16 && pow2 < size {
            pow2 *= 2;
        }
        while pow2 * 8 < size {
            pow2 *= 2;
        }
        let mut multiple = size.div_ceil(pow2);
        if multiple == 7 {
            multiple += 1;
        }
        multiple * pow2
    }
    let real_fast_size_above = |size: usize| complex_fast_size_above(size.div_ceil(2)) * 2;
    for rate in [44_100.0, 48_000.0, 88_200.0, 96_000.0, 192_000.0] {
        for block in [rate * 0.12, rate * 0.1] {
            let block = block as usize;
            let fft = real_fast_size_above(block.div_ceil(2)) * 2;
            let bands = fft / 2;
            assert_eq!(bands % 2, 0, "{rate} Hz, block {block}: {bands} bands");
        }
    }
}

#[test]
fn process_never_allocates() {
    let mut s = Stretcher::new(RATE);
    let mut noise_state = 1u32;
    let noise: Vec<f32> = (0..RATE as usize * 2 * CHANNELS)
        .map(|_| {
            noise_state = noise_state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (noise_state >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        })
        .collect();
    // Every other band strong: as many spectral peaks as possible.
    let comb: Vec<f32> = (0..RATE as usize * CHANNELS)
        .map(|i| {
            let t = (i / CHANNELS) as f32 / RATE as f32;
            (1..200).step_by(2).map(|k| (TAU * 200.0 * k as f32 * t).sin()).sum::<f32>() * 0.005
        })
        .collect();
    let tone = sine(440.0, RATE as usize);
    let silence = vec![0.0; RATE as usize * CHANNELS];
    let mut input = vec![0.0; 2048 * CHANNELS];
    let mut output = vec![0.0; 64 * CHANNELS];
    let count = allocations(|| {
        for (n, material) in [&noise, &comb, &tone, &silence, &noise].into_iter().enumerate() {
            s.reset();
            s.seek(&material[..4096 * CHANNELS], 1.0);
            s.set_pitch([1.0, 1.5, 0.7, 2.0, 1.0][n]);
            let frames = material.len() / CHANNELS;
            let mut pos = 0;
            for block in 0.. {
                let rate = [0.5, 1.0, 1.5, 0.25, 4.0][block % 5];
                let take = ((64.0 * rate) as usize).min(input.len() / CHANNELS);
                if pos + take > frames {
                    break;
                }
                input[..take * CHANNELS].copy_from_slice(&material[pos * CHANNELS..(pos + take) * CHANNELS]);
                s.process(&input[..take * CHANNELS], &mut output);
                pos += take;
            }
        }
    });
    assert_eq!(count, 0);
}

#[test]
fn pitch_factor_two_is_an_octave_up() {
    let mut s = Stretcher::new(RATE);
    s.set_pitch(2.0);
    let out = run(&mut s, &sine(440.0, RATE as usize * 2), 1.0, 256);
    let settled = &out[RATE as usize / 2 * CHANNELS..RATE as usize * 3 / 2 * CHANNELS];
    let f = frequency(settled);
    assert!((f - 880.0).abs() < 880.0 * 0.01, "{f} Hz");
}

#[test]
fn time_ratio_without_pitch_change() {
    let mut s = Stretcher::new(RATE);
    let input = sine(440.0, RATE as usize);
    let out = run(&mut s, &input, 0.5, 256);
    // Half speed: one second of input lasts two.
    let frames = out.len() / CHANNELS;
    assert!((frames as i64 - 2 * RATE as i64).abs() <= 512, "{frames} frames");
    let settled = &out[RATE as usize / 2 * CHANNELS..RATE as usize * 3 / 2 * CHANNELS];
    let f = frequency(settled);
    assert!((f - 440.0).abs() < 440.0 * 0.01, "{f} Hz");
}

#[test]
fn silence_stays_silent_and_finite() {
    let mut s = Stretcher::new(RATE);
    s.set_pitch(1.3);
    let out = run(&mut s, &vec![0.0; RATE as usize * CHANNELS], 0.8, 128);
    assert!(out.iter().all(|x| x.is_finite() && x.abs() < 1e-6));
}

#[test]
fn bad_pitch_factors_mean_unchanged() {
    let mut s = Stretcher::new(RATE);
    for factor in [f32::NAN, 0.0, -1.0, f32::INFINITY] {
        s.set_pitch(factor);
    }
    let out = run(&mut s, &sine(440.0, RATE as usize), 1.0, 256);
    let settled = &out[RATE as usize / 4 * CHANNELS..RATE as usize * 3 / 4 * CHANNELS];
    let f = frequency(settled);
    assert!((f - 440.0).abs() < 440.0 * 0.01, "{f} Hz");
}
