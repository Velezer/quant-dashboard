use quant_core::{
    PINTU_GOLDEN_CROSS_LEGACY_SIGNAL_STRATEGY_IDS, PINTU_GOLDEN_CROSS_SIGNAL_STRATEGY_ID,
    PINTU_GOLDEN_CROSS_STRATEGY_ID,
};
#[cfg(test)]
use quant_core::{
    TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID, TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID,
    TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
};
use rusqlite::Connection;
use serde_json::Value;
use std::sync::OnceLock;
mod crypto_signal_catalog;
mod crypto_signal_products;
mod stockbit_signal_catalog;
mod tokocrypto_catalog;
use crypto_signal_catalog::ensure_crypto_signal_catalog;
pub(super) use crypto_signal_products::signal_screener_master_data_for_connection;
#[cfg(test)]
pub(super) use crypto_signal_products::{
    signal_screener_master_data_from_catalog, signal_screener_master_data_with_registered_products,
};
use stockbit_signal_catalog::ensure_stockbit_signal_catalog;
pub(super) use tokocrypto_catalog::migrate_tokocrypto_confluence_catalog;
use tokocrypto_catalog::upgrade_tokocrypto_confluence_profiles;
#[cfg(test)]
pub(super) use tokocrypto_catalog::TOKOCRYPTO_CONFLUENCE_MIGRATION_ID;

