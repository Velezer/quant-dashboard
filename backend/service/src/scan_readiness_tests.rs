use super::*;
use crate::main_test_support::{remove_temporary_database, temporary_database_path};

fn add_bars(connection: &Connection, timeframe: &str, source: &str, count: usize, age_bars: i64) {
    let duration = timeframe_duration_ns(timeframe).unwrap();
    let now = current_time_ns();
    let latest = now.div_euclid(duration) * duration - age_bars * duration;
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                            timezone,price_basis,instrument_kind)
         VALUES('BTC-USD',?1,?2,'pintu','pintu_pro_spot','test','UTC','test','crypto_spot')",
            params![timeframe, source],
        )
        .unwrap();
    for index in 0..count {
        let timestamp = latest - (count - index - 1) as i64 * duration;
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
             VALUES('BTC-USD',?1,?2,?3,100,101,99,100,100,'test')",
                params![timeframe, source, timestamp],
            )
            .unwrap();
    }
}

fn scan(connection: &Connection) -> InstrumentReadiness {
    scan_for(connection, "pintu", "pintu_pro_spot", "crypto_spot")
}

fn scan_for(
    connection: &Connection,
    exchange: &str,
    product: &str,
    category: &str,
) -> InstrumentReadiness {
    let mut readiness = InstrumentReadiness::default();
    append_timeframe_signal_plans_with_readiness(
        TimeframeScanInput {
            app_connection: connection,
            market_connection: connection,
            sync_state: None,
            exchange_id: exchange,
            product_id: product,
            category_id: category,
            symbol: "BTC-USD",
            asset: "BTC",
            strategy: "tokocrypto_spot_day_trade",
            evidence_class: "crypto_spot",
            class: "CRYPTO_SPOT",
            exchange: "PINTU",
            time_window: "24/7",
            base_timeframe: "15m",
            is_tokenized: false,
            is_multibagger: false,
            retained_stockbit_signal_rows: &Default::default(),
        },
        &mut Vec::new(),
        &mut Vec::new(),
        &mut HashMap::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut readiness,
    )
    .unwrap();
    readiness
}

#[test]
fn readiness_distinguishes_dss_and_nwe_history_boundaries() {
    for (count, evaluated_models, insufficient) in
        [(49, 0, 1), (50, 1, 0), (500, 1, 0), (501, 2, 0)]
    {
        let path = temporary_database_path(&format!("readiness-warmup-{count}"));
        let connection = open_db_minimal(&path).unwrap();
        add_bars(
            &connection,
            "15m",
            "pintu_pro_spot_binance_spot_proxy",
            count,
            1,
        );
        let readiness = scan(&connection);
        let frame = readiness
            .timeframes
            .iter()
            .find(|frame| frame.timeframe == "15m")
            .unwrap();
        assert_eq!(readiness.price_status, "fresh");
        assert_eq!(
            frame.evaluated_model_count, evaluated_models,
            "{count} bars"
        );
        assert_eq!(
            frame.insufficient_history_count, insufficient,
            "{count} bars"
        );
        assert_eq!(
            frame.nwe_insufficient_history_count,
            usize::from(count < 501)
        );
        drop(connection);
        remove_temporary_database(&path);
    }
}

#[test]
fn readiness_never_calls_stale_or_cross_source_crypto_data_evaluated() {
    let path = temporary_database_path("readiness-stale-source");
    let connection = open_db_minimal(&path).unwrap();
    add_bars(
        &connection,
        "15m",
        "pintu_pro_spot_binance_spot_proxy",
        60,
        8,
    );
    let stale = scan(&connection);
    assert_eq!(stale.price_status, "stale");
    assert!(stale
        .timeframes
        .iter()
        .all(|frame| frame.evaluated_count == 0));
    connection
        .execute("DELETE FROM bars WHERE interval='15m'", [])
        .unwrap();
    connection
        .execute("DELETE FROM series WHERE interval='15m'", [])
        .unwrap();
    add_bars(
        &connection,
        "15m",
        "pintu_pro_spot_binance_spot_proxy",
        60,
        1,
    );
    add_bars(&connection, "4h", "pintu_pro_spot_bybit_spot_proxy", 60, 1);
    let mismatch = scan(&connection);
    let frame = mismatch
        .timeframes
        .iter()
        .find(|frame| frame.timeframe == "4h")
        .unwrap();
    // The preferred price source already has 15m candles, so its 4h rollup
    // takes precedence over another provider's longer 4h history. That short
    // same-source sample must stay unevaluated rather than borrowing Bybit.
    assert_eq!(frame.source_mismatch_count, 0);
    assert_eq!(frame.insufficient_history_count, 1);
    assert_eq!(frame.evaluated_count, 0);
    assert!(mismatch.issues.iter().any(|issue| issue.timeframe == "4h"
        && issue.reason == "insufficient_history"
        && issue.source.as_deref() == Some("pintu_pro_spot_binance_spot_proxy")));
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn readiness_never_counts_stockbit_scoring_without_its_fresh_matching_price() {
    let path = temporary_database_path("readiness-stockbit-stale-price");
    let connection = open_db_minimal(&path).unwrap();
    add_bars(&connection, "15m", "stockbit_yfinance_fallback", 60, 8);
    add_bars(&connection, "4h", "stockbit_yfinance_fallback", 60, 1);
    connection.execute("UPDATE series SET exchange_id='stockbit',product_id='stockbit_regular',instrument_kind='idx_stock'",[]).unwrap();
    let readiness = scan_for(&connection, "stockbit", "stockbit_regular", "idx_stock");
    assert_eq!(readiness.price_status, "stale");
    assert!(readiness
        .timeframes
        .iter()
        .all(|frame| frame.evaluated_count == 0));
    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn readiness_follows_the_generic_strategy_price_source_contract() {
    let path = temporary_database_path("readiness-tokenized-source");
    let connection = open_db_minimal(&path).unwrap();
    add_bars(&connection, "15m", "tokocrypto_native", 60, 1);
    add_bars(&connection, "4h", "tokocrypto_yfinance_fallback", 501, 1);
    connection.execute("UPDATE series SET exchange_id='tokocrypto',product_id='tokocrypto_tokenized_stock',instrument_kind='tokenized_stock'",[]).unwrap();
    let readiness = scan_for(
        &connection,
        "tokocrypto",
        "tokocrypto_tokenized_stock",
        "tokenized_stock",
    );
    let frame = readiness
        .timeframes
        .iter()
        .find(|frame| frame.timeframe == "4h")
        .unwrap();
    assert_eq!(readiness.price_status, "fresh");
    assert_eq!(frame.evaluated_count, 1);
    assert_eq!(frame.source_mismatch_count, 0);
    drop(connection);
    remove_temporary_database(&path);
}
