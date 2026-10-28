use rusqlite::{Connection, OpenFlags, OptionalExtension};
#[cfg(not(test))]
use std::collections::HashSet;
#[cfg(not(test))]
use std::sync::Mutex;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

const SQLITE_PAGE_CACHE_KIB: i64 = 256;
#[path = "database_connections/journal.rs"]
pub(super) mod journal;
pub(super) const MAX_OPEN_REGULAR_MARKET_DATABASES: usize = 9;
pub(crate) const SIGNAL_SCAN_DATABASE_RESERVE: usize = 2;
const MAX_OPEN_MARKET_DATABASES_TOTAL: usize =
    MAX_OPEN_REGULAR_MARKET_DATABASES + SIGNAL_SCAN_DATABASE_RESERVE;
const TICKER_FILE_PREFIX: &str = "s";
const TICKER_FILE_SUFFIX: &str = ".sqlite3";

#[derive(Clone)]
pub(crate) struct StoragePaths {
    pub(super) market_db: PathBuf,
    pub(super) app_db: PathBuf,
    pub(super) signals_db: PathBuf,
}

impl StoragePaths {
    pub(super) fn new(
        market_dir: impl Into<PathBuf>,
        app_db: impl Into<PathBuf>,
        signals_db: impl Into<PathBuf>,
    ) -> Self {
        Self {
            market_db: market_dir.into(),
            app_db: app_db.into(),
            signals_db: signals_db.into(),
        }
    }
}

pub(crate) fn ticker_database_path(
    market_dir: &Path,
    storage_symbol: &str,
) -> Result<PathBuf, rusqlite::Error> {
    if storage_symbol.trim().is_empty() {
        return Err(storage_error("storage symbol must not be empty"));
    }
    let mut encoded = String::with_capacity(storage_symbol.len() * 2);
    for byte in storage_symbol.as_bytes() {
        encoded.push(char::from_digit(u32::from(byte >> 4), 16).unwrap());
        encoded.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap());
    }
    if encoded.len() + TICKER_FILE_PREFIX.len() + TICKER_FILE_SUFFIX.len() > 240 {
        return Err(storage_error(
            "storage symbol is too long for a safe database filename",
        ));
    }
    Ok(market_dir.join(format!("{TICKER_FILE_PREFIX}{encoded}{TICKER_FILE_SUFFIX}")))
}

#[cfg(test)]
pub(crate) fn decode_storage_symbol_filename(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let encoded = file_name
        .strip_prefix(TICKER_FILE_PREFIX)?
        .strip_suffix(TICKER_FILE_SUFFIX)?;
    if encoded.is_empty() || encoded.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(encoded.len() / 2);
    for pair in encoded.as_bytes().chunks_exact(2) {
        let digit = |byte| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        };
        bytes.push((digit(pair[0])? << 4) | digit(pair[1])?);
    }
    String::from_utf8(bytes).ok()
}

pub(crate) struct TickerConnection {
    connection: Option<Connection>,
    kind: MarketConnectionKind,
    storage_symbol: String,
}

impl TickerConnection {
    fn new(connection: Connection, kind: MarketConnectionKind, storage_symbol: &str) -> Self {
        Self {
            connection: Some(connection),
            kind,
            storage_symbol: storage_symbol.to_string(),
        }
    }
}

impl std::ops::Deref for TickerConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.connection
            .as_ref()
            .expect("market connection lease exists")
    }
}

impl std::ops::DerefMut for TickerConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.connection
            .as_mut()
            .expect("market connection lease exists")
    }
}

impl Drop for TickerConnection {
    fn drop(&mut self) {
        drop(self.connection.take());
        market_connection_gate().release_ticker(self.kind, &self.storage_symbol);
    }
}

#[path = "database/connections/gate.rs"]
mod gate;
pub(super) use gate::{MarketConnectionGate, MarketConnectionKind};

