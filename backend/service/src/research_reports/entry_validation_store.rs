use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

const MAX_PAGE_ROWS: usize = 100;
const MAX_REPORT_JSON_BYTES: usize = 256 * 1024;

fn invalid(message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.to_string())
}

fn decode_json(encoded: String) -> Result<Value, rusqlite::Error> {
    serde_json::from_str(&encoded)
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
            0, rusqlite::types::Type::Text, Box::new(error),
        ))
}

fn scope_values(exchange: &str, product: Option<&str>, search: Option<&str>) -> (String, Vec<String>) {
    let mut clause = "i.is_active=1 AND i.exchange_id=?1".to_string();
    let mut values = vec![exchange.to_string()];
    if let Some(product) = product.filter(|value| !value.is_empty() && *value != "all") {
        values.push(product.to_string());
        clause.push_str(&format!(" AND i.product_id=?{}", values.len()));
    }
    if let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) {
        let escaped = search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        values.push(format!("%{escaped}%"));
        let parameter = values.len();
        clause.push_str(&format!(
            " AND (i.symbol LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE
              OR i.venue_symbol LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE
              OR i.storage_symbol LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE)"
        ));
    }
    (clause, values)
}

fn read_instrument(row: &rusqlite::Row<'_>) -> Result<Value, rusqlite::Error> {
    Ok(json!({
        "instrument_id": row.get::<_, String>(0)?,
        "exchange_id": row.get::<_, String>(1)?,
        "product_id": row.get::<_, String>(2)?,
        "asset_category_id": row.get::<_, String>(3)?,
        "symbol": row.get::<_, String>(4)?,
        "storage_symbol": row.get::<_, String>(5)?,
        "venue_verified": row.get::<_, bool>(6)?,
        "listing_status": row.get::<_, String>(7)?,
        "venue_symbol": row.get::<_, String>(8)?,
        "quote_currency": row.get::<_, String>(9)?,
        "underlying_symbol": row.get::<_, String>(10)?,
        "price_source_policy": row.get::<_, String>(11)?,
        "execution_supported": row.get::<_, bool>(12)?,
    }))
}

fn annotate_eligibility(conn: &Connection, instrument: &mut Value) -> Result<(), rusqlite::Error> {
    let text = |key| instrument.get(key).and_then(Value::as_str).unwrap_or("");
    let eligible = crate::instrument_registry::research_eligibility(
        text("exchange_id"), text("listing_status"),
        instrument.get("venue_verified").and_then(Value::as_bool).unwrap_or(false),
    );
    let excluded = crate::instrument_registry::is_excluded(
        conn, text("exchange_id"), text("asset_category_id"), text("symbol"),
    )?;
    let status = if excluded { "excluded" } else if eligible == "pending_verification" {
        "pending_venue_verification"
    } else { eligible };
    instrument["research_eligibility"] = json!(status);
    Ok(())
}

const INSTRUMENT_COLUMNS: &str = "i.instrument_id,i.exchange_id,i.product_id,i.asset_category_id,
    i.symbol,i.storage_symbol,i.venue_verified,i.listing_status,i.venue_symbol,i.quote_currency,
    i.underlying_symbol,i.price_source_policy,i.execution_supported";

pub(super) fn instrument(conn: &Connection, instrument_id: &str) -> Result<Option<Value>, rusqlite::Error> {
    let mut instrument = conn.query_row(
        &format!("SELECT {INSTRUMENT_COLUMNS} FROM app.instruments i WHERE i.instrument_id=?1 AND i.is_active=1"),
        [instrument_id], read_instrument,
    ).optional()?;
    if let Some(instrument) = instrument.as_mut() { annotate_eligibility(conn, instrument)?; }
    Ok(instrument)
}

