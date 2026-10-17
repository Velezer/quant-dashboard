use super::common::*;
use super::confirmations::apply_required_signal_confirmations;
use super::indicator_combination::{
    ema_series, rsi_series, strict_rolling_mean, IndicatorCombinationConfig,
};
use crate::{calculate_dss, DssConfig};

#[cfg(test)]
const PINTU_HOURLY_INTERVAL_NS: i64 = 4 * PINTU_GOLDEN_CROSS_INTERVAL_NS;

pub fn generate_pintu_signals(
    bars: &[StrategyBar],
    config: PintuStrategyConfig,
) -> Vec<StrategySignal> {
    let mut signals = generate_pintu_signals_unconfirmed(bars, config);
    apply_required_signal_confirmations(bars, &mut signals);
    signals
}

pub fn generate_pintu_golden_cross_signals(
    bars: &[StrategyBar],
    config: PintuGoldenCrossConfig,
) -> Vec<StrategySignal> {
    let context_bars = aggregate_complete_context_bars(
        bars,
        config.signal_interval_ns,
        config.context_interval_ns,
    );
    generate_pintu_golden_cross_signals_with_context(bars, &context_bars, config)
}

pub fn generate_pintu_golden_cross_signals_with_context(
    bars: &[StrategyBar],
    context_bars: &[StrategyBar],
    config: PintuGoldenCrossConfig,
) -> Vec<StrategySignal> {
    let mut signals = generate_pintu_golden_cross_signals_unconfirmed(bars, context_bars, config);
    apply_required_signal_confirmations(bars, &mut signals);
    signals
}

pub fn generate_pintu_confluence_signals(
    bars: &[StrategyBar],
    config: PintuConfluenceConfig,
) -> Vec<StrategySignal> {
    let mut signals = generate_pintu_confluence_signals_unconfirmed(bars, config);
    apply_required_signal_confirmations(bars, &mut signals);
    signals
}

/// Generate the registered Pintu perpetual-futures day-trade signal frame.

