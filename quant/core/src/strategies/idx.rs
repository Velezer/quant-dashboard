use super::common::*;
use super::confirmations::apply_required_signal_confirmations;
use std::collections::HashMap;

pub fn generate_idx_signals(
    bars: &[StrategyBar],
    config: IdxStrategyConfig,
) -> Vec<StrategySignal> {
    let mut signals = generate_idx_signals_unconfirmed(bars, config);
    apply_required_signal_confirmations(bars, &mut signals);
    signals
}

pub fn generate_idx_opening_range_signals(
    bars: &[StrategyBar],
    config: IdxOpeningRangeConfig,
) -> Vec<StrategySignal> {
    let mut signals = generate_idx_opening_range_signals_unconfirmed(bars, config);
    apply_required_signal_confirmations(bars, &mut signals);
    signals
}

pub fn generate_idx_first_half_hour_momentum_signals(
    bars: &[StrategyBar],
    config: IdxIntradayMomentumConfig,
) -> Vec<StrategySignal> {
    let mut signals = generate_idx_first_half_hour_momentum_signals_unconfirmed(bars, config);
    apply_required_signal_confirmations(bars, &mut signals);
    signals
}

pub fn generate_idx_time_of_day_momentum_signals(
    bars: &[StrategyBar],
    config: IdxIntradayMomentumConfig,
) -> Vec<StrategySignal> {
    let mut signals = generate_idx_time_of_day_momentum_signals_unconfirmed(bars, config);
    apply_required_signal_confirmations(bars, &mut signals);
    signals
}

pub fn generate_idx_quality_signals(
    bars: &[StrategyBar],
    config: IdxStrategyConfig,
) -> Vec<StrategySignal> {
    let mut signals = generate_idx_quality_signals_unconfirmed(bars, config);
    apply_required_signal_confirmations(bars, &mut signals);
    signals
}

pub fn generate_idx_time_series_momentum_signals(
    bars: &[StrategyBar],
    config: IdxTimeSeriesMomentumConfig,
) -> Vec<StrategySignal> {
    let mut signals = generate_idx_time_series_momentum_signals_unconfirmed(bars, config);
    apply_required_signal_confirmations(bars, &mut signals);
    signals
}

/// Generate the registered Stockbit/IDX long-only day-trade signal frame.

pub(super) fn generate_idx_signals_unconfirmed(
    bars: &[StrategyBar],
    config: IdxStrategyConfig,
) -> Vec<StrategySignal> {
    let n = bars.len();
    let volumes: Vec<f64> = bars.iter().map(|bar| bar.volume).collect();
    let references = idx_reference_closes(bars);
    let mut signals = vec![StrategySignal::default(); n];
    if n < 25 {
        return signals;
    }
    for index in 1..n {
        let bar = bars[index];
        let signal = &mut signals[index];
        // Only mark time stop exit for historical backtesting bars before the latest bar
        if index < n - 1
            && (stockbit_cutoff_due(bar.time) || stockbit_session_exit_due(bars, index))
        {
            signal.exit_signal = Some("DAY_TRADE_T0_TIME_STOP".to_string());
            continue;
        }
        let minimum_price = config.min_price.max(idx_min_price_at(bar.time));
        if !finite(bar.close) || bar.close < minimum_price {
            continue;
        }
        let volume_average = rolling_mean(&volumes, index, config.volume_ma_period);
        let volume_average = if volume_average > 0.0 {
            volume_average
        } else {
            1.0
        };
        let relative_volume = bar.volume / volume_average;
        let turnover = bar.close * bar.volume;
        let open_equal_low = ((bar.open - bar.low).abs() / bar.open.max(1.0)
            <= config.open_equal_low_tolerance)
            && bar.close >= bar.open;
        let volume_spike = relative_volume >= config.min_rvol;
        let breakout_yesterday = bar.close > bars[index - 1].high;
        // If a bounded history slice starts mid-session, use the current
        // close for price-limit headroom until a prior-session close exists.
        let reference_close = if finite(references[index]) {
            references[index]
        } else {
            bar.close
        };
        let (ara, arb, tick_size) = idx_ara_arb(reference_close, bar.time);
        let ticks_to_ara = if tick_size > 0.0 {
            (ara - bar.close) / tick_size
        } else {
            100.0
        };
        if ticks_to_ara < config.min_ticks_to_ara as f64 {
            continue;
        }
        if (open_equal_low || volume_spike)
            && (turnover >= config.min_turnover_idr || relative_volume >= 2.5)
        {
            signal.direction = "LONG".to_string();
            signal.strength = if open_equal_low && volume_spike {
                9.0
            } else {
                8.0
            };
            signal.regime = if open_equal_low && volume_spike {
                "STOCKBIT_OPEN_LOW_SPIKE"
            } else if open_equal_low {
                "STOCKBIT_OPEN_LOW"
            } else if breakout_yesterday {
                "STOCKBIT_R1_BREAKOUT"
            } else {
                "STOCKBIT_RVOL_MOMENTUM"
            }
            .to_string();
            signal.suggested_sl = round_idx_tick_at(
                arb.max(bar.close * (1.0 - config.sl_pct)),
                IdxTickRounding::Floor,
                bar.time,
            );
            signal.suggested_tp1 = round_idx_tick_at(
                ara.min(bar.close * (1.0 + config.tp1_pct)),
                IdxTickRounding::Ceil,
                bar.time,
            );
            signal.suggested_tp2 = round_idx_tick_at(
                ara.min(bar.close * (1.0 + config.tp2_pct)),
                IdxTickRounding::Ceil,
                bar.time,
            );
        }
    }
    signals
}

