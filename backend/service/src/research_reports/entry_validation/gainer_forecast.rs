use super::*;
use crate::tokocrypto_storage::SOURCE;

const FORECAST_TIMEFRAME: &str = "4h";
const FORECAST_PROTOCOL: &str = "tokocrypto_native_dss_next_4h_forecast_v1";
const NATIVE_TIMEFRAME: &str = "1h";
const HISTORY_LIMIT: i64 = 1_000;
const UNIVERSE_LIMIT: usize = 1_000;
const MIN_COVERAGE_PCT: usize = 80;
const MIN_COVERAGE_PAIRS: usize = 10;
const SHORTLIST_LIMIT: usize = 5;

#[derive(serde::Serialize)]
struct Candidate {
    instrument_id: String,
    symbol: String,
    venue_symbol: String,
    quote_currency: String,
    score: f64,
    source_family: &'static str,
}

#[derive(Default)]
struct Coverage {
    scored: usize,
    missing_or_stale: usize,
    invalid_history: usize,
    read_errors: usize,
}

enum InstrumentScore {
    Scored(f64),
    MissingOrStale,
    InvalidHistory,
}

fn minimum_scored_count(eligible: usize) -> usize {
    MIN_COVERAGE_PAIRS.max((eligible * MIN_COVERAGE_PCT).div_ceil(100))
}

pub(crate) fn gainer_forecast(
    connection: &Connection,
    directory: &str,
) -> Result<Value, rusqlite::Error> {
    let duration_ns =
        timeframe_duration_ns(FORECAST_TIMEFRAME).expect("supported forecast timeframe");
    let now_ns = (now_secs() as i64).saturating_mul(NANOS_PER_SECOND);
    let cutoff = now_ns - now_ns.rem_euclid(duration_ns);
    let instruments = store::verified_gainer_universe(connection, UNIVERSE_LIMIT)?;
    let mut coverage = Coverage::default();
    let mut candidates = Vec::new();
    for instrument in &instruments {
        match score_instrument(directory, instrument, cutoff, duration_ns) {
            Ok(InstrumentScore::Scored(score)) => {
                coverage.scored += 1;
                if score >= quant_core::SIX_INDICATOR_MINIMUM_SCORE_OUT_OF_60 {
                    candidates.push(Candidate {
                        instrument_id: text(instrument, "instrument_id").to_owned(),
                        symbol: text(instrument, "symbol").to_owned(),
                        venue_symbol: text(instrument, "venue_symbol").to_owned(),
                        quote_currency: text(instrument, "quote_currency").to_owned(),
                        score,
                        source_family: SOURCE,
                    });
                }
            }
            Ok(InstrumentScore::MissingOrStale) => coverage.missing_or_stale += 1,
            Ok(InstrumentScore::InvalidHistory) => coverage.invalid_history += 1,
            Err(_) => coverage.read_errors += 1,
        }
    }
    Ok(forecast_payload(
        cutoff,
        duration_ns,
        instruments.len(),
        coverage,
        candidates,
    ))
}

fn forecast_payload(
    cutoff: i64,
    duration_ns: i64,
    eligible: usize,
    coverage: Coverage,
    mut candidates: Vec<Candidate>,
) -> Value {
    let minimum_scored = minimum_scored_count(eligible);
    let status = if coverage.scored < minimum_scored {
        candidates.clear();
        "INSUFFICIENT_COVERAGE"
    } else if candidates.is_empty() {
        "NO_POSITIVE_CANDIDATES"
    } else {
        candidates.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.symbol.cmp(&b.symbol))
                .then_with(|| a.instrument_id.cmp(&b.instrument_id))
        });
        candidates.truncate(SHORTLIST_LIMIT);
        "READY"
    };
    json!({
        "protocol_id": FORECAST_PROTOCOL,
        "status": status,
        "as_of": cutoff / NANOS_PER_SECOND,
        "forecast_until": (cutoff + duration_ns) / NANOS_PER_SECOND,
        "eligible_count": eligible,
        "scored_count": coverage.scored,
        "minimum_scored_count": minimum_scored,
        "missing_or_stale_count": coverage.missing_or_stale,
        "invalid_history_count": coverage.invalid_history,
        "read_error_count": coverage.read_errors,
        "model": "DSS-led signed score",
        "validation_status": "UNVALIDATED_RESEARCH",
        "candidates": candidates,
    })
}