const REQUIRED_SIGNAL_CONFIRMATION_MODEL_SUFFIX: &str = "_dss_extreme_nwe_touch_v2";
const RETIRED_SIGNAL_CONFIRMATION_MODEL_SUFFIX: &str = "_dss_bressert_nwe_v1";
const MASTER_DATA_JSON: &str = r#"{"schema_version":"1.0.0","updated_on":"2026-09-27","purpose":"Canonical reference data for app-first instruments, trading profiles, signal models, indicators, and configured seed assets.","exchanges":[{"id":"stockbit","code":"STOCKBIT","label":"Stockbit","aliases":["stockbit"],"status":"enabled","kind":"broker","timezone":"Asia/Jakarta","asset_category_ids":["idx_stock"],"default_asset_category_id":"idx_stock","default_research_profile_id":"stockbit_orb_momentum"},{"id":"pintu_futures","code":"PINTU_FUTURES","label":"Pintu Futures","aliases":["pintu","pintu_futures"],"status":"enabled","kind":"crypto_exchange","timezone":"UTC","asset_category_ids":["crypto_perpetual"],"default_asset_category_id":"crypto_perpetual","default_research_profile_id":"pintu_futures_confluence"},{"id":"tokocrypto","code":"TOKOCRYPTO","label":"Tokocrypto","aliases":["tokocrypto"],"status":"enabled","kind":"crypto_exchange","timezone":"UTC","asset_category_ids":["crypto_spot","tokenized_stock"],"default_asset_category_id":"crypto_spot","default_research_profile_id":"tokocrypto_spot_day_trade"},{"id":"pluang","code":"PLUANG","label":"Pluang","aliases":["pluang"],"status":"unsupported","kind":"broker","timezone":"Asia/Jakarta","asset_category_ids":[],"status_reason":"Instrument identity and execution rules are not configured."}],"asset_categories":[{"id":"idx_stock","code":"IDX_STOCK","label":"IDX Stock","instrument_type":"listed_equity","research_aliases":["idx"],"quote_currency":"IDR","direction_policy":"long_only","exchange_ids":["stockbit"]},{"id":"crypto_perpetual","code":"CRYPTO_FUT","label":"Crypto Perpetual","instrument_type":"perpetual_contract","research_aliases":["crypto_futures"],"quote_currency":"USDT","direction_policy":"long_and_short","exchange_ids":["pintu_futures"]},{"id":"crypto_spot","code":"CRYPTO_SPOT","label":"Crypto Spot","instrument_type":"spot_asset","research_aliases":["crypto_spot"],"quote_currency":"IDR","reference_currency":"USD","direction_policy":"long_only","exchange_ids":["tokocrypto"],"price_proxy_policy":"Stored -USD crypto candles are research references, not Tokocrypto IDR execution quotes; venue-native history is required for execution validation."},{"id":"tokenized_stock","code":"TOKENIZED_STOCK","label":"Tokenized Stock","instrument_type":"tokenized_equity","research_aliases":["tokenized_stock"],"quote_currency":"USD","direction_policy":"long_only","exchange_ids":["tokocrypto"],"price_proxy_policy":"Underlying equity prices from yfinance are research proxies, not Tokocrypto execution quotes."}],"trade_styles":[{"id":"day_trade","label":"Day Trade","kind":"holding_horizon","objective":"Short holding window using the selected app's execution rules.","default_timeframe":"15m"},{"id":"swing","label":"Swing","kind":"holding_horizon","objective":"Multi-day trend capture with daily levels.","default_timeframe":"1d"},{"id":"multibagger","label":"Multibagger","kind":"discovery_objective","objective":"Long-horizon discovery. Goal: validate at least 100% fee-adjusted, risk-sized account return in one complete calendar month.","default_timeframe":"1d","minimum_validated_cagr_pct":100}],"indicators":[{"id":"ema_20_50","label":"EMA 20/50","kind":"indicator","signal_strategy":true},{"id":"rsi_14","label":"RSI 14","kind":"indicator","signal_strategy":true},{"id":"relative_volume","label":"Relative Volume","kind":"indicator","signal_strategy":true},{"id":"open_low_structure","label":"Open-Low Structure","kind":"market_structure_rule","signal_strategy":true},{"id":"range_breakout","label":"Range Breakout","kind":"market_structure_rule","signal_strategy":true},{"id":"wick_rejection","label":"Wick Rejection","kind":"price_action_rule","signal_strategy":true},{"id":"momentum_12_1_month","label":"12–1 Month Momentum","kind":"time_series_momentum","signal_strategy":true},{"id":"sma_200_session","label":"200-Session Moving Average","kind":"trend_filter","signal_strategy":true},{"id":"atr_20","label":"ATR 20","kind":"risk_measure","signal_strategy":false},{"id":"dss_bressert","label":"DSS Bressert","kind":"indicator","signal_strategy":false}],"signal_strategies":[{"id":"idx_open_low_rvol","label":"Open-Low + Relative Volume","short_label":"Open-Low + RVOL","icon":"📈","description":"Looks for an IDX stock that opens near its low, with above-usual volume and turnover, then checks for a breakout and ARA/ARB price limits.","status":"registered","implemented":true,"indicator_ids":["open_low_structure","relative_volume"],"asset_category_ids":["idx_stock"]},{"id":"idx_opening_range_momentum","label":"Opening Range + Intraday Momentum · Research","short_label":"09:00–09:30 ORB · Research","icon":"📈","description":"15m Stockbit/IDX, long-only: needs a positive first-30-minute move and a close above the opening range, confirmed by volume and turnover. It also checks tick size and ARA/ARB limits, then exits in the morning. Research only; unvalidated.","status":"research_only","validation_status":"UNVALIDATED","research_only":true,"implemented":true,"indicator_ids":["range_breakout","relative_volume"],"asset_category_ids":["idx_stock"]},{"id":"idx_first_half_hour_momentum","label":"First-Half-Hour Momentum · Research","short_label":"First 30m Momentum · Research","icon":"📈","description":"15m Stockbit/IDX, long-only: requires a positive first-30-minute move backed by volume and turnover. Entry is after 09:30 WIB. Research only; unvalidated.","status":"research_only","validation_status":"UNVALIDATED","research_only":true,"implemented":true,"indicator_ids":["relative_volume"],"asset_category_ids":["idx_stock"]},{"id":"idx_time_of_day_momentum","label":"Same-Time-of-Day Momentum · Research","short_label":"Same 15m Slot · Research","icon":"📈","description":"Long-only Stockbit/IDX: the current morning 15m return and that time slot's average return in prior sessions must both be positive. Research only; unvalidated.","status":"research_only","validation_status":"UNVALIDATED","research_only":true,"implemented":true,"indicator_ids":["relative_volume"],"asset_category_ids":["idx_stock"]},{"id":"range_breakout_rvol_wick","label":"Breakout + Relative Volume + Wick","short_label":"Breakout + RVOL + Wick","icon":"⚡","description":"Signals on a range breakout or wick rejection, confirmed by volume above its usual level.","status":"registered","implemented":true,"indicator_ids":["range_breakout","relative_volume","wick_rejection"],"asset_category_ids":["crypto_perpetual"]},{"id":"ema_rsi_breakout_volume","label":"EMA + RSI + Breakout Volume","short_label":"EMA/RSI + Volume","icon":"🧠","description":"Combines the moving-average trend (EMA), an RSI range, a price breakout, and above-usual volume. All must align.","status":"registered","implemented":true,"indicator_ids":["ema_20_50","rsi_14","range_breakout","relative_volume"],"asset_category_ids":["crypto_perpetual","crypto_spot","tokenized_stock"]},{"id":"ema_20_50_golden_cross_long","label":"EMA 20/50 Golden Cross + Filters · Research","short_label":"20/50 Golden Cross · Long","icon":"📈","description":"Pintu Futures, 15m, long-only: enter only on a closed-bar EMA 20/50 golden cross when RSI 55–78, relative volume ≥1.8, bullish close, and close breaks the prior 20-bar high. Fixed 1% stop. Research only; unvalidated.","status":"research_only","validation_status":"UNVALIDATED","research_only":true,"implemented":true,"indicator_ids":["ema_20_50","rsi_14","range_breakout","relative_volume"],"asset_category_ids":["crypto_perpetual"]},{"id":"idx_time_series_momentum","label":"12–1 Month Momentum · Research","short_label":"12–1M Momentum · Research","icon":"📈","description":"Daily Stockbit/IDX, long-only: looks for positive price return from 12 months ago to 1 month ago, with price above its 200-session average. ATR sets risk levels. Research only; unvalidated.","status":"research_only","validation_status":"UNVALIDATED","research_only":true,"implemented":true,"indicator_ids":["momentum_12_1_month","sma_200_session","atr_20"],"asset_category_ids":["idx_stock"]}],"plan_profiles":[{"id":"stockbit_orb_momentum","label":"Stockbit · Opening Range + Momentum (Research)","exchange_id":"stockbit","asset_category_id":"idx_stock","trade_style_id":"day_trade","signal_strategy_id":"idx_opening_range_momentum","engine_strategy_id":"idx_stockbit_opening_range_momentum","evidence_strategy_id":"idx_stockbit_opening_range_momentum","evidence_class":"idx","timeframe":"15m","exchange_code":"STOCKBIT","asset_class_code":"IDX_STOCK","time_window":"15m opening range · trade after 09:30 WIB · flatten by the morning-session end","research_label":"Opening Range + Intraday Momentum · Research only","research_enabled":true,"validation_status":"UNVALIDATED","execution_scope":"research_only"},{"id":"stockbit_first_half_hour_momentum","label":"Stockbit · First-Half-Hour Momentum (Research)","exchange_id":"stockbit","asset_category_id":"idx_stock","trade_style_id":"day_trade","signal_strategy_id":"idx_first_half_hour_momentum","engine_strategy_id":"idx_stockbit_first_half_hour_momentum","evidence_strategy_id":"idx_stockbit_first_half_hour_momentum","evidence_class":"idx","timeframe":"15m","exchange_code":"STOCKBIT","asset_class_code":"IDX_STOCK","time_window":"09:00–09:30 return · enter after 09:30 WIB · flatten by morning-session end","research_label":"First-Half-Hour Momentum · Research only","research_enabled":true,"validation_status":"UNVALIDATED","execution_scope":"research_only"},{"id":"stockbit_time_of_day_momentum","label":"Stockbit · Same-Time-of-Day Momentum (Research)","exchange_id":"stockbit","asset_category_id":"idx_stock","trade_style_id":"day_trade","signal_strategy_id":"idx_time_of_day_momentum","engine_strategy_id":"idx_stockbit_time_of_day_momentum","evidence_strategy_id":"idx_stockbit_time_of_day_momentum","evidence_class":"idx","timeframe":"15m","exchange_code":"STOCKBIT","asset_class_code":"IDX_STOCK","time_window":"15m morning slot with positive prior-session mean · morning-only","research_label":"Same-Time-of-Day Momentum · Research only","research_enabled":true,"validation_status":"UNVALIDATED","execution_scope":"research_only"},{"id":"stockbit_day_trade","label":"Stockbit · Day Trade","exchange_id":"stockbit","asset_category_id":"idx_stock","trade_style_id":"day_trade","signal_strategy_id":"idx_open_low_rvol","engine_strategy_id":"idx_stockbit_day_trade","evidence_strategy_id":"idx_stockbit_day_trade","evidence_class":"idx","timeframe":"15m","exchange_code":"STOCKBIT","asset_class_code":"IDX_STOCK","time_window":"Stockbit day trade · 15m setup and app cutoff","research_label":"Open-Low + Relative Volume · Day Trade","research_enabled":true},{"id":"stockbit_swing","label":"Stockbit · Swing","exchange_id":"stockbit","asset_category_id":"idx_stock","trade_style_id":"swing","signal_strategy_id":"idx_open_low_rvol","engine_strategy_id":"idx_stockbit_swing","evidence_strategy_id":"idx_stockbit_swing","evidence_class":"idx","timeframe":"1d","exchange_code":"STOCKBIT","asset_class_code":"IDX_STOCK","time_window":"Stockbit swing · daily close","research_label":"Open-Low + Relative Volume · Swing","research_enabled":true},{"id":"stockbit_tsmom_swing","label":"Stockbit · 12–1 Month Momentum Swing (Research)","exchange_id":"stockbit","asset_category_id":"idx_stock","trade_style_id":"swing","signal_strategy_id":"idx_time_series_momentum","engine_strategy_id":"idx_stockbit_time_series_momentum_swing","evidence_strategy_id":"idx_stockbit_time_series_momentum_swing","evidence_class":"idx","timeframe":"1d","exchange_code":"STOCKBIT","asset_class_code":"IDX_STOCK","time_window":"Daily 12–1 month momentum · 200-session trend · ATR brackets","research_label":"12–1 Month Momentum · Swing (Research only)","research_enabled":true,"validation_status":"UNVALIDATED"},{"id":"stockbit_multibagger","label":"Stockbit · Multibagger Discovery","exchange_id":"stockbit","asset_category_id":"idx_stock","trade_style_id":"multibagger","signal_strategy_id":"idx_open_low_rvol","engine_strategy_id":"multibagger","evidence_strategy_id":"multibagger","evidence_class":"idx","timeframe":"1d","exchange_code":"STOCKBIT","asset_class_code":"IDX_STOCK","time_window":"Stockbit multibagger discovery · daily close","research_label":"Open-Low + Relative Volume · Multibagger goal (+100% account return in a complete month)","research_enabled":true},{"id":"pintu_futures_day_trade","label":"Pintu Futures · Day Trade","exchange_id":"pintu_futures","asset_category_id":"crypto_perpetual","trade_style_id":"day_trade","signal_strategy_id":"range_breakout_rvol_wick","engine_strategy_id":"pintu_futures_day_trade","evidence_strategy_id":"pintu_futures_day_trade","evidence_class":"crypto_futures","timeframe":"15m","exchange_code":"PINTU_FUTURES","asset_class_code":"CRYPTO_FUT","time_window":"Pintu perpetual futures · entries require a confirmed signal","research_label":"Breakout + Relative Volume + Wick","research_enabled":true},{"id":"pintu_futures_confluence","label":"Pintu Futures · EMA Golden Cross (Research)","exchange_id":"pintu_futures","asset_category_id":"crypto_perpetual","trade_style_id":"day_trade","signal_strategy_id":"ema_20_50_golden_cross_long","engine_strategy_id":"pintu_futures_golden_cross_v1","evidence_strategy_id":"pintu_futures_golden_cross_v1","evidence_class":"crypto_futures","timeframe":"15m","exchange_code":"PINTU_FUTURES","asset_class_code":"CRYPTO_FUT","time_window":"Pintu perpetual futures · closed 15m EMA 20/50 golden cross","research_label":"EMA 20/50 Golden Cross + Filters · Research only","research_enabled":true,"validation_status":"UNVALIDATED","execution_scope":"research_only"},{"id":"tokocrypto_spot_day_trade","label":"Tokocrypto · Crypto Spot Day Trade","exchange_id":"tokocrypto","asset_category_id":"crypto_spot","trade_style_id":"day_trade","signal_strategy_id":"ema_rsi_breakout_volume","engine_strategy_id":"tokocrypto_spot_day_trade","evidence_strategy_id":"tokocrypto_spot_day_trade","evidence_class":"crypto_spot","timeframe":"15m","exchange_code":"TOKOCRYPTO","asset_class_code":"CRYPTO_SPOT","time_window":"Tokocrypto spot · 24/7","research_label":"EMA + RSI + Breakout Volume","research_enabled":true},{"id":"tokocrypto_tokenized_stock_day_trade","label":"Tokocrypto · Tokenized Stock Day Trade","exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","trade_style_id":"day_trade","signal_strategy_id":"ema_rsi_breakout_volume","engine_strategy_id":"tokenized_stock_day_trade","evidence_strategy_id":"tokenized_stock_day_trade","evidence_class":"crypto_spot","timeframe":"15m","exchange_code":"TOKOCRYPTO","asset_class_code":"TOKENIZED_STOCK","time_window":"Tokocrypto tokenized stock · verify venue quote before execution","research_label":"EMA + RSI + Breakout Volume","research_enabled":true}],"assets":[{"instrument_id":"tokocrypto.tokenized_stock.tsla","symbol":"TSLA","aliases":["TSLA-USD","TSLAUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"TSLA","storage_symbol":"TSLA","fallback_provider":"yfinance_fallback","fallback_symbol":"TSLA","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"tokocrypto.tokenized_stock.nvda","symbol":"NVDA","aliases":["NVDA-USD","NVDAUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"NVDA","storage_symbol":"NVDA","fallback_provider":"yfinance_fallback","fallback_symbol":"NVDA","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"tokocrypto.tokenized_stock.aapl","symbol":"AAPL","aliases":["AAPL-USD","AAPLUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"AAPL","storage_symbol":"AAPL","fallback_provider":"yfinance_fallback","fallback_symbol":"AAPL","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"tokocrypto.tokenized_stock.msft","symbol":"MSFT","aliases":["MSFT-USD","MSFTUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"MSFT","storage_symbol":"MSFT","fallback_provider":"yfinance_fallback","fallback_symbol":"MSFT","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"tokocrypto.tokenized_stock.amzn","symbol":"AMZN","aliases":["AMZN-USD","AMZNUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"AMZN","storage_symbol":"AMZN","fallback_provider":"yfinance_fallback","fallback_symbol":"AMZN","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"tokocrypto.tokenized_stock.googl","symbol":"GOOGL","aliases":["GOOGL-USD","GOOGLUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"GOOGL","storage_symbol":"GOOGL","fallback_provider":"yfinance_fallback","fallback_symbol":"GOOGL","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"tokocrypto.tokenized_stock.meta","symbol":"META","aliases":["META-USD","METAUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"META","storage_symbol":"META","fallback_provider":"yfinance_fallback","fallback_symbol":"META","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"tokocrypto.tokenized_stock.amd","symbol":"AMD","aliases":["AMD-USD","AMDUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"AMD","storage_symbol":"AMD","fallback_provider":"yfinance_fallback","fallback_symbol":"AMD","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"tokocrypto.tokenized_stock.coin","symbol":"COIN","aliases":["COIN-USD","COINUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"COIN","storage_symbol":"COIN","fallback_provider":"yfinance_fallback","fallback_symbol":"COIN","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"tokocrypto.tokenized_stock.spy","symbol":"SPY","aliases":["SPY-USD","SPYUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"SPY","storage_symbol":"SPY","fallback_provider":"yfinance_fallback","fallback_symbol":"SPY","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"tokocrypto.tokenized_stock.qqq","symbol":"QQQ","aliases":["QQQ-USD","QQQUSDT"],"exchange_id":"tokocrypto","asset_category_id":"tokenized_stock","underlying_symbol":"QQQ","storage_symbol":"QQQ","fallback_provider":"yfinance_fallback","fallback_symbol":"QQQ","quote_currency":"USD","role":"tokenized_stock_watchlist"},{"instrument_id":"stockbit.idx.truk","symbol":"TRUK","aliases":["TRUK.JK"],"exchange_id":"stockbit","asset_category_id":"idx_stock","underlying_symbol":"TRUK","storage_symbol":"TRUK.JK","fallback_provider":"yfinance_fallback","fallback_symbol":"TRUK.JK","quote_currency":"IDR","role":"multibagger_seed"},{"instrument_id":"stockbit.idx.agar","symbol":"AGAR","aliases":["AGAR.JK"],"exchange_id":"stockbit","asset_category_id":"idx_stock","underlying_symbol":"AGAR","storage_symbol":"AGAR.JK","fallback_provider":"yfinance_fallback","fallback_symbol":"AGAR.JK","quote_currency":"IDR","role":"multibagger_seed"}],"universe_rules":[{"id":"stockbit_idx_from_local_history","exchange_id":"stockbit","asset_category_id":"idx_stock","source":"durable_market_history","match_suffix":".JK","multibagger_scope":"all stored IDX daily instruments; seed assets are not an exhaustive candidate list"},{"id":"crypto_proxy_dual_app_views","source":"durable_market_history","match_suffix":"-USD","exclude_asset_category_ids":["tokenized_stock"],"plan_profile_ids":["pintu_futures_confluence","tokocrypto_spot_day_trade"],"execution_validation":"venue_native_history_required"}]}"#;
static MASTER_DATA: OnceLock<Value> = OnceLock::new();

