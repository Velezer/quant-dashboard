//! BTC options Max Pain derived from Deribit's public options open-interest snapshot.

use super::{
    treasury::valid_date, FetchStatus, MacroIndicatorId, MacroObservation, MacroSeriesProvider,
    ProviderSeriesSnapshot,
};
use crate::public_http::{get_bounded, RequestOptions};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const BOOK_SUMMARY_URL: &str =
    "https://www.deribit.com/api/v2/public/get_book_summary_by_currency?currency=BTC&kind=option";
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const CACHE_TTL_SECONDS: u64 = 15 * 60;
const MINIMUM_RETRY_INTERVAL_SECONDS: u64 = 60;
const MAX_EXPIRIES: usize = 32;

pub(super) static DERIBIT_PROVIDER: DeribitProvider = DeribitProvider;
pub(super) struct DeribitProvider;

#[derive(Clone)]
struct CachedSeries {
    observations: Vec<MacroObservation>,
    retrieved_at_seconds: u64,
    last_attempt_seconds: u64,
    refresh_error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct OptionPosition {
    strike: f64,
    open_interest: f64,
    is_call: bool,
}

static CACHE: OnceLock<Mutex<Option<CachedSeries>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<CachedSeries>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

pub(super) fn source_series_id(id: MacroIndicatorId) -> Option<&'static str> {
    (id == MacroIndicatorId::BitcoinOptionsMaxPain)
        .then_some("BTC option open interest · max pain by expiry")
}

pub(super) fn source_url(id: MacroIndicatorId) -> Option<&'static str> {
    (id == MacroIndicatorId::BitcoinOptionsMaxPain).then_some(BOOK_SUMMARY_URL)
}

impl MacroSeriesProvider for DeribitProvider {
    fn read_series(
        &self,
        id: MacroIndicatorId,
        force_refresh: bool,
    ) -> Result<ProviderSeriesSnapshot, String> {
        if id != MacroIndicatorId::BitcoinOptionsMaxPain {
            return Err("Deribit does not provide the requested macro series.".to_string());
        }
        load_series(force_refresh)
    }
}

fn load_series(force_refresh: bool) -> Result<ProviderSeriesSnapshot, String> {
    let now = now_secs();
    let mut cached = cache().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(current) = cached.as_ref() {
        let recently_retrieved =
            now.saturating_sub(current.retrieved_at_seconds) < CACHE_TTL_SECONDS;
        let retry_too_soon =
            now.saturating_sub(current.last_attempt_seconds) < MINIMUM_RETRY_INTERVAL_SECONDS;
        if (recently_retrieved && !force_refresh) || retry_too_soon {
            return Ok(snapshot_from_cache(current));
        }
    }
    if let Some(current) = cached.as_mut() {
        current.last_attempt_seconds = now;
    }

    let request = get_bounded(
        BOOK_SUMMARY_URL,
        RequestOptions {
            connect_timeout: Duration::from_secs(8),
            total_timeout: Duration::from_secs(20),
            max_body_bytes: MAX_RESPONSE_BYTES,
            max_redirects: 2,
            accept: "application/json",
            user_agent: Some("quant-dashboard-macro/1.0"),
            accept_encoding: None,
            referer: None,
        },
    );
    match request.and_then(|response| {
        if !(200..300).contains(&response.status) {
            return Err(format!("Deribit returned HTTP {}.", response.status));
        }
        let payload: Value = serde_json::from_slice(&response.body)
            .map_err(|error| format!("Deribit returned invalid JSON: {error}"))?;
        if !payload.get("error").unwrap_or(&Value::Null).is_null() {
            return Err("Deribit returned an API error for BTC options data.".to_string());
        }
        parse_options(&payload)
    }) {
        Ok(observations) => {
            let current = CachedSeries {
                observations,
                retrieved_at_seconds: now,
                last_attempt_seconds: now,
                refresh_error: None,
            };
            let snapshot = ProviderSeriesSnapshot {
                observations: current.observations.clone(),
                retrieved_at_seconds: now,
                fetch_status: FetchStatus::Fresh,
                refresh_error: None,
            };
            *cached = Some(current);
            Ok(snapshot)
        }
        Err(error) => {
            if let Some(current) = cached.as_mut() {
                current.last_attempt_seconds = now;
                current.refresh_error = Some(error.chars().take(240).collect());
                Ok(snapshot_from_cache(current))
            } else {
                Err(error)
            }
        }
    }
}

fn snapshot_from_cache(cached: &CachedSeries) -> ProviderSeriesSnapshot {
    ProviderSeriesSnapshot {
        observations: cached.observations.clone(),
        retrieved_at_seconds: cached.retrieved_at_seconds,
        fetch_status: if cached.refresh_error.is_some() {
            FetchStatus::Stale
        } else {
            FetchStatus::Cached
        },
        refresh_error: cached.refresh_error.clone(),
    }
}