fn score_instrument(
    directory: &str,
    instrument: &Value,
    cutoff: i64,
    duration_ns: i64,
) -> Result<InstrumentScore, rusqlite::Error> {
    let symbol = text(instrument, "storage_symbol");
    let Some(connection) = open_market_history_for_symbol(directory, symbol, false)? else {
        return Ok(InstrumentScore::MissingOrStale);
    };
    let market = connection.unchecked_transaction()?;
    let matches_instrument: bool = market.query_row(
        "SELECT EXISTS(SELECT 1 FROM series
         WHERE symbol=?1 AND interval=?2 AND source=?3 AND exchange_id=?4
           AND product_id=?5 AND instrument_id=?6 AND price_basis='venue_native')",
        params![
            symbol,
            NATIVE_TIMEFRAME,
            SOURCE,
            text(instrument, "exchange_id"),
            text(instrument, "product_id"),
            text(instrument, "instrument_id"),
        ],
        |row| row.get(0),
    )?;
    if !matches_instrument {
        return Ok(InstrumentScore::MissingOrStale);
    }
    let hour_ns = timeframe_duration_ns(NATIVE_TIMEFRAME).expect("supported native timeframe");
    let hourly = load_candles_from_source_between(
        &market,
        symbol,
        NATIVE_TIMEFRAME,
        HISTORY_LIMIT * (duration_ns / hour_ns),
        SOURCE,
        cutoff - HISTORY_LIMIT * duration_ns,
        cutoff,
    )?;
    market.commit()?;
    drop(connection);
    if hourly.last().is_none_or(|bar| bar.time != cutoff - hour_ns) {
        return Ok(InstrumentScore::MissingOrStale);
    }
    let bars = complete_four_hour_suffix(&hourly, cutoff, hour_ns, duration_ns);
    let kind = EntryModelKind::Dss;
    if bars.len() < kind.minimum_history_bars()
        || bars.iter().rev().take(2).any(|bar| bar.volume <= 0.0)
    {
        return Ok(InstrumentScore::InvalidHistory);
    }
    let mut score = None;
    quant_core::strategies::visit_sampled_entry_scores(
        &bars,
        kind,
        |index| index == bars.len() - 1,
        |_, value| score = Some(value),
    );
    Ok(score.map_or(InstrumentScore::InvalidHistory, InstrumentScore::Scored))
}

fn complete_four_hour_suffix(
    hourly: &[Candle],
    cutoff: i64,
    hour_ns: i64,
    duration_ns: i64,
) -> Vec<StrategyBar> {
    let mut bars = Vec::new();
    let mut end = cutoff;
    for hours in hourly.rchunks_exact((duration_ns / hour_ns) as usize) {
        let start = end - duration_ns;
        if !hours.iter().enumerate().all(|(index, bar)| {
            bar.time == start + index as i64 * hour_ns
                && [bar.open, bar.high, bar.low, bar.close]
                    .into_iter()
                    .all(|price| price.is_finite() && price > 0.0)
                && bar.high >= bar.open.max(bar.close)
                && bar.low <= bar.open.min(bar.close)
                && bar.volume.is_finite()
                && bar.volume >= 0.0
        }) {
            break;
        }
        let volume = hours.iter().map(|bar| bar.volume).sum::<f64>();
        if !volume.is_finite() {
            break;
        }
        bars.push(StrategyBar {
            time: start,
            open: hours[0].open,
            high: hours
                .iter()
                .map(|bar| bar.high)
                .fold(f64::NEG_INFINITY, f64::max),
            low: hours
                .iter()
                .map(|bar| bar.low)
                .fold(f64::INFINITY, f64::min),
            close: hours[hours.len() - 1].close,
            volume,
        });
        end = start;
    }
    bars.reverse();
    bars
}

#[cfg(test)]
#[path = "gainer_forecast_tests.rs"]
mod gainer_tests;