pub(super) fn initialize_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app.reference_catalogs (
             catalog_id TEXT PRIMARY KEY,
             payload_json TEXT NOT NULL,
             source_label TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS app.reference_catalog_migrations (
             migration_id TEXT PRIMARY KEY,
             source_hash TEXT NOT NULL,
             destination_hash TEXT NOT NULL,
             migrated_row_counts_json TEXT NOT NULL,
             completed_at TEXT NOT NULL
         );",
    )
}

pub(super) fn initialize(conn: &Connection) -> Result<(), rusqlite::Error> {
    initialize_schema(conn)?;
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO reference_catalogs(catalog_id, payload_json, source_label)
         VALUES('master_data', ?1, 'embedded_release_seed')",
        [MASTER_DATA_JSON],
    )?;
    let encoded = conn.query_row(
        "SELECT payload_json FROM reference_catalogs WHERE catalog_id='master_data'",
        [],
        |row| row.get::<_, String>(0),
    )?;
    let mut catalog: Value = super::wire::decode(&encoded)
        .expect("SQLite master-data catalog must be valid JSON reference data");
    if augment_app_products(&mut catalog, inserted > 0) {
        catalog["updated_on"] = Value::from("2026-10-07");
        let encoded = super::wire::encode(&catalog)
            .expect("extended app/product catalog must encode as JSON");
        conn.execute(
            "UPDATE reference_catalogs SET payload_json=?1 WHERE catalog_id='master_data'",
            [encoded],
        )?;
    }
    validate_master_data_catalog(&catalog)
        .expect("SQLite master-data catalog references must remain internally consistent");
    let _ = MASTER_DATA.set(catalog.clone());
    assert_eq!(
        MASTER_DATA.get().expect("master data was initialized"),
        &catalog,
        "one process cannot use different master-data catalogs from separate databases",
    );
    Ok(())
}

