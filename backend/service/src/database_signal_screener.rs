use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

const MIGRATION_ID: &str = "active_signal_snapshot_to_signal_db_v1";
const SNAPSHOT_TABLES: [&str; 2] = ["signal_screener_rows", "signal_screener_status"];

pub(crate) fn migrate_signal_screener_snapshot(
    app_database: &str,
    signals_database: &str,
) -> Result<String, String> {
    if !Path::new(app_database).is_file() {
        return Err(format!(
            "existing QUANT_APP_DB was not found at {app_database}"
        ));
    }
    super::connections::validate_signal_database_path(
        Path::new(app_database),
        Path::new(signals_database),
    )
    .map_err(|error| error.to_string())?;
    if let Some(parent) = Path::new(signals_database)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let signals_existed = Path::new(signals_database).exists();
    if !signals_existed {
        drop(Connection::open(signals_database).map_err(|error| error.to_string())?);
    }
    super::connections::journal::prepare_database(Path::new(app_database), false)
        .map_err(|error| error.to_string())?;
    super::connections::journal::prepare_database(Path::new(signals_database), false)
        .map_err(|error| error.to_string())?;
    let connection =
        Connection::open_with_flags(app_database, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|error| error.to_string())?;
    connection
        .execute("ATTACH DATABASE ?1 AS signals", [signals_database])
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA busy_timeout=5000;")
        .map_err(|error| error.to_string())?;
    super::connections::journal::configure(&connection, &["main", "signals"])
        .map_err(|error| error.to_string())?;
    super::connections::validate_signal_layout(&connection, signals_existed)
        .map_err(|error| error.to_string())?;
    crate::signal_alerts::initialize_screener_schema(&connection)
        .map_err(|error| error.to_string())?;

    let source_tables = SNAPSHOT_TABLES
        .into_iter()
        .map(|table| table_exists(&connection, "main", table))
        .collect::<Result<Vec<_>, _>>()?;
    let has_legacy_snapshot = source_tables.iter().any(|exists| *exists);
    if !has_legacy_snapshot {
        if let Some((source_hash, row_counts)) = existing_migration(&connection)? {
            return Ok(format!(
                "already complete ({source_hash}; row counts {row_counts})"
            ));
        }
        return Ok("no legacy signal snapshot found; signal database initialized".into());
    }
    if existing_migration(&connection)?.is_some() {
        return Err("migration marker exists but legacy signal tables reappeared".into());
    }

    ensure_legacy_columns(&connection, &source_tables)?;
    let source_hash = snapshot_hash(&connection, "main", &source_tables)?;
    let source_counts = snapshot_row_counts(&connection, "main", &source_tables)?;

    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let copy_result = copy_snapshot(&connection, &source_tables);
    match copy_result {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string())?,
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            return Err(error);
        }
    }

    let destination_tables = [true, true];
    let destination_counts = snapshot_row_counts(&connection, "signals", &destination_tables)?;
    if destination_counts != source_counts {
        return Err(format!(
            "signal snapshot row counts differ after copy: source {source_counts:?}, destination {destination_counts:?}"
        ));
    }
    for (index, table) in SNAPSHOT_TABLES.iter().enumerate() {
        if source_tables[index] && !tables_equal(&connection, table)? {
            return Err(format!(
                "signal snapshot values failed validation for {table}"
            ));
        }
        if !source_tables[index] && destination_counts[index] != 0 {
            return Err(format!(
                "unexpected destination rows for absent source table {table}"
            ));
        }
    }
    let destination_hash = snapshot_hash(&connection, "signals", &destination_tables)?;
    let copied_source_hash = snapshot_hash(&connection, "main", &source_tables)?;
    if copied_source_hash != source_hash {
        return Err(
            "QUANT_APP_DB signal snapshot changed during migration; source tables were retained"
                .into(),
        );
    }

    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let retire_result = (|| {
        if snapshot_hash(&connection, "main", &source_tables)? != source_hash {
            return Err("QUANT_APP_DB signal snapshot changed before retirement".to_string());
        }
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS main.signal_screener_migrations (
                     migration_id TEXT PRIMARY KEY,
                     source_path TEXT NOT NULL,
                     source_hash TEXT NOT NULL,
                     destination_hash TEXT NOT NULL,
                     migrated_row_counts_json TEXT NOT NULL,
                     completed_at INTEGER NOT NULL
                 );",
            )
            .map_err(|error| error.to_string())?;
        let row_counts_json = serde_json::json!({
            "signal_screener_rows": source_counts[0],
            "signal_screener_status": source_counts[1],
        })
        .to_string();
        connection
            .execute(
                "INSERT INTO main.signal_screener_migrations
                    (migration_id,source_path,source_hash,destination_hash,migrated_row_counts_json,completed_at)
                 VALUES(?1,?2,?3,?4,?5,unixepoch())",
                rusqlite::params![
                    MIGRATION_ID,
                    app_database,
                    source_hash,
                    destination_hash,
                    row_counts_json
                ],
            )
            .map_err(|error| error.to_string())?;
        for (table, exists) in SNAPSHOT_TABLES.iter().zip(&source_tables) {
            if *exists {
                connection
                    .execute_batch(&format!("DROP TABLE main.{table}"))
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    })();
    match retire_result {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string())?,
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            return Err(error);
        }
    }

    Ok(format!(
        "complete; source hash {source_hash}; destination hash {destination_hash}; row counts {source_counts:?}"
    ))
}