#[derive(Debug, Clone, Copy)]
struct IdxOpeningRangeDay {
    reference_close: f64,
    high: f64,
    low: f64,
    first_half_hour_close: f64,
}

fn build_idx_opening_ranges(
    bars: &[StrategyBar],
    references: &[f64],
) -> HashMap<i64, IdxOpeningRangeDay> {
    let mut opening_indices: HashMap<i64, [Option<usize>; 2]> = HashMap::new();
    for (index, bar) in bars.iter().enumerate() {
        if !idx_regular_morning_bar(bar.time) {
            continue;
        }
        let seconds = idx_local_seconds_in_day(bar.time);
        let slots = opening_indices
            .entry(idx_local_day(bar.time))
            .or_insert([None, None]);
        if seconds == 9 * crate::SECONDS_PER_HOUR {
            slots[0] = Some(index);
        } else if seconds == 9 * crate::SECONDS_PER_HOUR + 15 * crate::SECONDS_PER_MINUTE {
            slots[1] = Some(index);
        }
    }

    let mut opening_ranges = HashMap::new();
    for (day, [first_index, second_index]) in opening_indices {
        let (Some(first_index), Some(second_index)) = (first_index, second_index) else {
            continue;
        };
        let first = bars[first_index];
        let second = bars[second_index];
        let reference_close = references[first_index];
        if second_index != first_index.saturating_add(1)
            || second.time.saturating_sub(first.time)
                != 15 * crate::SECONDS_PER_MINUTE * crate::NANOS_PER_SECOND
            || !finite(reference_close)
            || reference_close <= 0.0
            || !finite(first.high)
            || !finite(first.low)
            || !finite(second.high)
            || !finite(second.low)
            || !finite(second.close)
            || first.high < first.low
            || second.high < second.low
        {
            continue;
        }
        opening_ranges.insert(
            day,
            IdxOpeningRangeDay {
                reference_close,
                high: first.high.max(second.high),
                low: first.low.min(second.low),
                first_half_hour_close: second.close,
            },
        );
    }
    opening_ranges
}

