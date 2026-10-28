use super::*;
use rusqlite::OptionalExtension;
use serde::Serialize;

const READINESS_META_KEY: &str = "scan_readiness_v1";
const MAX_PRODUCT_ISSUE_EXAMPLES: usize = 3;

#[derive(Default, Serialize)]
pub(crate) struct TimeframeReadiness {
    pub(crate) timeframe: String,
    pub(crate) missing_history_count: usize,
    pub(crate) stale_history_count: usize,
    pub(crate) insufficient_history_count: usize,
    pub(crate) nwe_insufficient_history_count: usize,
    pub(crate) source_mismatch_count: usize,
    pub(crate) data_error_count: usize,
    pub(crate) evaluated_count: usize,
    pub(crate) evaluated_model_count: usize,
    pub(crate) qualifying_signal_count: usize,
}

#[derive(Default, Serialize)]
pub(crate) struct ReadinessIssue {
    pub(crate) symbol: String,
    pub(crate) reason: String,
    pub(crate) timeframe: String,
    pub(crate) source: Option<String>,
    pub(crate) data_provider: Option<String>,
    pub(crate) last_bar_time: Option<i64>,
    pub(crate) last_success_at: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) available_bars: Option<usize>,
    pub(crate) required_bars: Option<usize>,
}

#[derive(Default)]
pub(crate) struct InstrumentReadiness {
    pub(crate) price_status: &'static str,
    pub(crate) timeframes: Vec<TimeframeReadiness>,
    pub(crate) issues: Vec<ReadinessIssue>,
}

impl InstrumentReadiness {
    pub(crate) fn issue(&mut self, issue: ReadinessIssue) {
        if self.issues.len() < MAX_PRODUCT_ISSUE_EXAMPLES {
            self.issues.push(issue);
        }
    }
}

#[derive(Default, Serialize)]
struct ProductReadiness {
    exchange_id: String,
    product_id: String,
    instrument_count: usize,
    fresh_price_count: usize,
    missing_price_count: usize,
    stale_price_count: usize,
    data_error_count: usize,
    first_checked_at: i64,
    checked_at: i64,
    examples: Vec<ReadinessIssue>,
    timeframes: Vec<TimeframeReadiness>,
}

pub(crate) struct SignalReadiness {
    scan_started_at: i64,
    products: BTreeMap<(String, String), ProductReadiness>,
}

impl SignalReadiness {
    pub(crate) fn new() -> Self {
        Self {
            scan_started_at: now_secs() as i64,
            products: BTreeMap::new(),
        }
    }

    #[cfg(not(test))]
    pub(crate) fn merge(&mut self, mut other: Self) {
        self.scan_started_at = self.scan_started_at.min(other.scan_started_at);
        self.products.append(&mut other.products);
    }

    pub(crate) fn record(
        &mut self,
        exchange: &str,
        product_id: &str,
        storage_symbol: &str,
        mut instrument: InstrumentReadiness,
        accepted_signals: &BTreeMap<String, usize>,
        market_connection: Option<&Connection>,
    ) {
        let now = now_secs() as i64;
        let product = self
            .products
            .entry((exchange.into(), product_id.into()))
            .or_insert_with(|| ProductReadiness {
                exchange_id: exchange.into(),
                product_id: product_id.into(),
                first_checked_at: now,
                ..ProductReadiness::default()
            });
        product.instrument_count += 1;
        product.checked_at = now;
        match instrument.price_status {
            "fresh" => product.fresh_price_count += 1,
            "stale" => product.stale_price_count += 1,
            "error" => product.data_error_count += 1,
            _ => product.missing_price_count += 1,
        }
        for frame in &mut instrument.timeframes {
            frame.qualifying_signal_count =
                accepted_signals.get(&frame.timeframe).copied().unwrap_or(0);
        }
        for frame in instrument.timeframes {
            let index = product
                .timeframes
                .iter()
                .position(|item| item.timeframe == frame.timeframe);
            if let Some(index) = index {
                let existing = &mut product.timeframes[index];
                existing.missing_history_count += frame.missing_history_count;
                existing.stale_history_count += frame.stale_history_count;
                existing.insufficient_history_count += frame.insufficient_history_count;
                existing.nwe_insufficient_history_count += frame.nwe_insufficient_history_count;
                existing.source_mismatch_count += frame.source_mismatch_count;
                existing.data_error_count += frame.data_error_count;
                existing.evaluated_count += frame.evaluated_count;
                existing.evaluated_model_count += frame.evaluated_model_count;
                existing.qualifying_signal_count += frame.qualifying_signal_count;
            } else {
                product.timeframes.push(frame);
            }
        }
        for mut issue in instrument.issues {
            if product.examples.len() >= MAX_PRODUCT_ISSUE_EXAMPLES {
                break;
            }
            if product
                .examples
                .iter()
                .any(|example| example.reason == issue.reason)
            {
                continue;
            }
            if let (Some(connection), Some(source)) = (market_connection, issue.source.as_deref()) {
                let metadata = connection
                    .query_row(
                        "SELECT last_success,last_error FROM series
                         WHERE symbol=?1 AND source=?2 AND interval=?3",
                        params![storage_symbol, source, issue.timeframe],
                        |row| {
                            Ok((
                                row.get::<_, Option<String>>(0)?,
                                row.get::<_, Option<String>>(1)?,
                            ))
                        },
                    )
                    .optional();
                if let Ok(Some((success, error))) = metadata {
                    issue.last_success_at = success;
                    if issue.error.is_none() {
                        issue.error = error;
                    }
                }
            }
            issue.error = issue.error.map(|error| error.chars().take(250).collect());
            product.examples.push(issue);
        }
    }

    pub(crate) fn into_value(self) -> Value {
        json!({
            "version": 1,
            "scan_started_at": self.scan_started_at,
            "checked_at": now_secs() as i64,
            "dss_required_bars": quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS,
            "nwe_required_bars": quant_core::stockbit_nwe_4h_minimum_history_bars(),
            "products": self.products.into_values().collect::<Vec<_>>(),
        })
    }
}

pub(super) fn save_scan_readiness(
    connection: &Connection,
    readiness: Option<&Value>,
) -> Result<(), rusqlite::Error> {
    if let Some(readiness) = readiness {
        let encoded = serde_json::to_string(readiness)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        connection.execute(
            "INSERT INTO signals.signal_store_meta(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![READINESS_META_KEY, encoded],
        )?;
    } else {
        connection.execute(
            "DELETE FROM signals.signal_store_meta WHERE key=?1",
            [READINESS_META_KEY],
        )?;
    }
    Ok(())
}

pub(super) fn load_scan_readiness(connection: &Connection) -> Result<Value, rusqlite::Error> {
    let encoded = connection
        .query_row(
            "SELECT value FROM signals.signal_store_meta WHERE key=?1",
            [READINESS_META_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(encoded
        .and_then(|value| serde_json::from_str::<Value>(&value).ok())
        .filter(|value| value["version"] == 1 && value["products"].is_array())
        .unwrap_or(Value::Null))
}

#[cfg(test)]
#[path = "readiness_tests.rs"]
mod tests;
