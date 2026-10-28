use super::*;

#[path = "database_split.rs"]
mod split;
#[cfg(test)]
pub(super) use split::app_database_path;
pub(super) use split::split_market_database;
#[path = "database_connections.rs"]
mod connections;
#[cfg(test)]
pub(super) use connections::decode_storage_symbol_filename;
pub(super) use connections::ticker_database_path;
#[cfg(not(test))]
pub(super) use connections::TickerConnection;
#[cfg(test)]
use connections::{MarketConnectionGate, MarketConnectionKind, MAX_OPEN_REGULAR_MARKET_DATABASES};
#[cfg(not(test))]
pub(super) use connections::SIGNAL_SCAN_DATABASE_RESERVE;
pub(super) const LAYOUT_VERSION: &str = "3";

#[cfg(not(test))]
pub(super) struct SignalScanConnection(TickerConnection);
#[cfg(test)]
pub(super) struct SignalScanConnection(Connection);

impl std::ops::Deref for SignalScanConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        #[cfg(not(test))]
        {
            &self.0
        }
        #[cfg(test)]
        {
            &self.0
        }
    }
}
#[path = "database_sharding.rs"]
mod sharding;
pub(super) use sharding::shard_market_database;
#[cfg(test)]
use sharding::within;
#[path = "database_signal_screener.rs"]
mod signal_screener_migration;
pub(super) use signal_screener_migration::migrate_signal_screener_snapshot;
#[path = "database_signal_score_scale.rs"]
mod signal_score_scale;
pub(crate) use signal_score_scale::migrate_signal_score_scale;
#[cfg(test)]
#[path = "database_signal_score_scale_tests.rs"]
mod signal_score_scale_tests;

#[path = "database/market_identity.rs"]
mod market_identity;
#[path = "database/shared_crypto.rs"]
mod shared_crypto;
pub(crate) use shared_crypto::share_crypto_candles;
#[path = "database/schema.rs"]
mod schema;
use schema::{APP_RUNTIME_SCHEMA, MARKET_SCHEMA};
pub(super) fn initialize_app_storage_schema(
    connection: &Connection,
) -> Result<(), rusqlite::Error> {
    initialize_app_runtime_storage_schema(connection)?;
    signal_alerts::initialize_screener_schema(connection)
}

pub(super) fn initialize_app_runtime_storage_schema(
    connection: &Connection,
) -> Result<(), rusqlite::Error> {
    signal_alerts::reject_legacy_screener_tables(connection)?;
    connection.execute_batch(APP_RUNTIME_SCHEMA)?;
    connections::initialize_market_app_schema(connection)?;
    master_data::initialize_schema(connection)?;
    instrument_registry::initialize_schema(connection)?;
    foreign_flow::initialize_schema(connection)?;
    signal_alerts::initialize_schema(connection)?;
    pine_registry::initialize_schema(connection)
}

pub(super) fn initialize_empty_market_schema(
    connection: &Connection,
) -> Result<(), rusqlite::Error> {
    connection.execute_batch(MARKET_SCHEMA)
}

pub(crate) fn retry_sqlite_busy<T>(
    mut operation: impl FnMut() -> Result<T, rusqlite::Error>,
) -> Result<T, rusqlite::Error> {
    const RETRY_DELAYS_MS: [u64; 2] = [100, 300];
    for attempt in 0..=RETRY_DELAYS_MS.len() {
        match operation() {
            Err(error)
                if attempt < RETRY_DELAYS_MS.len() && is_sqlite_busy_or_locked(&error) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAYS_MS[attempt]));
            }
            result => return result,
        }
    }
    unreachable!("the final SQLite retry returns without another retry")
}

fn is_sqlite_busy_or_locked(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(code, _)
            if matches!(
                code.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

#[cfg(not(test))]
pub(super) fn open_db_minimal(market_directory: &str) -> Result<Connection, rusqlite::Error> {
    open_app_db_for_market_directory(market_directory)
}

#[cfg(test)]
pub(super) fn open_db_minimal(market_database: &str) -> Result<Connection, rusqlite::Error> {
    split::open_market_database(&split::shared_storage_paths(
        market_database,
        app_database_path(market_database),
        ":memory:",
    ))
}

pub(super) fn configured_signal_database_path() -> std::path::PathBuf {
    std::env::var_os("QUANT_SIGNAL_DB")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(super::DEFAULT_SIGNAL_DB))
}

pub(super) fn paths_for_market_directory(market_directory: &str) -> connections::StoragePaths {
    let app_db = std::env::var_os("QUANT_APP_DB")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            #[cfg(test)]
            {
                std::path::PathBuf::from(app_database_path(market_directory))
            }
            #[cfg(not(test))]
            {
                std::path::PathBuf::from(super::DEFAULT_APP_DB)
            }
        });
    #[cfg(test)]
    let signals_db = std::path::PathBuf::from(":memory:");
    #[cfg(not(test))]
    let signals_db = configured_signal_database_path();
    connections::StoragePaths::new(market_directory, app_db, signals_db)
}