fn ensure_legacy_columns(connection: &Connection, source_tables: &[bool]) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let result = (|| {
        for (index, table) in SNAPSHOT_TABLES.iter().enumerate() {
            if !source_tables[index] {
                continue;
            }
            let columns = super::split::table_columns(connection, "main", table)?;
            let upgrades: &[(&str, &str)] = match *table {
                "signal_screener_rows" => &[("storage_symbol", "TEXT NOT NULL DEFAULT ''")],
                "signal_screener_status" => &[
                    ("price_data_status", "TEXT NOT NULL DEFAULT 'ready'"),
                    ("price_data_unavailable_count", "INTEGER NOT NULL DEFAULT 0"),
                ],
                _ => &[],
            };
            for (column, definition) in upgrades {
                if !columns.iter().any(|current| current == column) {
                    connection
                        .execute_batch(&format!(
                            "ALTER TABLE main.{table} ADD COLUMN {column} {definition}"
                        ))
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string()),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn copy_snapshot(connection: &Connection, source_tables: &[bool]) -> Result<(), String> {
    for (index, table) in SNAPSHOT_TABLES.iter().enumerate() {
        connection
            .execute_batch(&format!("DELETE FROM signals.{table}"))
            .map_err(|error| error.to_string())?;
        if !source_tables[index] {
            continue;
        }
        let columns = super::split::table_columns(connection, "main", table)?;
        let names = columns
            .iter()
            .map(|column| super::split::quote_identifier(column))
            .collect::<Vec<_>>()
            .join(",");
        connection
            .execute_batch(&format!(
                "INSERT INTO signals.{table}({names}) SELECT {names} FROM main.{table}"
            ))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn tables_equal(connection: &Connection, table: &str) -> Result<bool, String> {
    let source_columns = super::split::table_columns(connection, "main", table)?;
    let destination_columns = super::split::table_columns(connection, "signals", table)?;
    if source_columns != destination_columns {
        return Ok(false);
    }
    let columns = source_columns
        .iter()
        .map(|column| super::split::quote_identifier(column))
        .collect::<Vec<_>>()
        .join(",");
    let differences: bool = connection
        .query_row(
            &format!(
                "SELECT EXISTS(
                     SELECT {columns} FROM main.{table}
                     EXCEPT SELECT {columns} FROM signals.{table}
                 ) OR EXISTS(
                     SELECT {columns} FROM signals.{table}
                     EXCEPT SELECT {columns} FROM main.{table}
                 )"
            ),
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(!differences)
}

fn snapshot_hash(
    connection: &Connection,
    schema: &str,
    tables_exist: &[bool],
) -> Result<String, String> {
    let mut hasher = super::split::Fnv64::new();
    for (index, table) in SNAPSHOT_TABLES.iter().enumerate() {
        if tables_exist[index] {
            let columns = super::split::table_columns(connection, schema, table)?;
            super::split::hash_table(connection, schema, table, &columns, &mut hasher)?;
        } else {
            hasher.add(table.as_bytes());
            hasher.add(b"absent");
        }
    }
    Ok(format!("fnv1a64:{:016x}", hasher.value()))
}

fn snapshot_row_counts(
    connection: &Connection,
    schema: &str,
    tables_exist: &[bool],
) -> Result<Vec<i64>, String> {
    SNAPSHOT_TABLES
        .iter()
        .enumerate()
        .map(|(index, table)| {
            if tables_exist[index] {
                super::split::row_count(connection, schema, table)
            } else {
                Ok(0)
            }
        })
        .collect()
}

fn table_exists(connection: &Connection, schema: &str, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM {schema}.sqlite_schema WHERE type='table' AND name=?1)"
            ),
            [table],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn existing_migration(connection: &Connection) -> Result<Option<(String, String)>, String> {
    if !table_exists(connection, "main", "signal_screener_migrations")? {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT source_hash,migrated_row_counts_json
               FROM main.signal_screener_migrations WHERE migration_id=?1",
            [MIGRATION_ID],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())
}
