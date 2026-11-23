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
#[derive(Clone, Copy, Debug)]
pub struct Fit {
    n: f64,
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
            n: x.len() as f64,
            mean_x,
            mean_y,
            sxx,
            syy,
            sxy,
        }
    }

    /// The same sums with point (x, y) taken out. Exact algebra on the
    /// centred sums, so leaving each point out in turn costs O(n) in all.
    fn without(&self, x: f64, y: f64) -> Fit {
        let n = self.n;
        let (dx, dy) = (x - self.mean_x, y - self.mean_y);
        let k = n / (n - 1.0);
        Fit {
            n: n - 1.0,
            mean_x: (n * self.mean_x - x) / (n - 1.0),
            mean_y: (n * self.mean_y - y) / (n - 1.0),
            sxx: self.sxx - k * dx * dx,
            syy: self.syy - k * dy * dy,
            sxy: self.sxy - k * dx * dy,
        }
    }

    /// 95% confidence intervals for the least-squares slope and intercept,
    /// from the residual scatter and Student's t with n − 2 degrees of
    /// freedom. Needs at least three points that do not all share one x.
    pub fn intervals(&self) -> Option<Interval2> {
        let b = self.slope()?;
        let a = self.intercept()?;
        if self.n < 3.0 {
            return None;
        }
        let sse = (self.syy - self.sxy * self.sxy / self.sxx).max(0.0);
        let s2 = sse / (self.n - 2.0);
        let se_b = (s2 / self.sxx).sqrt();
        let se_a = (s2 * (1.0 / self.n + self.mean_x * self.mean_x / self.sxx)).sqrt();
        let t = t_quantile(0.975, self.n - 2.0);
        Some(Interval2 {
            slope: (b - t * se_b, b + t * se_b),
            intercept: (a - t * se_a, a + t * se_a),
        })
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

/// Lower and upper 95% limits for a slope and an intercept.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval2 {
    pub slope: (f64, f64),
    pub intercept: (f64, f64),
}

/// A fitted line with 95% confidence intervals, when there are enough
/// points to give them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub slope: f64,
    pub intercept: f64,
    pub ci: Option<Interval2>,
}

// ------------------------------------------------------------ Deming

/// Deming slope through the centred sums. `ratio` is the variance of the
/// measurement error in x divided by that in y (the convention of CLSI
/// EP09 and the R package mcr). `None` when x and y do not vary together.
fn deming_slope(f: &Fit, ratio: f64) -> Option<f64> {
    if f.sxy == 0.0 || !(f.sxx > 0.0 || f.syy > 0.0) {
        return None;
    }
    // In terms of d = σ²(y error) / σ²(x error) = 1 / ratio:
    // b = (u + √(u² + 4·d·sxy²)) / (2·sxy) with u = syy − d·sxx. The second
    // form is the same number without cancellation when u < 0.
    let d = 1.0 / ratio;
    let u = f.syy - d * f.sxx;
    let r = (u * u + 4.0 * d * f.sxy * f.sxy).sqrt();
    let b = if u >= 0.0 {
        (u + r) / (2.0 * f.sxy)
    } else {
        2.0 * d * f.sxy / (r - u)
    };
    b.is_finite().then_some(b)
}

/// Deming regression: a straight line that allows for measurement error in
/// both x and y. Confidence intervals come from the jackknife (leaving
/// each point out in turn), as Linnet (1990) describes, with Student's t on
/// n − 2 degrees of freedom; they need at least three points.
pub fn deming(x: &[f64], y: &[f64], ratio: f64) -> Option<Line> {
    let f = Fit::new(x, y);
    let b = deming_slope(&f, ratio)?;
    let a = f.mean_y - b * f.mean_x;
    let ci = deming_intervals(&f, x, y, ratio, b, a);
    Some(Line {
        slope: b,
        intercept: a,
        ci,
    })
}