fn build_stockbit_intraday_signal(
    bar: StrategyBar,
    index: usize,
    volumes: &[f64],
    reference_close: f64,
    stop_reference: f64,
    config: IdxIntradayMomentumConfig,
) -> Option<StrategySignal> {
    let minimum_price = config.min_price.max(idx_min_price_at(bar.time));
    if !finite(bar.open)
        || !finite(bar.high)
        || !finite(bar.low)
        || !finite(bar.close)
        || !finite(bar.volume)
        || bar.open <= 0.0
        || bar.close < minimum_price
        || bar.high < bar.low
        || bar.volume < 0.0
        || !finite(reference_close)
        || reference_close <= 0.0
    {
        return None;
    }
    let volume_start = index.saturating_sub(config.volume_lookback);
    let prior_volumes = &volumes[volume_start..index];
    if prior_volumes.len() < 5
        || !prior_volumes
            .iter()
            .all(|value| finite(*value) && *value >= 0.0)
    {
        return None;
    }
    let average_volume = prior_volumes.iter().sum::<f64>() / prior_volumes.len() as f64;
    if average_volume <= 0.0 || bar.volume / average_volume < config.min_rvol {
        return None;
    }
    if bar.close * bar.volume < config.min_turnover_idr {
        return None;
    }
    let (ara, arb, tick_size) = idx_ara_arb(reference_close, bar.time);
    if !finite(ara) || !finite(arb) || !finite(tick_size) {
        return None;
    }
    if (ara - bar.close) / tick_size.max(1.0) < config.min_ticks_to_ara as f64 {
        return None;
    }
    let stop =
        round_idx_tick_at(stop_reference.max(arb), IdxTickRounding::Floor, bar.time).max(arb);
    let risk = bar.close - stop;
    if risk <= 0.0 || risk / bar.close > config.max_stop_pct {
        return None;
    }
    let tp1 = round_idx_tick_at(
        (bar.close + risk * config.tp1_r_multiple).min(ara),
        IdxTickRounding::Ceil,
        bar.time,
    );
    let tp2 = round_idx_tick_at(
        (bar.close + risk * config.tp2_r_multiple).min(ara),
        IdxTickRounding::Ceil,
        bar.time,
    );
    if tp1 <= bar.close || tp2 < tp1 {
        return None;
    }
    Some(StrategySignal {
        direction: "LONG".to_string(),
        strength: 8.0,
        strategy_strength: 0.0,
        confluence: None,
        regime: "STOCKBIT_INTRADAY_MOMENTUM_LONG".to_string(),
        suggested_sl: stop,
        suggested_tp1: tp1,
        suggested_tp2: tp2,
        exit_signal: None,
    })
}

/// Generate the Stockbit/IDX opening-range momentum research candidate.
///
/// Bars must be 15m OHLCV in chronological order with timestamps at candle
/// open in UTC. The first two bars must be stamped 09:00 and 09:15 WIB. The
/// rule then requires a positive first-half-hour return, a close one tick
/// above the opening-range high, relative volume, minimum turnover, room
/// below ARA, and a stop no wider than the configured risk cap.
pub(super) fn generate_idx_opening_range_signals_unconfirmed(
    bars: &[StrategyBar],
    config: IdxOpeningRangeConfig,
) -> Vec<StrategySignal> {
    let n = bars.len();
    let mut signals = vec![StrategySignal::default(); n];
    if n < 3 || config.opening_range_bars != 2 {
        return signals;
    }
    let references = idx_reference_closes(bars);
    let mut opening_indices: HashMap<i64, [Option<usize>; 2]> = HashMap::new();
    for (index, bar) in bars.iter().enumerate() {
        if !idx_regular_morning_bar(bar.time) {
            continue;
        }
        let seconds = idx_local_seconds_in_day(bar.time);
        let slots = opening_indices
            .entry(idx_local_day(bar.time))
            .or_insert([None, None]);
        if seconds == 9 * crate::SECONDS_PER_HOUR {
            slots[0] = Some(index);
        } else if seconds == 9 * crate::SECONDS_PER_HOUR + 15 * crate::SECONDS_PER_MINUTE {
            slots[1] = Some(index);
        }
    }

    let mut opening_ranges = HashMap::new();
    for (day, [first_index, second_index]) in opening_indices {
        let (Some(first_index), Some(second_index)) = (first_index, second_index) else {
            continue;
        };
        let first = bars[first_index];
        let second = bars[second_index];
        let reference_close = references[first_index];
        if second_index != first_index.saturating_add(1)
            || second.time.saturating_sub(first.time)
                != 15 * crate::SECONDS_PER_MINUTE * crate::NANOS_PER_SECOND
            || !finite(reference_close)
            || reference_close <= 0.0
            || !finite(first.high)
            || !finite(first.low)
            || !finite(second.high)
            || !finite(second.low)
            || !finite(second.close)
            || first.high < first.low
            || second.high < second.low
        {
            continue;
        }
        opening_ranges.insert(
            day,
            IdxOpeningRangeDay {
                reference_close,
                high: first.high.max(second.high),
                low: first.low.min(second.low),
                first_half_hour_close: second.close,
            },
        );
    }

    let volumes: Vec<f64> = bars.iter().map(|bar| bar.volume).collect();
    let mut traded_days = std::collections::HashSet::new();
    for index in 0..n {
        let bar = bars[index];
        let signal = &mut signals[index];
        if index + 1 < n
            && (stockbit_cutoff_due(bar.time) || stockbit_session_exit_due(bars, index))
        {
            signal.exit_signal = Some("STOCKBIT_ORB_T0_SESSION_EXIT".to_string());
            continue;
        }
        if !idx_orb_signal_window(bar.time) {
            continue;
        }
        let day = idx_local_day(bar.time);
        if traded_days.contains(&day) {
            continue;
        }
        let Some(opening_range) = opening_ranges.get(&day).copied() else {
            continue;
        };
        let minimum_price = config.min_price.max(idx_min_price_at(bar.time));
        if !finite(bar.close)
            || !finite(bar.open)
            || !finite(bar.high)
            || !finite(bar.low)
            || !finite(bar.volume)
            || bar.close < minimum_price
            || bar.high < bar.low
            || opening_range.first_half_hour_close <= opening_range.reference_close
        {
            continue;
        }
        let tick_size = idx_tick_size(opening_range.reference_close) as f64;
        if bar.close < opening_range.high + tick_size {
            continue;
        }
        let volume_start = index.saturating_sub(config.volume_lookback);
        let prior_volumes = &volumes[volume_start..index];
        if prior_volumes.len() < 5
            || !prior_volumes
                .iter()
                .all(|value| finite(*value) && *value >= 0.0)
        {
            continue;
        }
        let average_volume = prior_volumes.iter().sum::<f64>() / prior_volumes.len() as f64;
        if average_volume <= 0.0 {
            continue;
        }
        let relative_volume = bar.volume / average_volume;
        let turnover = bar.close * bar.volume;
        if relative_volume < config.min_rvol || turnover < config.min_turnover_idr {
            continue;
        }
        let (ara, arb, limit_tick) = idx_ara_arb(opening_range.reference_close, bar.time);
        if !finite(ara) || !finite(arb) || !finite(limit_tick) {
            continue;
        }
        let ticks_to_ara = (ara - bar.close) / limit_tick.max(1.0);
        if ticks_to_ara < config.min_ticks_to_ara as f64 {
            continue;
        }
        let stop = round_idx_tick_at(opening_range.low.max(arb), IdxTickRounding::Floor, bar.time)
            .max(arb);
        let risk = bar.close - stop;
        if risk <= 0.0 || risk / bar.close > config.max_stop_pct {
            continue;
        }
        let tp1 = round_idx_tick_at(
            (bar.close + risk * config.tp1_r_multiple).min(ara),
            IdxTickRounding::Ceil,
            bar.time,
        );
        let tp2 = round_idx_tick_at(
            (bar.close + risk * config.tp2_r_multiple).min(ara),
            IdxTickRounding::Ceil,
            bar.time,
        );
        if tp1 <= bar.close || tp2 < tp1 {
            continue;
        }
        signal.direction = "LONG".to_string();
        signal.strength = 8.5;
        signal.regime = "STOCKBIT_ORB_INTRADAY_MOMENTUM_LONG".to_string();
        signal.suggested_sl = stop;
        signal.suggested_tp1 = tp1;
        signal.suggested_tp2 = tp2;
        traded_days.insert(day);
    }
    signals
}

