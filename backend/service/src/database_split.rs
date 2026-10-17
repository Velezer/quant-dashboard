#[cfg(test)]
use rusqlite::OpenFlags;
use rusqlite::{Connection, OptionalExtension};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

pub(crate) const LAYOUT_VERSION: &str = "2";
const SPLIT_LAYOUT_VERSION: &str = LAYOUT_VERSION;
#[cfg(test)]
const SQLITE_PAGE_CACHE_KIB: i64 = 512;
const SPLIT_MIGRATION_ID: &str = "single_database_to_market_and_app_v1";
const APP_METADATA_TABLES: [&str; 5] = [
    "storage_layout_meta",
    "storage_split_migrations",
    "market_store_inventory",
    "storage_shard_migrations",
    "storage_shard_checkpoints",
];
const APP_TABLES: [&str; 14] = [
    "exchanges",
    "exchange_products",
    "data_providers",
    "plan_profiles",
    "listing_sync_runs",
    "instruments",
    "instrument_exclusions",
    "trade_plans",
    "plan_evidence",
    "reference_catalogs",
    "stockbit_foreign_flow",
    "instrument_registry_meta",
    "plan_reviews",
    "market_sync_cursor",
];
const MARKET_TABLES: [&str; 4] = ["bars", "series", "ingestion_runs", "storage_layout_meta"];

#[derive(Clone)]
#[cfg(test)]
pub(crate) struct StoragePaths {
    pub(super) market_db: PathBuf,
    pub(super) app_db: PathBuf,
    pub(super) signals_db: PathBuf,
}

#[cfg(test)]
impl StoragePaths {
    pub(super) fn new(
        market_db: impl Into<PathBuf>,
        app_db: impl Into<PathBuf>,
        signals_db: impl Into<PathBuf>,
    ) -> Self {
        Self {
            market_db: market_db.into(),
            app_db: app_db.into(),
            signals_db: signals_db.into(),
        }
    }
}

#[cfg(test)]
pub(crate) fn shared_storage_paths(
    market_db: impl Into<PathBuf>,
    app_db: impl Into<PathBuf>,
    signals_db: impl Into<PathBuf>,
) -> StoragePaths {
    StoragePaths::new(market_db, app_db, signals_db)
}

#[cfg(test)]
pub(crate) fn open_market_database(paths: &StoragePaths) -> Result<Connection, rusqlite::Error> {
    preflight_market_database(paths)?;
    create_database_parent(&paths.market_db.to_string_lossy()).map_err(io_sqlite_error)?;
    create_database_parent(&paths.app_db.to_string_lossy()).map_err(io_sqlite_error)?;
    create_database_parent(&paths.signals_db.to_string_lossy()).map_err(io_sqlite_error)?;
    let app_existed = paths.app_db.exists();
    let signals_existed = paths.signals_db.exists();
    let connection = if paths.market_db.to_string_lossy() == ":memory:" {
        Connection::open_in_memory()?
    } else {
        Connection::open(&paths.market_db)?
    };
    connection.execute(
        "ATTACH DATABASE ?1 AS app",
        [paths.app_db.to_string_lossy().as_ref()],
    )?;
    connection.execute(
        "ATTACH DATABASE ?1 AS signals",
        [paths.signals_db.to_string_lossy().as_ref()],
    )?;
    configure_connection(&connection)?;
    super::connections::configure_signal_database(&connection)?;
    validate_preexisting_app_layout(&connection, app_existed)?;
    super::connections::validate_signal_layout(&connection, signals_existed)?;
    validate_layout_markers(&connection)?;
    super::initialize_market_storage(&connection, &paths.market_db, &paths.app_db)?;
    Ok(connection)
}

#[cfg(test)]
pub(super) fn open_market_history_for_symbol(
    paths: &StoragePaths,
    storage_symbol: &str,
    create: bool,
) -> Result<Option<Connection>, rusqlite::Error> {
    let connection = open_market_database(paths)?;
    if !create {
        let has_history = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM main.series WHERE symbol=?1
                           UNION ALL SELECT 1 FROM main.bars WHERE symbol=?1
                           UNION ALL SELECT 1 FROM main.ingestion_runs WHERE symbol=?1)",
            [storage_symbol],
            |row| row.get::<_, bool>(0),
        )?;
        if !has_history {
            return Ok(None);
        }
    }
    Ok(Some(connection))
}