fn deming_intervals(
    f: &Fit,
    x: &[f64],
    y: &[f64],
    ratio: f64,
    b: f64,
    a: f64,
) -> Option<Interval2> {
    let n = x.len();
    if n < 3 {
        return None;
    }
    let mut bs = Vec::with_capacity(n);
    let mut is = Vec::with_capacity(n);
    for (xi, yi) in x.iter().zip(y) {
        let g = f.without(*xi, *yi);
        let bi = deming_slope(&g, ratio)?;
        bs.push(bi);
        is.push(g.mean_y - bi * g.mean_x);
    }
    let t = t_quantile(0.975, n as f64 - 2.0);
    // Jackknife standard error: √((n − 1)/n · Σ(θ₋ᵢ − mean θ₋ᵢ)²).
    let se = |v: &[f64]| {
        let m = mean(v);
        let ss: f64 = v.iter().map(|e| (e - m) * (e - m)).sum();
        ((n as f64 - 1.0) / n as f64 * ss).sqrt()
    };
    let (se_b, se_a) = (se(&bs), se(&is));
    Some(Interval2 {
        slope: (b - t * se_b, b + t * se_b),
        intercept: (a - t * se_a, a + t * se_a),
    })
}

// -------------------------------------------------- Passing-Bablok

/// Most points Passing-Bablok takes. It looks at every pair of points, so
/// the work grows with the square: 1,000 points are half a million slopes.
pub const PB_MAX_POINTS: usize = 1_000;

/// Why Passing-Bablok gave no line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PbError {
    TooFewPoints,
    /// Most pairs of points slope downwards: the method assumes the two
    /// measurements rise together.
    Negative,
}

/// Passing-Bablok regression (Passing and Bablok, 1983): the slope is a
/// shifted median of the slopes between every pair of points, so a few
/// outliers barely move it and no assumption is made about the errors.
/// The 95% intervals need about eight points or more.
pub fn passing_bablok(x: &[f64], y: &[f64]) -> Result<Line, PbError> {
    let n = x.len();
    // Sorting by x (then y) makes pairs with the same x vertical upwards,
    // so they count as +∞ whatever order the data came in.
    let mut pts: Vec<(f64, f64)> = x.iter().copied().zip(y.iter().copied()).collect();
    pts.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1)));
    let mut s = Vec::with_capacity(n * n.saturating_sub(1) / 2);
    for (i, p) in pts.iter().enumerate() {
        for q in &pts[i + 1..] {
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            if dx == 0.0 && dy == 0.0 {
                continue;
            }
            let v = if dx == 0.0 { f64::INFINITY } else { dy / dx };
            // A slope of exactly −1 says nothing about the line, since the
            // method treats x and y alike.
            if v != -1.0 {
                s.push(v);
            }
        }
    }
    let big_n = s.len();
    if big_n == 0 {
        return Err(PbError::TooFewPoints);
    }
    // Shift by the number of slopes below −1 so that the estimate does not
    // depend on which method is called x.
    let k = s.iter().filter(|v| **v < -1.0).count();
    // The m-th smallest slope, counting from 1, shifted by k.
    let mut nth = |m: usize| -> Option<f64> {
        let i = (m + k).checked_sub(1)?;
        (i < big_n).then(|| *s.select_nth_unstable_by(i, f64::total_cmp).1)
    };
    let b = if big_n % 2 == 1 {
        nth(big_n.div_ceil(2))
    } else {
        nth(big_n / 2)
            .zip(nth(big_n / 2 + 1))
            .map(|(l, h)| (l + h) / 2.0)
    }
    .filter(|b| b.is_finite() && *b > 0.0)
    .ok_or(PbError::Negative)?;
    // Rank limits for the 95% interval of the slope.
    let nf = n as f64;
    let c = Z95 * (nf * (nf - 1.0) * (2.0 * nf + 5.0) / 18.0).sqrt();
    let m1 = ((big_n as f64 - c) / 2.0).round();
    let ci = if m1 >= 1.0 {
        let m1 = m1 as usize;
        let m2 = big_n - m1 + 1;
        match (nth(m1), nth(m2)) {
            (Some(lo), Some(hi)) if lo.is_finite() && hi.is_finite() => Some((lo, hi)),
            _ => None,
        }
    } else {
        None
    };
    let icpt = |slope: f64| median(&pts.iter().map(|p| p.1 - slope * p.0).collect::<Vec<_>>());
    Ok(Line {
        slope: b,
        intercept: icpt(b),
        ci: ci.map(|(lo, hi)| Interval2 {
            slope: (lo, hi),
            intercept: (icpt(hi), icpt(lo)),
        }),
    })
}