fn market_connection_gate() -> &'static MarketConnectionGate {
    static GATE: OnceLock<MarketConnectionGate> = OnceLock::new();
    GATE.get_or_init(MarketConnectionGate::new)
}

const MARKET_APP_METADATA_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS app.market_store_inventory (
    storage_symbol TEXT PRIMARY KEY,
    database_filename TEXT NOT NULL UNIQUE,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS app.storage_shard_migrations (
    migration_id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    source_table_row_counts_json TEXT NOT NULL,
    source_symbol_row_counts_json TEXT NOT NULL,
    completed_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS app.storage_shard_checkpoints (
    migration_id TEXT NOT NULL,
    storage_symbol TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    migrated_row_counts_json TEXT NOT NULL,
    destination_hash TEXT NOT NULL,
    completed_at TEXT NOT NULL,
    PRIMARY KEY(migration_id, storage_symbol)
);
"#;

pub(crate) fn initialize_market_app_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(MARKET_APP_METADATA_SCHEMA)
}

/// Create the market tables in a known attached SQLite schema. `schema` is
/// restricted to the fixed aliases used by runtime and migration code.
pub(crate) fn create_ticker_schema(
    connection: &Connection,
    schema: &str,
) -> Result<(), rusqlite::Error> {
    if !matches!(schema, "main" | "ticker") {
        return Err(storage_error("invalid market database schema alias"));
    }
    connection.execute_batch(&format!(
        "CREATE TABLE {schema}.storage_layout_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
         CREATE TABLE {schema}.bars (
             symbol TEXT NOT NULL, interval TEXT NOT NULL, source TEXT NOT NULL DEFAULT 'unknown',
             ts INTEGER NOT NULL, open REAL NOT NULL, high REAL NOT NULL, low REAL NOT NULL,
             close REAL NOT NULL, volume REAL NOT NULL, adj_close REAL, dividends REAL,
             splits REAL, ingested_at TEXT NOT NULL,
             PRIMARY KEY(symbol,interval,source,ts)
         );
         CREATE INDEX {schema}.idx_bars_series_ts ON bars(symbol,interval,source,ts);
         CREATE TABLE {schema}.series (
             symbol TEXT NOT NULL, interval TEXT NOT NULL, source TEXT NOT NULL,
             exchange_id TEXT NOT NULL DEFAULT 'unknown', product_id TEXT NOT NULL DEFAULT '',
             instrument_id TEXT, data_provider TEXT NOT NULL DEFAULT 'unknown',
             timezone TEXT NOT NULL, price_basis TEXT NOT NULL, instrument_kind TEXT NOT NULL,
             last_success TEXT, last_attempt TEXT, last_error TEXT,
             PRIMARY KEY(symbol,interval,source)
         );
         CREATE TABLE {schema}.ingestion_runs (
             id INTEGER PRIMARY KEY, symbol TEXT, interval TEXT, source TEXT,
             exchange_id TEXT NOT NULL DEFAULT 'unknown', product_id TEXT NOT NULL DEFAULT '',
             data_provider TEXT NOT NULL DEFAULT 'unknown', attempted_at TEXT, status TEXT,
             rows_received INTEGER, error TEXT
         );
         CREATE INDEX {schema}.idx_ingestion_runs_symbol_id ON ingestion_runs(symbol,id DESC);"
    ))
}

pub(crate) fn initialize_ticker_storage(
    connection: &Connection,
    storage_symbol: &str,
) -> Result<bool, rusqlite::Error> {
    let has_layout_table: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema
                       WHERE type='table' AND name='storage_layout_meta')",
        [],
        |row| row.get(0),
    )?;
    if has_layout_table {
        let version: Option<String> = connection
            .query_row(
                "SELECT value FROM main.storage_layout_meta WHERE key='storage_layout_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let stored_symbol: Option<String> = connection
            .query_row(
                "SELECT value FROM main.storage_layout_meta WHERE key='storage_symbol'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if version.as_deref() == Some(super::LAYOUT_VERSION)
            && stored_symbol.as_deref() == Some(storage_symbol)
            && has_ticker_tables(connection)?
        {
            return Ok(false);
        }
        return Err(storage_error(
            "ticker database has an incomplete or mismatched layout",
        ));
    }
    let has_user_tables: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema
                       WHERE type='table' AND name NOT LIKE 'sqlite_%')",
        [],
        |row| row.get(0),
    )?;
    if has_user_tables {
        return Err(storage_error(
            "ticker database has user tables but no completed layout marker",
        ));
    }
    connection.execute_batch("BEGIN")?;
    let result = (|| {
        create_ticker_schema(connection, "main")?;
        connection.execute(
            "INSERT INTO main.storage_layout_meta(key,value)
             VALUES('storage_layout_version',?1),('storage_symbol',?2)",
            rusqlite::params![super::LAYOUT_VERSION, storage_symbol],
        )?;
        Ok::<(), rusqlite::Error>(())
    })();
    match result {
        Ok(()) => match connection.execute_batch("COMMIT") {
            Ok(()) => Ok(true),
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(error)
            }
        },
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn has_ticker_tables(connection: &Connection) -> Result<bool, rusqlite::Error> {
    connection.query_row(
        "SELECT COUNT(*)=3 FROM main.sqlite_schema
         WHERE type='table' AND name IN ('bars','series','ingestion_runs')",
        [],
        |row| row.get(0),
    )
}

