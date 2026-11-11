/// Shape of a gain ramp.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Curve {
    #[default]
    Linear,
    /// Smoothstep in amplitude: gentle start and end, steeper in the middle.
    SCurve,
    /// In dB, as the ear hears a fade, along `shape`. A ramp to silence ends
    /// at the gain `floor` (e.g. the show's minimum level) and steps to
    /// silence from there; one from silence starts there.
    Decibel { floor: f32, shape: Shape },
}

/// How a [`Curve::Decibel`] ramp moves through its dB range over time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    /// Even steps.
    #[default]
    Linear,
    /// Smoothstep: gentle start and end.
    S,
    /// Slow near the louder level, whichever way the ramp goes: a fade-out
    /// stays up long and then falls (t²), a fade-in comes up quickly and
    /// settles gently (t·(2 − t)).
    SlowTop,
    /// Slow near the quieter level: a fade-out drops first and glides into
    /// silence, a fade-in leaves silence slowly.
    SlowBottom,
}

impl Shape {
    /// Share of the way at `t` (0…1); `rising`: the ramp goes up.
    #[inline]
    fn at(self, t: f32, rising: bool) -> f32 {
        let ease_in = || t * t;
        let ease_out = || t * (2.0 - t);
        match self {
            Shape::Linear => t,
            Shape::S => t * t * (3.0 - 2.0 * t),
            Shape::SlowTop if rising => ease_out(),
            Shape::SlowTop => ease_in(),
            Shape::SlowBottom if rising => ease_in(),
            Shape::SlowBottom => ease_out(),
        }
    }
}

impl Curve {
    /// [`Curve::Decibel`] with its floor at `db` (e.g. −60).
    pub fn decibel(db: f32, shape: Shape) -> Self {
        Curve::Decibel { floor: db_to_gain(db), shape }
    }
}

/// Floors outside this range are taken as its nearest end (−120 dB … −6 dB);
/// NaN as the default −60 dB.
fn sane_floor(floor: f32) -> f32 {
    if floor.is_nan() { 0.001 } else { floor.clamp(1e-6, 0.5) }
}

/// Gain that moves towards a target over a fixed number of frames.
/// Interpolated per sample, never per GUI frame.
#[derive(Clone, Copy, Debug)]
pub struct Ramp {
    from: f32,
    to: f32,
    total: u32,
    elapsed: u32,
    curve: Curve,
    /// Curves in dB: `ln(to / from)`, both at least the floor.
    log_ratio: f32,
}

impl Ramp {
    pub fn constant(gain: f32) -> Self {
        Self { from: gain, to: gain, total: 0, elapsed: 0, curve: Curve::Linear, log_ratio: 0.0 }
    }

    /// Starts a new ramp from the current value. `frames == 0` jumps immediately.
    pub fn start(&mut self, to: f32, frames: u32, curve: Curve) {
        let mut from = self.value();
        let mut curve = curve;
        let mut log_ratio = 0.0;
        if let Curve::Decibel { floor, shape } = curve {
            let floor = sane_floor(floor);
            if from < floor && to <= floor {
                // Below the floor all the way, inaudible: never up to the
                // floor (a silent cue would sound for the whole fade).
                curve = Curve::Linear;
            } else {
                curve = Curve::Decibel { floor, shape };
                from = from.max(floor);
                log_ratio = (to.max(floor) / from).ln();
            }
        }
        *self = Self { from, to, total: frames, elapsed: 0, curve, log_ratio };
    }

    #[inline]
    pub fn value(&self) -> f32 {
        if self.elapsed >= self.total {
            return self.to;
        }
        let t = self.elapsed as f32 / self.total as f32;
        match self.curve {
            Curve::Linear => self.from + (self.to - self.from) * t,
            Curve::SCurve => self.from + (self.to - self.from) * Shape::S.at(t, false),
            Curve::Decibel { shape, .. } => {
                self.from * (self.log_ratio * shape.at(t, self.log_ratio > 0.0)).exp()
            }
        }
    }

    /// Returns the gain for the current frame and steps one frame ahead.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let v = self.value();
        if self.elapsed < self.total {
            self.elapsed += 1;
        }
        v
    }

    pub fn is_done(&self) -> bool {
        self.elapsed >= self.total
    }

    pub fn target(&self) -> f32 {
        self.to
    }
}

/// Converts decibels to linear gain; anything at or below -120 dB is silence.
pub fn db_to_gain(db: f32) -> f32 {
    if db <= -120.0 { 0.0 } else { 10f32.powf(db / 20.0) }
}

