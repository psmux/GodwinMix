//! A straight line through measured points: cost against megapixels a
//! second. Two shapes are calibrated, so this is usually exact; with one it
//! goes through the origin.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    /// Cost with no picture at all: the encoder's fixed overhead.
    pub fixed: f64,
    /// Cost per megapixel a second.
    pub slope: f64,
}

impl Line {
    pub fn at(&self, x: f64) -> f64 {
        (self.fixed + self.slope * x).max(0.0)
    }

    /// Least squares over `(x, y)`, with neither part allowed below zero: a
    /// noisy pair that gives a negative fixed cost is read as proportional.
    pub fn through(points: &[(f64, f64)]) -> Option<Line> {
        let pts: Vec<(f64, f64)> = points.iter().copied().filter(|(x, y)| *x > 0.0 && y.is_finite()).collect();
        match pts.len() {
            0 => None,
            1 => Some(Line { fixed: 0.0, slope: pts[0].1 / pts[0].0 }),
            n => {
                let n = n as f64;
                let (sx, sy) = pts.iter().fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
                let (mx, my) = (sx / n, sy / n);
                let var: f64 = pts.iter().map(|(x, _)| (x - mx) * (x - mx)).sum();
                if var <= f64::EPSILON {
                    return Some(Line { fixed: 0.0, slope: my / mx });
                }
                let slope = pts.iter().map(|(x, y)| (x - mx) * (y - my)).sum::<f64>() / var;
                let fixed = my - slope * mx;
                if fixed < 0.0 || slope < 0.0 {
                    let slope = sy / sx;
                    return Some(Line { fixed: 0.0, slope });
                }
                Some(Line { fixed, slope })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_points_give_the_line_through_them() {
        let l = Line::through(&[(27.6, 400.0), (62.2, 800.0)]).unwrap();
        assert!((l.at(27.6) - 400.0).abs() < 1e-6);
        assert!((l.at(62.2) - 800.0).abs() < 1e-6);
        assert!(l.fixed > 0.0);
    }

    #[test]
    fn a_negative_fixed_cost_is_read_as_proportional() {
        let l = Line::through(&[(10.0, 10.0), (20.0, 100.0)]).unwrap();
        assert_eq!(l.fixed, 0.0);
        assert!(l.slope > 0.0);
    }

    #[test]
    fn one_point_goes_through_the_origin() {
        let l = Line::through(&[(50.0, 500.0)]).unwrap();
        assert_eq!(l.at(100.0), 1000.0);
        assert!(Line::through(&[]).is_none());
    }
}
