use super::*;

/// Convert provider implementation errors into a short explanation that can
/// be shown beside a chart without hiding the raw error kept in SQLite.

pub fn sync_error_reason(error: &str) -> &'static str {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("exit code: some(28)")
        || normalized.contains("timed out")
        || normalized.contains("timeout")
    {
        "Market-data provider request timed out"
    } else if normalized.contains("exit code: some(6)")
        || normalized.contains("could not resolve host")
        || normalized.contains("dns")
    {
        "Market-data provider hostname could not be resolved"
    } else if normalized.contains("exit code: some(35)")
        || normalized.contains("ssl")
        || normalized.contains("tls")
    {
        "Market-data provider TLS connection failed"
    } else if normalized.contains("exit code: some(7)")
        || normalized.contains("could not connect")
        || normalized.contains("connection refused")
        || normalized.contains("connect error")
        || normalized.contains("network is unreachable")
    {
        "Market-data provider could not connect"
    } else if normalized.contains("disabled") {
        "Market-data provider sync is disabled"
    } else if normalized.contains("no chart result")
        || normalized.contains("no timestamps")
        || normalized.contains("no quote indicators")
        || normalized.contains("missing ohlc")
    {
        "Provider returned no usable candle data"
    } else {
        "Provider sync failed; stored history was retained"
    }
}

/// Return the current unresolved sync problem separately from the historical
/// run list. A successful run for one series must not erase the reason another
/// series still carries in its local metadata.
pub fn load_sync_diagnostics(connection: &Connection) -> Result<Value, rusqlite::Error> {
    let failed_series_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM series WHERE last_error IS NOT NULL AND TRIM(last_error) <> ''",
        [],
        |row| row.get(0),
    )?;
    let failed_run_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM ingestion_runs WHERE status='error'",
        [],
        |row| row.get(0),
    )?;
    let latest_failure = connection
        .query_row(
            "SELECT r.symbol, r.interval, r.attempted_at, s.last_success,
                    r.error, COUNT(b.ts)
             FROM ingestion_runs r
             LEFT JOIN series s ON s.symbol=r.symbol AND s.interval=r.interval AND s.source=r.source
             LEFT JOIN bars b ON b.symbol=r.symbol AND b.interval=r.interval AND b.source=r.source
             WHERE r.status='error'
               AND r.error IS NOT NULL
               AND TRIM(r.error) <> ''
               AND (
                   s.symbol IS NULL
                   OR (
                       s.last_error IS NOT NULL
                       AND TRIM(s.last_error) <> ''
                       AND NOT EXISTS (
                           SELECT 1
                           FROM ingestion_runs successful
                           WHERE successful.symbol=r.symbol
                             AND successful.interval=r.interval
                             AND successful.source=r.source
                             AND successful.status='success'
                             AND successful.id > r.id
                       )
                   )
               )
             GROUP BY r.id
             ORDER BY r.id DESC
             LIMIT 1",
            [],
            |row| {
                let error = row.get::<_, String>(4)?;
                Ok(serde_json::json!({
                    "symbol": row
                        .get::<_, Option<String>>(0)?
                        .unwrap_or_else(|| "unknown series".to_string()),
                    "interval": row
                        .get::<_, Option<String>>(1)?
                        .unwrap_or_else(|| "unknown interval".to_string()),
                    "attempted_at": row.get::<_, Option<String>>(2)?,
                    "last_success": row.get::<_, Option<String>>(3)?,
                    "error": error,
                    "reason": sync_error_reason(&error),
                    "rows": row.get::<_, i64>(5)?,
                }))
            },
        )
        .optional()?;

    let status = if !provider_sync_enabled() {
        "offline"
    } else if latest_failure.is_some() {
        "degraded"
    } else {
        "ready"
    };
    Ok(serde_json::json!({
        "status": status,
        "provider_sync_allowed": provider_sync_enabled(),
        "provider_sync_by_exchange": provider_sync_by_exchange(),
        "failed_series_count": failed_series_count,
        "failed_run_count": failed_run_count,
        "latest_failure": latest_failure,
    }))
}