// ------------------------------------------------- machine learning

/// The logistic function, 1 / (1 + e⁻ˣ), without overflow for large |x|.
pub fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

/// ln Σ eˣ, without overflow: the largest value is taken out first.
pub fn logsumexp(v: &[f64]) -> f64 {
    let m = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !m.is_finite() {
        return m;
    }
    m + v.iter().map(|x| (x - m).exp()).sum::<f64>().ln()
}

/// Probabilities from scores (logits): eˣ / Σ eˣ.
pub fn softmax(v: &[f64]) -> Vec<f64> {
    let l = logsumexp(v);
    v.iter().map(|x| (x - l).exp()).collect()
}

// ------------------------------------------------------- evaluation

/// Wilson score interval (95%) for a proportion of `k` successes out of
/// `n`. Unlike the textbook p ± 1.96·√(p(1 − p)/n) it stays inside 0 to 1
/// and behaves for small n and for accuracies near 0% or 100%.
pub fn wilson(k: f64, n: f64) -> (f64, f64) {
    let z = Z95;
    let p = k / n;
    let z2n = z * z / n;
    let centre = (p + z2n / 2.0) / (1.0 + z2n);
    let half = z * (p * (1.0 - p) / n + z2n / (4.0 * n)).sqrt() / (1.0 + z2n);
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

/// McNemar's test for two models scored on the same items: `b` items only
/// the first got right, `c` only the second. Two-sided p-value: exact
/// (binomial) below 25 disagreements, otherwise the chi-squared test with
/// continuity correction.
pub fn mcnemar(b: u64, c: u64) -> f64 {
    let n = b + c;
    if n == 0 {
        return 1.0;
    }
    if n < 25 {
        // P(X ≤ min(b, c)) for X ~ Binomial(n, ½), doubled.
        let k = b.min(c);
        let ln_half_n = n as f64 * 0.5f64.ln();
        let tail: f64 = (0..=k).map(|i| (ln_choose(n, i) + ln_half_n).exp()).sum();
        return (2.0 * tail).min(1.0);
    }
    let d = (b as f64 - c as f64).abs() - 1.0;
    let chi2 = d.max(0.0).powi(2) / n as f64;
    gamma_q(0.5, chi2 / 2.0)
}

fn ln_choose(n: u64, k: u64) -> f64 {
    ln_gamma(n as f64 + 1.0) - ln_gamma(k as f64 + 1.0) - ln_gamma((n - k) as f64 + 1.0)
}

/// 95% confidence interval for the mean of paired differences b − a, from
/// Student's t. For per-item scores of two models on the same items.
pub fn paired_interval(a: &[f64], b: &[f64]) -> Option<(f64, f64)> {
    let n = a.len();
    if n < 2 || b.len() != n {
        return None;
    }
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| y - x).collect();
    let m = mean(&d);
    let se = (variance(&d, true) / n as f64).sqrt();
    let t = t_quantile(0.975, n as f64 - 1.0);
    Some((m - t * se, m + t * se))
}

/// Items needed so that the 95% interval of an accuracy `p` is ± `margin`.
pub fn items_needed(p: f64, margin: f64) -> f64 {
    (Z95 * Z95 * p * (1.0 - p) / (margin * margin)).ceil()
}

/// Holm's step-down adjustment of p-values for many comparisons, in the
/// order given. Each is at least as large as the p-value it adjusts.
pub fn holm(p: &[f64]) -> Vec<f64> {
    let m = p.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|a, b| p[*a].total_cmp(&p[*b]));
    let mut out = vec![0.0; m];
    let mut running = 0.0f64;
    for (rank, i) in order.into_iter().enumerate() {
        running = running.max(((m - rank) as f64 * p[i]).min(1.0));
        out[i] = running;
    }
    out
}

// ------------------------------------------------------ distributions

/// The normal quantile for a two-sided 95% interval.
pub const Z95: f64 = 1.959_963_984_540_054;

