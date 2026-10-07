//! Time-based animation. Values are computed from `Instant`s, never from
//! frame counts, so motion takes the same time on a fast PC and on the Pi.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ease {
    Linear,
    /// Fast start, gentle stop. Default for anything moving into place.
    OutCubic,
}

impl Ease {
    /// Maps linear progress `t` in 0..=1 to eased progress.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::OutCubic => 1.0 - (1.0 - t).powi(3),
        }
    }
}

/// One animated number (a position, an opacity...).
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
    ease: Ease,
}

impl Tween {
    /// A tween already at rest at `value`.
    pub fn at(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start: Instant::now(),
            duration: Duration::ZERO,
            ease: Ease::Linear,
        }
    }

    /// Animates to `to`, starting from wherever the value is *now*. A new
    /// target mid-animation continues smoothly instead of jumping.
    pub fn retarget(&mut self, to: f32, now: Instant, duration: Duration, ease: Ease) {
        self.from = self.value(now);
        self.to = to;
        self.start = now;
        self.duration = duration;
        self.ease = ease;
    }

    pub fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let t =
            now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32();
        self.from + (self.to - self.from) * self.ease.apply(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn eases_hit_both_ends() {
        for e in [Ease::Linear, Ease::OutCubic] {
            assert_eq!(e.apply(0.0), 0.0);
            assert_eq!(e.apply(1.0), 1.0);
            assert_eq!(e.apply(2.0), 1.0, "clamped");
        }
        assert!(
            Ease::OutCubic.apply(0.5) > 0.5,
            "out-cubic front-loads motion"
        );
    }

    #[test]
    fn tween_runs_from_start_to_target() {
        let t0 = Instant::now();
        let mut tw = Tween::at(0.0);
        tw.retarget(100.0, t0, 100 * MS, Ease::Linear);
        assert_eq!(tw.value(t0), 0.0);
        assert_eq!(tw.value(t0 + 50 * MS), 50.0);
        assert_eq!(tw.value(t0 + 200 * MS), 100.0);
    }

    #[test]
    fn retarget_mid_flight_continues_from_current_value() {
        let t0 = Instant::now();
        let mut tw = Tween::at(0.0);
        tw.retarget(100.0, t0, 100 * MS, Ease::Linear);
        let mid = t0 + 50 * MS;
        tw.retarget(0.0, mid, 100 * MS, Ease::Linear);
        assert_eq!(tw.value(mid), 50.0, "no jump");
    }
}