pub(super) fn open_app_db_for_market_directory(
    market_directory: &str,
) -> Result<Connection, rusqlite::Error> {
    connections::open_app_database(&paths_for_market_directory(market_directory))
}

pub(super) fn open_app_db_without_signal_store_for_market_directory(
    market_directory: &str,
) -> Result<Connection, rusqlite::Error> {
    connections::open_app_database_without_signal_store(&paths_for_market_directory(
        market_directory,
    ))
}

pub(super) fn validate_runtime_market_directory(market_directory: &str) -> Result<(), String> {
    let market_path = std::path::Path::new(market_directory);
    if market_path.is_file() {
        return Err("QUANT_MARKET_DIR must name a directory, not a database file".into());
    }
    let legacy_source = std::path::Path::new(super::DEFAULT_LEGACY_MARKET_DB);
    if legacy_source.is_file() {
        return Err(format!("shared market source still exists at {}; stop the service, run the explicit split if it contains legacy app tables, then shard migrations", legacy_source.display()));
    }
    Ok(())
}

#[cfg(not(test))]
pub(super) fn open_market_history_for_symbol(
    market_directory: &str,
    storage_symbol: &str,
    create: bool,
) -> Result<Option<TickerConnection>, rusqlite::Error> {
    connections::open_market_history_for_symbol(
        &paths_for_market_directory(market_directory),
        storage_symbol,
        create,
    )
}

#[cfg(not(test))]
pub(super) fn open_market_history_for_signal_scan(
    market_directory: &str,
    storage_symbol: &str,
    create: bool,
) -> Result<Option<SignalScanConnection>, rusqlite::Error> {
    connections::open_market_history_for_signal_scan(
        &paths_for_market_directory(market_directory),
        storage_symbol,
        create,
    )
    .map(|connection| connection.map(SignalScanConnection))
}

#[cfg(test)]
pub(super) fn open_market_history_for_symbol(
    market_database: &str,
    storage_symbol: &str,
    create: bool,
) -> Result<Option<Connection>, rusqlite::Error> {
    split::open_market_history_for_symbol(
        &split::shared_storage_paths(
            market_database,
            app_database_path(market_database),
            ":memory:",
        ),
        storage_symbol,
        create,
    )
}

#[cfg(test)]
pub(super) fn open_market_history_for_signal_scan(
    market_database: &str,
    storage_symbol: &str,
    create: bool,
) -> Result<Option<SignalScanConnection>, rusqlite::Error> {
    split::open_market_history_for_symbol(
        &split::shared_storage_paths(
            market_database,
            app_database_path(market_database),
            ":memory:",
        ),
        storage_symbol,
        create,
    )
    .map(|connection| connection.map(SignalScanConnection))
}

#[cfg(test)]
pub(super) fn initialize_market_storage(
    connection: &Connection,
    market_path: &std::path::Path,
    app_path: &std::path::Path,
) -> Result<(), rusqlite::Error> {
    static INITIALIZED_DATABASES: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashSet<(std::path::PathBuf, std::path::PathBuf)>>,
    > = std::sync::OnceLock::new();
    let initialized_databases = INITIALIZED_DATABASES
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    let mut initialized = initialized_databases
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let pair = (market_path.to_path_buf(), app_path.to_path_buf());
    if market_path.to_string_lossy() != ":memory:" && initialized.contains(&pair) {
        return Ok(());
    }
    connection.execute_batch(MARKET_SCHEMA)?;
    initialize_app_storage_schema(connection)?;
    split::initialize_layout_markers(connection, &market_path.to_string_lossy())?;
    market_identity::migrate_candle_source_identity(connection)?;
    market_identity::migrate_candle_provider_identity(connection)?;
    master_data::initialize(connection)?;
    instrument_registry::initialize(connection)?;
    market_identity::migrate_normalized_market_model(connection)?;
    foreign_flow::initialize(connection)?;
    for token in tokenized_stock_symbols() {
        let _ = connection.execute("UPDATE main.series SET instrument_kind='tokenized_stock_yfinance_proxy' WHERE symbol=?1", params![token]);
    }
    if market_path.to_string_lossy() != ":memory:" {
        initialized.insert(pair);
    }
    Ok(())
}
/// Provider access is an ingestion concern, never a chart read concern.
/// Tokocrypto and Pintu Futures have native public market-data feeds; bounded
/// Binance, Coinbase, Bybit, OKX, and Yahoo proxies fill only unsupported data.
pub(super) fn provider_sync_enabled() -> bool {
    true
}

pub(super) fn provider_sync_by_exchange() -> Value {
    serde_json::json!({
        "tokocrypto": true,
        "stockbit": false,
        "idx": false,
        "pintu": true,
        "pintu_futures": true,
        "pluang": false
    })
}

pub(super) fn provider_fallback_by_exchange() -> Value {
    serde_json::json!({
        "tokocrypto": true,
        "stockbit": true,
        "pintu_futures": true,
        "pluang": true
    })
}

#[cfg(test)]
#[path = "database_connections_tests.rs"]
mod connections_tests;
#[cfg(test)]
#[path = "database_sharding_tests.rs"]
mod sharding_tests;