fn parse_options(payload: &Value) -> Result<Vec<MacroObservation>, String> {
    let rows = payload
        .get("result")
        .and_then(Value::as_array)
        .ok_or_else(|| "Deribit BTC options response did not contain a result list.".to_string())?;
    let mut by_expiry = BTreeMap::<String, Vec<OptionPosition>>::new();
    for row in rows {
        let Some(name) = row.get("instrument_name").and_then(Value::as_str) else {
            continue;
        };
        let Some((expiry, strike, is_call)) = parse_instrument_name(name) else {
            continue;
        };
        let Some(open_interest) = number(row.get("open_interest")) else {
            continue;
        };
        if open_interest <= 0.0 {
            continue;
        }
        by_expiry.entry(expiry).or_default().push(OptionPosition {
            strike,
            open_interest,
            is_call,
        });
    }

    let observations = by_expiry
        .into_iter()
        .take(MAX_EXPIRIES)
        .filter_map(|(date, positions)| {
            max_pain_strike(&positions).map(|value| MacroObservation {
                date,
                value,
                classification: None,
            })
        })
        .collect::<Vec<_>>();
    if observations.is_empty() {
        return Err("Deribit returned no BTC options with positive open interest.".to_string());
    }
    Ok(observations)
}

fn parse_instrument_name(name: &str) -> Option<(String, f64, bool)> {
    let mut parts = name.split('-');
    if parts.next()? != "BTC" {
        return None;
    }
    let expiry = parse_expiry_date(parts.next()?)?;
    let strike = parts.next()?.parse::<f64>().ok()?;
    let option_type = parts.next()?;
    if parts.next().is_some() || !strike.is_finite() || strike <= 0.0 {
        return None;
    }
    let is_call = match option_type {
        "C" => true,
        "P" => false,
        _ => return None,
    };
    Some((expiry, strike, is_call))
}

fn parse_expiry_date(value: &str) -> Option<String> {
    if value.len() != 7 {
        return None;
    }
    let day = value.get(0..2)?.parse::<u32>().ok()?;
    let month = match value.get(2..5)?.to_ascii_uppercase().as_str() {
        "JAN" => 1,
        "FEB" => 2,
        "MAR" => 3,
        "APR" => 4,
        "MAY" => 5,
        "JUN" => 6,
        "JUL" => 7,
        "AUG" => 8,
        "SEP" => 9,
        "OCT" => 10,
        "NOV" => 11,
        "DEC" => 12,
        _ => return None,
    };
    let year = 2000 + value.get(5..7)?.parse::<i32>().ok()?;
    let date = format!("{year:04}-{month:02}-{day:02}");
    valid_date(&date).then_some(date)
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .or_else(|| value.and_then(Value::as_str)?.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn max_pain_strike(positions: &[OptionPosition]) -> Option<f64> {
    let mut strikes = positions
        .iter()
        .map(|position| position.strike)
        .collect::<Vec<_>>();
    strikes.sort_by(f64::total_cmp);
    strikes.dedup_by(|left, right| (*left - *right).abs() < 1e-9);
    strikes.into_iter().min_by(|left, right| {
        total_payout(*left, positions).total_cmp(&total_payout(*right, positions))
    })
}

fn total_payout(settlement_price: f64, positions: &[OptionPosition]) -> f64 {
    positions
        .iter()
        .map(|position| {
            let intrinsic = if position.is_call {
                (settlement_price - position.strike).max(0.0)
            } else {
                (position.strike - settlement_price).max(0.0)
            };
            intrinsic * position.open_interest
        })
        .sum()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_deribit_expiries_and_computes_each_expiry_max_pain() {
        let payload = serde_json::json!({
            "result": [
                {"instrument_name":"BTC-27NOV26-90000-C","open_interest":2.0},
                {"instrument_name":"BTC-27NOV26-80000-P","open_interest":2.0},
                {"instrument_name":"BTC-25DEC26-100000-C","open_interest":1.0},
                {"instrument_name":"BTC-25DEC26-90000-P","open_interest":1.0},
                {"instrument_name":"BTC-25DEC26-70000-P","open_interest":0.0},
                {"instrument_name":"BTC-25DEC26-70000-X","open_interest":100.0},
                {"instrument_name":"ETH-25DEC26-70000-C","open_interest":100.0}
            ]
        });
        let parsed = parse_options(&payload).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].date, "2026-11-27");
        assert_eq!(parsed[0].value, 80_000.0);
        assert_eq!(parsed[1].date, "2026-12-25");
        assert_eq!(parsed[1].value, 90_000.0);
    }

    #[test]
    fn rejects_empty_or_malformed_option_snapshots() {
        assert!(parse_options(&serde_json::json!({"result": []})).is_err());
        assert!(parse_options(&serde_json::json!({"result": "bad"})).is_err());
        assert_eq!(parse_expiry_date("29FEB24").as_deref(), Some("2024-02-29"));
        assert!(parse_expiry_date("29FEB23").is_none());
        assert!(parse_instrument_name("BTC-25DEC26-NaN-C").is_none());
    }
}