pub(crate) fn open_app_database(paths: &StoragePaths) -> Result<Connection, rusqlite::Error> {
    open_app_database_with_signal_store(paths, true)
}

pub(crate) fn open_app_database_without_signal_store(
    paths: &StoragePaths,
) -> Result<Connection, rusqlite::Error> {
    open_app_database_with_signal_store(paths, false)
}

fn open_app_database_with_signal_store(
    paths: &StoragePaths,
    include_signal_store: bool,
) -> Result<Connection, rusqlite::Error> {
    #[cfg(not(test))]
    super::validate_runtime_market_directory(&paths.market_db.to_string_lossy())
        .map_err(|error| storage_error(&error))?;
    if include_signal_store {
        validate_signal_database_path(&paths.app_db, &paths.signals_db)?;
    }
    create_parent(&paths.app_db)?;
    if include_signal_store {
        create_parent(&paths.signals_db)?;
    }
    let app_existed = paths.app_db.exists();
    let signals_existed = include_signal_store && paths.signals_db.exists();
    journal::prepare_database(&paths.app_db, true)?;
    if include_signal_store {
        journal::prepare_database(&paths.signals_db, true)?;
    }
    let connection = Connection::open_in_memory()?;
    create_ticker_schema(&connection, "main")?;
    connection.execute(
        "INSERT INTO main.storage_layout_meta(key,value)
         VALUES('storage_layout_version',?1),('storage_symbol','')",
        [super::LAYOUT_VERSION],
    )?;
    connection.execute(
        "ATTACH DATABASE ?1 AS app",
        [paths.app_db.to_string_lossy().as_ref()],
    )?;
    if include_signal_store {
        connection.execute(
            "ATTACH DATABASE ?1 AS signals",
            [paths.signals_db.to_string_lossy().as_ref()],
        )?;
    }
    configure_connection(&connection)?;
    validate_app_layout(&connection, app_existed)?;
    initialize_application_storage_once(&connection, &paths.app_db)?;
    if include_signal_store {
        configure_signal_database(&connection)?;
        initialize_signal_screener_schema_once(
            &connection,
            &paths.signals_db,
            signals_existed,
        )?;
    }
    Ok(connection)
}

