use super::*;

const MAX_SIGNAL_SCREENER_ROWS: usize = 2_000;
const MAX_SIGNAL_SCREENER_PAGE_SIZE: usize = MAX_SIGNAL_SCREENER_ROWS;

fn stockbit_signal_score_model_id(strategy_id: &str, timeframe: &str) -> Option<&'static str> {
    if quant_core::stockbit_nwe_signal_strategy_id(timeframe).as_deref() == Some(strategy_id) {
        Some(quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID)
    } else if quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe).as_deref()
        == Some(strategy_id)
    {
        Some(quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID)
    } else {
        None
    }
}

fn stockbit_signal_score_row_scope_sql() -> String {
    let model_timeframe_pairs = quant_core::DSS_BRESSERT_MTF_TIMEFRAMES
        .iter()
        .flat_map(|definition| {
            let timeframe = definition.timeframe;
            [
                quant_core::stockbit_nwe_signal_strategy_id(timeframe),
                quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe),
            ]
            .into_iter()
            .flatten()
            .map(move |strategy_id| {
                format!("(timeframe='{timeframe}' AND signal_strategy_id='{strategy_id}')")
            })
        })
        .collect::<Vec<_>>()
        .join(" OR ");
    format!(
        "exchange_id='{}' AND product_id='{}' AND asset_category_id='{}' AND ({model_timeframe_pairs})",
        quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID,
        quant_core::STOCKBIT_NWE_4H_PRODUCT_ID,
        quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID,
    )
}

pub(super) fn signal_screener_display_order_sql() -> String {
    format!(
        "(CASE WHEN timeframe='15m' THEN 0 ELSE 1 END),
         (CASE WHEN {} THEN 0 ELSE 1 END),
         score DESC,last_updated_at DESC,exchange_id,product_id,symbol,
         signal_strategy_id,timeframe",
        stockbit_signal_score_row_scope_sql()
    )
}

fn is_stockbit_signal_score_row(
    exchange_id: &str,
    product_id: &str,
    asset_category_id: &str,
    strategy_id: &str,
    timeframe: &str,
) -> bool {
    exchange_id == quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID
        && product_id == quant_core::STOCKBIT_NWE_4H_PRODUCT_ID
        && asset_category_id == quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID
        && stockbit_signal_score_model_id(strategy_id, timeframe).is_some()
}

fn is_crypto_nwe_dss_signal_score_row(
    asset_category_id: &str,
    strategy_id: &str,
    timeframe: &str,
) -> bool {
    asset_category_id.starts_with("crypto_")
        && (quant_core::crypto_nwe_signal_strategy_id(timeframe).as_deref() == Some(strategy_id)
            || quant_core::crypto_dss_bressert_signal_strategy_id(timeframe).as_deref()
                == Some(strategy_id))
}

pub(crate) fn stockbit_signal_score_rows_for_symbol(
    connection: &Connection,
    symbol: &str,
) -> Result<BTreeSet<(String, String)>, rusqlite::Error> {
    let mut statement = connection.prepare(&format!(
        "SELECT signal_strategy_id,timeframe FROM signals.signal_screener_rows
          WHERE symbol=?1 AND {}",
        stockbit_signal_score_row_scope_sql()
    ))?;
    let rows = statement.query_map([symbol], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    rows.collect()
}

pub(crate) fn initialize_screener_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    let rows_table_existed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM signals.sqlite_schema
                       WHERE type='table' AND name='signal_screener_rows')",
        [],
        |row| row.get(0),
    )?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS signals.signal_store_meta (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS signals.signal_screener_status (
             id INTEGER PRIMARY KEY CHECK(id=1),
             last_attempt_at INTEGER NOT NULL,
             last_success_at INTEGER,
             last_attempt_status TEXT NOT NULL,
             last_error TEXT NOT NULL DEFAULT '',
             signal_count INTEGER NOT NULL DEFAULT 0,
             price_data_status TEXT NOT NULL DEFAULT 'ready',
             price_data_unavailable_count INTEGER NOT NULL DEFAULT 0
         );",
    )?;
    create_signal_screener_rows_table(
        connection,
        "signal_screener_rows",
        true,
        quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX,
    )?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS signals.idx_signal_screener_score
             ON signal_screener_rows(score DESC,last_updated_at DESC);",
    )?;
    let display_order = signal_screener_display_order_sql();
    connection.execute_batch(&format!(
        "CREATE INDEX IF NOT EXISTS signals.idx_signal_screener_display_order
             ON signal_screener_rows({display_order});"
    ))?;
    connection.execute(
        "INSERT INTO signals.signal_store_meta(key,value)
         VALUES('signal_layout_version',?1)
         ON CONFLICT(key) DO NOTHING",
        [SIGNAL_LAYOUT_VERSION],
    )?;
    if !rows_table_existed {
        connection.execute(
            "INSERT INTO signals.signal_store_meta(key,value)
             VALUES('signal_score_schema_version',?1)
             ON CONFLICT(key) DO NOTHING",
            [SIGNAL_SCORE_SCHEMA_VERSION],
        )?;
    }
    Ok(())
}