/// The value t below which a fraction `p` of Student's t distribution with
/// `df` degrees of freedom lies, for p in (0.5, 1).
pub fn t_quantile(p: f64, df: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 1.0);
    while t_cdf(hi, df) < p && hi < 1e12 {
        lo = hi;
        hi *= 2.0;
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        if mid == lo || mid == hi {
            break;
        }
        if t_cdf(mid, df) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

/// P(T ≤ t) for t ≥ 0.
fn t_cdf(t: f64, df: f64) -> f64 {
    1.0 - 0.5 * beta_inc(df / 2.0, 0.5, df / (df + t * t))
}

/// Regularised incomplete beta function I_x(a, b), by the continued
/// fraction in Numerical Recipes (modified Lentz).
fn beta_inc(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let ln_front = ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln();
    if x > (a + 1.0) / (a + b + 2.0) {
        return 1.0 - beta_inc(b, a, 1.0 - x);
    }
    const TINY: f64 = 1e-300;
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < TINY {
        d = TINY;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..300 {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-16 {
            break;
        }
    }
    ln_front.exp() * h / a
}

/// Upper regularised incomplete gamma function Q(a, x), by its series for
/// x < a + 1 and its continued fraction otherwise (Numerical Recipes).
fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    let ln_front = a * x.ln() - x - ln_gamma(a);
    if x < a + 1.0 {
        let (mut ap, mut sum) = (a, 1.0 / a);
        let mut del = sum;
        for _ in 0..500 {
            ap += 1.0;
            del *= x / ap;
            sum += del;
            if del.abs() < sum.abs() * 1e-16 {
                break;
            }
        }
        return 1.0 - sum * ln_front.exp();
    }
    const TINY: f64 = 1e-300;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / TINY;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..500 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < TINY {
            d = TINY;
        }
        c = b + an / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-16 {
            break;
        }
    }
    ln_front.exp() * h
}