#[cfg(test)]
fn preflight_market_database(paths: &StoragePaths) -> Result<(), rusqlite::Error> {
    if paths.market_db.to_string_lossy() == ":memory:" || !paths.market_db.exists() {
        return Ok(());
    }
    let connection =
        Connection::open_with_flags(&paths.market_db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if legacy_app_tables_in_market(&connection)? {
        return Err(layout_error("legacy app tables remain in the market database; stop the service and run --split-market-database"));
    }
    let has_market_tables = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM main.sqlite_master WHERE type='table' AND name IN ('bars','series','ingestion_runs'))",
        [], |row| row.get::<_, bool>(0),
    )?;
    let market_marker = read_layout_marker(&connection, "main")?;
    if has_market_tables && market_marker.is_none() {
        return Err(layout_error(
            "market database has no completed split marker; run --split-market-database explicitly",
        ));
    }
    if market_marker.is_some() && !paths.app_db.exists() {
        return Err(layout_error(
            "app database is missing for the completed market/app split; restore it before startup",
        ));
    }
    Ok(())
}

#[cfg(test)]
fn validate_preexisting_app_layout(
    connection: &Connection,
    existed: bool,
) -> Result<(), rusqlite::Error> {
    if !existed {
        return Ok(());
    }
    let has_metadata = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM app.sqlite_schema WHERE type='table' AND name='storage_layout_meta')",
        [], |row| row.get::<_, bool>(0),
    )?;
    if !has_metadata {
        let has_any_table = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM app.sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%')",
            [], |row| row.get::<_, bool>(0),
        )?;
        if has_any_table {
            return Err(layout_error(
                "existing app database has no storage layout marker",
            ));
        }
        return Ok(());
    }
    let marker = read_layout_marker(connection, "app")?;
    if marker.as_deref() != Some(SPLIT_LAYOUT_VERSION) {
        return Err(layout_error(
            "app database requires an explicit storage layout migration",
        ));
    }
    Ok(())
}

#[cfg(test)]
fn configure_connection(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(&format!(
        "PRAGMA busy_timeout=5000; PRAGMA main.cache_size=-{SQLITE_PAGE_CACHE_KIB};
         PRAGMA app.cache_size=-{SQLITE_PAGE_CACHE_KIB}; PRAGMA temp_store=FILE;
         PRAGMA main.mmap_size=0; PRAGMA app.mmap_size=0;
         PRAGMA main.synchronous=NORMAL; PRAGMA app.synchronous=NORMAL;
         PRAGMA foreign_keys=ON; PRAGMA main.journal_mode=WAL; PRAGMA app.journal_mode=WAL;"
    ))
}

#[cfg(test)]
fn create_database_parent(path: &str) -> Result<(), std::io::Error> {
    let path = Path::new(path);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
fn io_sqlite_error(error: std::io::Error) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR),
        Some(error.to_string()),
    )
}
#[cfg(test)]
pub(crate) fn app_database_path(market_path: &str) -> String {
    if let Ok(path) = env::var("QUANT_APP_DB") {
        return path;
    }
    if is_default_market_path(market_path) {
        return crate::DEFAULT_APP_DB.to_string();
    }
    if market_path == ":memory:" {
        return ":memory:".to_string();
    }
    let market = Path::new(market_path);
    let stem = market
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("market");
    market
        .with_file_name(format!("{stem}.app.sqlite3"))
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
fn is_default_market_path(path: &str) -> bool {
    let normalize = |path: &str| {
        fs::canonicalize(path).unwrap_or_else(|_| {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(path)
            }
        })
    };
    normalize(path)
        .to_string_lossy()
        .eq_ignore_ascii_case(&normalize(crate::DEFAULT_LEGACY_MARKET_DB).to_string_lossy())
}