/// Research-derived early-session momentum hypothesis. It enters on the next
/// bar after the 09:00–09:30 return turns positive, without requiring an
/// opening-range breakout. Shared Stockbit filters keep it long-only,
/// liquid, price-limit aware, and flat before lunch.
pub(super) fn generate_idx_first_half_hour_momentum_signals_unconfirmed(
    bars: &[StrategyBar],
    config: IdxIntradayMomentumConfig,
) -> Vec<StrategySignal> {
    let mut signals = vec![StrategySignal::default(); bars.len()];
    if bars.len() < 3 {
        return signals;
    }
    let references = idx_reference_closes(bars);
    let opening_ranges = build_idx_opening_ranges(bars, &references);
    let volumes: Vec<f64> = bars.iter().map(|bar| bar.volume).collect();
    let mut traded_days = std::collections::HashSet::new();

    for index in 0..bars.len() {
        let bar = bars[index];
        if index + 1 < bars.len()
            && (stockbit_cutoff_due(bar.time) || stockbit_session_exit_due(bars, index))
        {
            signals[index].exit_signal = Some("STOCKBIT_FIRST_HALF_HOUR_SESSION_EXIT".to_string());
            continue;
        }
        if idx_local_seconds_in_day(bar.time)
            != 9 * crate::SECONDS_PER_HOUR + 15 * crate::SECONDS_PER_MINUTE
        {
            continue;
        }
        let day = idx_local_day(bar.time);
        if traded_days.contains(&day) {
            continue;
        }
        let Some(opening_range) = opening_ranges.get(&day).copied() else {
            continue;
        };
        if opening_range.first_half_hour_close <= opening_range.reference_close
            || bar.close < bar.open
        {
            continue;
        }
        let Some(mut candidate) = build_stockbit_intraday_signal(
            bar,
            index,
            &volumes,
            opening_range.reference_close,
            opening_range.low,
            config,
        ) else {
            continue;
        };
        candidate.strength = 7.5;
        candidate.regime = "STOCKBIT_FIRST_30M_MOMENTUM_LONG".to_string();
        signals[index] = candidate;
        traded_days.insert(day);
    }
    signals
}

