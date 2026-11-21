//! Conservative single-stroke shape recognition. Coordinates are in screen pixels,
//! so the gesture has the same tolerance at every document zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point(pub f64, pub f64);
impl Point {
    fn distance(self, b: Self) -> f64 {
        (self.0 - b.0).hypot(self.1 - b.1)
    }
}
fn line_distance(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy).max(1e-9)).clamp(0., 1.);
    p.distance(Point(a.0 + t * dx, a.1 + t * dy))
}
fn simplify(points: &[Point], tolerance: f64, out: &mut Vec<Point>) {
    if points.len() < 2 {
        return;
    }
    let a = points[0];
    let b = points[points.len() - 1];
    let (index, distance) = points
        .iter()
        .enumerate()
        .map(|(i, p)| (i, line_distance(*p, a, b)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    if distance > tolerance && index > 0 && index + 1 < points.len() {
        simplify(&points[..=index], tolerance, out);
        simplify(&points[index..], tolerance, out);
    } else {
        out.push(b);
    }
}
/// Resample by distance along the stroke, so a slow corner and a fast side have
/// the same weight in the fit. The last point is kept even on a short stroke.
fn resample(points: &[Point], intervals: usize) -> Vec<Point> {
    let length: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut result = Vec::with_capacity(intervals + 1);
    let mut segment = 0;
    let mut start = 0.;
    for i in 0..intervals {
        let target = length * i as f64 / intervals as f64;
        let mut span = points[segment].distance(points[segment + 1]);
        while segment + 2 < points.len() && start + span <= target {
            start += span;
            segment += 1;
            span = points[segment].distance(points[segment + 1]);
        }
        let (a, b) = (points[segment], points[segment + 1]);
        let t = ((target - start) / span.max(1e-9)).clamp(0., 1.);
        result.push(Point(a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
    }
    result.push(*points.last().unwrap());
    result
}

/// Fit an ellipse in its own principal axes, including ovals drawn diagonally.
fn ellipse(ring: &[Point]) -> Option<Vec<Point>> {
    let count = ring.len() as f64;
    let mean = Point(
        ring.iter().map(|p| p.0).sum::<f64>() / count,
        ring.iter().map(|p| p.1).sum::<f64>() / count,
    );
    let (mut xx_yy, mut xy) = (0., 0.);
    for p in ring {
        let (x, y) = (p.0 - mean.0, p.1 - mean.1);
        xx_yy += x * x - y * y;
        xy += x * y;
    }
    let (sin, cos) = (0.5 * (2. * xy).atan2(xx_yy)).sin_cos();
    let local: Vec<_> = ring
        .iter()
        .map(|p| {
            let (x, y) = (p.0 - mean.0, p.1 - mean.1);
            Point(cos * x + sin * y, -sin * x + cos * y)
        })
        .collect();
    let bounds = |axis: fn(&Point) -> f64| {
        let mut v: Vec<_> = local.iter().map(axis).collect();
        v.sort_unstable_by(f64::total_cmp);
        let n = v.len();
        ((v[0] + v[1]) * 0.5, (v[n - 1] + v[n - 2]) * 0.5)
    };
    let (lx, hx) = bounds(|p| p.0);
    let (ly, hy) = bounds(|p| p.1);
    let (cx, cy, mut rx, mut ry) = (
        (lx + hx) * 0.5,
        (ly + hy) * 0.5,
        (hx - lx) * 0.5,
        (hy - ly) * 0.5,
    );
    if rx.min(ry) < 4.5 {
        return None;
    }
    let mut angles = Vec::with_capacity(ring.len() + 1);
    let mut error = 0.;
    for q in local {
        let (x, y) = ((q.0 - cx) / rx, (q.1 - cy) / ry);
        error += (x.hypot(y) - 1.).powi(2);
        angles.push(y.atan2(x));
    }
    let first = angles[0];
    angles.push(first);
    let (mut turn, mut travel) = (0., 0.);
    for pair in angles.windows(2) {
        let delta = pair[1] - pair[0];
        let delta = delta.sin().atan2(delta.cos());
        turn += delta;
        travel += delta.abs();
    }
    if error / count > 0.013 || (turn.abs() - std::f64::consts::TAU).abs() > 0.3 || travel > 7.4 {
        return None;
    }
    if (rx - ry).abs() < 0.12 * rx.max(ry) {
        let r = 0.5 * (rx + ry);
        rx = r;
        ry = r;
    }
    let mut result: Vec<_> = (0..96)
        .map(|i| {
            let angle = first + turn.signum() * std::f64::consts::TAU * i as f64 / 96.;
            let (x, y) = (cx + rx * angle.cos(), cy + ry * angle.sin());
            Point(mean.0 + cos * x - sin * y, mean.1 + sin * x + cos * y)
        })
        .collect();
    result.push(result[0]);
    Some(result)
}

fn arrow(v: &[Point], diagonal: f64) -> Option<Vec<Point>> {
    // shaft -> tip -> wing -> tip -> other wing, optionally returning to the tip.
    if !(v.len() == 5 || (v.len() == 6 && v[5].distance(v[1]) < diagonal * 0.14))
        || v[1].distance(v[3]) > diagonal * 0.14
    {
        return None;
    }
    let (a, t, l, r) = (v[0], v[1], v[2], v[4]);
    let shaft = a.distance(t);
    if shaft < diagonal * 0.60 {
        return None;
    }
    let (dx, dy) = ((t.0 - a.0) / shaft, (t.1 - a.1) / shaft);
    let back = |q: Point| (t.0 - q.0) * dx + (t.1 - q.1) * dy;
    let side = |q: Point| (q.0 - t.0) * (-dy) + (q.1 - t.1) * dx;
    if ![l, r]
        .iter()
        .all(|&q| (shaft * 0.055..shaft * 0.56).contains(&back(q)) && side(q).abs() > shaft * 0.04)
        || side(l) * side(r) >= 0.
    {
        return None;
    }
    let depth = (back(l) + back(r)) * 0.5;
    let half = (side(l).abs() + side(r).abs()) * 0.5 * side(l).signum();
    Some(vec![
        a,
        t,
        Point(t.0 - dx * depth - dy * half, t.1 - dy * depth + dx * half),
        t,
        Point(t.0 - dx * depth + dy * half, t.1 - dy * depth - dx * half),
    ])
}

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn polygon(ring: &[Point], diagonal: f64) -> Option<(Vec<Point>, bool)> {
    let first = ring[0];
    let mut p = ring.to_vec();
    p.push(first);
    let split = p
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.distance(first).total_cmp(&b.1.distance(first)))?
        .0;
    let tolerance = (diagonal * 0.037).max(1.35);
    let mut vertices = vec![first];
    simplify(&p[..=split], tolerance, &mut vertices);
    simplify(&p[split..], tolerance, &mut vertices);
    vertices.pop();
    loop {
        let n = vertices.len();
        if n < 4 {
            break;
        }
        let redundant = (0..n).find(|&i| {
            line_distance(
                vertices[i],
                vertices[(i + n - 1) % n],
                vertices[(i + 1) % n],
            ) < tolerance
        });
        if let Some(i) = redundant {
            vertices.remove(i);
        } else {
            break;
        }
    }
    let n = vertices.len();
    if !(3..=8).contains(&n) {
        return None;
    }
    let edges: Vec<_> = (0..n)
        .map(|i| (vertices[i], vertices[(i + 1) % n]))
        .collect();
    let mut perimeter = 0.;
    let mut signed_area = 0.;
    for (i, &(a, b)) in edges.iter().enumerate() {
        let side = a.distance(b);
        if side < (diagonal * 0.06).max(3.) {
            return None;
        }
        perimeter += side;
        signed_area += cross(first, a, b);
        for &(c, d) in edges.iter().skip(i + 2) {
            if d == a {
                continue;
            } // adjacent across the closing edge
            if cross(a, b, c) * cross(a, b, d) < 0. && cross(c, d, a) * cross(c, d, b) < 0. {
                return None;
            }
        }
    }
    let length: f64 = p.windows(2).map(|w| w[0].distance(w[1])).sum();
    if signed_area.abs() < 0.06 * diagonal * diagonal || length > 1.20 * perimeter {
        return None;
    }
    let winding = signed_area;
    // Count evidence along the middle of each side. One locally flat section of
    // an oval is insufficient to classify the whole outline as a polygon.
    let mut error = vec![0.; n];
    let mut counts = vec![0; n];
    for &q in ring {
        let (i, &(a, b)) = edges.iter().enumerate().min_by(|(_, l), (_, r)| {
            line_distance(q, l.0, l.1).total_cmp(&line_distance(q, r.0, r.1))
        })?;
        let len = a.distance(b);
        let t = ((q.0 - a.0) * (b.0 - a.0) + (q.1 - a.1) * (b.1 - a.1)) / (len * len);
        if (0.18..0.82).contains(&t) {
            error[i] += line_distance(q, a, b).powi(2);
            counts[i] += 1;
        }
    }
    let straight = (0..n)
        .filter(|&i| {
            let (a, b) = edges[i];
            counts[i] >= 2
                && (error[i] / counts[i] as f64).sqrt()
                    <= (a.distance(b) * 0.040).max(diagonal * 0.006).max(0.65)
        })
        .count();
    if straight * 2 < n {
        return None;
    }
    // Four near-perpendicular sides become an exact rotated rectangle.
    if n == 4 {
        let (a, b, c, d) = (vertices[0], vertices[1], vertices[2], vertices[3]);
        let (ux, uy) = (b.0 - a.0 + c.0 - d.0, b.1 - a.1 + c.1 - d.1);
        let norm = ux.hypot(uy);
        let (ux, uy) = (ux / norm, uy / norm);
        let dot = ((c.0 - b.0) * ux + (c.1 - b.1) * uy).abs() / b.distance(c);
        let right_angles = (0..4).all(|i| {
            let (p, q, r) = (vertices[i], vertices[(i + 1) % 4], vertices[(i + 2) % 4]);
            ((q.0 - p.0) * (r.0 - q.0) + (q.1 - p.1) * (r.1 - q.1)).abs()
                < 0.38 * p.distance(q) * q.distance(r)
        });
        if right_angles && dot < 0.35 {
            let center = Point(
                (a.0 + b.0 + c.0 + d.0) * 0.25,
                (a.1 + b.1 + c.1 + d.1) * 0.25,
            );
            let hw = (a.distance(b) + c.distance(d)) * 0.25;
            let hh = (b.distance(c) + d.distance(a)) * 0.25;
            let sign = winding.signum();
            vertices = vec![
                Point(
                    center.0 - ux * hw + uy * hh * sign,
                    center.1 - uy * hw - ux * hh * sign,
                ),
                Point(
                    center.0 + ux * hw + uy * hh * sign,
                    center.1 + uy * hw - ux * hh * sign,
                ),
                Point(
                    center.0 + ux * hw - uy * hh * sign,
                    center.1 + uy * hw + ux * hh * sign,
                ),
                Point(
                    center.0 - ux * hw - uy * hh * sign,
                    center.1 - uy * hw + ux * hh * sign,
                ),
            ];
        }
    }
    vertices.push(vertices[0]);
    Some((vertices, straight * 4 >= n * 3))
}

/// Return a cleaned single stroke, or leave freehand input alone when the fit is ambiguous.
pub fn recognize(input: &[Point]) -> Option<Vec<Point>> {
    if input.len() < 2 || input.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
        return None;
    }
    let first = input[0];
    let mut lo = first;
    let mut hi = first;
    for &q in input {
        lo = Point(lo.0.min(q.0), lo.1.min(q.1));
        hi = Point(hi.0.max(q.0), hi.1.max(q.1));
    }
    let diagonal = lo.distance(hi);
    if !diagonal.is_finite() || diagonal < 20. {
        return None;
    }
    let mut filtered = vec![first];
    let spacing = (diagonal * 0.015).clamp(2.8, 4.5);
    for &q in &input[1..] {
        if filtered.last()?.distance(q) > spacing {
            filtered.push(q);
        }
    }
    let last = *input.last()?;
    if *filtered.last()? != last {
        filtered.push(last);
    }
    if filtered.len() < 2 {
        return None;
    }
    let raw_length: f64 = filtered.windows(2).map(|w| w[0].distance(w[1])).sum();
    if raw_length > diagonal * 6. {
        return None;
    }
    // Bound the input to recursive simplification and remove sub-pixel zigzags
    // before measuring perimeter or fitting sides.
    let uniform = resample(&filtered, 160);
    let mut clean = vec![first];
    simplify(&uniform, (diagonal * 0.009).clamp(0.8, 2.8), &mut clean);
    let length: f64 = clean.windows(2).map(|w| w[0].distance(w[1])).sum();
    let chord = first.distance(last);
    if chord >= 19.
        && length < 1.18 * chord
        && clean
            .iter()
            .all(|&q| line_distance(q, first, last) < (diagonal * 0.047).max(2.5))
    {
        return Some(vec![first, last]);
    }
    if chord > diagonal * 0.21 {
        let mut corners = vec![first];
        simplify(&clean, (diagonal * 0.035).max(2.0), &mut corners);
        if let Some(path) = arrow(&corners, diagonal) {
            return Some(path);
        }
        corners.reverse();
        let mut path = arrow(&corners, diagonal)?;
        path.reverse();
        return Some(path);
    }
    if length < diagonal * 1.8 || length > diagonal * 4.0 {
        return None;
    }
    if *clean.last()? != first {
        clean.push(first);
    }
    let outline = resample(&clean, 128);
    let ring = &outline[..128];
    let polygon = polygon(ring, diagonal);
    if let Some((path, true)) = polygon.as_ref() {
        return Some(path.clone());
    }
    ellipse(ring).or_else(|| polygon.map(|(path, _)| path))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sampled(v: &[Point]) -> Vec<Point> {
        v.windows(2)
            .flat_map(|w| {
                (0..20).map(move |i| {
                    let t = i as f64 / 20.;
                    Point(
                        w[0].0 + (w[1].0 - w[0].0) * t,
                        w[0].1 + (w[1].1 - w[0].1) * t,
                    )
                })
            })
            .chain(v.last().copied())
            .collect()
    }
    #[test]
    fn uneven_handdrawn_circle() {
        let points: Vec<_> = (0..=100)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 100.;
                let r = 70. + 5. * (3. * a).sin() + 2. * (11. * a).cos();
                Point(r * a.cos(), r * a.sin())
            })
            .collect();
        assert_eq!(recognize(&points).unwrap().len(), 97);
    }
    #[test]
    fn handdrawn_rectangle_with_small_closing_gap() {
        let mut points = sampled(&[
            Point(0., 0.),
            Point(100., 4.),
            Point(104., 80.),
            Point(-2., 78.),
            Point(4., 5.),
        ]);
        for (i, p) in points.iter_mut().enumerate() {
            p.0 += (i as f64 * 0.7).sin() * 1.5;
            p.1 += (i as f64 * 0.9).cos() * 1.5;
        }
        assert_eq!(recognize(&points).unwrap().len(), 5);
    }
    #[test]
    fn line() {
        assert_eq!(
            recognize(&sampled(&[Point(0., 0.), Point(120., 30.)]))
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn rectangle() {
        let r = recognize(&sampled(&[
            Point(0., 0.),
            Point(100., 2.),
            Point(102., 80.),
            Point(1., 79.),
            Point(0., 0.),
        ]))
        .unwrap();
        assert_eq!(r.len(), 5);
        let dot = (r[1].0 - r[0].0) * (r[2].0 - r[1].0) + (r[1].1 - r[0].1) * (r[2].1 - r[1].1);
        assert!(dot.abs() < 1e-6);
    }
    #[test]
    fn polygons() {
        for n in [3, 5, 6, 7, 8] {
            let v: Vec<_> = (0..=n)
                .map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / n as f64;
                    Point(100. * a.cos(), 100. * a.sin())
                })
                .collect();
            assert_eq!(recognize(&sampled(&v)).unwrap().len(), n + 1);
        }
    }
    #[test]
    fn circle() {
        let p: Vec<_> = (0..=80)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 80.;
                Point(70. * a.cos(), 70. * a.sin())
            })
            .collect();
        assert_eq!(recognize(&p).unwrap().len(), 97);
    }
    #[test]
    fn arrow() {
        let p = sampled(&[
            Point(0., 0.),
            Point(120., 0.),
            Point(90., -20.),
            Point(120., 0.),
            Point(90., 20.),
        ]);
        assert_eq!(recognize(&p).unwrap().len(), 5);
    }
    #[test]
    fn reject_scribble() {
        let p = sampled(&[
            Point(0., 0.),
            Point(100., 80.),
            Point(0., 80.),
            Point(100., 0.),
            Point(0., 0.),
        ]);
        assert!(recognize(&p).is_none());
    }
    #[test]
    fn reject_tap_and_small_marks() {
        assert!(recognize(&vec![Point(1., 1.); 50]).is_none());
        assert!(recognize(&sampled(&[Point(0., 0.), Point(10., 10.)])).is_none());
    }

    #[test]
    fn coalesced_corners_and_partial_closure() {
        let r = recognize(&[
            Point(0., 0.),
            Point(88., 2.),
            Point(86., 52.),
            Point(1., 50.),
            Point(3., 6.),
        ])
        .unwrap();
        assert_eq!(r.len(), 5);
        assert_eq!(r.first(), r.last());
        let triangle = recognize(&[
            Point(0., 0.),
            Point(31., -39.),
            Point(65., 2.),
            Point(3., 2.),
        ])
        .unwrap();
        assert_eq!(triangle.len(), 4);
        // A small deliberate shape is common between lines of handwriting.
        let square = sampled(&[
            Point(0., 0.),
            Point(18., 1.),
            Point(18., 18.),
            Point(0., 17.),
            Point(0., 1.),
        ]);
        assert_eq!(recognize(&square).unwrap().len(), 5);
    }

    #[test]
    fn rotated_oval_is_an_oval_at_uneven_pen_speeds() {
        for sign in [-1., 1.] {
            let (sin, cos) = 0.68_f64.sin_cos();
            let trace: Vec<_> = (0..=128)
                .map(|i| {
                    let t = i as f64 / 128.;
                    let a = sign * std::f64::consts::TAU * t.powf(1.55) + 0.4;
                    let (x, y) = (
                        76. * a.cos() + (13. * a).sin(),
                        24. * a.sin() + (7. * a).cos(),
                    );
                    Point(130. + cos * x - sin * y, 85. + sin * x + cos * y)
                })
                .collect();
            let result = recognize(&trace).unwrap();
            assert_eq!(result.len(), 97);
            assert_eq!(result.first(), result.last());
            for p in result {
                let (x, y) = (p.0 - 130., p.1 - 85.);
                let radius = ((cos * x + sin * y) / 76.).hypot((-sin * x + cos * y) / 24.);
                assert!((radius - 1.).abs() < 0.12);
            }
        }
    }

    #[test]
    fn fast_straight_sides_and_a_slow_wobbly_corner() {
        let mut trace = vec![Point(10., 10.), Point(108., 12.)];
        for i in 0..140 {
            trace.push(Point(
                108. + (i as f64 * 1.03).cos() * 1.8,
                12. + (i as f64 * 0.89).sin() * 1.8,
            ));
        }
        trace.extend([Point(109., 74.), Point(9., 73.), Point(11., 14.)]);
        assert_eq!(recognize(&trace).unwrap().len(), 5);
    }

    #[test]
    fn reverse_arrow_and_concave_polygon() {
        let mut p = sampled(&[
            Point(0., 0.),
            Point(120., 0.),
            Point(95., -17.),
            Point(117., 2.),
            Point(92., 18.),
        ]);
        p.reverse();
        assert_eq!(recognize(&p).unwrap().len(), 5);
        let l = sampled(&[
            Point(0., 0.),
            Point(100., 0.),
            Point(100., 32.),
            Point(41., 33.),
            Point(40., 89.),
            Point(0., 88.),
            Point(1., 1.),
        ]);
        assert_eq!(recognize(&l).unwrap().len(), 7);
    }

    #[test]
    fn leaves_unfinished_and_retraced_marks_freehand() {
        let u = sampled(&[
            Point(0., 0.),
            Point(0., 75.),
            Point(85., 75.),
            Point(85., 5.),
        ]);
        let backtrack = sampled(&[
            Point(0., 0.),
            Point(90., 4.),
            Point(35., 2.),
            Point(95., 3.),
        ]);
        assert!(recognize(&u).is_none());
        assert!(recognize(&backtrack).is_none());
    }
}
