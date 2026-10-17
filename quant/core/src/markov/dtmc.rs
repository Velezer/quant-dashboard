//! Discrete-Time Markov Chain (DTMC) Transition Probability Matrix & Stationary Eigensolver.

use serde::{Deserialize, Serialize};

pub const NUM_CANONICAL_REGIMES: usize = 6;
pub const TRANSITION_DIRICHLET_ALPHA: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum RegimeId {
    BullTrend = 0,
    BearTrend = 1,
    Accumulation = 2,
    Distribution = 3,
    PullbackReset = 4,
    ConsolidationChop = 5,
}

impl RegimeId {
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => RegimeId::BullTrend,
            1 => RegimeId::BearTrend,
            2 => RegimeId::Accumulation,
            3 => RegimeId::Distribution,
            4 => RegimeId::PullbackReset,
            _ => RegimeId::ConsolidationChop,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            RegimeId::BullTrend => "BULL_TREND",
            RegimeId::BearTrend => "BEAR_TREND",
            RegimeId::Accumulation => "ACCUMULATION",
            RegimeId::Distribution => "DISTRIBUTION",
            RegimeId::PullbackReset => "PULLBACK_RESET",
            RegimeId::ConsolidationChop => "CONSOLIDATION_CHOP",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DtmcModel {
    pub num_states: usize,
    pub transition_counts: Vec<Vec<f64>>,
    pub transition_matrix: Vec<Vec<f64>>,
    pub persistence: Vec<f64>,
    pub expected_duration: Vec<f64>,
    pub stationary_distribution: Vec<f64>,
    pub transition_row_support: Vec<u64>,
}

impl DtmcModel {
    /// Builds and solves a DTMC model from historical regime sequences.
    pub fn from_sequences(sequences: &[Vec<usize>], num_states: usize) -> Self {
        let k = num_states;
        let mut counts = vec![vec![0.0; k]; k];

        // 1. Accumulate transition counts N_ij
        for seq in sequences {
            if seq.len() < 2 {
                continue;
            }
            for t in 0..(seq.len() - 1) {
                let from_s = seq[t];
                let to_s = seq[t + 1];
                if from_s < k && to_s < k {
                    counts[from_s][to_s] += 1.0;
                }
            }
        }

        // 2. Apply a weak symmetric Dirichlet prior so sparse rows remain
        // estimable without pretending unsupported transitions were observed.
        let mut p_matrix = vec![vec![0.0; k]; k];
        let mut transition_row_support = vec![0; k];
        for i in 0..k {
            let row_sum: f64 = counts[i].iter().sum();
            transition_row_support[i] = row_sum as u64;
            if row_sum > 0.0 {
                let denominator = row_sum + TRANSITION_DIRICHLET_ALPHA * k as f64;
                for j in 0..k {
                    p_matrix[i][j] = (counts[i][j] + TRANSITION_DIRICHLET_ALPHA) / denominator;
                }
            } else {
                for probability in &mut p_matrix[i] {
                    *probability = 1.0 / k as f64;
                }
            }
        }

        // 3. Diagonal persistence P_ii
        let mut persistence = vec![0.0; k];
        for i in 0..k {
            persistence[i] = p_matrix[i][i];
        }

        // 4. Expected dwell time E[T_i] = 1 / (1 - P_ii)
        let mut expected_duration = vec![1.0; k];
        for i in 0..k {
            let p_stay = persistence[i];
            if p_stay < 0.9999 {
                expected_duration[i] = 1.0 / (1.0 - p_stay).max(1e-4);
            } else {
                expected_duration[i] = 99.0;
            }
        }

        // 5. Stationary distribution solver via power iteration: pi * P = pi
        let mut pi = vec![1.0 / (k as f64); k];
        for _ in 0..100 {
            let mut next_pi = vec![0.0; k];
            for j in 0..k {
                let mut sum = 0.0;
                for i in 0..k {
                    sum += pi[i] * p_matrix[i][j];
                }
                next_pi[j] = sum;
            }

            let mut diff = 0.0;
            for i in 0..k {
                diff += (next_pi[i] - pi[i]).abs();
            }

            pi = next_pi;
            if diff < 1e-7 {
                break;
            }
        }

        // Re-normalize stationary distribution
        let total_pi: f64 = pi.iter().sum();
        if total_pi > 0.0 {
            for v in pi.iter_mut() {
                *v /= total_pi;
            }
        }

        DtmcModel {
            num_states: k,
            transition_counts: counts,
            transition_matrix: p_matrix,
            persistence,
            expected_duration,
            stationary_distribution: pi,
            transition_row_support,
        }
    }

    /// Returns the most frequent next state, including persistence in the current state.
    pub fn predict_next_state(&self, current_state: usize) -> (usize, f64) {
        if current_state >= self.num_states {
            return (0, 0.0);
        }
        if self.transition_row_support[current_state] == 0 {
            return (current_state, 0.0);
        }
        let row = &self.transition_matrix[current_state];
        let mut best_target = current_state;
        let mut best_prob = 0.0;

        for (j, &probability) in row.iter().enumerate().take(self.num_states) {
            if probability > best_prob {
                best_prob = probability;
                best_target = j;
            }
        }

        (best_target, best_prob)
    }
}

#[cfg(test)]
#[path = "dtmc_tests.rs"]
mod tests;