/// Research-derived same-time-of-day continuation hypothesis. It compares the
/// current 15-minute return with the same morning slot on prior sessions,
/// requiring a positive historical mean and positive current candle.
pub(super) fn generate_idx_time_of_day_momentum_signals_unconfirmed(
    bars: &[StrategyBar],
    config: IdxIntradayMomentumConfig,
) -> Vec<StrategySignal> {
    let mut signals = vec![StrategySignal::default(); bars.len()];
    if bars.len() < 3 || config.time_of_day_lookback_sessions == 0 {
        return signals;
    }
    let references = idx_reference_closes(bars);
    let volumes: Vec<f64> = bars.iter().map(|bar| bar.volume).collect();
    let mut traded_days = std::collections::HashSet::new();

    for index in 0..bars.len() {
        let bar = bars[index];
        if index + 1 < bars.len()
            && (stockbit_cutoff_due(bar.time) || stockbit_session_exit_due(bars, index))
        {
            signals[index].exit_signal = Some("STOCKBIT_TIME_OF_DAY_SESSION_EXIT".to_string());
            continue;
        }
        if !idx_orb_signal_window(bar.time) || bar.close <= bar.open || bar.open <= 0.0 {
            continue;
        }
        let day = idx_local_day(bar.time);
        if traded_days.contains(&day) {
            continue;
        }
        let slot = idx_local_seconds_in_day(bar.time);
        let mut same_slot_returns = Vec::new();
        for prior in bars[..index].iter().rev() {
            if idx_local_day(prior.time) >= day
                || idx_local_seconds_in_day(prior.time) != slot
                || !idx_regular_morning_bar(prior.time)
                || !finite(prior.open)
                || !finite(prior.close)
                || prior.open <= 0.0
            {
                continue;
            }
            same_slot_returns.push(prior.close / prior.open - 1.0);
            if same_slot_returns.len() >= config.time_of_day_lookback_sessions {
                break;
            }
        }
        if same_slot_returns.len() < config.minimum_time_of_day_observations
            || same_slot_returns.iter().sum::<f64>() / same_slot_returns.len() as f64 <= 0.0
        {
            continue;
        }
        let Some(mut candidate) = build_stockbit_intraday_signal(
            bar,
            index,
            &volumes,
            references[index],
            bar.low,
            config,
        ) else {
            continue;
        };
        candidate.strength = 7.0;
        candidate.regime = "STOCKBIT_TIME_OF_DAY_CONTINUATION_LONG".to_string();
        signals[index] = candidate;
        traded_days.insert(day);
    }
    signals
}

/// Generate a stricter Stockbit candidate by adding trend and prior-high
/// confirmation to the registered open-low/RVOL signal. This is intentionally
/// a separate deterministic rule set so the strategy lab can test it without
/// changing the live registered strategy.
pub(super) fn generate_idx_quality_signals_unconfirmed(
    bars: &[StrategyBar],
    config: IdxStrategyConfig,
) -> Vec<StrategySignal> {
    let mut signals = generate_idx_signals_unconfirmed(bars, config);
    const TREND_PERIOD: usize = 10;

    for index in 0..signals.len() {
        if signals[index].direction != "LONG" {
            continue;
        }
        if index < TREND_PERIOD {
            signals[index] = StrategySignal::default();
            continue;
        }
        let trend_window = &bars[index - TREND_PERIOD..index];
        let trend_average = trend_window
            .iter()
            .map(|bar| bar.close)
            .filter(|value| finite(*value))
            .collect::<Vec<_>>();
        let trend_average = if trend_average.len() == TREND_PERIOD {
            trend_average.iter().sum::<f64>() / TREND_PERIOD as f64
        } else {
            f64::NAN
        };
        let bar = bars[index];
        let previous = bars[index - 1];
        let confirmed = finite(trend_average)
            && finite(bar.close)
            && finite(previous.close)
            && bar.close >= trend_average
            && bar.close >= previous.close
            && bar.high >= previous.high;
        if !confirmed {
            signals[index] = StrategySignal::default();
            continue;
        }
        let prior_regime = signals[index].regime.clone();
        signals[index].regime = format!(
            "STOCKBIT_QUALITY_{}",
            prior_regime
                .strip_prefix("STOCKBIT_")
                .unwrap_or(&prior_regime)
        );
        signals[index].strength = (signals[index].strength + 0.5).min(10.0);
    }

    signals
}

