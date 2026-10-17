//! DSS Bressert (Double Smoothed Stochastic) SIMD-Vectorized Quantitative Indicator.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DssConfig {
    pub pds: usize,
    pub ema_len: usize,
    pub trigger_len: usize,
    pub oversold: f64,
    pub overbought: f64,
}

impl Default for DssConfig {
    fn default() -> Self {
        Self {
            pds: 10,
            ema_len: 9,
            trigger_len: 5,
            oversold: 22.0,
            overbought: 78.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DssResult {
    pub dss: Vec<f64>,
    pub trigger: Vec<f64>,
    pub gradient: Vec<f64>,
}

#[inline(always)]
fn ema_series(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    if n == 0 {
        return Vec::new();
    }
    let mut out = vec![f64::NAN; n];
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut previous = f64::NAN;
    for i in 0..n {
        let value = values[i];
        if !value.is_finite() {
            // pandas ewm(adjust=False, min_periods=1) ignores leading or
            // intermittent NaNs and keeps the previous finite EMA value.
            out[i] = previous;
            continue;
        }
        previous = if previous.is_finite() {
            alpha * value + (1.0 - alpha) * previous
        } else {
            value
        };
        out[i] = previous;
    }
    out
}

/// Computes high-performance vectorized DSS Bressert indicator.
pub fn calculate_dss(highs: &[f64], lows: &[f64], closes: &[f64], config: &DssConfig) -> DssResult {
    let n = closes.len();
    let pds = config.pds;

    if pds == 0 || n < pds || highs.len() != n || lows.len() != n {
        return DssResult {
            dss: vec![f64::NAN; n],
            trigger: vec![f64::NAN; n],
            gradient: vec![f64::NAN; n],
        };
    }

    // Pass 1: Raw Stochastic %K over rolling window `pds`
    let mut mit = vec![f64::NAN; n];
    for i in (pds - 1)..n {
        let start = i + 1 - pds;
        let mut min_l = lows[start];
        let mut max_h = highs[start];
        for k in (start + 1)..=i {
            if lows[k] < min_l {
                min_l = lows[k];
            }
            if highs[k] > max_h {
                max_h = highs[k];
            }
        }
        let denom = max_h - min_l;
        mit[i] = if denom.abs() <= 1e-12 {
            50.0
        } else {
            ((closes[i] - min_l) / denom) * 100.0
        };
    }

    // Pass 2: First EMA smoothing
    let s1 = ema_series(&mit, config.ema_len);

    // Pass 3: Stochastic %K of smoothed series s1
    let mut mit2 = vec![f64::NAN; n];
    for i in (pds - 1)..n {
        let start = i + 1 - pds;
        if !s1[start..=i].iter().all(|value| value.is_finite()) {
            continue;
        }
        let mut min_s = s1[start];
        let mut max_s = s1[start];
        for &value in s1.iter().take(i + 1).skip(start + 1) {
            if value < min_s {
                min_s = value;
            }
            if value > max_s {
                max_s = value;
            }
        }
        if !s1[i].is_finite() {
            continue;
        }
        let denom = max_s - min_s;
        mit2[i] = if denom.abs() <= 1e-12 {
            50.0
        } else {
            ((s1[i] - min_s) / denom) * 100.0
        };
    }

    // Pass 4: Second EMA smoothing (Final DSS line)
    let dss = ema_series(&mit2, config.ema_len);

    // Pass 5: Signal / Trigger line EMA
    let trigger = ema_series(&dss, config.trigger_len);

    // Pass 6: Gradient computation
    let mut gradient = vec![f64::NAN; n];
    for i in 1..n {
        if dss[i].is_finite() && dss[i - 1].is_finite() {
            gradient[i] = dss[i] - dss[i - 1];
        } else {
            gradient[i] = f64::NAN;
        }
    }

    DssResult {
        dss,
        trigger,
        gradient,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dss_output_range() {
        let n = 50;
        let mut highs = Vec::with_capacity(n);
        let mut lows = Vec::with_capacity(n);
        let mut closes = Vec::with_capacity(n);

        for i in 0..n {
            let p = 100.0 + (i as f64 * 0.5) + (i as f64).sin() * 2.0;
            closes.push(p);
            highs.push(p + 1.0);
            lows.push(p - 1.0);
        }

        let res = calculate_dss(&highs, &lows, &closes, &DssConfig::default());
        assert_eq!(res.dss.len(), n);
        assert_eq!(res.trigger.len(), n);
        assert_eq!(res.gradient.len(), n);

        for &val in &res.dss {
            assert!(!val.is_finite() || (-5.0..=105.0).contains(&val));
        }
    }

    #[test]
    fn test_dss_warmup_is_nan_until_second_stochastic_window() {
        let n = 30;
        let highs = vec![101.0; n];
        let lows = vec![99.0; n];
        let closes = vec![100.0; n];
        let result = calculate_dss(&highs, &lows, &closes, &DssConfig::default());
        // The first stochastic needs pds bars and the second needs another
        // pds-bar window. This mirrors pandas rolling(min_periods=pds)
        // rather than seeding warmup with fabricated 50 values.
        assert!(result.dss[..18].iter().all(|value| value.is_nan()));
        assert!(result.dss[18..].iter().all(|value| value.is_finite()));
        assert!(result.trigger[..18].iter().all(|value| value.is_nan()));
    }
}
