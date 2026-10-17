use rusqlite::{params, Connection};
use serde_json::{json, Value};

const MAX_IMPORT_ROWS: usize = 100;
const FLOW_WINDOW_SESSIONS: usize = 5;
const FOREIGN_FLOW_SOURCE_LABEL: &str = "Stockbit manual import";

pub(super) fn initialize(conn: &Connection) -> Result<(), rusqlite::Error> {
    initialize_schema(conn)
}

pub(super) fn initialize_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS app.stockbit_foreign_flow (
             symbol TEXT NOT NULL,
             session_date TEXT NOT NULL,
             net_foreign_value_idr REAL NOT NULL,
             foreign_buy_value_idr REAL,
             foreign_sell_value_idr REAL,
             foreign_buy_shares REAL,
             foreign_sell_shares REAL,
             source_label TEXT NOT NULL DEFAULT '{FOREIGN_FLOW_SOURCE_LABEL}',
             observed_at TEXT,
             imported_at INTEGER NOT NULL DEFAULT (unixepoch()),
             PRIMARY KEY(symbol, session_date)
         );
         CREATE INDEX IF NOT EXISTS app.idx_stockbit_foreign_flow_symbol_date
             ON stockbit_foreign_flow(symbol, session_date DESC);"
    ))
}

pub(super) fn is_stockbit_day_trade_strategy(strategy: &str) -> bool {
    matches!(
        strategy.trim().to_ascii_lowercase().as_str(),
        "idx_stockbit_day_trade"
            | "stockbit_day_trade"
            | "stockbit"
            | "idx"
            | "idx_stockbit_quality_day_trade_v1"
            | "idx_stockbit_pullback_day_trade_v1"
            | "idx_stockbit_momentum_day_trade_v1"
            | "idx_stockbit_opening_range_momentum"
            | "idx_stockbit_first_half_hour_momentum"
            | "idx_stockbit_time_of_day_momentum"
    )
}

pub(super) fn attach_to_plan(conn: &Connection, plan: &mut Value, symbol: &str) {
    let exchange = plan
        .get("Exchange")
        .or_else(|| plan.get("exchange"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let is_day_trade = plan
        .get("Trade_Style")
        .and_then(Value::as_str)
        .is_some_and(|style| style.eq_ignore_ascii_case("day_trade"));
    let has_day_trade_strategy = ["Signal_Strategy_ID", "Strategy_ID", "Execution_Profile_ID"]
        .iter()
        .filter_map(|key| plan.get(*key).and_then(Value::as_str))
        .any(is_stockbit_day_trade_strategy);
    if !exchange.eq_ignore_ascii_case("stockbit") || !(is_day_trade || has_day_trade_strategy) {
        return;
    }
    if let Ok(summary) = summary_for_symbol(conn, symbol) {
        if let Some(object) = plan.as_object_mut() {
            object.insert("stockbit_foreign_flow".to_string(), summary);
        }
    }
}

pub(super) fn normalize_symbol(raw: &str) -> Result<String, String> {
    let symbol = raw.trim().to_ascii_uppercase();
    let bare = symbol.strip_suffix(".JK").unwrap_or(&symbol);
    if bare.is_empty() || bare.len() > 6 || !bare.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(format!("Invalid IDX symbol: {raw}"));
    }
    Ok(format!("{bare}.JK"))
}

fn valid_session_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u16>().unwrap_or(0);
    let month = value[5..7].parse::<u8>().unwrap_or(0);
    let day = value[8..10].parse::<u8>().unwrap_or(0);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    day > 0 && day <= max_day
}

fn numeric_field(row: &Value, key: &str) -> Result<Option<f64>, String> {
    let Some(value) = row.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let parsed = value
        .as_f64()
        .or_else(|| {
            value
                .as_str()
                .and_then(|text| text.trim().parse::<f64>().ok())
        })
        .filter(|number| number.is_finite());
    parsed
        .map(Some)
        .ok_or_else(|| format!("{key} must be a finite number"))
}

