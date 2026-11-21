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
/// Returns a cleaned path, or None for handwriting, taps, scribbles or ambiguous shapes.
pub fn recognize(input: &[Point]) -> Option<Vec<Point>> {
    if input.len() < 6
        || input.len() > 10000
        || input.iter().any(|p| !p.0.is_finite() || !p.1.is_finite())
    {
        return None;
    }
    let mut p = vec![input[0]];
    for point in &input[1..] {
        if p.last()?.distance(*point) > 1. {
            p.push(*point);
        }
    }
    if p.len() < 5 {
        return None;
    }
    let (mut minx, mut miny, mut maxx, mut maxy) = (p[0].0, p[0].1, p[0].0, p[0].1);
    for q in &p {
        minx = minx.min(q.0);
        miny = miny.min(q.1);
        maxx = maxx.max(q.0);
        maxy = maxy.max(q.1);
    }
    let (w, h) = (maxx - minx, maxy - miny);
    let diagonal = w.hypot(h);
    if diagonal < 28. {
        return None;
    }
    let length: f64 = p.windows(2).map(|w| w[0].distance(w[1])).sum();
    let (first, last) = (p[0], *p.last()?);
    let chord = first.distance(last);
    if chord > 25.
        && length / chord < 1.12
        && p.iter()
            .all(|q| line_distance(*q, first, last) < (diagonal * 0.035).max(2.5))
    {
        return Some(vec![first, last]);
    }
    if chord > diagonal * 0.18 {
        // A one-stroke arrow is shaft -> tip -> wing -> tip -> other wing.
        let mut v = vec![first];
        simplify(&p, (diagonal * 0.035).max(3.), &mut v);
        if v.len() == 5 && v[1].distance(v[3]) < diagonal * 0.10 {
            let (a, t, l, r) = (v[0], v[1], v[2], v[4]);
            let shaft = a.distance(t);
            let (dx, dy) = ((t.0 - a.0) / shaft, (t.1 - a.1) / shaft);
            let back = |q: Point| (t.0 - q.0) * dx + (t.1 - q.1) * dy;
            let side = |q: Point| (q.0 - t.0) * (-dy) + (q.1 - t.1) * dx;
            if shaft > diagonal * 0.65
                && back(l) > shaft * 0.08
                && back(r) > shaft * 0.08
                && back(l) < shaft * 0.5
                && back(r) < shaft * 0.5
                && side(l) * side(r) < 0.
                && side(l).abs() > shaft * 0.05
                && side(r).abs() > shaft * 0.05
            {
                let depth = (back(l) + back(r)) * 0.5;
                let half = (side(l).abs() + side(r).abs()) * 0.5;
                let sign = side(l).signum();
                return Some(vec![
                    a,
                    t,
                    Point(
                        t.0 - dx * depth - dy * half * sign,
                        t.1 - dy * depth + dx * half * sign,
                    ),
                    t,
                    Point(
                        t.0 - dx * depth + dy * half * sign,
                        t.1 - dy * depth - dx * half * sign,
                    ),
                ]);
            }
        }
        return None;
    }
    if w < 20. || h < 20. || length > diagonal * 4.0 {
        return None;
    }
    // Ellipse fit: require small radial error and one complete, ordered revolution.
    let (cx, cy, rx, ry) = ((minx + maxx) * 0.5, (miny + maxy) * 0.5, w * 0.5, h * 0.5);
    let radial: f64 = p
        .iter()
        .map(|q| (((q.0 - cx) / rx).hypot((q.1 - cy) / ry) - 1.).powi(2))
        .sum::<f64>()
        / p.len() as f64;
    let angles: Vec<f64> = p
        .iter()
        .map(|q| ((q.1 - cy) / ry).atan2((q.0 - cx) / rx))
        .collect();
    let deltas: Vec<f64> = angles
        .windows(2)
        .map(|a| {
            let d = a[1] - a[0];
            (d + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
        })
        .collect();
    let turn: f64 = deltas.iter().sum();
    let travel: f64 = deltas.iter().map(|d| d.abs()).sum();
    let has_straight_side = (0..p.len()).any(|i| {
        let end = (i + 1..p.len()).find(|&j| p[i].distance(p[j]) > diagonal * 0.25);
        end.is_some_and(|j| {
            p[i..=j]
                .iter()
                .all(|q| line_distance(*q, p[i], p[j]) < diagonal * 0.008)
        })
    });
    if !has_straight_side && radial < 0.012 && turn.abs() > 5.4 && turn.abs() < 6.9 && travel < 7.2
    {
        let (rx, ry) = if (rx - ry).abs() / rx.max(ry) < 0.12 {
            let r = (rx + ry) * 0.5;
            (r, r)
        } else {
            (rx, ry)
        };
        return Some(
            (0..=96)
                .map(|i| {
                    let a = angles[0] + turn.signum() * std::f64::consts::TAU * i as f64 / 96.;
                    Point(cx + rx * a.cos(), cy + ry * a.sin())
                })
                .collect(),
        );
    }
    // Simplify the closed outline into 3–8 straight sides. Split at the farthest
    // point, avoiding the zero-length baseline of a closed RDP input.
    // Discard a short closing tail: joining it directly creates a spurious
    // extra corner or reverses the winding of an otherwise valid polygon.
    while p.len() > 5 && p.last().unwrap().distance(first) < diagonal * 0.10 {
        p.pop();
    }
    p.push(first);
    let split = p
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.distance(first).total_cmp(&b.1.distance(first)))?
        .0;
    let tolerance = (diagonal * 0.05).max(3.0);
    let mut vertices = vec![first];
    simplify(&p[..=split], tolerance, &mut vertices);
    simplify(&p[split..], tolerance, &mut vertices);
    vertices.pop();
    // The start may be in the middle of an edge.
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
    let mut winding = 0.;
    for i in 0..n {
        let (a, b, c) = (vertices[i], vertices[(i + 1) % n], vertices[(i + 2) % n]);
        if a.distance(b) < diagonal * 0.10 {
            return None;
        }
        let cross = (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0);
        if cross.abs() < 1. || (winding != 0. && cross * winding < 0.) {
            return None;
        }
        winding = cross;
    }
    let perimeter: f64 = (0..n)
        .map(|i| vertices[i].distance(vertices[(i + 1) % n]))
        .sum();
    if length / perimeter > 1.15 {
        return None;
    }
    // Four near-perpendicular sides become an exact rotated rectangle.
    if n == 4 {
        let (a, b, c, d) = (vertices[0], vertices[1], vertices[2], vertices[3]);
        let (ux, uy) = (b.0 - a.0 + c.0 - d.0, b.1 - a.1 + c.1 - d.1);
        let norm = ux.hypot(uy);
        let (ux, uy) = (ux / norm, uy / norm);
        let dot = ((c.0 - b.0) * ux + (c.1 - b.1) * uy).abs() / b.distance(c);
        if dot < 0.18 {
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
    Some(vertices)
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
}
