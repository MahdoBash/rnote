//! Finger velocity and GTK-style exponential deceleration, in logical pixels.
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub(super) const STOP_SPEED: f64 = 10.0;
const FRICTION: f64 = 4.0;
const HISTORY: Duration = Duration::from_millis(120);

#[derive(Debug, Default)]
pub(super) struct TouchMotion {
    active: bool,
    samples: VecDeque<(Instant, [f64; 2])>,
}

impl TouchMotion {
    pub(super) fn begin(&mut self, now: Instant) {
        self.active = true;
        self.samples.clear();
        self.samples.push_back((now, [0.0; 2]));
    }

    pub(super) fn cancel(&mut self) {
        self.active = false;
        self.samples.clear();
    }

    pub(super) fn sample(&mut self, now: Instant, position: [f64; 2]) {
        if !self.active {
            return;
        }
        while self
            .samples
            .front()
            .is_some_and(|(time, _)| now.duration_since(*time) > HISTORY)
        {
            self.samples.pop_front();
        }
        if self.samples.back().is_some_and(|(time, _)| *time == now) {
            self.samples.pop_back();
        }
        self.samples.push_back((now, position));
    }

    pub(super) fn release(&mut self, now: Instant, position: [f64; 2]) -> [f64; 2] {
        if !self.active {
            return [0.0; 2];
        }
        self.sample(now, position);
        let mut velocity = [0.0; 2];
        if let (Some((first, start)), Some((last, end))) =
            (self.samples.front(), self.samples.back())
        {
            let span = last.duration_since(*first).as_secs_f64();
            if span >= 0.008 && (end[0] - start[0]).hypot(end[1] - start[1]) >= 8.0 {
                // Regression over recent samples avoids a noisy final event deciding the fling.
                let n = self.samples.len() as f64;
                let mean_t = self
                    .samples
                    .iter()
                    .map(|(t, _)| t.duration_since(*first).as_secs_f64())
                    .sum::<f64>()
                    / n;
                let variance = self
                    .samples
                    .iter()
                    .map(|(t, _)| (t.duration_since(*first).as_secs_f64() - mean_t).powi(2))
                    .sum::<f64>();
                for axis in 0..2 {
                    let mean_p = self.samples.iter().map(|(_, p)| p[axis]).sum::<f64>() / n;
                    let covariance = self
                        .samples
                        .iter()
                        .map(|(t, p)| {
                            (t.duration_since(*first).as_secs_f64() - mean_t) * (p[axis] - mean_p)
                        })
                        .sum::<f64>();
                    velocity[axis] = (-covariance / variance).clamp(-4500.0, 4500.0);
                }
            }
        }
        self.cancel();
        velocity
    }
}

pub(super) fn advance(velocity: &mut [f64; 2], seconds: f64) -> [f64; 2] {
    // Never jump across the document after a suspended or stalled frame clock.
    if !seconds.is_finite() || seconds <= 0.0 || seconds > 0.25 {
        *velocity = [0.0; 2];
        return [0.0; 2];
    }
    let decay = (-FRICTION * seconds).exp();
    let distance = velocity.map(|v| v * (1.0 - decay) / FRICTION);
    *velocity = velocity.map(|v| v * decay);
    distance
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flick_continues_in_drag_direction() {
        let now = Instant::now();
        let mut m = TouchMotion::default();
        m.begin(now);
        m.sample(now + Duration::from_millis(40), [0.0, -40.0]);
        let v = m.release(now + Duration::from_millis(80), [0.0, -80.0]);
        assert!((v[1] - 1000.0).abs() < 0.001);
        assert_eq!(v[0], 0.0);
    }
    #[test]
    fn hold_before_release_does_not_fling() {
        let now = Instant::now();
        let mut m = TouchMotion::default();
        m.begin(now);
        m.sample(now + Duration::from_millis(40), [0.0, -80.0]);
        assert_eq!(
            m.release(now + Duration::from_millis(200), [0.0, -80.0]),
            [0.0; 2]
        );
    }
    #[test]
    fn cancellation_and_taps_do_not_fling() {
        let now = Instant::now();
        let mut m = TouchMotion::default();
        m.begin(now);
        assert_eq!(
            m.release(now + Duration::from_millis(20), [1.0, 1.0]),
            [0.0; 2]
        );
        m.begin(now);
        m.cancel();
        assert_eq!(
            m.release(now + Duration::from_millis(20), [100.0, 100.0]),
            [0.0; 2]
        );
    }
    #[test]
    fn travel_does_not_depend_on_refresh_rate() {
        let travel = |hz| {
            let mut v = [1000.0, -500.0];
            let mut p = [0.0; 2];
            for _ in 0..hz {
                let d = advance(&mut v, 1.0 / hz as f64);
                p[0] += d[0];
                p[1] += d[1];
            }
            p
        };
        let a = travel(60);
        let b = travel(144);
        assert!((a[0] - b[0]).abs() < 1e-9);
        assert!((a[1] - b[1]).abs() < 1e-9);
    }
    #[test]
    fn deceleration_is_monotonic_and_stalled_frames_stop() {
        let mut v = [1000.0, 0.0];
        let a = advance(&mut v, 0.016);
        let b = advance(&mut v, 0.016);
        assert!(a[0] > b[0] && b[0] > 0.0);
        assert_eq!(advance(&mut v, 1.0), [0.0; 2]);
        assert_eq!(v, [0.0; 2]);
    }
}
