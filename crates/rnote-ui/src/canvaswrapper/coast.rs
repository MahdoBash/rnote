//! Screen-space release velocity and time-based deceleration, independent of GTK.
//! Keep the recent movement when release shares a timestamp with the last update.
use std::collections::VecDeque;

const HISTORY_MS: u32 = 100;
const PAUSE_MS: u32 = 140;
const FRICTION: f64 = 4.5;
const START_SPEED: f64 = 60.0;
const STOP_SPEED: f64 = 12.0;
const MAX_SPEED: f64 = 4000.0;

#[derive(Debug)]
pub(super) struct Motion {
    samples: VecDeque<(u32, [f64; 2])>,
    last_move: u32,
}
impl Motion {
    pub(super) fn new(time: u32, position: [f64; 2]) -> Self {
        Self {
            samples: VecDeque::from([(time, position)]),
            last_move: time,
        }
    }
    pub(super) fn push(&mut self, time: u32, position: [f64; 2]) {
        if !position.iter().all(|v| v.is_finite()) {
            return;
        }
        if let Some(&(previous_time, previous_position)) = self.samples.back() {
            if time.wrapping_sub(previous_time) > i32::MAX as u32 {
                return;
            }
            if position != previous_position {
                self.last_move = time;
            }
            if time == previous_time {
                self.samples.pop_back();
            }
        }
        self.samples.push_back((time, position));
        while self.samples.len() > 2 && time.wrapping_sub(self.samples[1].0) > HISTORY_MS {
            self.samples.pop_front();
        }
    }
    pub(super) fn release(&self, time: u32) -> Option<Coast> {
        let idle = time.wrapping_sub(self.last_move);
        if idle >= PAUSE_MS || self.samples.len() < 2 {
            return None;
        }
        let last_time = self.samples.back()?.0;
        let first_time = self.samples.front()?.0;
        if last_time.wrapping_sub(first_time) < 16 {
            return None;
        }
        // Least-squares slope over the recent samples rejects event batching jitter.
        let n = self.samples.len() as f64;
        let mean_t = self
            .samples
            .iter()
            .map(|s| s.0.wrapping_sub(first_time) as f64 / 1000.0)
            .sum::<f64>()
            / n;
        let mut mean = [0.0; 2];
        for (_, p) in &self.samples {
            for axis in 0..2 {
                mean[axis] += p[axis] / n;
            }
        }
        let mut covariance = [0.0; 2];
        let mut variance = 0.0;
        for (t, p) in &self.samples {
            let dt = t.wrapping_sub(first_time) as f64 / 1000.0 - mean_t;
            variance += dt * dt;
            for axis in 0..2 {
                covariance[axis] += dt * (p[axis] - mean[axis]);
            }
        }
        if variance < 1e-9 {
            return None;
        }
        let attenuation = (-FRICTION * idle as f64 / 1000.0).exp();
        let mut velocity = covariance.map(|c| -c / variance * attenuation);
        let speed = velocity[0].hypot(velocity[1]);
        if !speed.is_finite() || speed < START_SPEED {
            return None;
        }
        if speed > MAX_SPEED {
            velocity = velocity.map(|v| v * MAX_SPEED / speed);
        }
        Some(Coast { velocity })
    }
}
#[derive(Debug)]
pub(super) struct Coast {
    velocity: [f64; 2],
}
impl Coast {
    pub(super) fn step(&mut self, seconds: f64) -> Option<[f64; 2]> {
        if !seconds.is_finite()
            || seconds < 0.0
            || self.velocity[0].hypot(self.velocity[1]) < STOP_SPEED
        {
            return None;
        }
        let decay = (-FRICTION * seconds).exp();
        let delta = self.velocity.map(|v| v * (1.0 - decay) / FRICTION);
        self.velocity = self.velocity.map(|v| v * decay);
        Some(delta)
    }
    pub(super) fn stop_axis(&mut self, axis: usize) {
        self.velocity[axis] = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn flick(start: u32, direction: f64) -> Motion {
        let mut m = Motion::new(start, [100.0, 500.0]);
        for i in 1..=20 {
            m.push(
                start.wrapping_add(i * 12),
                [100.0, 500.0 + direction * i as f64 * 12.0],
            );
        }
        m
    }
    #[test]
    fn same_timestamp_release_keeps_momentum() {
        let mut c = flick(1000, -1.0).release(1240).unwrap();
        assert!(c.step(0.1).unwrap()[1] > 70.0);
    }
    #[test]
    fn stationary_release_does_not_erase_recent_motion() {
        let mut m = flick(1000, -1.0);
        m.push(1240, [100.0, 260.0]);
        assert!(m.release(1280).unwrap().step(0.1).unwrap()[1] > 50.0);
    }
    #[test]
    fn holding_still_cancels_fling() {
        assert!(flick(1000, -1.0).release(1490).is_none());
    }
    #[test]
    fn tap_and_slow_drift_do_not_fling() {
        assert!(Motion::new(1000, [0.0, 0.0]).release(1010).is_none());
        let mut m = Motion::new(0, [0.0, 0.0]);
        m.push(100, [0.0, 1.0]);
        assert!(m.release(100).is_none());
    }
    #[test]
    fn direction_reversal_uses_recent_movement() {
        let mut m = flick(1000, -1.0);
        for i in 1..=15 {
            m.push(1240 + i * 12, [100.0, 260.0 + i as f64 * 12.0]);
        }
        assert!(m.release(1420).unwrap().step(0.1).unwrap()[1] < -70.0);
    }
    #[test]
    fn timestamp_wraparound_is_supported() {
        assert!(
            flick(u32::MAX - 120, -1.0)
                .release(119)
                .unwrap()
                .step(0.1)
                .unwrap()[1]
                > 70.0
        );
    }
    #[test]
    fn distance_does_not_depend_on_frame_rate() {
        fn distance(hz: u32) -> f64 {
            let mut c = flick(0, -1.0).release(240).unwrap();
            (0..hz / 2)
                .map(|_| c.step(1.0 / hz as f64).unwrap()[1])
                .sum()
        }
        assert!((distance(60) - distance(120)).abs() < 1e-6);
        assert!((distance(60) - distance(30)).abs() < 1e-6);
    }
    #[test]
    fn normal_flick_slows_monotonically_and_stops_promptly() {
        let mut c = flick(0, -1.0).release(240).unwrap();
        let mut last = f64::MAX;
        let mut ticks = 0;
        while let Some(delta) = c.step(1.0 / 60.0) {
            assert!(delta[1] > 0.0 && delta[1] < last);
            last = delta[1];
            ticks += 1;
            assert!(ticks < 90);
        }
        assert!(ticks > 30);
    }
    #[test]
    fn hitting_one_edge_keeps_other_axis_moving() {
        let mut c = Coast {
            velocity: [200.0, 500.0],
        };
        c.stop_axis(0);
        let d = c.step(0.016).unwrap();
        assert_eq!(d[0], 0.0);
        assert!(d[1] > 0.0);
        c.stop_axis(1);
        assert!(c.step(0.016).is_none());
    }
    #[test]
    fn coalesced_and_duplicate_samples_remain_finite() {
        let mut m = Motion::new(0, [0.0, 500.0]);
        for i in 1..20 {
            m.push(i * 12, [0.0, 500.0 - i as f64 * 12.0]);
            m.push(i * 12, [0.0, 500.0 - i as f64 * 12.0]);
        }
        assert!(m.release(228).unwrap().step(0.05).unwrap()[1].is_finite());
    }
}
