use super::{qualifies, EvaluationPolicy, Observation};
use crate::strategies::StrategyBar;
use serde_json::{json, Value};

const MAX_STRATEGY_RETURN_AUTOCORRELATION_LAG_BARS: usize = 12;
const MIN_STRATEGY_RETURN_AUTOCORRELATION_PAIRS: usize = 10;

#[derive(Default)]
struct LaggedCorrelation {
    pairs: usize,
    x_sum: f64,
    y_sum: f64,
    x_square_sum: f64,
    y_square_sum: f64,
    product_sum: f64,
}

impl LaggedCorrelation {
    fn add(&mut self, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        self.pairs += 1;
        self.x_sum += x;
        self.y_sum += y;
        self.x_square_sum += x * x;
        self.y_square_sum += y * y;
        self.product_sum += x * y;
    }

    fn coefficient(&self) -> Option<f64> {
        if self.pairs < MIN_STRATEGY_RETURN_AUTOCORRELATION_PAIRS {
            return None;
        }
        let count = self.pairs as f64;
        let x_variance = count * self.x_square_sum - self.x_sum * self.x_sum;
        let y_variance = count * self.y_square_sum - self.y_sum * self.y_sum;
        let denominator = (x_variance * y_variance).sqrt();
        let coefficient = (count * self.product_sum - self.x_sum * self.y_sum) / denominator;
        (denominator > 0.0 && coefficient.is_finite())
            .then(|| coefficient.clamp(-1.0, 1.0))
    }
}

fn next_bar_strategy_return_pct(
    bars: &[StrategyBar],
    signal_index: usize,
    observation: Observation,
    policy: EvaluationPolicy,
    end: usize,
) -> Option<f64> {
    let entry_index = signal_index.checked_add(1)?;
    let bar = bars.get(entry_index).filter(|_| entry_index < end)?;
    let entry_price = bar.open;
    let exit_price = bar.close;
    if !entry_price.is_finite()
        || !exit_price.is_finite()
        || entry_price <= 0.0
        || exit_price <= 0.0
    {
        return None;
    }

    let price_change = (exit_price - entry_price) / entry_price * 100.0;
    let (gross_return_pct, entry_fee_pct, exit_fee_pct) = if observation.long {
        (price_change, policy.buy_fee_pct, policy.sell_fee_pct)
    } else {
        (-price_change, policy.sell_fee_pct, policy.buy_fee_pct)
    };
    let return_pct = if policy.fees_known {
        crate::venue_rules::net_return_after_transaction_fees(
            gross_return_pct,
            exit_price / entry_price,
            entry_fee_pct,
            exit_fee_pct,
        )
    } else {
        gross_return_pct
    };
    return_pct.is_finite().then_some(return_pct)
}

fn series_report(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    start: usize,
    end: usize,
    policy: EvaluationPolicy,
) -> Value {
    let mut strategy_returns = vec![None; bars.len()];
    let mut signal_events = 0;
    for signal_index in start..end.min(observations.len()) {
        let Some(observation) = observations[signal_index].filter(|value| qualifies(*value, policy))
        else {
            continue;
        };
        let Some(return_pct) =
            next_bar_strategy_return_pct(bars, signal_index, observation, policy, end)
        else {
            continue;
        };
        strategy_returns[signal_index] = Some(return_pct);
        signal_events += 1;
    }

    let lags = (1..=MAX_STRATEGY_RETURN_AUTOCORRELATION_LAG_BARS)
        .map(|lag_bars| {
            let mut correlation = LaggedCorrelation::default();
            for left_index in start..end.min(strategy_returns.len()) {
                let Some(right_index) = left_index.checked_add(lag_bars) else {
                    break;
                };
                if right_index >= end || right_index >= strategy_returns.len() {
                    break;
                }
                if let (Some(left), Some(right)) =
                    (strategy_returns[left_index], strategy_returns[right_index])
                {
                    correlation.add(left, right);
                }
            }
            json!({
                "lag_bars": lag_bars,
                "pairs": correlation.pairs,
                "minimum_pairs": MIN_STRATEGY_RETURN_AUTOCORRELATION_PAIRS,
                "coefficient": correlation.coefficient(),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "weights_out_of_40": policy.weights,
        "minimum_score_out_of_50": policy.minimum_score,
        "qualifying_signal_events": signal_events,
        "lags": lags,
    })
}

pub(super) fn report(
    bars: &[StrategyBar],
    observations: &[Option<Observation>],
    training_start: usize,
    training_end: usize,
    baseline_policy: EvaluationPolicy,
    selected_weights: Option<[u8; 4]>,
) -> Value {
    let baseline = series_report(
        bars,
        observations,
        training_start,
        training_end,
        baseline_policy,
    );
    let training_candidate = selected_weights.map(|weights| {
        series_report(
            bars,
            observations,
            training_start,
            training_end,
            EvaluationPolicy {
                weights,
                ..baseline_policy
            },
        )
    });
    let has_available_coefficient = series_has_available_coefficient(&baseline)
        || training_candidate
            .as_ref()
            .is_some_and(series_has_available_coefficient);

    json!({
        "status": if has_available_coefficient { "AVAILABLE" } else { "INSUFFICIENT_SAMPLE" },
        "training_only": true,
        "return_horizon_bars": 1,
        "maximum_lag_bars": MAX_STRATEGY_RETURN_AUTOCORRELATION_LAG_BARS,
        "minimum_pairs": MIN_STRATEGY_RETURN_AUTOCORRELATION_PAIRS,
        "return_basis": if baseline_policy.fees_known { "after configured transaction fees" } else { "gross; fees unknown" },
        "basis": "One-bar open-to-close returns after each qualifying model signal, signed positive when the model direction is correct. Each lag correlates outcomes from qualifying signals separated by that many timeframe bars. The equal-weight baseline and training-fitted candidate are separate series.",
        "interpretation": "A negative coefficient is a reversal-like dependence pattern to investigate, not a position-flip instruction or proof of profitable prediction. Validate any reverse-direction rule on validation data and the untouched holdout.",
        "baseline": baseline,
        "training_candidate": training_candidate,
        "live_signals_changed": false,
    })
}

fn series_has_available_coefficient(series: &Value) -> bool {
    series["lags"]
        .as_array()
        .is_some_and(|lags| lags.iter().any(|lag| lag["coefficient"].is_number()))
}