pub(super) fn legacy_app_tables_in_market(
    connection: &Connection,
) -> Result<bool, rusqlite::Error> {
    for table in APP_TABLES {
        if table_exists(connection, "main", table)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn validate_layout_markers(connection: &Connection) -> Result<(), rusqlite::Error> {
    let market_version = read_layout_marker(connection, "main")?;
    let app_version = read_layout_marker(connection, "app")?;
    match (market_version, app_version) {
        (None, None) => Ok(()),
        (Some(market), Some(app))
            if market == SPLIT_LAYOUT_VERSION && app == SPLIT_LAYOUT_VERSION =>
        {
            Ok(())
        }
        (Some(_), Some(_)) => Err(layout_error(
            "market and app databases have incompatible storage layout versions",
        )),
        _ => Err(layout_error(
            "market and app databases do not belong to the same completed split",
        )),
    }
}

pub(super) fn initialize_layout_markers(
    connection: &Connection,
    _market_path: &str,
) -> Result<(), rusqlite::Error> {
    connection.execute(
        "INSERT OR IGNORE INTO main.storage_layout_meta(key,value)
         VALUES('storage_layout_version',?1)",
        [SPLIT_LAYOUT_VERSION],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO app.storage_layout_meta(key,value)
         VALUES('storage_layout_version',?1)",
        [SPLIT_LAYOUT_VERSION],
    )?;
    validate_layout_markers(connection)
}

pub(crate) fn split_market_database(market_path: &str, app_path: &str) -> Result<String, String> {
    if same_path(market_path, app_path)? {
        return Err("QUANT_MARKET_DB and QUANT_APP_DB must point to different files".into());
    }
    if !Path::new(market_path).is_file() {
        return Err(format!("market database does not exist: {market_path}"));
    }
    create_parent(app_path)?;

    let connection = Connection::open(market_path).map_err(|error| error.to_string())?;
    connection
        .execute("ATTACH DATABASE ?1 AS app", [app_path])
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;
             PRAGMA main.cache_size=-512;
             PRAGMA app.cache_size=-512;
             PRAGMA temp_store=FILE;
             PRAGMA main.mmap_size=0;
             PRAGMA app.mmap_size=0;
             PRAGMA main.journal_mode=DELETE;
             PRAGMA app.journal_mode=DELETE;
             PRAGMA main.synchronous=FULL;
             PRAGMA app.synchronous=FULL;",
        )
        .map_err(|error| {
            format!(
                "could not prepare both SQLite files for an offline transaction; stop the service and retry: {error}"
            )
        })?;
    super::initialize_app_storage_schema(&connection).map_err(|error| error.to_string())?;
    validate_app_inventory(&connection)?;

    let already_completed = connection
        .query_row(
            "SELECT source_hash FROM app.storage_split_migrations WHERE migration_id=?1",
            [SPLIT_MIGRATION_ID],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(source_hash) = already_completed {
        if legacy_app_tables_in_market(&connection).map_err(|error| error.to_string())? {
            return Err(
                "split migration marker exists but legacy app tables remain in the market database"
                    .into(),
            );
        }
        return Ok(format!("already complete ({source_hash})"));
    }

    validate_market_inventory(&connection)?;
    let legacy_tables = APP_TABLES
        .iter()
        .copied()
        .filter_map(|table| match table_exists(&connection, "main", table) {
            Ok(true) => Some(Ok(table)),
            Ok(false) => None,
            Err(error) => Some(Err(error.to_string())),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if legacy_tables.is_empty() {
        initialize_market_layout_table(&connection).map_err(|error| error.to_string())?;
        initialize_layout_markers(&connection, market_path).map_err(|error| error.to_string())?;
        return Ok("no legacy app tables found; storage layout markers initialized".into());
    }
    ensure_empty_app_destination(&connection)?;
    initialize_market_layout_table(&connection).map_err(|error| error.to_string())?;

    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let migration = (|| -> Result<(String, BTreeMap<String, i64>), String> {
        let mut source_hasher = Fnv64::new();
        let mut row_counts = BTreeMap::new();
        for table in APP_TABLES {
            if !table_exists(&connection, "main", table).map_err(|error| error.to_string())? {
                continue;
            }
            let columns = table_columns(&connection, "main", table)?;
            let destination_columns = table_columns(&connection, "app", table)?;
            for column in &columns {
                if !destination_columns.contains(column) {
                    return Err(format!(
                        "app table {table} contains unsupported column {column}; apply its schema migration before splitting"
                    ));
                }
            }
            hash_table(&connection, "main", table, &columns, &mut source_hasher)?;
            let count = row_count(&connection, "main", table)?;
            row_counts.insert(table.to_string(), count);
            let names = columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>()
                .join(",");
            let query =
                format!("INSERT INTO app.{table} ({names}) SELECT {names} FROM main.{table}");
            connection
                .execute(&query, [])
                .map_err(|error| format!("copy {table}: {error}"))?;
            let copied = row_count(&connection, "app", table)?;
            if copied != count {
                return Err(format!(
                    "row count mismatch for {table}: source={count}, app={copied}"
                ));
            }
            let mut destination_hasher = Fnv64::new();
            hash_table(&connection, "app", table, &columns, &mut destination_hasher)?;
            let mut source_table_hasher = Fnv64::new();
            hash_table(
                &connection,
                "main",
                table,
                &columns,
                &mut source_table_hasher,
            )?;
            if destination_hasher.value() != source_table_hasher.value() {
                return Err(format!("content validation failed for {table}"));
            }
        }
        validate_app_foreign_keys(&connection)?;
        let source_hash = format!("fnv1a64:{:016x}", source_hasher.value());
        let row_counts_json =
            serde_json::to_string(&row_counts).map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO app.storage_split_migrations(
                     migration_id,source_path,source_hash,migrated_row_counts_json,completed_at
                 ) VALUES(?1,?2,?3,?4,datetime('now'))",
                rusqlite::params![
                    SPLIT_MIGRATION_ID,
                    market_path,
                    source_hash,
                    row_counts_json
                ],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT OR REPLACE INTO main.storage_layout_meta(key,value)
                 VALUES('storage_layout_version',?1)",
                [SPLIT_LAYOUT_VERSION],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT OR REPLACE INTO app.storage_layout_meta(key,value)
                 VALUES('storage_layout_version',?1)",
                [SPLIT_LAYOUT_VERSION],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute_batch(
                "DROP TRIGGER IF EXISTS main.register_series_provider_insert;
                 DROP TRIGGER IF EXISTS main.register_series_provider_update;
                 DROP TRIGGER IF EXISTS main.register_ingestion_provider_insert;",
            )
            .map_err(|error| error.to_string())?;
        for table in APP_TABLES.iter().rev() {
            if table_exists(&connection, "main", table).map_err(|error| error.to_string())? {
                connection
                    .execute_batch(&format!("DROP TABLE main.{table}"))
                    .map_err(|error| format!("retire legacy {table}: {error}"))?;
            }
        }
        Ok((source_hash, row_counts))
    })();

    match migration {
        Ok((source_hash, row_counts)) => {
            if let Err(error) = connection.execute_batch("COMMIT") {
                let _ = connection.execute_batch("ROLLBACK");
                return Err(error.to_string());
            }
            Ok(format!(
                "complete; source {source_hash}; migrated row counts {}",
                serde_json::to_string(&row_counts).unwrap_or_else(|_| "{}".into())
            ))
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn initialize_market_layout_table(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS main.storage_layout_meta (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    )
}

fn validate_market_inventory(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM main.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .map_err(|error| error.to_string())?;
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for table in tables {
        if !APP_TABLES.contains(&table.as_str()) && !MARKET_TABLES.contains(&table.as_str()) {
            return Err(format!(
                "unrecognized table {table} exists in the market database; no data was moved"
            ));
        }
    }
    Ok(())
}

fn ensure_empty_app_destination(connection: &Connection) -> Result<(), String> {
    for table in APP_TABLES.iter().chain(APP_METADATA_TABLES.iter()) {
        let count = row_count(connection, "app", table)?;
        if count != 0 {
            return Err(format!(
                "app database table {table} already contains {count} rows and has no completed split record"
            ));
        }
    }
    Ok(())
}

fn validate_app_inventory(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM app.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .map_err(|error| error.to_string())?;
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for table in tables {
        if !APP_TABLES.contains(&table.as_str()) && !APP_METADATA_TABLES.contains(&table.as_str()) {
            return Err(format!(
                "unrecognized table {table} exists in the app database; no market data was moved"
            ));
        }
    }
    Ok(())
}

fn validate_app_foreign_keys(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA app.foreign_key_check")
        .map_err(|error| error.to_string())?;
    let mut rows = statement.query([]).map_err(|error| error.to_string())?;
    if rows.next().map_err(|error| error.to_string())?.is_some() {
        return Err("app database foreign-key validation failed".into());
    }
    Ok(())
}

fn table_exists(
    connection: &Connection,
    schema: &str,
    table: &str,
) -> Result<bool, rusqlite::Error> {
    connection.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM {schema}.sqlite_master WHERE type='table' AND name=?1)"
        ),
        [table],
        |row| row.get(0),
    )
}

fn read_layout_marker(
    connection: &Connection,
    schema: &str,
) -> Result<Option<String>, rusqlite::Error> {
    if !table_exists(connection, schema, "storage_layout_meta")? {
        return Ok(None);
    }
    connection
        .query_row(
            &format!(
                "SELECT value FROM {schema}.storage_layout_meta WHERE key='storage_layout_version'"
            ),
            [],
            |row| row.get(0),
        )
        .optional()
}

pub(super) fn table_columns(
    connection: &Connection,
    schema: &str,
    table: &str,
) -> Result<Vec<String>, String> {
    let escaped = table.replace('\'', "''");
    let mut statement = connection
        .prepare(&format!("PRAGMA {schema}.table_info('{escaped}')"))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string());
    columns
}

pub(super) fn row_count(connection: &Connection, schema: &str, table: &str) -> Result<i64, String> {
    connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {schema}.{table}"),
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

pub(super) fn hash_table(
    connection: &Connection,
    schema: &str,
    table: &str,
    columns: &[String],
    hasher: &mut Fnv64,
) -> Result<(), String> {
    let names = columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>();
    let selected = names
        .iter()
        .map(|column| format!("quote({column})"))
        .collect::<Vec<_>>()
        .join(",");
    let primary_key = table_primary_key(connection, schema, table)?;
    let ordered = primary_key
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>();
    let ordered = if ordered.is_empty() { &names } else { &ordered };
    let ordered = ordered.join(",");
    let query = format!(
        "SELECT {selected} FROM {schema}.{} ORDER BY {ordered}",
        quote_identifier(table)
    );
    hasher.add(table.as_bytes());
    for column in columns {
        hasher.add(column.as_bytes());
    }
    let mut statement = connection
        .prepare(&query)
        .map_err(|error| error.to_string())?;
    let mut rows = statement.query([]).map_err(|error| error.to_string())?;
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        hasher.add(b"row");
        for index in 0..columns.len() {
            let value: String = row.get(index).map_err(|error| error.to_string())?;
            hasher.add(value.as_bytes());
        }
    }
    Ok(())
}

fn table_primary_key(
    connection: &Connection,
    schema: &str,
    table: &str,
) -> Result<Vec<String>, String> {
    let escaped = table.replace('\'', "''");
    let mut statement = connection
        .prepare(&format!("PRAGMA {schema}.table_info('{escaped}')"))
        .map_err(|error| error.to_string())?;
    let mut columns = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    columns.retain(|(_, position)| *position > 0);
    columns.sort_by_key(|(_, position)| *position);
    Ok(columns.into_iter().map(|(name, _)| name).collect())
}

pub(super) fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn create_parent(path: &str) -> Result<(), String> {
    if let Some(parent) = Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn same_path(left: &str, right: &str) -> Result<bool, String> {
    let normalize = |value: &str| -> Result<PathBuf, String> {
        let path = PathBuf::from(value);
        if path.exists() {
            return fs::canonicalize(path).map_err(|error| error.to_string());
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .ok_or_else(|| format!("database path has no file name: {value}"))?;
        let normalized_parent = fs::canonicalize(parent).unwrap_or_else(|_| {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(parent)
        });
        Ok(normalized_parent.join(file_name))
    };
    let left = normalize(left)?;
    let right = normalize(right)?;
    Ok(left
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy()))
}

fn layout_error(message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.to_string())
}

pub(super) struct Fnv64(u64);

impl Fnv64 {
    pub(super) fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    pub(super) fn add(&mut self, bytes: &[u8]) {
        self.0 ^= bytes.len() as u64;
        self.0 = self.0.wrapping_mul(0x100000001b3);
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    pub(super) fn value(&self) -> u64 {
        self.0
    }
}