pub(crate) fn create_signal_screener_rows_table(
    connection: &Connection,
    table_name: &str,
    if_not_exists: bool,
    score_scale_max: f64,
) -> Result<(), rusqlite::Error> {
    if !matches!(
        table_name,
        "signal_screener_rows" | "signal_screener_rows_v2" | "signal_screener_rows_v3"
    ) || !score_scale_max.is_finite()
        || score_scale_max <= 0.0
        || score_scale_max > quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "unsupported signal screener table name".into(),
        ));
    }
    let if_not_exists = if if_not_exists { "IF NOT EXISTS " } else { "" };
    connection.execute_batch(&format!(
        "CREATE TABLE {if_not_exists}signals.{table_name} (
             exchange_id TEXT NOT NULL,
             product_id TEXT NOT NULL,
             asset_category_id TEXT NOT NULL,
             symbol TEXT NOT NULL,
             storage_symbol TEXT NOT NULL DEFAULT '',
             signal_strategy_id TEXT NOT NULL,
             signal_strategy_name TEXT NOT NULL,
             timeframe TEXT NOT NULL,
             direction TEXT NOT NULL CHECK(direction IN ('LONG','SHORT')),
             score REAL NOT NULL CHECK(score >= 0 AND score <= {:.0}),
             strategy_strength REAL,
             confluence_score REAL,
             score_basis_json TEXT NOT NULL,
             current_price REAL NOT NULL,
             price_timeframe TEXT NOT NULL,
             price_bar_time INTEGER NOT NULL,
             signal_bar_time INTEGER NOT NULL,
             data_source TEXT NOT NULL DEFAULT 'unknown',
             data_provider TEXT NOT NULL DEFAULT 'unknown',
             data_quality TEXT NOT NULL DEFAULT 'unknown',
             evidence_status TEXT NOT NULL DEFAULT 'VALIDATION_PENDING',
             evidence_source TEXT NOT NULL DEFAULT '',
             last_updated_at INTEGER NOT NULL,
             PRIMARY KEY(exchange_id,product_id,asset_category_id,symbol,signal_strategy_id,timeframe)
         );",
        score_scale_max,
    ))
}
#[path = "screener_row.rs"]
mod row;
pub(crate) use row::signal_screener_row;

#[derive(Clone, Debug)]
pub(crate) struct StockbitSignalScoreUpdate {
    exchange_id: String,
    product_id: String,
    asset_category_id: String,
    symbol: String,
    #[cfg(test)]
    storage_symbol: String,
    signal_strategy_id: String,
    timeframe: String,
    direction: Option<String>,
    signal_bar_time: Option<i64>,
    score: f64,
    #[cfg(test)]
    score_status: String,
    #[cfg(test)]
    score_bar_time: i64,
    #[cfg(test)]
    score_bar_is_forming: bool,
    strategy_strength: Option<f64>,
    confluence_score: Option<f64>,
    score_basis_json: String,
    current_price: f64,
    price_timeframe: String,
    price_bar_time: i64,
    data_source: String,
    data_provider: String,
    data_quality: String,
}

#[cfg(test)]
impl StockbitSignalScoreUpdate {
    fn from_test_value(value: &Value) -> Option<Self> {
        let string = |key: &str| value.get(key)?.as_str().map(ToOwned::to_owned);
        Some(Self {
            exchange_id: string("exchange_id")?,
            product_id: string("product_id")?,
            asset_category_id: string("asset_category_id")?,
            symbol: string("symbol")?,
            storage_symbol: string("storage_symbol")?,
            signal_strategy_id: string("signal_strategy_id")?,
            timeframe: string("timeframe")?,
            direction: value
                .get("direction")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            signal_bar_time: value.get("signal_bar_time").and_then(Value::as_i64),
            score: value.get("score")?.as_f64()?,
            score_status: string("score_status")?,
            score_bar_time: value.get("score_bar_time")?.as_i64()?,
            score_bar_is_forming: value.get("score_bar_is_forming")?.as_bool()?,
            strategy_strength: value.get("strategy_strength").and_then(Value::as_f64),
            confluence_score: value.get("confluence_score").and_then(Value::as_f64),
            score_basis_json: serde_json::to_string(value.get("score_basis")?).ok()?,
            current_price: value.get("current_price")?.as_f64()?,
            price_timeframe: string("price_timeframe")?,
            price_bar_time: value.get("price_bar_time")?.as_i64()?,
            data_source: string("data_source")?,
            data_provider: string("data_provider")?,
            data_quality: string("data_quality")?,
        })
    }
}

