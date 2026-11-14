//! The WPF easings and keyframe loops the island used, reimplemented (PORTING.md Appendix D
//! "Easing formulas"), and a small tween that holds its end value like `FillBehavior.HoldEnd`.

use std::f64::consts::PI;

/// WPF `BackEase` (amplitude `a`) in EaseIn mode: `t³ − a·t·sin(πt)`.
pub fn back_in(t: f64, a: f64) -> f64 {
    t * t * t - a * t * (PI * t).sin()
}

/// WPF `BackEase` in EaseOut mode: `1 − f(1 − t)`. Overshoots a little past 1 before settling.
pub fn back_out(t: f64, a: f64) -> f64 {
    1.0 - back_in(1.0 - t, a)
}

/// WPF `CubicEase` in EaseInOut mode.
pub fn cubic_in_out(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// WPF `SineEase` in EaseInOut mode.
pub fn sine_in_out(t: f64) -> f64 {
    (1.0 - (PI * t).cos()) / 2.0
}

/// A CSS `cubic-bezier(x1, y1, x2, y2)` / WPF `KeySpline`: solves x(s) = t for s, returns y(s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// The design's `--ease`, cubic-bezier(.45,0,.2,1), applied per keyframe segment.
pub const EASE: CubicBezier = CubicBezier {
    x1: 0.45,
    y1: 0.0,
    x2: 0.2,
    y2: 1.0,
};

impl CubicBezier {
    fn coord(p1: f64, p2: f64, s: f64) -> f64 {
        let u = 1.0 - s;
        3.0 * u * u * s * p1 + 3.0 * u * s * s * p2 + s * s * s
    }

    fn slope(p1: f64, p2: f64, s: f64) -> f64 {
        let u = 1.0 - s;
        3.0 * u * u * p1 + 6.0 * u * s * (p2 - p1) + 3.0 * s * s * (1.0 - p2)
    }

    pub fn ease(&self, t: f64) -> f64 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        // Newton first, bisection if the slope is too flat to trust.
        let mut s = t;
        for _ in 0..8 {
            let err = Self::coord(self.x1, self.x2, s) - t;
            if err.abs() < 1e-7 {
                return Self::coord(self.y1, self.y2, s);
            }
            let d = Self::slope(self.x1, self.x2, s);
            if d.abs() < 1e-6 {
                break;
            }
            s -= err / d;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        s = t;
        for _ in 0..60 {
            let x = Self::coord(self.x1, self.x2, s);
            if (x - t).abs() < 1e-7 {
                break;
            }
            if x < t {
                lo = s;
            } else {
                hi = s;
            }
            s = (lo + hi) / 2.0;
        }
        Self::coord(self.y1, self.y2, s)
    }
}

/// The value of CSS-style keyframes `(fraction, value)` at `frac` (0..1), each segment eased
/// with [`EASE`]. Before the first key the first value holds, after the last the last.
pub fn keyframes(keys: &[(f64, f64)], frac: f64) -> f64 {
    let Some(&(first_at, first)) = keys.first() else {
        return 0.0;
    };
    if frac <= first_at {
        return first;
    }
    for w in keys.windows(2) {
        let (a, va) = w[0];
        let (b, vb) = w[1];
        if frac <= b {
            let local = if b > a { (frac - a) / (b - a) } else { 1.0 };
            return va + (vb - va) * EASE.ease(local);
        }
    }
    keys[keys.len() - 1].1
}

/// A keyframe loop of `period` seconds at `t` seconds since it began. With `repeat`, it stops
/// after that many cycles and holds its last value.
pub fn looped(keys: &[(f64, f64)], period: f64, t: f64, repeat: Option<u32>) -> f64 {
    if let Some(n) = repeat {
        if t >= period * f64::from(n) {
            return keyframes(keys, 1.0);
        }
    }
    let frac = (t.max(0.0) % period) / period;
    keyframes(keys, frac)
}

/// `from → low` over `seconds`, auto-reversed, forever, SineEase InOut: the glow's breath.
pub fn breath(low: f64, seconds: f64, t: f64) -> f64 {
    let phase = t.max(0.0) % (2.0 * seconds);
    let forward = if phase < seconds { phase } else { 2.0 * seconds - phase };
    1.0 + (low - 1.0) * sine_in_out(forward / seconds)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    BackOut(f64),
    CubicInOut,
}

impl Ease {
    pub fn apply(self, t: f64) -> f64 {
        match self {
            Ease::Linear => t,
            Ease::BackOut(a) => back_out(t, a),
            Ease::CubicInOut => cubic_in_out(t),
        }
    }
}

/// One value animating from `from` to `to`, in milliseconds on the island's clock. A tween
/// with no length is simply `to`. Restarting from `value(now)` is what WPF's
/// `BeginAnimation(to)` without a From does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub from: f64,
    pub to: f64,
    pub start: i64,
    pub dur: i64,
    pub ease: Ease,
}