fn report_summary(report: &Value) -> Value {
    let models = report.get("models").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
        .iter().map(|model| json!({
            "kind": model.get("kind"), "label": model.get("label"),
            "score_model_id": model.get("score_model_id"),
            "entry_validation_status": model.get("entry_validation_status"),
            "holdout_trades": model.pointer("/backtest/holdout/trades"),
            "holdout_win_rate_pct": model.pointer("/backtest/holdout/win_rate_pct"),
            "holdout_mean_return_pct": model.pointer("/backtest/holdout/mean_return_pct"),
            "selected_exit_bars": model.pointer("/backtest/selected_exit_bars"),
            "blocking_reasons": model.get("blocking_reasons"),
        })).collect::<Vec<_>>();
    json!({
        "computed_at": report.get("computed_at"), "protocol_id": report.get("protocol_id"),
        "status": report.get("status"), "source": report.pointer("/source/id"),
        "source_is_proxy": report.pointer("/source/is_proxy"),
        "history_bars": report.pointer("/history/bars"), "models": models,
        "sizing_eligible": false,
    })
}

pub(super) fn save_report(conn: &Connection, report: &Value) -> Result<(), rusqlite::Error> {
    let instrument_id = report.pointer("/instrument/instrument_id").and_then(Value::as_str)
        .filter(|value| !value.is_empty()).ok_or_else(|| invalid("report instrument_id is required"))?;
    let timeframe = report.get("timeframe").and_then(Value::as_str)
        .filter(|value| crate::DSS_BRESSERT_MTF_TIMEFRAMES.iter().any(|frame| frame.timeframe == *value))
        .ok_or_else(|| invalid("report timeframe is not supported"))?;
    let protocol_id = report.get("protocol_id").and_then(Value::as_str)
        .filter(|value| *value == super::PROTOCOL_ID)
        .ok_or_else(|| invalid("report protocol does not match the current entry validation protocol"))?;
    let computed_at = report.get("computed_at").and_then(Value::as_i64)
        .filter(|value| *value > 0).ok_or_else(|| invalid("report computed_at is required"))?;
    let registered = instrument(conn, instrument_id)?
        .ok_or_else(|| invalid("report instrument is not actively registered"))?;
    for key in ["exchange_id", "product_id", "asset_category_id", "symbol", "storage_symbol"] {
        if report.get("instrument").and_then(|value| value.get(key)) != registered.get(key) {
            return Err(invalid("report identity does not match the exact registered instrument"));
        }
    }
    let report_json = serde_json::to_string(report).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    if report_json.len() > MAX_REPORT_JSON_BYTES { return Err(invalid("entry validation report exceeds its bounded size")); }
    let summary = report_summary(report);
    let backtest_complete = report.get("status").and_then(Value::as_str) == Some("BACKTEST_COMPLETE");
    let has_validated_model = report.get("models").and_then(Value::as_array).is_some_and(|models| {
        models.iter().any(|model| model.get("entry_validation_status").and_then(Value::as_str) == Some("VALIDATED_HOLDOUT"))
    });
    conn.execute(
        "INSERT INTO app.entry_model_validation_reports
         (instrument_id,timeframe,protocol_id,computed_at,report_json,summary_json,backtest_complete,has_validated_model)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(instrument_id,timeframe) DO UPDATE SET protocol_id=excluded.protocol_id,
             computed_at=excluded.computed_at,report_json=excluded.report_json,summary_json=excluded.summary_json,
             backtest_complete=excluded.backtest_complete,has_validated_model=excluded.has_validated_model",
        params![instrument_id,timeframe,protocol_id,computed_at,report_json,summary.to_string(),backtest_complete,has_validated_model],
    )?;
    Ok(())
}

pub(super) fn get_report(conn: &Connection, instrument_id: &str, timeframe: &str) -> Result<Option<Value>, rusqlite::Error> {
    let encoded = conn.query_row(
        "SELECT report_json FROM app.entry_model_validation_reports WHERE instrument_id=?1 AND timeframe=?2",
        params![instrument_id,timeframe], |row| row.get::<_, String>(0),
    ).optional()?;
    encoded.map(decode_json).transpose()
}

