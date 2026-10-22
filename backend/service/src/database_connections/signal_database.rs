use super::*;

pub(in crate::database) fn configure_signal_database(
    connection: &Connection,
) -> Result<(), rusqlite::Error> {
    connection.execute_batch(&format!(
        "PRAGMA signals.cache_size=-{SQLITE_PAGE_CACHE_KIB};
         PRAGMA signals.mmap_size=0;"
    ))?;
    super::journal::configure(connection, &["signals"])
}

pub(in crate::database) fn validate_signal_database_path(
    app_database: &Path,
    signal_database: &Path,
) -> Result<(), rusqlite::Error> {
    let signal_path = signal_database.to_string_lossy();
    let app_path = app_database.to_string_lossy();
    if super::super::sharding::same_path(&signal_path, &app_path)
        .map_err(|error| storage_error(&error))?
    {
        return Err(storage_error(
            "QUANT_SIGNAL_DB must be a separate SQLite file from QUANT_APP_DB",
        ));
    }
    let migration_market_path = std::env::var("QUANT_MARKET_DB")
        .unwrap_or_else(|_| super::super::DEFAULT_LEGACY_MARKET_DB.to_owned());
    if super::super::sharding::same_path(&signal_path, &migration_market_path)
        .map_err(|error| storage_error(&error))?
    {
        return Err(storage_error(
            "QUANT_SIGNAL_DB must not use the offline QUANT_MARKET_DB migration source",
        ));
    }
    Ok(())
}

pub(in crate::database) fn validate_signal_layout(
    connection: &Connection,
    existed: bool,
) -> Result<(), rusqlite::Error> {
    let has_meta_table: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM signals.sqlite_schema
                       WHERE type='table' AND name='signal_store_meta')",
        [],
        |row| row.get(0),
    )?;
    if !has_meta_table {
        if existed {
            let has_user_tables: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM signals.sqlite_schema
                               WHERE type='table' AND name NOT LIKE 'sqlite_%')",
                [],
                |row| row.get(0),
            )?;
            if has_user_tables {
                return Err(storage_error(
                    "existing signal database has no layout marker",
                ));
            }
        }
        return Ok(());
    }
    let version: Option<String> = connection
        .query_row(
            "SELECT value FROM signals.signal_store_meta WHERE key='signal_layout_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if version.as_deref() != Some(crate::signal_alerts::SIGNAL_LAYOUT_VERSION) {
        return Err(storage_error(
            "signal database layout marker is missing or unsupported",
        ));
    }
    let has_screener_rows: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM signals.sqlite_schema
                       WHERE type='table' AND name='signal_screener_rows')",
        [],
        |row| row.get(0),
    )?;
    if has_screener_rows {
        let score_schema_version: Option<String> = connection
            .query_row(
                "SELECT value FROM signals.signal_store_meta
                  WHERE key='signal_score_schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if score_schema_version.as_deref()
            != Some(crate::signal_alerts::SIGNAL_SCORE_SCHEMA_VERSION)
        {
            return Err(storage_error(
                "signal score schema requires an explicit migration; stop the service and run `tooling\\migrate-signal-score-scale.bat`",
            ));
        }
        let score_schema_sql: Option<String> = connection
            .query_row(
                "SELECT sql FROM signals.sqlite_schema
                    WHERE type='table' AND name='signal_screener_rows'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let supports_twenty_point_scores = score_schema_sql.as_deref().is_some_and(|sql| {
            sql.to_ascii_lowercase()
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>()
                .contains("score<=40")
        });
        if !supports_twenty_point_scores {
            return Err(storage_error(
                "signal score schema marker does not match the screener table",
            ));
        }
    }
    Ok(())
}
