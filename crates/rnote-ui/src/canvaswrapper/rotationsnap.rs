//! Angular detents for two-finger rotation. All angles are in radians.
use std::f64::consts::FRAC_PI_2;

const CAPTURE: f64 = 5.0_f64.to_radians();
const OUTER: f64 = 14.0_f64.to_radians();

fn nearby_detent(angle: f64) -> Option<f64> {
    let quarter = (angle / FRAC_PI_2).round();
    let target = quarter * FRAC_PI_2;
    // Half-turns from either direction are the same orientation. Leave upright free.
    (quarter.rem_euclid(4.0) != 0.0 && (angle - target).abs() < OUTER).then_some(target)
}

fn mapped_distance(distance: f64) -> f64 {
    let t = ((distance - CAPTURE) / (OUTER - CAPTURE)).clamp(0.0, 1.0);
    // Smoothstep joins the flat center and ordinary rotation without a sudden change of speed.
    distance * t * t * (3.0 - 2.0 * t)
}

pub(crate) fn display_angle(input: f64) -> f64 {
    let Some(target) = nearby_detent(input) else {
        return input;
    };
    let delta = input - target;
    target + delta.signum() * mapped_distance(delta.abs())
}

/// Start a new gesture at the displayed orientation, including angles typed in the menu.
/// Invert the short transition once on contact so regripping never reapplies the attraction.
pub(crate) fn gesture_origin(display: f64) -> f64 {
    let Some(target) = nearby_detent(display) else {
        return display;
    };
    let delta = display - target;
    if delta.abs() < 1e-12 {
        return target;
    }
    let (mut lo, mut hi) = (CAPTURE, OUTER);
    for _ in 0..32 {
        let mid = (lo + hi) * 0.5;
        if mapped_distance(mid) < delta.abs() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    target + delta.signum() * (lo + hi) * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locks_from_both_sides_and_across_the_wrap() {
        for target in [-270.0_f64, -180.0, -90.0, 90.0, 180.0, 270.0] {
            for offset in [-4.99, -2.0, 0.0, 2.0, 4.99] {
                assert!(
                    (display_angle((target + offset).to_radians()).to_degrees() - target).abs()
                        < 1e-10
                );
            }
        }
        for free in [-24.0_f64, -2.0, 2.0, 65.0, 108.0, 156.0] {
            assert_eq!(display_angle(free.to_radians()), free.to_radians());
        }
    }
    #[test]
    fn release_is_continuous_and_regripping_preserves_every_angle() {
        let mut previous = -360_f64.to_radians();
        for i in -36000..=36000 {
            let raw = (i as f64 * 0.01).to_radians();
            let shown = display_angle(raw);
            assert!(shown >= previous - 1e-12 && shown - previous < 0.03_f64.to_radians());
            assert!((display_angle(gesture_origin(raw)) - raw).abs() < 1e-9);
            previous = shown;
        }
    }
}