/// Generate a long-only daily time-series momentum signal frame for Stockbit.
///
/// A long regime is active when the 12-to-1-month return is positive and the
/// close is above its 200-session simple moving average. ATR-based brackets
/// give the existing position-sizing path a volatility-aware stop distance;
/// a strategy-specific exit marker closes the research trade when the regime
/// ends. This is a research signal, not a validated plan or an order.
pub(super) fn generate_idx_time_series_momentum_signals_unconfirmed(
    bars: &[StrategyBar],
    config: IdxTimeSeriesMomentumConfig,
) -> Vec<StrategySignal> {
    let n = bars.len();
    let mut signals = vec![StrategySignal::default(); n];
    if config.lookback_bars == 0
        || config.skip_recent_bars == 0
        || config.skip_recent_bars >= config.lookback_bars
        || config.trend_period == 0
        || config.atr_period == 0
        || !finite(config.stop_atr_multiple)
        || !finite(config.target_atr_multiple)
        || config.stop_atr_multiple <= 0.0
        || config.target_atr_multiple <= 0.0
    {
        return signals;
    }

    let warmup = config
        .lookback_bars
        .max(config.trend_period)
        .max(config.atr_period);
    if n <= warmup {
        return signals;
    }
    let closes: Vec<f64> = bars.iter().map(|bar| bar.close).collect();

    for index in warmup..n {
        let bar = bars[index];
        let momentum_start = index.saturating_sub(config.lookback_bars);
        let momentum_end = index.saturating_sub(config.skip_recent_bars);
        let trend_start = index + 1 - config.trend_period;
        let atr_start = index + 1 - config.atr_period;
        let momentum_base = closes[momentum_start];
        let momentum_end_price = closes[momentum_end];
        let trend_window = &closes[trend_start..=index];
        if !finite(bar.close)
            || bar.close <= 0.0
            || !finite(momentum_base)
            || momentum_base <= 0.0
            || !finite(momentum_end_price)
            || !trend_window.iter().all(|value| finite(*value))
        {
            signals[index].exit_signal = Some("TSMOM_TREND_EXIT".to_string());
            continue;
        }

        let trend_average = trend_window.iter().sum::<f64>() / config.trend_period as f64;
        let mut true_range_sum = 0.0;
        let mut valid_true_ranges = true;
        for atr_index in atr_start..=index {
            let current = bars[atr_index];
            let previous_close = bars[atr_index - 1].close;
            if !finite(current.high)
                || !finite(current.low)
                || !finite(previous_close)
                || current.high < current.low
            {
                valid_true_ranges = false;
                break;
            }
            let true_range = (current.high - current.low)
                .max((current.high - previous_close).abs())
                .max((current.low - previous_close).abs());
            true_range_sum += true_range;
        }
        let atr = if valid_true_ranges {
            true_range_sum / config.atr_period as f64
        } else {
            f64::NAN
        };
        if bar.close < idx_min_price_at(bar.time) {
            signals[index].exit_signal = Some("TSMOM_TREND_EXIT".to_string());
            continue;
        }
        let momentum = momentum_end_price / momentum_base - 1.0;
        let signal = &mut signals[index];
        if !finite(atr) || atr <= 0.0 {
            signal.exit_signal = Some("TSMOM_TREND_EXIT".to_string());
            continue;
        }
        if momentum > 0.0 && bar.close > trend_average {
            signal.direction = "LONG".to_string();
            signal.strength = 7.0;
            signal.regime = "IDX_TSMOM_12_1_ABOVE_200SMA_LONG".to_string();
            signal.suggested_sl = round_idx_tick_at(
                bar.close - atr * config.stop_atr_multiple,
                IdxTickRounding::Floor,
                bar.time,
            );
            signal.suggested_tp1 = round_idx_tick_at(
                bar.close + atr * config.target_atr_multiple,
                IdxTickRounding::Ceil,
                bar.time,
            );
            signal.suggested_tp2 = round_idx_tick_at(
                bar.close + atr * config.target_atr_multiple * 1.5,
                IdxTickRounding::Ceil,
                bar.time,
            );
        } else {
            signal.exit_signal = Some("TSMOM_TREND_EXIT".to_string());
        }
    }
    signals
}