fn initialize_application_storage_once(
    connection: &Connection,
    _app_database: &Path,
) -> Result<(), rusqlite::Error> {
    #[cfg(test)]
    {
        super::initialize_app_runtime_storage_schema(connection)?;
        crate::master_data::initialize(connection)?;
        crate::instrument_registry::initialize(connection)?;
        crate::foreign_flow::initialize(connection)?;
        ensure_app_layout_marker(connection)
    }
    #[cfg(not(test))]
    {
        static INITIALIZED_DATABASES: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
        let initialized = INITIALIZED_DATABASES.get_or_init(|| Mutex::new(HashSet::new()));
        let app_database_key = _app_database
            .canonicalize()
            .unwrap_or_else(|_| _app_database.to_path_buf());
        let mut initialized = initialized
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if initialized.contains(&app_database_key) {
            return Ok(());
        }
        super::initialize_app_runtime_storage_schema(connection)?;
        crate::master_data::initialize(connection)?;
        crate::instrument_registry::initialize(connection)?;
        crate::foreign_flow::initialize(connection)?;
        ensure_app_layout_marker(connection)?;
        initialized.insert(app_database_key);
        Ok(())
    }
}

fn initialize_signal_screener_schema_once(
    connection: &Connection,
    signal_database: &Path,
    signal_database_existed: bool,
) -> Result<(), rusqlite::Error> {
    #[cfg(test)]
    {
        let _ = signal_database;
        validate_signal_layout(connection, signal_database_existed)?;
        crate::signal_alerts::initialize_screener_schema(connection)
    }
    #[cfg(not(test))]
    {
        static INITIALIZED_SIGNAL_DATABASES: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
        let initialized = INITIALIZED_SIGNAL_DATABASES.get_or_init(|| Mutex::new(HashSet::new()));
        let database_key = signal_database
            .canonicalize()
            .unwrap_or_else(|_| signal_database.to_path_buf());
        let mut initialized = initialized
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        validate_signal_layout(connection, signal_database_existed)?;
        if initialized.contains(&database_key) {
            return Ok(());
        }
        crate::signal_alerts::initialize_screener_schema(connection)?;
        initialized.insert(database_key);
        Ok(())
    }
}

pub(crate) fn open_market_history_for_symbol(
    paths: &StoragePaths,
    storage_symbol: &str,
    create: bool,
) -> Result<Option<TickerConnection>, rusqlite::Error> {
    open_market_history_for_symbol_with_kind(
        paths,
        storage_symbol,
        create,
        MarketConnectionKind::Regular,
    )
}

#[cfg(not(test))]
pub(crate) fn open_market_history_for_signal_scan(
    paths: &StoragePaths,
    storage_symbol: &str,
    create: bool,
) -> Result<Option<TickerConnection>, rusqlite::Error> {
    open_market_history_for_symbol_with_kind(
        paths,
        storage_symbol,
        create,
        MarketConnectionKind::SignalScan,
    )
}

fn open_market_history_for_symbol_with_kind(
    paths: &StoragePaths,
    storage_symbol: &str,
    create: bool,
    kind: MarketConnectionKind,
) -> Result<Option<TickerConnection>, rusqlite::Error> {
    let ticker_path = ticker_database_path(&paths.market_db, storage_symbol)?;
    let new_database = !ticker_path.exists();
    if !create && new_database {
        return Ok(None);
    }
    // Rollback-journal readers can block a provider's writer. Keep each
    // ticker's scan, chart, and sync connection exclusive while retaining
    // bounded parallel access across different ticker databases.
    market_connection_gate().acquire_ticker(kind, storage_symbol);
    match open_ticker_database(paths, &ticker_path, storage_symbol, create, new_database) {
        Ok(connection) => Ok(Some(TickerConnection::new(
            connection,
            kind,
            storage_symbol,
        ))),
        Err(error) => {
            market_connection_gate().release_ticker(kind, storage_symbol);
            Err(error)
        }
    }
}