impl Tween {
    pub const fn hold(v: f64) -> Self {
        Self {
            from: v,
            to: v,
            start: 0,
            dur: 0,
            ease: Ease::Linear,
        }
    }

    pub fn value(&self, now: i64) -> f64 {
        if self.dur <= 0 || now >= self.start + self.dur {
            return self.to;
        }
        let t = ((now - self.start) as f64 / self.dur as f64).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * self.ease.apply(t)
    }

    pub fn running(&self, now: i64) -> bool {
        self.dur > 0 && now < self.start + self.dur
    }

    /// From wherever it is now to `to`.
    pub fn retarget(&mut self, now: i64, to: f64, dur: i64, ease: Ease) {
        *self = Self {
            from: self.value(now),
            to,
            start: now,
            dur,
            ease,
        };
    }

    /// Jumps to `v` with no animation.
    pub fn set(&mut self, v: f64) {
        *self = Self::hold(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn easings_start_at_0_and_end_at_1() {
        for f in [
            |t| back_out(t, 0.3),
            |t| back_out(t, 0.45),
            cubic_in_out,
            sine_in_out,
            |t| EASE.ease(t),
        ] {
            assert!(close(f(0.0), 0.0));
            assert!(close(f(1.0), 1.0));
        }
    }

    #[test]
    fn back_out_overshoots() {
        let peak = (1..100).map(|i| back_out(f64::from(i) / 100.0, 0.3)).fold(0.0, f64::max);
        assert!(peak > 1.0 && peak < 1.1, "{peak}");
        // WPF formula at the half: 1 - (0.125 - 0.3*0.5*sin(pi/2)) = 1.025
        assert!(close(back_out(0.5, 0.3), 1.025));
    }

    #[test]
    fn cubic_and_sine_are_symmetric() {
        assert!(close(cubic_in_out(0.5), 0.5));
        assert!(close(cubic_in_out(0.25), 0.0625));
        assert!(close(cubic_in_out(0.75), 0.9375));
        assert!(close(sine_in_out(0.5), 0.5));
    }

    #[test]
    fn key_spline_is_monotonic_and_slow_at_the_ends() {
        let mut last = 0.0;
        for i in 1..=100 {
            let v = EASE.ease(f64::from(i) / 100.0);
            assert!(v >= last - 1e-9);
            last = v;
        }
        assert!(EASE.ease(0.1) < 0.1);
        assert!(EASE.ease(0.9) > 0.9);
    }

    #[test]
    fn keyframes_hit_their_values() {
        let keys = [(0.0, 0.0), (0.25, -1.5), (0.75, 1.5), (1.0, 0.0)];
        assert!(close(keyframes(&keys, 0.0), 0.0));
        assert!(close(keyframes(&keys, 0.25), -1.5));
        assert!(close(keyframes(&keys, 0.75), 1.5));
        assert!(close(keyframes(&keys, 0.5), -1.5 + 3.0 * EASE.ease(0.5)));
        assert!(close(keyframes(&keys, 1.0), 0.0));
    }

    #[test]
    fn loops_repeat_and_a_counted_loop_holds_its_end() {
        let keys = [(0.0, 1.0), (0.5, 2.0), (1.0, 1.0)];
        assert!(close(looped(&keys, 2.0, 1.0, None), 2.0));
        assert!(close(looped(&keys, 2.0, 5.0, None), 2.0));
        let jump = [(0.0, 0.0), (0.14, 0.0), (0.44, -10.0), (0.72, 0.0), (1.0, 0.0)];
        assert!(close(looped(&jump, 1.0, 1.44, Some(2)), -10.0));
        assert!(close(looped(&jump, 1.0, 2.44, Some(2)), 0.0));
    }

    #[test]
    fn breath_goes_down_and_back() {
        assert!(close(breath(0.6, 2.8, 0.0), 1.0));
        assert!(close(breath(0.6, 2.8, 2.8), 0.6));
        assert!(close(breath(0.6, 2.8, 5.6), 1.0));
        assert!(close(breath(0.6, 2.8, 1.4), 0.8));
    }

    #[test]
    fn tween_holds_its_end_and_retargets_from_where_it_is() {
        let mut t = Tween::hold(-60.0);
        t.retarget(1000, 0.0, 450, Ease::BackOut(0.3));
        assert!(t.running(1200));
        assert!(close(t.value(1000), -60.0));
        assert!(close(t.value(1450), 0.0));
        assert!(!t.running(1450));
        t.retarget(1100, -60.0, 260, Ease::CubicInOut);
        assert!(close(t.value(1100), Tween { from: -60.0, to: 0.0, start: 1000, dur: 450, ease: Ease::BackOut(0.3) }.value(1100)));
    }
}