mod upgrades;
#[cfg(test)]
use upgrades::upgrade_pintu_golden_cross_profile;
use upgrades::{
    augment_app_products, ensure_required_signal_confirmation_models,
    retire_stockbit_nwe_4h_legacy_strategies, upsert_catalog_record,
};

pub(super) fn master_data_catalog() -> &'static Value {
    MASTER_DATA
        .get()
        .expect("SQLite master-data catalog must be initialized before use")
}

pub(super) fn master_data_records(key: &str) -> &'static [Value] {
    master_data_catalog()
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub(super) fn catalog_string<'a>(record: &'a Value, key: &str) -> Option<&'a str> {
    record.get(key).and_then(Value::as_str)
}

mod validation;
pub(super) use validation::{catalog_array_contains, validate_master_data_catalog};

pub(super) fn profile_by_id(profile_id: &str) -> Option<&'static Value> {
    master_data_records("plan_profiles")
        .iter()
        .find(|profile| catalog_string(profile, "id") == Some(profile_id))
}

pub(super) fn profile_for_app_product(
    exchange_id: &str,
    product_id: &str,
) -> Option<&'static Value> {
    master_data_records("plan_profiles").iter().find(|profile| {
        catalog_string(profile, "exchange_id")
            .is_some_and(|value| value.eq_ignore_ascii_case(exchange_id))
            && catalog_string(profile, "product_id") == Some(product_id)
    })
}