pub(crate) fn compact_stockbit_signal_score_update(
    candidate: &Value,
) -> Option<StockbitSignalScoreUpdate> {
    if candidate.get("Score_Update_Only").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let exchange_id = first_candidate_string(candidate, &["Exchange_ID", "exchange_id"])?;
    let product_id = first_candidate_string(candidate, &["Product_ID", "product_id"])?;
    let asset_category_id =
        first_candidate_string(candidate, &["Asset_Category_ID", "asset_category_id"])?;
    let symbol = first_candidate_string(candidate, &["Symbol", "symbol"])?;
    let storage_symbol = first_candidate_string(candidate, &["Data_Symbol", "data_symbol"])
        .unwrap_or_else(|| symbol.clone());
    let signal_strategy_id =
        first_candidate_string(candidate, &["Signal_Strategy_ID", "signal_strategy_id"])?;
    let timeframe = first_candidate_string(candidate, &["timeframe"])?;
    let exchange_id = canonical_exchange_id(&exchange_id);
    let Some(expected_score_model_id) =
        stockbit_signal_score_model_id(&signal_strategy_id, &timeframe)
    else {
        return None;
    };
    if exchange_id != quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID
        || product_id != quant_core::STOCKBIT_NWE_4H_PRODUCT_ID
        || asset_category_id != quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID
    {
        return None;
    }
    let current_price = candidate
        .get("Current_Price")
        .and_then(Value::as_f64)
        .filter(|price| price.is_finite() && *price > 0.0)?;
    let score_bar_time = candidate
        .get("Signal_Score_Bar_Time")
        .and_then(Value::as_i64)?;
    let score_bar_is_forming = candidate
        .get("Signal_Score_Bar_Is_Forming")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let now_ns = i64::try_from(now_secs())
        .unwrap_or(i64::MAX)
        .saturating_mul(crate::NANOS_PER_SECOND);
    let score_bar_duration_ns = crate::timeframe_duration_ns(&timeframe).unwrap_or_default();
    let forming_bar_is_valid = score_bar_is_forming
        && score_bar_time <= now_ns
        && score_bar_time.saturating_add(score_bar_duration_ns) > now_ns;
    if score_bar_duration_ns <= 0
        || (!forming_bar_is_valid
            && !crate::completed_bar_is_fresh_at(
                &storage_symbol,
                &timeframe,
                score_bar_time,
                now_ns,
            ))
    {
        return None;
    }
    let data_source = first_candidate_string(candidate, &["Data_Source", "data_source"])?;
    let data_provider = first_candidate_string(candidate, &["Data_Provider", "data_provider"])?;
    let data_quality = first_candidate_string(candidate, &["Data_Quality", "data_quality"])?;
    let score_model_id = first_candidate_string(candidate, &["Signal_Scoring_Model_ID"])?;
    if score_model_id != expected_score_model_id {
        return None;
    }
    let four_indicator_score_model = super::is_four_indicator_score_model(&score_model_id);
    let score_status = candidate
        .get("Score_Update_Status")
        .and_then(Value::as_str)
        .filter(|status| matches!(*status, "qualifying" | "not_qualifying"))
        .unwrap_or("not_qualifying");
    let score = candidate
        .get("Signal_Strength")
        .and_then(Value::as_f64)
        .filter(|score| {
            score.is_finite()
                && (0.0..=if four_indicator_score_model {
                    quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX
                } else {
                    quant_core::NWE_DSS_SIGNAL_SCORE_SCALE_MAX
                })
                    .contains(score)
        })
        .or_else(|| (score_status == "not_qualifying").then_some(0.0))?;
    if score_status == "qualifying" && score < quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40 {
        return None;
    }
    let strategy_strength = candidate
        .get("Strategy_Strength")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite());
    let confluence_score = candidate
        .get("Signal_Confluence_Score")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite());
    let direction = candidate
        .get("Signal_Direction")
        .and_then(Value::as_str)
        .filter(|direction| matches!(*direction, "LONG" | "SHORT"));
    if score_status == "qualifying" && direction.is_none() {
        return None;
    }
    let signal_bar_time = candidate.get("Signal_Bar_Time").and_then(Value::as_i64);
    let components = candidate
        .get("Signal_Score_Components")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if score_status == "qualifying" {
        let (Some(strategy_strength), Some(confluence_score)) =
            (strategy_strength, confluence_score)
        else {
            return None;
        };
        let group_score_scale_max = if four_indicator_score_model {
            quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX
        } else {
            quant_core::NWE_DSS_COMPONENT_SCORE_SCALE_MAX
        };
        let component_points_max = if four_indicator_score_model {
            quant_core::FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX
        } else {
            quant_core::NWE_DSS_COMPONENT_SCORE_SCALE_MAX
        };
        if !super::score_components_match(
            &components,
            four_indicator_score_model,
            component_points_max,
            group_score_scale_max,
            strategy_strength,
            confluence_score,
            score,
            false,
        ) {
            return None;
        }
    }
    let price_timeframe = candidate["Current_Price_Timeframe"]
        .as_str()
        .unwrap_or(&timeframe)
        .to_string();
    let price_bar_time = candidate["Current_Price_Bar_Time"]
        .as_i64()
        .unwrap_or(score_bar_time);
    let score_basis_json = serde_json::to_string(&json!({
        "model_id": score_model_id,
        "score_scale_max": if four_indicator_score_model {
            quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX
        } else {
            quant_core::NWE_DSS_SIGNAL_SCORE_SCALE_MAX
        },
        "minimum_score_out_of_20": if four_indicator_score_model {
            None
        } else {
            Some(quant_core::NWE_DSS_SIGNAL_MINIMUM_SCORE_OUT_OF_20)
        },
        "minimum_score_out_of_40": if four_indicator_score_model {
            Some(quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40)
        } else {
            None
        },
        "component_score_scale_max": if four_indicator_score_model {
            quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX
        } else {
            quant_core::NWE_DSS_COMPONENT_SCORE_SCALE_MAX
        },
        "indicator_component_score_scale_max": if four_indicator_score_model {
            quant_core::FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX
        } else {
            quant_core::NWE_DSS_COMPONENT_SCORE_SCALE_MAX
        },
        "score_model_status": "current",
        "strategy_strength": strategy_strength,
        "confluence_score": confluence_score,
        "components": components,
        "status": score_status,
        "score_bar_time": score_bar_time,
        "score_bar_is_forming": score_bar_is_forming,
    }))
    .ok()?;
    Some(StockbitSignalScoreUpdate {
        exchange_id,
        product_id,
        asset_category_id,
        symbol,
        #[cfg(test)]
        storage_symbol,
        signal_strategy_id,
        timeframe,
        direction: direction.map(ToOwned::to_owned),
        signal_bar_time,
        score,
        #[cfg(test)]
        score_status: score_status.to_string(),
        #[cfg(test)]
        score_bar_time,
        #[cfg(test)]
        score_bar_is_forming,
        strategy_strength,
        confluence_score,
        score_basis_json,
        current_price,
        price_timeframe,
        price_bar_time,
        data_source,
        data_provider,
        data_quality,
    })
}

