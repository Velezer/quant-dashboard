//! Institutional Liquidity Area Tail Risk & Cascade Probability Engine.

use super::MIN_RELATIVE_DISTANCE_PRICE_DENOMINATOR;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityZoneInput {
    pub mid_price: f64,
    /// Optional pre-rounded distance used by the legacy Python contract.
    /// Generic callers can omit it and derive distance from current price.
    #[serde(default)]
    pub distance_pct: Option<f64>,
    pub density_score: f64,
    pub is_liquidation_cluster: bool,
    pub is_unswept: bool,
    pub is_partially_swept: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailRiskScoreResult {
    pub zone_index: usize,
    pub tail_risk_score: u32,
    pub tail_risk_label: String,
    pub density_component: f64,
    pub leverage_component: f64,
    pub clustering_component: f64,
    pub distance_component: f64,
    pub status_component: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailRiskReport {
    pub zone_scores: Vec<TailRiskScoreResult>,
    pub max_tail_risk_score: u32,
    pub max_tail_risk_label: String,
    pub cascade_probability: String,
}

fn tail_risk_label(score: u32) -> &'static str {
    if score <= 25 {
        "LOW"
    } else if score <= 50 {
        "MEDIUM"
    } else if score <= 75 {
        "HIGH"
    } else {
        "EXTREME"
    }
}

pub(super) fn cascade_probability_for_count(count: usize) -> &'static str {
    match count {
        0 => "NEGLIGIBLE",
        1 => "POSSIBLE",
        2 => "LIKELY",
        _ => "CRITICAL",
    }
}

/// Computes multi-factor institutional tail risk scores for liquidity zones.
pub fn compute_tail_risk(zones: &[LiquidityZoneInput], current_price: f64) -> TailRiskReport {
    let n = zones.len();
    if n == 0 || current_price <= 0.0 {
        return TailRiskReport {
            zone_scores: Vec::new(),
            max_tail_risk_score: 0,
            max_tail_risk_label: "LOW".to_string(),
            cascade_probability: "NEGLIGIBLE".to_string(),
        };
    }

    let mut zone_scores = Vec::with_capacity(n);
    let mut high_risk_near_zones = 0usize;

    for i in 0..n {
        let z = &zones[i];
        let dist_pct = z
            .distance_pct
            .map(|value| value.abs())
            .unwrap_or_else(|| ((z.mid_price - current_price) / current_price).abs() * 100.0);

        // 1. Zone Density (25% weight)
        let density_comp = (z.density_score.clamp(0.0, 100.0) / 100.0) * 25.0;

        // 2. Leverage Proximity (30% weight)
        let mut min_cluster_dist = f64::MAX;
        for other in zones.iter() {
            if other.is_liquidation_cluster {
                let d = ((z.mid_price - other.mid_price)
                    / z.mid_price.max(MIN_RELATIVE_DISTANCE_PRICE_DENOMINATOR))
                .abs()
                    * 100.0;
                if d < min_cluster_dist {
                    min_cluster_dist = d;
                }
            }
        }
        let lev_comp = if min_cluster_dist <= 1.0 {
            30.0
        } else if min_cluster_dist <= 2.0 {
            20.0
        } else if min_cluster_dist <= 5.0 {
            10.0
        } else {
            0.0
        };

        // 3. Zone Clustering (20% weight)
        let mut cluster_count = 0usize;
        for (j, other) in zones.iter().enumerate() {
            if i != j {
                let d = ((z.mid_price - other.mid_price) / z.mid_price).abs() * 100.0;
                if d <= 2.0 {
                    cluster_count += 1;
                }
            }
        }
        let cluster_comp = match cluster_count {
            0 => 0.0,
            1 => 7.0,
            2 => 14.0,
            _ => 20.0,
        };

        // 4. Distance from Price (15% weight)
        let dist_comp = (15.0 - dist_pct * 2.0).max(0.0);

        // 5. Status (10% weight)
        let status_comp = if z.is_unswept {
            10.0
        } else if z.is_partially_swept {
            5.0
        } else {
            0.0
        };

        let raw_score = density_comp + lev_comp + cluster_comp + dist_comp + status_comp;
        // Python's built-in round uses ties-to-even; use the same rule for
        // exact parity at .5 boundaries (Rust's `round` rounds away from 0).
        let score = raw_score.round_ties_even().clamp(0.0, 100.0) as u32;

        let label = tail_risk_label(score);

        if dist_pct <= 3.0 && score > 50 {
            high_risk_near_zones += 1;
        }

        zone_scores.push(TailRiskScoreResult {
            zone_index: i,
            tail_risk_score: score,
            tail_risk_label: label.to_string(),
            density_component: density_comp,
            leverage_component: lev_comp,
            clustering_component: cluster_comp,
            distance_component: dist_comp,
            status_component: status_comp,
        });
    }

    let max_score = zone_scores
        .iter()
        .map(|z| z.tail_risk_score)
        .max()
        .unwrap_or(0);
    let max_label = tail_risk_label(max_score);
    let cascade_prob = cascade_probability_for_count(high_risk_near_zones);

    TailRiskReport {
        zone_scores,
        max_tail_risk_score: max_score,
        max_tail_risk_label: max_label.to_string(),
        cascade_probability: cascade_prob.to_string(),
    }
}