fn open_ticker_database(
    paths: &StoragePaths,
    ticker_path: &Path,
    storage_symbol: &str,
    create: bool,
    new_database: bool,
) -> Result<Connection, rusqlite::Error> {
    drop(open_app_database_without_signal_store(paths)?);
    if create {
        create_parent(ticker_path)?;
    }
    journal::prepare_database(ticker_path, create)?;
    let connection = if create {
        Connection::open(ticker_path)?
    } else {
        Connection::open_with_flags(ticker_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?
    };
    connection.execute(
        "ATTACH DATABASE ?1 AS app",
        [paths.app_db.to_string_lossy().as_ref()],
    )?;
    configure_connection(&connection)?;
    validate_app_layout(&connection, true)?;
    let initialized_empty_database = initialize_ticker_storage(&connection, storage_symbol)?;
    if new_database || initialized_empty_database {
        let file_name = ticker_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| storage_error("ticker database filename is not valid UTF-8"))?;
        connection.execute(
            "INSERT INTO app.market_store_inventory(storage_symbol,database_filename,updated_at)
             VALUES(?1,?2,unixepoch())
             ON CONFLICT(storage_symbol) DO UPDATE SET
                 database_filename=excluded.database_filename,updated_at=excluded.updated_at",
            rusqlite::params![storage_symbol, file_name],
        )?;
    }
    let file_name = ticker_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| storage_error("ticker database filename is not valid UTF-8"))?;
    let inventory_filename: Option<String> = connection
        .query_row(
            "SELECT database_filename FROM app.market_store_inventory
             WHERE storage_symbol=?1",
            [storage_symbol],
            |row| row.get(0),
        )
        .optional()?;
    if inventory_filename.as_deref() != Some(file_name) {
        return Err(storage_error(
            "ticker database is missing from the app inventory or has a mismatched filename",
        ));
    }
    Ok(connection)
}

fn validate_app_layout(connection: &Connection, existed: bool) -> Result<(), rusqlite::Error> {
    let has_marker_table: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM app.sqlite_schema
                       WHERE type='table' AND name='storage_layout_meta')",
        [],
        |row| row.get(0),
    )?;
    if !has_marker_table {
        if existed {
            let has_data: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM app.sqlite_schema
                               WHERE type='table' AND name NOT LIKE 'sqlite_%')",
                [],
                |row| row.get(0),
            )?;
            if has_data {
                return Err(storage_error("existing app database has no layout marker"));
            }
        }
        return Ok(());
    }
    let version: Option<String> = connection
        .query_row(
            "SELECT value FROM app.storage_layout_meta WHERE key='storage_layout_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if version.as_deref() == Some(super::LAYOUT_VERSION) {
        Ok(())
    } else {
        Err(storage_error(
            "app database requires the explicit split and market sharding migrations",
        ))
    }
}

fn ensure_app_layout_marker(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute(
        "INSERT OR IGNORE INTO app.storage_layout_meta(key,value)
         VALUES('storage_layout_version',?1)",
        [super::LAYOUT_VERSION],
    )?;
    Ok(())
}

fn configure_connection(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(&format!(
        "PRAGMA busy_timeout=5000;
         PRAGMA main.cache_size=-{SQLITE_PAGE_CACHE_KIB};
         PRAGMA app.cache_size=-{SQLITE_PAGE_CACHE_KIB};
         PRAGMA temp_store=FILE;
         PRAGMA main.mmap_size=0;
         PRAGMA app.mmap_size=0;
         PRAGMA foreign_keys=ON;"
    ))?;
    journal::configure(connection, &["main", "app"])
}

#[path = "database_connections/signal_database.rs"]
mod signal_database;
pub(super) use signal_database::{
    configure_signal_database, validate_signal_database_path, validate_signal_layout,
};

fn create_parent(path: &Path) -> Result<(), rusqlite::Error> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(io_sqlite_error)?;
    }
    Ok(())
}

fn storage_error(message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.to_string())
}

fn io_sqlite_error(error: std::io::Error) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR),
        Some(error.to_string()),
    )
}
