pub(super) const APP_RUNTIME_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS app.storage_layout_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS app.storage_split_migrations (
    migration_id TEXT PRIMARY KEY, source_path TEXT NOT NULL, source_hash TEXT NOT NULL,
    migrated_row_counts_json TEXT NOT NULL, completed_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS app.market_sync_cursor (
    exchange_id TEXT NOT NULL, product_id TEXT NOT NULL, storage_symbol TEXT NOT NULL,
    interval TEXT NOT NULL, attempted_at TEXT NOT NULL, status TEXT NOT NULL,
    error TEXT NOT NULL DEFAULT '', PRIMARY KEY(exchange_id, product_id, storage_symbol, interval)
);
CREATE TABLE IF NOT EXISTS app.trade_plans (
    id INTEGER PRIMARY KEY AUTOINCREMENT, symbol TEXT NOT NULL, asset TEXT NOT NULL,
    strategy_id TEXT NOT NULL, direction TEXT NOT NULL, state TEXT NOT NULL,
    entry_price REAL NOT NULL, stop_loss REAL NOT NULL, tp1 REAL NOT NULL, tp2 REAL,
    risk_reward REAL, plan_json TEXT NOT NULL DEFAULT '{}', exchange_id TEXT, product_id TEXT,
    instrument_id TEXT, profile_id TEXT, created_at INTEGER NOT NULL, is_active INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS app.idx_plans_active ON trade_plans(is_active, state);
CREATE INDEX IF NOT EXISTS app.idx_plans_asset_active ON trade_plans(asset, is_active);
CREATE TABLE IF NOT EXISTS app.plan_reviews (
    plan_id TEXT PRIMARY KEY, status TEXT NOT NULL CHECK(status IN ('NEEDS_REVIEW','WATCHING','INVALIDATED','DISMISSED')),
    thesis TEXT NOT NULL DEFAULT '', trigger_condition TEXT NOT NULL DEFAULT '',
    invalidation_condition TEXT NOT NULL DEFAULT '', risk_rule TEXT NOT NULL DEFAULT '',
    next_review_date TEXT, updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS app.entry_model_validation_reports (
    instrument_id TEXT NOT NULL, timeframe TEXT NOT NULL, protocol_id TEXT NOT NULL,
    computed_at INTEGER NOT NULL, report_json TEXT NOT NULL, summary_json TEXT NOT NULL,
    backtest_complete INTEGER NOT NULL CHECK(backtest_complete IN (0,1)),
    has_validated_model INTEGER NOT NULL CHECK(has_validated_model IN (0,1)),
    PRIMARY KEY(instrument_id,timeframe),
    FOREIGN KEY(instrument_id) REFERENCES instruments(instrument_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS app.idx_entry_validation_reports_timeframe
    ON entry_model_validation_reports(timeframe,protocol_id,computed_at);
CREATE TABLE IF NOT EXISTS app.saved_reward_risk_rule (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    risk_pct REAL NOT NULL, reward_pct REAL NOT NULL, win_rate_pct REAL NOT NULL,
    max_trades_per_day INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
"#;

pub(super) const MARKET_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS main.storage_layout_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS main.bars (
    symbol TEXT NOT NULL, interval TEXT NOT NULL, source TEXT NOT NULL DEFAULT 'unknown', ts INTEGER NOT NULL,
    open REAL NOT NULL, high REAL NOT NULL, low REAL NOT NULL, close REAL NOT NULL, volume REAL NOT NULL,
    adj_close REAL, dividends REAL, splits REAL, ingested_at TEXT NOT NULL,
    PRIMARY KEY(symbol, interval, source, ts)
);
CREATE INDEX IF NOT EXISTS main.idx_bars_series_ts ON bars(symbol, interval, source, ts);
CREATE TABLE IF NOT EXISTS main.series (
    symbol TEXT NOT NULL, interval TEXT NOT NULL, source TEXT NOT NULL,
    exchange_id TEXT NOT NULL DEFAULT 'unknown', product_id TEXT NOT NULL DEFAULT '', instrument_id TEXT,
    data_provider TEXT NOT NULL DEFAULT 'unknown', timezone TEXT NOT NULL, price_basis TEXT NOT NULL,
    instrument_kind TEXT NOT NULL, last_success TEXT, last_attempt TEXT, last_error TEXT,
    PRIMARY KEY(symbol, interval, source)
);
CREATE TABLE IF NOT EXISTS main.ingestion_runs (
    id INTEGER PRIMARY KEY, symbol TEXT, interval TEXT, source TEXT,
    exchange_id TEXT NOT NULL DEFAULT 'unknown', product_id TEXT NOT NULL DEFAULT '',
    data_provider TEXT NOT NULL DEFAULT 'unknown', attempted_at TEXT, status TEXT,
    rows_received INTEGER, error TEXT
);
"#;