fn generate_pintu_signals_unconfirmed(
    bars: &[StrategyBar],
    config: PintuStrategyConfig,
) -> Vec<StrategySignal> {
    let n = bars.len();
    let indicator_config = IndicatorCombinationConfig::default();
    let volumes: Vec<f64> = bars.iter().map(|bar| bar.volume).collect();
    let mut signals = vec![StrategySignal::default(); n];
    if n < indicator_config.breakout_lookback + 5 {
        return signals;
    }
    for index in indicator_config.breakout_lookback..n {
        let bar = bars[index];
        if !finite(bar.close) || bar.close <= 0.0 {
            continue;
        }
        let volume_average = rolling_mean(&volumes, index, config.volume_ma_period);
        let volume_average = if volume_average > 0.0 {
            volume_average
        } else {
            1.0
        };
        let relative_volume = bar.volume / volume_average;
        let previous_high = bars[index - indicator_config.breakout_lookback..index]
            .iter()
            .map(|item| item.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let previous_low = bars[index - indicator_config.breakout_lookback..index]
            .iter()
            .map(|item| item.low)
            .fold(f64::INFINITY, f64::min);
        let volume_spike = relative_volume >= config.min_rvol;
        let bullish_breakout = bar.close > previous_high && bar.close >= bar.open;
        let bearish_breakdown = bar.close < previous_low && bar.close <= bar.open;
        let candle_range = (bar.high - bar.low).max(1e-8);
        let lower_wick = (bar.open.min(bar.close) - bar.low) / candle_range;
        let upper_wick = (bar.high - bar.open.max(bar.close)) / candle_range;
        let buyer_wick = lower_wick >= indicator_config.wick_fraction_min && bar.close >= bar.open;
        let seller_wick = upper_wick >= indicator_config.wick_fraction_min && bar.close <= bar.open;
        let signal = &mut signals[index];

        if (bullish_breakout && volume_spike) || (buyer_wick && volume_spike) {
            signal.direction = "LONG".to_string();
            signal.strength = if bullish_breakout && relative_volume >= 2.0 {
                9.0
            } else {
                8.0
            };
            signal.regime = if bullish_breakout {
                "PINTU_FUTURES_BREAKOUT_LONG"
            } else {
                "PINTU_FUTURES_SUPPORT_BOUNCE_LONG"
            }
            .to_string();
            signal.suggested_sl = round_price(bar.close * (1.0 - config.sl_underlying_pct));
            signal.suggested_tp1 = round_price(bar.close * (1.0 + config.tp1_underlying_pct));
            signal.suggested_tp2 = round_price(bar.close * (1.0 + config.tp2_underlying_pct));
        } else if (bearish_breakdown && volume_spike) || (seller_wick && volume_spike) {
            signal.direction = "SHORT".to_string();
            signal.strength = if bearish_breakdown && relative_volume >= 2.0 {
                9.0
            } else {
                8.0
            };
            signal.regime = if bearish_breakdown {
                "PINTU_FUTURES_BREAKDOWN_SHORT"
            } else {
                "PINTU_FUTURES_RESISTANCE_REJECT_SHORT"
            }
            .to_string();
            signal.suggested_sl = round_price(bar.close * (1.0 + config.sl_underlying_pct));
            signal.suggested_tp1 = round_price(bar.close * (1.0 - config.tp1_underlying_pct));
            signal.suggested_tp2 = round_price(bar.close * (1.0 - config.tp2_underlying_pct));
        }
    }
    signals
}

/// Aggregate only complete, contiguous, UTC-aligned 15m candles into 1h bars.
#[cfg(test)]
fn aggregate_complete_hourly_bars(bars: &[StrategyBar]) -> Vec<StrategyBar> {
    aggregate_complete_context_bars(
        bars,
        PINTU_GOLDEN_CROSS_INTERVAL_NS,
        PINTU_HOURLY_INTERVAL_NS,
    )
}

fn aggregate_complete_context_bars(
    bars: &[StrategyBar],
    signal_interval_ns: i64,
    context_interval_ns: i64,
) -> Vec<StrategyBar> {
    if signal_interval_ns <= 0
        || context_interval_ns < signal_interval_ns
        || context_interval_ns % signal_interval_ns != 0
    {
        return Vec::new();
    }
    let bars_per_context =
        usize::try_from(context_interval_ns / signal_interval_ns).unwrap_or(usize::MAX);
    if bars_per_context == 0 || bars_per_context == usize::MAX {
        return Vec::new();
    }
    let mut context = Vec::with_capacity(bars.len() / bars_per_context);
    let mut index = 0;
    while bars.len().saturating_sub(index) >= bars_per_context {
        let start = bars[index].time;
        if start.rem_euclid(context_interval_ns) != 0 {
            index += 1;
            continue;
        }
        let complete = (0..bars_per_context).all(|offset| {
            bars[index + offset].time == start.saturating_add(offset as i64 * signal_interval_ns)
        });
        if !complete {
            index += 1;
            continue;
        }

        let window = &bars[index..index + bars_per_context];
        if window.iter().any(|bar| {
            !finite(bar.open)
                || !finite(bar.high)
                || !finite(bar.low)
                || !finite(bar.close)
                || !finite(bar.volume)
                || bar.volume < 0.0
        }) {
            index += bars_per_context;
            continue;
        }
        context.push(StrategyBar {
            time: start,
            open: window[0].open,
            high: window
                .iter()
                .map(|bar| bar.high)
                .fold(f64::NEG_INFINITY, f64::max),
            low: window
                .iter()
                .map(|bar| bar.low)
                .fold(f64::INFINITY, f64::min),
            close: window[bars_per_context - 1].close,
            volume: window.iter().map(|bar| bar.volume).sum(),
        });
        index += bars_per_context;
    }
    context
}

/// Find the most recent completed hour, rejecting a stale hourly context.
#[cfg(test)]
fn latest_completed_hourly_index(
    hourly_bars: &[StrategyBar],
    signal_close_time: i64,
) -> Option<usize> {
    latest_completed_context_index(hourly_bars, signal_close_time, PINTU_HOURLY_INTERVAL_NS)
}

fn latest_completed_context_index(
    context_bars: &[StrategyBar],
    signal_close_time: i64,
    context_interval_ns: i64,
) -> Option<usize> {
    if context_interval_ns <= 0 {
        return None;
    }
    let completed_count = context_bars
        .partition_point(|bar| bar.time.saturating_add(context_interval_ns) <= signal_close_time);
    let index = completed_count.checked_sub(1)?;
    let context_close = context_bars[index].time.saturating_add(context_interval_ns);
    (signal_close_time.saturating_sub(context_close) <= context_interval_ns).then_some(index)
}

/// Require an oversold DSS reading on a recent, contiguous set of completed
/// hourly bars. The current hour is included in the configured lookback.
fn has_recent_context_dss_oversold(
    hourly_bars: &[StrategyBar],
    dss: &[f64],
    latest_index: usize,
    config: &PintuGoldenCrossConfig,
) -> bool {
    if latest_index >= hourly_bars.len()
        || latest_index >= dss.len()
        || config.context_dss_oversold_lookback_bars == 0
    {
        return false;
    }

    let mut cursor = latest_index;
    for age in 0..config.context_dss_oversold_lookback_bars {
        if finite(dss[cursor]) && dss[cursor] < config.context_dss_oversold {
            return true;
        }
        if age + 1 == config.context_dss_oversold_lookback_bars || cursor == 0 {
            break;
        }
        if hourly_bars[cursor]
            .time
            .saturating_sub(hourly_bars[cursor - 1].time)
            != config.context_interval_ns
        {
            break;
        }
        cursor -= 1;
    }
    false
}

/// Require an extreme DSS reading in the current/preceding closed bars and a
/// rising DSS on the cross candle. The lookback counts prior bars, not hours.
fn has_recent_signal_dss_extreme_turn(
    bars: &[StrategyBar],
    dss: &[f64],
    index: usize,
    config: &PintuGoldenCrossConfig,
) -> bool {
    if index == 0
        || index >= bars.len()
        || index >= dss.len()
        || config.dss_extreme_max_age_bars == 0
        || !finite(dss[index])
        || !finite(dss[index - 1])
        || dss[index] <= dss[index - 1]
    {
        return false;
    }

    let mut start = index;
    while start > 0
        && index - start < config.dss_extreme_max_age_bars
        && bars[start].time.saturating_sub(bars[start - 1].time) == config.signal_interval_ns
    {
        start -= 1;
    }
    dss[start..=index]
        .iter()
        .any(|value| finite(*value) && *value < config.max_signal_dss_extreme)
}

/// Return the swing-high price if this closed candle created a bullish market
/// structure shift from a recent, confirmed lower-high/lower-low sequence.
/// Swing pivots use the configured candles on each side; only pivots confirmed
/// before the candidate candle are considered, so no future bars can affect it.
fn bullish_market_structure_shift_at(
    bars: &[StrategyBar],
    index: usize,
    config: &PintuGoldenCrossConfig,
) -> Option<f64> {
    let swing_width = config.structure_swing_width;

    if index < swing_width.saturating_add(2) || index >= bars.len() {
        return None;
    }
    let start = index.saturating_sub(config.structure_lookback);
    let mut segment_start = start;
    for cursor in start + 1..=index {
        if bars[cursor].time.saturating_sub(bars[cursor - 1].time) != config.signal_interval_ns {
            segment_start = cursor;
        }
    }
    if index.saturating_sub(segment_start) < swing_width.saturating_mul(2).saturating_add(2) {
        return None;
    }

    // The current candle is the shift candle. Pivots must have their full
    // right-hand confirmation window before it begins.
    let last_confirmable_pivot = index.checked_sub(swing_width.saturating_add(1))?;
    if last_confirmable_pivot < segment_start + swing_width {
        return None;
    }
    let mut swing_highs = Vec::new();
    let mut swing_lows = Vec::new();
    for pivot in segment_start + swing_width..=last_confirmable_pivot {
        let high = bars[pivot].high;
        let low = bars[pivot].low;
        let neighbors = pivot - swing_width..=pivot + swing_width;
        if !finite(high) || !finite(low) {
            continue;
        }
        let mut is_swing_high = true;
        let mut is_swing_low = true;
        for neighbor in neighbors {
            if neighbor == pivot {
                continue;
            }
            let neighbor_high = bars[neighbor].high;
            let neighbor_low = bars[neighbor].low;
            if !finite(neighbor_high) || !finite(neighbor_low) {
                is_swing_high = false;
                is_swing_low = false;
                break;
            }
            is_swing_high &= high > neighbor_high;
            is_swing_low &= low < neighbor_low;
        }
        if is_swing_high {
            swing_highs.push((pivot, high));
        }
        if is_swing_low {
            swing_lows.push((pivot, low));
        }
    }

    let [(prior_high_index, prior_high), (last_high_index, last_high)] = swing_highs
        .as_slice()
        .get(swing_highs.len().saturating_sub(2)..)?
    else {
        return None;
    };
    let [(prior_low_index, prior_low), (last_low_index, last_low)] = swing_lows
        .as_slice()
        .get(swing_lows.len().saturating_sub(2)..)?
    else {
        return None;
    };

    let bearish_swing_sequence = prior_low_index < prior_high_index
        && prior_high_index < last_low_index
        && last_low_index < last_high_index
        && last_high < prior_high
        && last_low < prior_low;
    let shift_bar = bars[index];
    let prior_close = bars[index - 1].close;
    (bearish_swing_sequence
        && finite(shift_bar.close)
        && finite(shift_bar.open)
        && finite(prior_close)
        && shift_bar.close >= shift_bar.open
        && prior_close <= *last_high
        && shift_bar.close > *last_high)
        .then_some(*last_high)
}

/// Require a recent bullish MSS within the configured bar age, with price
/// still above the broken swing high.
fn has_recent_bullish_market_structure_shift(
    bars: &[StrategyBar],
    index: usize,
    config: &PintuGoldenCrossConfig,
) -> bool {
    let start = index.saturating_sub(config.structure_max_age_bars);
    (start..=index).any(|shift_index| {
        let continuous = (shift_index + 1..=index).all(|cursor| {
            bars[cursor].time.saturating_sub(bars[cursor - 1].time) == config.signal_interval_ns
        });
        continuous
            && bullish_market_structure_shift_at(bars, shift_index, config)
                .is_some_and(|broken_high| bars[index].close > broken_high)
    })
}

/// Generate a research-only Pintu Futures long signal after a closed 15m EMA
/// cross, a recent completed 1h oversold DSS reading, a recent rising 15m DSS
/// extreme, and a fresh bullish MSS. Stops and targets use the supplied config.
fn generate_pintu_golden_cross_signals_unconfirmed(
    bars: &[StrategyBar],
    context_bars: &[StrategyBar],
    config: PintuGoldenCrossConfig,
) -> Vec<StrategySignal> {
    let n = bars.len();
    let closes: Vec<f64> = bars.iter().map(|bar| bar.close).collect();
    let highs: Vec<f64> = bars.iter().map(|bar| bar.high).collect();
    let lows: Vec<f64> = bars.iter().map(|bar| bar.low).collect();
    let ema_fast = ema_series(&closes, config.fast_ema_period);
    let ema_slow = ema_series(&closes, config.slow_ema_period);
    let dss_config = DssConfig::default();
    let dss_signal = calculate_dss(&highs, &lows, &closes, &dss_config).dss;
    let context_highs: Vec<f64> = context_bars.iter().map(|bar| bar.high).collect();
    let context_lows: Vec<f64> = context_bars.iter().map(|bar| bar.low).collect();
    let context_closes: Vec<f64> = context_bars.iter().map(|bar| bar.close).collect();
    let dss_context =
        calculate_dss(&context_highs, &context_lows, &context_closes, &dss_config).dss;

    let mut signals = vec![StrategySignal::default(); n];
    let minimum_structure_lookback = config
        .structure_swing_width
        .saturating_mul(2)
        .saturating_add(2);
    if config.fast_ema_period == 0
        || config.slow_ema_period <= config.fast_ema_period
        || config.signal_interval_ns <= 0
        || config.context_interval_ns < config.signal_interval_ns
        || config.context_interval_ns % config.signal_interval_ns != 0
        || !finite(config.context_dss_oversold)
        || config.context_dss_oversold < 0.0
        || config.context_dss_oversold > 100.0
        || config.context_dss_oversold_lookback_bars == 0
        || !finite(config.max_signal_dss_extreme)
        || config.max_signal_dss_extreme < 0.0
        || config.max_signal_dss_extreme > 100.0
        || config.dss_extreme_max_age_bars == 0
        || config.structure_swing_width == 0
        || config.structure_swing_width >= n
        || config.structure_lookback < minimum_structure_lookback
        || !finite(config.signal_strength)
        || config.signal_strength < 0.0
        || n <= config.slow_ema_period
    {
        return signals;
    }
    if !finite(config.target_r_multiple)
        || config.target_r_multiple < PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE
        || !finite(config.stop_loss_pct)
        || config.stop_loss_pct <= 0.0
        || config.stop_loss_pct >= 1.0
    {
        return signals;
    }
    let first_signal_index = config.slow_ema_period;
    for index in first_signal_index..n {
        let bar = bars[index];
        if bar.time.saturating_sub(bars[index - 1].time) != config.signal_interval_ns {
            continue;
        }
        let previous_fast = ema_fast[index - 1];
        let previous_slow = ema_slow[index - 1];
        if !finite(bar.open)
            || !finite(bar.high)
            || !finite(bar.low)
            || !finite(bar.close)
            || bar.close <= 0.0
            || !finite(previous_fast)
            || !finite(previous_slow)
            || !finite(ema_fast[index])
            || !finite(ema_slow[index])
        {
            continue;
        }

        let golden_cross = previous_fast <= previous_slow && ema_fast[index] > ema_slow[index];
        if !golden_cross || !has_recent_signal_dss_extreme_turn(bars, &dss_signal, index, &config) {
            continue;
        }

        let signal_close_time = bar.time.saturating_add(config.signal_interval_ns);
        let Some(context_index) = latest_completed_context_index(
            context_bars,
            signal_close_time,
            config.context_interval_ns,
        ) else {
            continue;
        };
        if !has_recent_context_dss_oversold(context_bars, &dss_context, context_index, &config)
            || !has_recent_bullish_market_structure_shift(bars, index, &config)
        {
            continue;
        }

        let signal = &mut signals[index];
        let stop = round_price(bar.close * (1.0 - config.stop_loss_pct));
        let risk = bar.close - stop;
        if !finite(risk) || risk <= 0.0 {
            continue;
        }
        let target_1 = round_price_up(bar.close + risk * config.target_r_multiple);
        let target_1_r = (target_1 - bar.close) / risk;
        if !finite(target_1_r) || target_1_r + f64::EPSILON < PINTU_GOLDEN_CROSS_MIN_R_MULTIPLE {
            continue;
        }
        signal.direction = "LONG".to_string();
        signal.strength = config.signal_strength;
        signal.regime = "PINTU_FUTURES_GOLDEN_CROSS_LONG".to_string();
        signal.suggested_sl = stop;
        signal.suggested_tp1 = target_1;
        signal.suggested_tp2 = round_price_up(bar.close + risk * config.target_r_multiple * 2.0);
    }
    signals
}

/// Generate the selective multi-factor Pintu confluence strategy.
fn generate_pintu_confluence_signals_unconfirmed(
    bars: &[StrategyBar],
    config: PintuConfluenceConfig,
) -> Vec<StrategySignal> {
    let n = bars.len();
    let indicator_config = IndicatorCombinationConfig::default();
    let closes: Vec<f64> = bars.iter().map(|bar| bar.close).collect();
    let volumes: Vec<f64> = bars.iter().map(|bar| bar.volume).collect();
    let ema20 = ema_series(&closes, indicator_config.fast_ema_period);
    let ema50 = ema_series(&closes, indicator_config.slow_ema_period);
    let rsi = rsi_series(&closes, indicator_config.rsi_period);
    let mut signals = vec![StrategySignal::default(); n];
    if n < 60 {
        return signals;
    }
    for index in indicator_config.breakout_lookback..n {
        let bar = bars[index];
        let volume_average =
            strict_rolling_mean(&volumes, index, indicator_config.relative_volume_period);
        if !finite(volume_average) || volume_average <= 0.0 || !finite(rsi[index]) {
            continue;
        }
        let previous_high = bars[index - indicator_config.breakout_lookback..index]
            .iter()
            .map(|item| item.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let previous_low = bars[index - indicator_config.breakout_lookback..index]
            .iter()
            .map(|item| item.low)
            .fold(f64::INFINITY, f64::min);
        let relative_volume = bar.volume / volume_average;
        let long = bar.close > previous_high
            && bar.close > ema20[index]
            && ema20[index] > ema50[index]
            && (indicator_config.rsi_long_min..=indicator_config.rsi_long_max)
                .contains(&rsi[index])
            && relative_volume >= config.volume_ratio
            && bar.close >= bar.open;
        let short = bar.close < previous_low
            && bar.close < ema20[index]
            && ema20[index] < ema50[index]
            && (indicator_config.rsi_short_min..=indicator_config.rsi_short_max)
                .contains(&rsi[index])
            && relative_volume >= config.volume_ratio
            && bar.close <= bar.open;
        let signal = &mut signals[index];
        if long {
            signal.direction = "LONG".to_string();
            signal.strength = 9.0;
            signal.regime = "PINTU_CONFLUENCE_BREAKOUT_LONG".to_string();
            signal.suggested_sl = round_price(bar.close * (1.0 - config.sl_pct));
            signal.suggested_tp1 = round_price(bar.close * (1.0 + config.tp_pct));
            signal.suggested_tp2 = round_price(bar.close * (1.0 + config.tp_pct * 2.0));
        } else if short {
            signal.direction = "SHORT".to_string();
            signal.strength = 9.0;
            signal.regime = "PINTU_CONFLUENCE_BREAKDOWN_SHORT".to_string();
            signal.suggested_sl = round_price(bar.close * (1.0 + config.sl_pct));
            signal.suggested_tp1 = round_price(bar.close * (1.0 - config.tp_pct));
            signal.suggested_tp2 = round_price(bar.close * (1.0 - config.tp_pct * 2.0));
        }
    }
    signals
}

#[cfg(test)]
#[path = "pintu_tests.rs"]
mod tests;