pub(super) fn exchange_by_alias(exchange: &str) -> Option<&'static Value> {
    let normalized = exchange.trim();
    master_data_records("exchanges").iter().find(|record| {
        catalog_string(record, "id").is_some_and(|id| id.eq_ignore_ascii_case(normalized))
            || catalog_string(record, "code")
                .is_some_and(|code| code.eq_ignore_ascii_case(normalized))
            || record
                .get("aliases")
                .and_then(Value::as_array)
                .is_some_and(|aliases| {
                    aliases.iter().any(|alias| {
                        alias
                            .as_str()
                            .is_some_and(|alias| alias.eq_ignore_ascii_case(normalized))
                    })
                })
    })
}

pub(super) fn category_by_id_or_research_alias(category: &str) -> Option<&'static Value> {
    let normalized = category.trim();
    master_data_records("asset_categories")
        .iter()
        .find(|record| {
            catalog_string(record, "id").is_some_and(|id| id.eq_ignore_ascii_case(normalized))
                || catalog_string(record, "code")
                    .is_some_and(|code| code.eq_ignore_ascii_case(normalized))
                || record
                    .get("research_aliases")
                    .and_then(Value::as_array)
                    .is_some_and(|aliases| {
                        aliases.iter().any(|alias| {
                            alias
                                .as_str()
                                .is_some_and(|alias| alias.eq_ignore_ascii_case(normalized))
                        })
                    })
        })
}