/// Converts linear gain to decibels; silence is `-inf`.
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 { f32::NEG_INFINITY } else { 20.0 * gain.log10() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_ramp_hits_target_exactly() {
        let mut r = Ramp::constant(0.0);
        r.start(1.0, 4, Curve::Linear);
        let vals: Vec<f32> = (0..6).map(|_| r.tick()).collect();
        assert_eq!(vals, [0.0, 0.25, 0.5, 0.75, 1.0, 1.0]);
        assert!(r.is_done());
    }

    #[test]
    fn restart_continues_from_current_value() {
        let mut r = Ramp::constant(1.0);
        r.start(0.0, 4, Curve::Linear);
        r.tick();
        r.tick();
        r.start(1.0, 2, Curve::SCurve);
        assert_eq!(r.tick(), 0.5);
    }

    #[test]
    fn s_curve_is_symmetric() {
        let mut r = Ramp::constant(0.0);
        r.start(1.0, 2, Curve::SCurve);
        r.tick();
        assert!((r.value() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn decibel_ramp_moves_evenly_in_db() {
        let mut r = Ramp::constant(1.0);
        r.start(0.01, 4, Curve::decibel(-60.0, Shape::Linear));
        let db: Vec<f32> = (0..5).map(|_| gain_to_db(r.tick())).collect();
        for (got, want) in db.iter().zip([0.0, -10.0, -20.0, -30.0, -40.0]) {
            assert!((got - want).abs() < 1e-3, "{db:?}");
        }
    }

    #[test]
    fn decibel_ramp_reaches_and_leaves_silence_via_the_floor() {
        let mut r = Ramp::constant(1.0);
        r.start(0.0, 4, Curve::decibel(-60.0, Shape::Linear));
        let out: Vec<f32> = (0..5).map(|_| r.tick()).collect();
        assert!((gain_to_db(out[2]) + 30.0).abs() < 1e-3, "halfway to −60 dB: {out:?}");
        assert!((gain_to_db(out[3]) + 45.0).abs() < 1e-3, "{out:?}");
        assert_eq!(out[4], 0.0, "silence exactly at the end");

        let mut r = Ramp::constant(0.0);
        r.start(0.5, 2, Curve::decibel(-40.0, Shape::Linear));
        assert!((gain_to_db(r.tick()) + 40.0).abs() < 1e-3, "fade-in starts at the floor");
        r.tick();
        assert_eq!(r.tick(), 0.5);
    }

    #[test]
    fn decibel_s_curve_is_smoothstep_in_db() {
        let mut r = Ramp::constant(1.0);
        r.start(0.01, 4, Curve::decibel(-60.0, Shape::S));
        let db: Vec<f32> = (0..5).map(|_| gain_to_db(r.tick())).collect();
        // Smoothstep at 1/4, 1/2, 3/4: 0.15625, 0.5, 0.84375 of −40 dB.
        for (got, want) in db.iter().zip([0.0, -6.25, -20.0, -33.75, -40.0]) {
            assert!((got - want).abs() < 1e-3, "{db:?}");
        }
        let mut r = Ramp::constant(1.0);
        r.start(0.0, 2, Curve::decibel(-60.0, Shape::S));
        r.tick();
        assert!((gain_to_db(r.tick()) + 30.0).abs() < 1e-3, "halfway to the floor");
        assert_eq!(r.tick(), 0.0);
    }

    #[test]
    fn decibel_slow_top_and_bottom_follow_the_level_both_ways() {
        let db = |shape, from, to| {
            let mut r = Ramp::constant(from);
            r.start(to, 4, Curve::decibel(-60.0, shape));
            (0..5).map(|_| gain_to_db(r.tick())).collect::<Vec<f32>>()
        };
        // Between 0 and −40 dB: t² and t·(2 − t) at 1/4, 1/2, 3/4.
        let top_down = [0.0, -2.5, -10.0, -22.5, -40.0];
        let bottom_down = [0.0, -17.5, -30.0, -37.5, -40.0];
        // Up, slow near the top is the mirror image of down.
        let top_up = bottom_down.map(|d| -40.0 - d);
        let bottom_up = top_down.map(|d| -40.0 - d);
        for (shape, from, to, want) in [
            (Shape::SlowTop, 1.0, 0.01, top_down),
            (Shape::SlowBottom, 1.0, 0.01, bottom_down),
            (Shape::SlowTop, 0.01, 1.0, top_up),
            (Shape::SlowBottom, 0.01, 1.0, bottom_up),
        ] {
            let got = db(shape, from, to);
            assert!(
                got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-3),
                "{shape:?} {from}→{to}: {got:?}"
            );
        }
    }

    #[test]
    fn decibel_floor_is_kept_sane() {
        for (floor, curve) in [f32::NAN, 0.0, -1.0, f32::INFINITY].into_iter().flat_map(|f| {
            [Shape::Linear, Shape::S, Shape::SlowTop, Shape::SlowBottom]
                .map(|shape| (f, Curve::Decibel { floor: f, shape }))
        }) {
            let mut r = Ramp::constant(1.0);
            r.start(0.0, 4, curve);
            let out: Vec<f32> = (0..5).map(|_| r.tick()).collect();
            assert!(out.iter().all(|g| g.is_finite() && (0.0..=1.0).contains(g)), "{floor}: {out:?}");
            assert_eq!(out[4], 0.0);
        }
    }

    #[test]
    fn db_round_trip() {
        assert_eq!(db_to_gain(0.0), 1.0);
        assert!((db_to_gain(-6.0) - 0.501).abs() < 1e-3);
        assert_eq!(db_to_gain(f32::NEG_INFINITY), 0.0);
        assert!((gain_to_db(db_to_gain(-14.0)) + 14.0).abs() < 1e-4);
    }

    #[test]
    fn decibel_ramp_below_the_floor_stays_below() {
        let floor = 0.01;
        let mut r = Ramp::constant(0.0);
        r.start(0.0, 4, Curve::decibel(-40.0, Shape::SlowTop));
        assert!((0..6).all(|_| r.tick() == 0.0), "silence stays silent");
        let mut r = Ramp::constant(0.001);
        r.start(0.0, 4, Curve::decibel(-40.0, Shape::Linear));
        assert!((0..6).all(|_| r.tick() <= 0.001));
        // From silence upwards it still starts at the floor.
        let mut r = Ramp::constant(0.0);
        r.start(1.0, 4, Curve::decibel(-40.0, Shape::Linear));
        r.tick();
        assert!(r.tick() > floor);
    }
}
