use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;
use std::path::Path;

const MIGRATION_ID: &str = "signal_score_scale_v3";
const SCORE_SCHEMA_VERSION_KEY: &str = "signal_score_schema_version";

pub(crate) fn migrate_signal_score_scale(signal_database: &str) -> Result<String, String> {
    if !Path::new(signal_database).is_file() {
        return Err(format!(
            "signal database does not exist: {signal_database}; start the service once before migrating"
        ));
    }
    let connection = Connection::open_in_memory().map_err(|error| error.to_string())?;
    connection
        .execute("ATTACH DATABASE ?1 AS signals", [signal_database])
        .map_err(|error| error.to_string())?;
    migrate_signal_score_scale_in_connection(&connection)
}

pub(super) fn migrate_signal_score_scale_in_connection(
    connection: &Connection,
) -> Result<String, String> {
    let has_meta: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM signals.sqlite_schema
                           WHERE type='table' AND name='signal_store_meta')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !has_meta {
        let has_tables: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM signals.sqlite_schema
                               WHERE type='table' AND name NOT LIKE 'sqlite_%')",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if has_tables {
            return Err("existing signal database has no layout marker".into());
        }
        crate::signal_alerts::initialize_screener_schema(connection)
            .map_err(|error| error.to_string())?;
        return Ok("initialized empty signal database at score schema v3".into());
    }

    let layout_version = connection
        .query_row(
            "SELECT value FROM signals.signal_store_meta WHERE key='signal_layout_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if layout_version.as_deref() != Some(crate::signal_alerts::SIGNAL_LAYOUT_VERSION) {
        return Err("signal database layout marker is missing or unsupported".into());
    }

    let score_schema_version = connection
        .query_row(
            "SELECT value FROM signals.signal_store_meta WHERE key=?1",
            [SCORE_SCHEMA_VERSION_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let table_sql = connection
        .query_row(
            "SELECT sql FROM signals.sqlite_schema
              WHERE type='table' AND name='signal_screener_rows'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if score_schema_version.as_deref() == Some(crate::signal_alerts::SIGNAL_SCORE_SCHEMA_VERSION) {
        let current_schema = table_sql
            .as_deref()
            .is_some_and(|sql| normalized_sql(sql).contains("score<=40"));
        if !current_schema {
            return Err("signal score schema marker does not match the screener table".into());
        }
        return Ok("already complete (score schema v3)".into());
    }
    if !matches!(score_schema_version.as_deref(), None | Some("2")) {
        return Err(format!(
            "unsupported signal score schema version: {}",
            score_schema_version.unwrap_or_default()
        ));
    }
    let Some(table_sql) = table_sql else {
        crate::signal_alerts::initialize_screener_schema(connection)
            .map_err(|error| error.to_string())?;
        return Ok("initialized empty signal screener at score schema v3".into());
    };
    let normalized_source_sql = normalized_sql(&table_sql);
    let source_scale_max = match score_schema_version.as_deref() {
        Some("2") if normalized_source_sql.contains("score<=20") => 20.0,
        Some("2") => {
            return Err(
                "signal score schema v2 marker does not match its 20-point screener table".into(),
            )
        }
        None if normalized_source_sql.contains("score<=10") => 10.0,
        None => {
            return Err(
                "unmarked signal screener schema is not the expected 10-point schema".into(),
            )
        }
        _ => unreachable!(),
    };
    let upgrading_twenty_point_rows = source_scale_max == 20.0;

    let source_count = super::split::row_count(connection, "signals", "signal_screener_rows")?;
    let source_symbol_count: i64 = connection
        .query_row(
            "SELECT COUNT(DISTINCT symbol) FROM signals.signal_screener_rows",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let invalid_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM signals.signal_screener_rows
              WHERE score < 0 OR score > ?1
                 OR CASE WHEN json_valid(score_basis_json)=0 THEN 1
                         WHEN json_type(score_basis_json,'$') <> 'object' THEN 1
                         WHEN COALESCE(json_type(score_basis_json,'$.model_id'),'') <> 'text' THEN 1
                         ELSE 0 END",
            [source_scale_max],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if invalid_rows != 0 {
        return Err(format!(
            "cannot migrate {invalid_rows} invalid signal rows; correct the stored scores or score basis first"
        ));
    }

    let columns = super::split::table_columns(connection, "signals", "signal_screener_rows")?;
    let preserved_columns = columns
        .iter()
        .filter(|column| column.as_str() != "score_basis_json")
        .map(|column| format!("\"{}\"", column.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(",");
    if preserved_columns.is_empty() {
        return Err("signal screener schema has no migratable columns".into());
    }
    let mut hasher = super::split::Fnv64::new();
    super::split::hash_table(
        connection,
        "signals",
        "signal_screener_rows",
        &columns,
        &mut hasher,
    )?;
    let source_hash = format!("fnv1a64:{:016x}", hasher.value());
    let migration_row_counts = json!({
        "signal_screener_rows": source_count,
        "symbols": source_symbol_count,
    });

    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let migration = (|| -> Result<(), String> {
        crate::signal_alerts::create_signal_screener_rows_table(
            connection,
            "signal_screener_rows_v3",
            false,
            quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX,
        )
        .map_err(|error| error.to_string())?;
        let destination_columns =
            super::split::table_columns(connection, "signals", "signal_screener_rows_v3")?;
        if columns != destination_columns {
            return Err("signal score migration found an unexpected screener column layout".into());
        }
        connection
            .execute_batch(
                "INSERT INTO signals.signal_screener_rows_v3
                 SELECT * FROM signals.signal_screener_rows;",
            )
            .map_err(|error| error.to_string())?;
        if upgrading_twenty_point_rows {
            connection
                .execute(
                    "UPDATE signals.signal_screener_rows_v3
                    SET score_basis_json=json_set(
                        score_basis_json,
                        '$.score_model_status', 'legacy',
                        '$.status', 'stale',
                        '$.score_bar_is_forming', json('false')
                    )",
                    [],
                )
                .map_err(|error| error.to_string())?;
        } else {
            connection
                .execute(
                    "UPDATE signals.signal_screener_rows_v3
                    SET score_basis_json=json_set(
                        score_basis_json,
                        '$.score_scale_max', ?1,
                        '$.minimum_score_out_of_10', ?2,
                        '$.component_score_scale_max', ?3,
                        '$.score_model_status', 'legacy',
                        '$.strategy_weight', ?4,
                        '$.confluence_weight', ?5,
                        '$.status', 'stale',
                        '$.score_bar_is_forming', json('false')
                    )",
                    params![
                        quant_core::SIGNAL_STRENGTH_SCORE_SCALE_MAX,
                        quant_core::NWE_DSS_LEGACY_MINIMUM_SCORE_OUT_OF_10,
                        quant_core::NWE_DSS_COMPONENT_SCORE_SCALE_MAX,
                        quant_core::SIGNAL_STRENGTH_STRATEGY_WEIGHT,
                        quant_core::SIGNAL_STRENGTH_CONFLUENCE_WEIGHT,
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        let migrated_count =
            super::split::row_count(connection, "signals", "signal_screener_rows_v3")?;
        let invalid_destination_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM signals.signal_screener_rows_v3
                  WHERE score < 0 OR score > ?1
                     OR CASE WHEN json_valid(score_basis_json)=0 THEN 1
                             WHEN json_extract(score_basis_json,'$.score_scale_max') <> ?2 THEN 1
                             WHEN json_extract(score_basis_json,'$.score_model_status') <> 'legacy' THEN 1
                             ELSE 0 END",
                params![
                    quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX,
                    source_scale_max,
                ],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if migrated_count != source_count || invalid_destination_rows != 0 {
            return Err(format!(
                "signal score migration validation failed: source={source_count}, destination={migrated_count}, invalid={invalid_destination_rows}"
            ));
        }
        let differing_rows: i64 = connection
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM (
                         SELECT {preserved_columns} FROM signals.signal_screener_rows
                         EXCEPT
                         SELECT {preserved_columns} FROM signals.signal_screener_rows_v3
                     )"
                ),
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if differing_rows != 0 {
            return Err(format!(
                "signal score migration changed {differing_rows} stored signal values outside the score basis"
            ));
        }
        connection
            .execute_batch(
                "DROP INDEX IF EXISTS signals.idx_signal_screener_score;
                 DROP TABLE signals.signal_screener_rows;
                 ALTER TABLE signals.signal_screener_rows_v3 RENAME TO signal_screener_rows;
                 CREATE INDEX signals.idx_signal_screener_score
                     ON signal_screener_rows(score DESC,last_updated_at DESC);",
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO signals.signal_store_meta(key,value) VALUES(?1,?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![
                    SCORE_SCHEMA_VERSION_KEY,
                    crate::signal_alerts::SIGNAL_SCORE_SCHEMA_VERSION
                ],
            )
            .map_err(|error| error.to_string())?;
        let migrated_counts_json =
            serde_json::to_string(&migration_row_counts).map_err(|error| error.to_string())?;
        for (key, value) in [
            ("signal_score_scale_v3_source_hash", source_hash.as_str()),
            (
                "signal_score_scale_v3_migrated_row_counts_json",
                migrated_counts_json.as_str(),
            ),
            ("signal_score_scale_v3_migration_id", MIGRATION_ID),
        ] {
            connection
                .execute(
                    "INSERT INTO signals.signal_store_meta(key,value) VALUES(?1,?2)
                     ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    params![key, value],
                )
                .map_err(|error| error.to_string())?;
        }
        connection
            .execute(
                "INSERT INTO signals.signal_store_meta(key,value)
                 VALUES('signal_score_scale_v3_completed_at',datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    })();

    match migration {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string())?,
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            return Err(error);
        }
    }
    Ok(format!(
        "complete ({source_count} rows across {source_symbol_count} symbols; source {source_hash})"
    ))
}

fn normalized_sql(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}