pub(super) fn default_profile_for_exchange(exchange: &str) -> Option<&'static Value> {
    let exchange = exchange_by_alias(exchange)?;
    let profile_id = catalog_string(exchange, "default_research_profile_id")?;
    profile_by_id(profile_id)
}

pub(super) fn profile_for_strategy(strategy: &str) -> Option<&'static Value> {
    let normalized = strategy.trim().to_ascii_lowercase();
    master_data_records("plan_profiles").iter().find(|profile| {
        ["id", "engine_strategy_id", "evidence_strategy_id"]
            .iter()
            .any(|field| {
                catalog_string(profile, field)
                    .is_some_and(|value| value.eq_ignore_ascii_case(&normalized))
            })
    })
}

pub(super) fn tokenized_stock_symbols() -> impl Iterator<Item = &'static str> {
    master_data_records("assets")
        .iter()
        .filter(|asset| catalog_string(asset, "asset_category_id") == Some("tokenized_stock"))
        .filter_map(|asset| catalog_string(asset, "storage_symbol"))
}

pub(super) fn multibagger_seed_symbols() -> impl Iterator<Item = &'static str> {
    master_data_records("assets")
        .iter()
        .filter(|asset| catalog_string(asset, "role") == Some("multibagger_seed"))
        .filter_map(|asset| catalog_string(asset, "storage_symbol"))
}

#[cfg(test)]
#[path = "master_data_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "master_data_crypto_products_tests.rs"]
mod crypto_products_tests;