fn products(conn: &Connection, exchange: &str) -> Result<Vec<Value>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT p.product_id,p.display_name FROM app.exchange_products p
           WHERE p.exchange_id=?1 AND p.status='enabled'
         UNION ALL SELECT DISTINCT i.product_id,i.product_id FROM app.instruments i
           WHERE i.exchange_id=?1 AND i.is_active=1 AND NOT EXISTS(
             SELECT 1 FROM app.exchange_products p WHERE p.exchange_id=i.exchange_id AND p.product_id=i.product_id)
         ORDER BY 1",
    )?;
    let rows = statement.query_map([exchange], |row| Ok(json!({"id":row.get::<_,String>(0)?,"label":row.get::<_,String>(1)?})))?
        .collect();
    rows
}

pub(super) fn list(
    conn: &Connection, exchange: &str, product: Option<&str>, timeframe: &str,
    search: Option<&str>, offset: usize, limit: usize,
) -> Result<Value, rusqlite::Error> {
    let (filter, values) = scope_values(exchange, product, search);
    let mut count_values = values.clone();
    count_values.push(timeframe.to_string());
    let frame_parameter = count_values.len();
    count_values.push(super::PROTOCOL_ID.to_string());
    let protocol_parameter = count_values.len();
    let (total, backtested, validated) = conn.query_row(
        &format!("SELECT COUNT(*),
            COALESCE(SUM(CASE WHEN r.protocol_id=?{protocol_parameter} THEN r.backtest_complete ELSE 0 END),0),
            COALESCE(SUM(CASE WHEN r.protocol_id=?{protocol_parameter} THEN r.has_validated_model ELSE 0 END),0)
          FROM app.instruments i LEFT JOIN app.entry_model_validation_reports r
            ON r.instrument_id=i.instrument_id AND r.timeframe=?{frame_parameter}
          WHERE {filter}"),
        rusqlite::params_from_iter(count_values.iter()),
        |row| Ok((row.get::<_, usize>(0)?,row.get::<_, usize>(1)?,row.get::<_, usize>(2)?)),
    )?;
    let page_limit = limit.clamp(1, MAX_PAGE_ROWS);
    let page_offset = offset.min(total);
    let mut row_values = values;
    row_values.push(timeframe.to_string());
    let frame_parameter = row_values.len();
    let mut statement = conn.prepare(&format!(
        "SELECT {INSTRUMENT_COLUMNS},r.summary_json,r.protocol_id
           FROM app.instruments i LEFT JOIN app.entry_model_validation_reports r
             ON r.instrument_id=i.instrument_id AND r.timeframe=?{frame_parameter}
           WHERE {filter} ORDER BY i.exchange_id,i.product_id,i.symbol,i.instrument_id
           LIMIT {page_limit} OFFSET {page_offset}"
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(row_values.iter()), |row| {
        let mut instrument = read_instrument(row)?;
        let encoded: Option<String> = row.get(13)?;
        let protocol: Option<String> = row.get(14)?;
        let mut evidence = encoded.map(decode_json).transpose()?.unwrap_or(Value::Null);
        if evidence.is_object() { evidence["protocol_current"] = json!(protocol.as_deref() == Some(super::PROTOCOL_ID)); }
        instrument["evidence"] = evidence;
        Ok(instrument)
    })?.collect::<Result<Vec<_>, _>>()?;
    let rows = rows.into_iter().map(|mut row| {
        annotate_eligibility(conn, &mut row)?;
        Ok(row)
    }).collect::<Result<Vec<_>, rusqlite::Error>>()?;
    let next_offset = page_offset + rows.len();
    Ok(json!({
        "rows":rows,"total":total,"offset":page_offset,"limit":page_limit,
        "has_more":next_offset < total,"next_offset":next_offset,
        "summary":{"registered":total,"backtested":backtested,"validated":validated},
        "products":products(conn,exchange)?,
        "timeframes":crate::DSS_BRESSERT_MTF_TIMEFRAMES.iter().map(|frame|frame.timeframe).collect::<Vec<_>>(),
        "protocol_id":super::PROTOCOL_ID,
    }))
}

#[cfg(test)]
#[path = "entry_validation_store_tests.rs"]
mod tests;