#[cfg(test)]
#[path = "screener_query_plan_tests.rs"]
mod query_plan_tests;

#[cfg(test)]
pub(crate) fn signal_screener_score_update(candidate: &Value) -> Option<Value> {
    let update = compact_stockbit_signal_score_update(candidate)?;
    let score_basis = serde_json::from_str::<Value>(&update.score_basis_json).ok()?;
    Some(json!({
        "exchange_id": update.exchange_id,
        "product_id": update.product_id,
        "asset_category_id": update.asset_category_id,
        "symbol": update.symbol,
        "storage_symbol": update.storage_symbol,
        "signal_strategy_id": update.signal_strategy_id,
        "timeframe": update.timeframe,
        "direction": update.direction,
        "signal_bar_time": update.signal_bar_time,
        "score": update.score,
        "strategy_strength": update.strategy_strength,
        "confluence_score": update.confluence_score,
        "score_basis": score_basis,
        "current_price": update.current_price,
        "price_timeframe": update.price_timeframe,
        "price_bar_time": update.price_bar_time,
        "score_status": update.score_status,
        "score_bar_time": update.score_bar_time,
        "score_bar_is_forming": update.score_bar_is_forming,
        "data_source": update.data_source,
        "data_provider": update.data_provider,
        "data_quality": update.data_quality,
    }))
}

#[cfg(test)]
#[path = "../signal_alerts_screener_tests.rs"]
mod screener_tests;

#[cfg(test)]
#[path = "../signal_alerts_screener_expiry_tests.rs"]
mod expiry_tests;

#[path = "screener/snapshot.rs"]
mod snapshot;
pub(crate) use snapshot::{
    record_signal_screener_failure, replace_signal_screener_snapshot_with_readiness,
};
#[cfg(test)]
pub(crate) use snapshot::{
    replace_signal_screener_snapshot, replace_signal_screener_snapshot_with_score_updates,
    replace_signal_screener_snapshot_with_status,
};

#[path = "screener/listing.rs"]
mod listing;
#[cfg(test)]
use listing::list_signal_screener;
pub(crate) use listing::{count_current_signal_screener_rows, list_signal_screener_page};
