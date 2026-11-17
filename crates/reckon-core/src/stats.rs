//! Statistics on plain slices of numbers. Units are the caller's business.
//!
//! Every sum of squares is taken around the mean in a second pass. The
//! textbook shortcut (`Σx² − n·mean²`) subtracts two nearly equal numbers
//! and loses most of its digits on data such as 10000000.1, 10000000.3.

pub fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

/// Middle value, or the mean of the two middle values.
pub fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

/// Variance; `sample` divides by n − 1 instead of n.
pub fn variance(v: &[f64], sample: bool) -> f64 {
    let m = mean(v);
    let ss: f64 = v.iter().map(|x| (x - m) * (x - m)).sum();
    ss / (v.len() - usize::from(sample)) as f64
}

/// Sums behind a least-squares line through points (x, y).
pub struct Fit {
    mean_x: f64,
    mean_y: f64,
    sxx: f64,
    syy: f64,
    sxy: f64,
}

impl Fit {
    /// `x` and `y` must have the same, non-zero length.
    pub fn new(x: &[f64], y: &[f64]) -> Fit {
        let (mean_x, mean_y) = (mean(x), mean(y));
        let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
        for (a, b) in x.iter().zip(y) {
            let (dx, dy) = (a - mean_x, b - mean_y);
            sxx += dx * dx;
            syy += dy * dy;
            sxy += dx * dy;
        }
        Fit {
            mean_x,
            mean_y,
            sxx,
            syy,
            sxy,
        }
    }

    /// Change in y per unit of x. `None` when every x is the same.
    pub fn slope(&self) -> Option<f64> {
        (self.sxx > 0.0).then(|| self.sxy / self.sxx)
    }

    /// Value of y where the line crosses x = 0.
    pub fn intercept(&self) -> Option<f64> {
        self.slope().map(|b| self.mean_y - b * self.mean_x)
    }

    /// Pearson correlation. `None` when either list never varies.
    pub fn correlation(&self) -> Option<f64> {
        (self.sxx > 0.0 && self.syy > 0.0)
            .then(|| (self.sxy / (self.sxx.sqrt() * self.syy.sqrt())).clamp(-1.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(got: f64, want: f64, digits: i32) {
        let tol = want.abs().max(1e-300) * 10f64.powi(-digits);
        assert!((got - want).abs() <= tol, "got {got}, want {want}");
    }

    #[test]
    fn simple_cases() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
        let v = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert_eq!(variance(&v, false), 4.0);
        close(variance(&v, true), 32.0 / 7.0, 14);
        let f = Fit::new(&[1.0, 2.0, 3.0, 4.0, 5.0], &[2.0, 4.0, 5.0, 4.0, 5.0]);
        close(f.slope().unwrap(), 0.6, 14);
        close(f.intercept().unwrap(), 2.2, 14);
        close(f.correlation().unwrap().powi(2), 0.6, 14);
    }

    #[test]
    fn a_flat_line_has_no_slope_or_correlation() {
        let f = Fit::new(&[2.0, 2.0, 2.0], &[1.0, 2.0, 3.0]);
        assert!(f.slope().is_none() && f.intercept().is_none() && f.correlation().is_none());
        let f = Fit::new(&[1.0, 2.0, 3.0], &[5.0, 5.0, 5.0]);
        assert_eq!(f.slope(), Some(0.0));
        assert_eq!(f.intercept(), Some(5.0));
        assert!(f.correlation().is_none());
    }

    /// NIST Statistical Reference Datasets, univariate: values chosen so
    /// that careless arithmetic loses the answer.
    #[test]
    fn nist_numerical_accuracy() {
        // NumAcc1: mean 10000002, standard deviation 1.
        let v = [10000001.0, 10000003.0, 10000002.0];
        assert_eq!(mean(&v), 10000002.0);
        assert_eq!(variance(&v, true).sqrt(), 1.0);

        // NumAcc2: mean 1.2, standard deviation 0.1.
        let mut v = vec![1.2];
        for _ in 0..500 {
            v.extend([1.1, 1.3]);
        }
        close(mean(&v), 1.2, 14);
        close(variance(&v, true).sqrt(), 0.1, 13);

        // NumAcc4: mean 10000000.2, standard deviation 0.1.
        let mut v = vec![10000000.2];
        for _ in 0..500 {
            v.extend([10000000.1, 10000000.3]);
        }
        close(mean(&v), 10000000.2, 14);
        close(variance(&v, true).sqrt(), 0.1, 7);
    }

    /// NIST Statistical Reference Datasets, linear regression: Norris
    /// (36 observations, lower difficulty), against the certified values.
    #[test]
    fn nist_norris() {
        #[rustfmt::skip]
        let yx: [(f64, f64); 36] = [
            (0.1, 0.2), (338.8, 337.4), (118.1, 118.2), (888.0, 884.6),
            (9.2, 10.1), (228.1, 226.5), (668.5, 666.3), (998.5, 996.3),
            (449.1, 448.6), (778.9, 777.0), (559.2, 558.2), (0.3, 0.4),
            (0.1, 0.6), (778.1, 775.5), (668.8, 666.9), (339.3, 338.0),
            (448.9, 447.5), (10.8, 11.6), (557.7, 556.0), (228.3, 228.1),
            (998.0, 995.8), (888.8, 887.6), (119.6, 120.2), (0.3, 0.3),
            (0.6, 0.3), (557.6, 556.8), (339.3, 339.1), (888.0, 887.2),
            (998.5, 999.0), (778.9, 779.0), (10.2, 11.1), (117.6, 118.3),
            (228.9, 229.2), (668.4, 669.1), (449.2, 448.9), (0.2, 0.5),
        ];
        let x: Vec<f64> = yx.iter().map(|p| p.1).collect();
        let y: Vec<f64> = yx.iter().map(|p| p.0).collect();
        let f = Fit::new(&x, &y);
        close(f.slope().unwrap(), 1.00211681802045, 12);
        close(f.intercept().unwrap(), -0.262323073774029, 10);
        close(f.correlation().unwrap().powi(2), 0.999993745883712, 12);
    }
}