pub(super) fn import_payload(conn: &Connection, payload: &Value) -> Result<Value, String> {
    let records = payload
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| "records must be an array".to_string())?;
    if records.is_empty() || records.len() > MAX_IMPORT_ROWS {
        return Err(format!("Import 1–{MAX_IMPORT_ROWS} rows per request"));
    }

    let mut validated = Vec::with_capacity(records.len());
    for row in records {
        let symbol = row
            .get("symbol")
            .and_then(Value::as_str)
            .ok_or_else(|| "Each row needs a symbol".to_string())?;
        let symbol = normalize_symbol(symbol)?;
        let session_date = row
            .get("session_date")
            .and_then(Value::as_str)
            .ok_or_else(|| "Each row needs session_date in YYYY-MM-DD format".to_string())?
            .trim()
            .to_string();
        if !valid_session_date(&session_date) {
            return Err(format!("Invalid session_date: {session_date}"));
        }

        let buy_value = numeric_field(row, "foreign_buy_value_idr")?;
        let sell_value = numeric_field(row, "foreign_sell_value_idr")?;
        let net_value =
            match numeric_field(row, "net_foreign_value_idr")? {
                Some(value) => value,
                None => match (buy_value, sell_value) {
                    (Some(buy), Some(sell)) => buy - sell,
                    _ => return Err(
                        "Each row needs net_foreign_value_idr or both foreign buy and sell values"
                            .to_string(),
                    ),
                },
            };
        if net_value.abs() > 1.0e15
            || buy_value.is_some_and(|value| value < 0.0 || value > 1.0e15)
            || sell_value.is_some_and(|value| value < 0.0 || value > 1.0e15)
        {
            return Err(format!(
                "Flow value is outside the supported IDR range for {symbol}"
            ));
        }
        let buy_shares = numeric_field(row, "foreign_buy_shares")?;
        let sell_shares = numeric_field(row, "foreign_sell_shares")?;
        if buy_shares.is_some_and(|value| value < 0.0)
            || sell_shares.is_some_and(|value| value < 0.0)
        {
            return Err(format!(
                "Foreign share volumes cannot be negative for {symbol}"
            ));
        }
        let observed_at = row
            .get("observed_at")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        validated.push((
            symbol,
            session_date,
            net_value,
            buy_value,
            sell_value,
            buy_shares,
            sell_shares,
            observed_at,
        ));
    }

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let result = (|| {
        for (
            symbol,
            session_date,
            net_value,
            buy_value,
            sell_value,
            buy_shares,
            sell_shares,
            observed_at,
        ) in &validated
        {
            conn.execute(
                "INSERT INTO stockbit_foreign_flow (
                     symbol, session_date, net_foreign_value_idr,
                     foreign_buy_value_idr, foreign_sell_value_idr,
                     foreign_buy_shares, foreign_sell_shares,
                     source_label, observed_at, imported_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, unixepoch())
                 ON CONFLICT(symbol, session_date) DO UPDATE SET
                     net_foreign_value_idr=excluded.net_foreign_value_idr,
                     foreign_buy_value_idr=excluded.foreign_buy_value_idr,
                     foreign_sell_value_idr=excluded.foreign_sell_value_idr,
                     foreign_buy_shares=excluded.foreign_buy_shares,
                     foreign_sell_shares=excluded.foreign_sell_shares,
                     source_label=excluded.source_label,
                     observed_at=excluded.observed_at,
                     imported_at=excluded.imported_at",
                params![
                    symbol,
                    session_date,
                    net_value,
                    buy_value,
                    sell_value,
                    buy_shares,
                    sell_shares,
                    FOREIGN_FLOW_SOURCE_LABEL,
                    observed_at
                ],
            )
            .map_err(|error| error.to_string())?;
        }
        Ok::<(), String>(())
    })();
    match result {
        Ok(()) => conn
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string())?,
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(error);
        }
    }

    let symbols = validated
        .iter()
        .map(|record| record.0.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    Ok(json!({
        "status": "ok",
        "imported_rows": validated.len(),
        "symbols": symbols,
        "source": FOREIGN_FLOW_SOURCE_LABEL,
        "note": "Flow is session-based context, not a live intraday feed or a standalone entry signal."
    }))
}

pub(super) fn summary_for_symbol(
    conn: &Connection,
    raw_symbol: &str,
) -> Result<Value, rusqlite::Error> {
    let symbol = normalize_symbol(raw_symbol).map_err(|_| {
        rusqlite::Error::InvalidParameterName("invalid Stockbit symbol".to_string())
    })?;
    let mut statement = conn.prepare(
        "SELECT session_date, net_foreign_value_idr, source_label, observed_at
         FROM stockbit_foreign_flow
         WHERE symbol=?1
         ORDER BY session_date DESC
         LIMIT ?2",
    )?;
    let rows = statement.query_map(params![symbol, FLOW_WINDOW_SESSIONS as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut observations = rows.collect::<Result<Vec<_>, _>>()?;
    if observations.is_empty() {
        return Ok(json!({
            "status": "NO_DATA",
            "bias": "UNKNOWN",
            "label": "Foreign flow unavailable",
            "symbol": symbol,
            "window_sessions": FLOW_WINDOW_SESSIONS,
            "observations": 0,
            "note": "Import dated Stockbit net foreign flow to add context."
        }));
    }

    let latest_session_date = observations[0].0.clone();
    let latest_net_value = observations[0].1;
    let latest_source = observations[0].2.clone();
    let latest_observed_at = observations[0].3.clone();
    let session_count = observations.len();
    let net_sum = observations.iter().map(|row| row.1).sum::<f64>();
    let positive = observations.iter().filter(|row| row.1 > 0.0).count();
    let negative = observations.iter().filter(|row| row.1 < 0.0).count();
    observations.reverse();
    let sessions = observations
        .iter()
        .map(|(date, net, _, _)| json!({ "session_date": date, "net_foreign_value_idr": net }))
        .collect::<Vec<_>>();

    let session_window_label = format!("{FLOW_WINDOW_SESSIONS}-session");
    let (status, bias, label) = if session_count < FLOW_WINDOW_SESSIONS {
        (
            "INCOMPLETE",
            "UNKNOWN",
            format!(
                "Foreign flow history incomplete ({session_count}/{FLOW_WINDOW_SESSIONS} sessions)"
            ),
        )
    } else if net_sum > 0.0 {
        (
            "READY",
            "NET_BUY",
            format!("{session_window_label} net foreign buy"),
        )
    } else if net_sum < 0.0 {
        (
            "READY",
            "NET_SELL",
            format!("{session_window_label} net foreign sell"),
        )
    } else {
        (
            "READY",
            "BALANCED",
            format!("{session_window_label} net foreign flow is balanced"),
        )
    };

    Ok(json!({
        "status": status,
        "bias": bias,
        "label": label,
        "symbol": symbol,
        "window_sessions": FLOW_WINDOW_SESSIONS,
        "observations": session_count,
        "latest_session_date": latest_session_date,
        "latest_net_foreign_value_idr": latest_net_value,
        "net_foreign_value_idr_5s": net_sum,
        "positive_sessions": positive,
        "negative_sessions": negative,
        "source": latest_source,
        "observed_at": latest_observed_at,
        "sessions": sessions,
        "note": "Uses the latest imported sessions; session flow is context, not a live intraday signal."
    }))
}