/// ln Γ(x) for x > 0 (Lanczos approximation, g = 7, about 15 digits).
fn ln_gamma(x: f64) -> f64 {
    const G: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        // Reflection: Γ(x)Γ(1 − x) = π / sin(πx).
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let t = x + 7.5;
    let s = G[1..]
        .iter()
        .enumerate()
        .fold(G[0], |s, (i, g)| s + g / (x + i as f64 + 1.0));
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + s.ln()
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
        // Certified standard deviations of the estimates, which give the
        // 95% intervals with t(0.975, 34).
        let ci = f.intervals().unwrap();
        let t = t_quantile(0.975, 34.0);
        close(
            (ci.slope.1 - ci.slope.0) / (2.0 * t),
            0.429796848199937e-3,
            10,
        );
        close(
            (ci.intercept.1 - ci.intercept.0) / (2.0 * t),
            0.232818234301152,
            10,
        );
    }

    /// Reference values computed in R 4.6 by independent code: `qt`, `lm`
    /// with `confint`, Deming as the major axis of rescaled data with the
    /// jackknife written out, and Passing-Bablok written from the paper.
    /// Thirty points with one outlier (12) and a tie in x (5 and 20).
    #[rustfmt::skip]
    const X: [f64; 30] = [
        398.8, 183.6, 88.2, 74.0, 140.9, 332.0, 173.2, 397.8, 99.7, 218.6,
        111.1, 135.5, 323.6, 78.1, 207.3, 74.8, 250.3, 53.7, 394.5, 140.9,
        270.6, 159.0, 392.4, 366.1, 397.9, 80.3, 264.8, 220.3, 382.1, 174.7,
    ];
    #[rustfmt::skip]
    const Y: [f64; 30] = [
        405.6, 204.4, 95.3, 74.8, 144.8, 335.0, 172.7, 405.5, 123.0, 222.2,
        115.2, 180.7, 332.3, 85.7, 207.2, 80.8, 254.1, 56.5, 413.4, 167.0,
        290.8, 162.3, 421.2, 388.8, 414.4, 76.4, 269.4, 223.0, 395.7, 168.0,
    ];

    fn same_line(got: Line, want: [f64; 6], digits: i32) {
        let ci = got.ci.unwrap();
        let got = [
            got.slope,
            got.intercept,
            ci.slope.0,
            ci.slope.1,
            ci.intercept.0,
            ci.intercept.1,
        ];
        for (g, w) in got.iter().zip(want) {
            close(*g, w, digits);
        }
    }

    #[test]
    fn t_quantiles() {
        let want = [
            (1.0, 12.7062047361747),
            (2.0, 4.30265272974946),
            (5.0, 2.57058183563631),
            (10.0, 2.22813885198627),
            (28.0, 2.04840714179524),
            (30.0, 2.04227245630124),
            (100.0, 1.98397151852355),
            (1000.0, 1.96233908082641),
        ];
        for (df, t) in want {
            close(t_quantile(0.975, df), t, 12);
        }
    }

    #[test]
    fn least_squares_intervals() {
        let f = Fit::new(&X, &Y);
        let ci = f.intervals().unwrap();
        let line = Line {
            slope: f.slope().unwrap(),
            intercept: f.intercept().unwrap(),
            ci: Some(ci),
        };
        same_line(
            line,
            [
                1.02181651049259,
                5.25808805694664,
                0.98601411241609,
                1.05761890856909,
                -3.65571027940811,
                14.1718863933014,
            ],
            12,
        );
        assert!(Fit::new(&[1.0, 2.0], &[1.0, 3.0]).intervals().is_none());
    }

    #[test]
    fn deming_against_reference() {
        let want1 = [
            1.0261010744375,
            4.31765483479907,
            0.997284379935127,
            1.05491776893987,
            -4.21152850742233,
            12.8468381770205,
        ];
        same_line(deming(&X, &Y, 1.0).unwrap(), want1, 11);
        let want = [
            1.0235516019089,
            4.87724705834276,
            0.99362754522936,
            1.05347565858844,
            -3.98370409908105,
            13.7381982157666,
        ];
        same_line(deming(&X, &Y, 0.25).unwrap(), want, 11);
    }

    #[test]
    fn deming_properties() {
        // Points on a line give that line, whatever the ratio.
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y: Vec<f64> = x.iter().map(|v| 2.0 * v + 3.0).collect();
        for ratio in [0.01, 1.0, 100.0] {
            let l = deming(&x, &y, ratio).unwrap();
            close(l.slope, 2.0, 13);
            close(l.intercept, 3.0, 12);
        }
        // Swapping the axes inverts the slope (and the ratio).
        let l = deming(&X, &Y, 0.25).unwrap();
        let m = deming(&Y, &X, 4.0).unwrap();
        close(l.slope * m.slope, 1.0, 13);
        // With no error in x it becomes least squares of y on x.
        let ols = Fit::new(&X, &Y).slope().unwrap();
        close(deming(&X, &Y, 1e-9).unwrap().slope, ols, 7);
        // No covariance, no line; two points, no interval.
        assert!(deming(&[1.0, 2.0, 3.0], &[5.0, 5.0, 5.0], 1.0).is_none());
        assert!(deming(&[1.0, 2.0], &[1.0, 3.0], 1.0).unwrap().ci.is_none());
    }

    #[test]
    fn passing_bablok_against_reference() {
        let want = [
            1.0261780104712,
            0.710209424083764,
            0.999399759903961,
            1.06024096385542,
            -5.57168674698796,
            6.54213685474193,
        ];
        same_line(passing_bablok(&X, &Y).unwrap(), want, 12);
        // The order of the points makes no difference.
        let mut p: Vec<(f64, f64)> = X.iter().copied().zip(Y).collect();
        p.reverse();
        p.swap(0, 7);
        let (x, y): (Vec<f64>, Vec<f64>) = p.into_iter().unzip();
        same_line(passing_bablok(&x, &y).unwrap(), want, 12);
    }

    #[test]
    fn passing_bablok_properties() {
        // Points on a line give that line exactly.
        let x = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let y: Vec<f64> = x.iter().map(|v| 2.0 * v + 3.0).collect();
        let l = passing_bablok(&x, &y).unwrap();
        assert_eq!((l.slope, l.intercept), (2.0, 3.0));
        assert_eq!(l.ci.unwrap().slope, (2.0, 2.0));
        // Three points by hand: slopes 1 (1→2), 3 (2→3) and 2 (1→3); the
        // median is 2 and the intercept the median of 1, 0, 1.
        let l = passing_bablok(&[1.0, 2.0, 3.0], &[3.0, 4.0, 7.0]).unwrap();
        assert_eq!((l.slope, l.intercept), (2.0, 1.0));
        assert!(l.ci.is_none());
        // Swapping the axes inverts the slope.
        let a = passing_bablok(&X, &Y).unwrap();
        let b = passing_bablok(&Y, &X).unwrap();
        close(a.slope * b.slope, 1.0, 12);
        // An outlier barely moves it; least squares moves a lot more.
        let mut y2 = Y;
        y2[3] += 400.0;
        let moved = passing_bablok(&X, &y2).unwrap().slope - a.slope;
        let ols = Fit::new(&X, &y2).slope().unwrap() - Fit::new(&X, &Y).slope().unwrap();
        assert!(moved.abs() < 0.01 && ols.abs() > 0.1, "{moved} {ols}");
        // Falling data, and a single point, give no line.
        assert_eq!(
            passing_bablok(&[1.0, 2.0, 3.0, 4.0], &[8.0, 6.0, 4.0, 2.0]),
            Err(PbError::Negative)
        );
        assert_eq!(passing_bablok(&[1.0], &[1.0]), Err(PbError::TooFewPoints));
    }

    #[test]
    fn machine_learning_functions() {
        assert_eq!(sigmoid(0.0), 0.5);
        close(sigmoid(2.0), 0.880797077977882, 14);
        assert_eq!(sigmoid(-1000.0), 0.0);
        assert_eq!(sigmoid(1000.0), 1.0);
        close(logsumexp(&[1.0, 2.0, 3.0]), 3.40760596444438, 14);
        // Large scores do not overflow.
        close(logsumexp(&[1000.0, 1000.0]), 1000.0 + 2f64.ln(), 14);
        let p = softmax(&[1.0, 2.0, 3.0]);
        close(p[2], 0.665240955774822, 14);
        close(p.iter().sum::<f64>(), 1.0, 14);
    }

    /// Reference values from R 4.6: prop.test (correct = FALSE) for the
    /// Wilson interval, mcnemar.test and binom.test, t.test (paired),
    /// p.adjust (holm), pchisq and qnorm.
    #[test]
    fn evaluation_statistics() {
        let pair = |got: (f64, f64), want: (f64, f64)| {
            close(got.0 + 1.0, want.0 + 1.0, 13);
            close(got.1, want.1, 13);
        };
        pair(wilson(82.0, 100.0), (0.733326416136993, 0.882997745010536));
        pair(wilson(0.0, 20.0), (0.0, 0.161125158052819));
        pair(
            wilson(1990.0, 2000.0),
            (0.990820304050058, 0.997281819141501),
        );
        close(mcnemar(30, 12), 0.00871191296237959, 12);
        close(mcnemar(120, 90), 0.0453719050658386, 12);
        close(mcnemar(7, 1), 0.0703125, 13);
        close(mcnemar(1, 7), 0.0703125, 13);
        assert_eq!(mcnemar(12, 12), 1.0);
        close(mcnemar(3, 0), 0.25, 13);
        assert_eq!(mcnemar(0, 0), 1.0);
        close(gamma_q(0.5, 3.84 / 2.0), 0.050043521248705, 12);
        close(gamma_q(0.5, 0.3 / 2.0), 0.583882420770365, 12);
        let a = [0.61, 0.72, 0.55, 0.80, 0.67, 0.59, 0.70, 0.64, 0.75, 0.58];
        let b = [0.66, 0.74, 0.60, 0.79, 0.71, 0.65, 0.73, 0.70, 0.77, 0.62];
        pair(
            paired_interval(&a, &b).unwrap(),
            (0.0204731110206968, 0.0515268889793032),
        );
        assert!(paired_interval(&[1.0], &[2.0]).is_none());
        assert_eq!(holm(&[0.01, 0.04, 0.03, 0.2]), [0.04, 0.09, 0.09, 0.2]);
        assert_eq!(holm(&[0.5, 0.9]), [1.0, 1.0]);
        assert_eq!(items_needed(0.8, 0.02), 1537.0);
    }
}
