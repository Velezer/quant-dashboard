use super::*;

fn dss_market_connection() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory market database should open");
    connection
        .execute_batch(
            "CREATE TABLE series (
                symbol TEXT NOT NULL, interval TEXT NOT NULL, source TEXT NOT NULL,
                exchange_id TEXT NOT NULL, product_id TEXT NOT NULL,
                data_provider TEXT NOT NULL, timezone TEXT NOT NULL,
                price_basis TEXT NOT NULL, instrument_kind TEXT NOT NULL,
                last_success TEXT, last_attempt TEXT, last_error TEXT,
                PRIMARY KEY(symbol, interval, source)
            );
            CREATE TABLE bars (
                symbol TEXT NOT NULL, interval TEXT NOT NULL, source TEXT NOT NULL,
                ts INTEGER NOT NULL, open REAL NOT NULL, high REAL NOT NULL,
                low REAL NOT NULL, close REAL NOT NULL, volume REAL NOT NULL,
                PRIMARY KEY(symbol, interval, source, ts)
            );",
        )
        .expect("market tables should be ready");
    connection
}

#[test]
fn every_app_prefers_fresh_same_product_history_before_stale_provider_priority() {
    let now = (now_secs() as i64) * NANOS_PER_SECOND;
    for (app, product) in [("stockbit", "stockbit_regular"), ("pintu", "pintu_pro_futures"),
        ("pluang", "pluang_perpetual"), ("tokocrypto", "tokocrypto_spot")] {
        let connection = dss_market_connection();
        for (source, exchange, product_id, provider, timestamp) in [
            ("native", app, product, app, now - 4 * 3600 * NANOS_PER_SECOND),
            ("fallback", app, product, "yfinance", now - 16 * 60 * NANOS_PER_SECOND),
            ("other_product", app, "different_product", app, now - 15 * 60 * NANOS_PER_SECOND),
        ] {
            connection.execute(
                "INSERT INTO series(symbol,interval,source,exchange_id,product_id,data_provider,
                    timezone,price_basis,instrument_kind) VALUES('TEST','15m',?1,?2,?3,?4,'UTC','research_proxy','spot')",
                params![source, exchange, product_id, provider],
            ).unwrap();
            connection.execute(
                "INSERT INTO bars VALUES('TEST','15m',?1,?2,100,102,99,101,100)",
                params![source, timestamp],
            ).unwrap();
        }
        let selected = selected_candle_source_for_product(&connection, "TEST", app, product, "15m").unwrap();
        assert_eq!(selected.as_deref(), Some("fallback"), "{app} must use its fresh fallback");
        let state = product_candle_scan_state_at(&connection, "TEST", app, product, "15m", now / NANOS_PER_SECOND)
            .unwrap().unwrap();
        assert!(!state.is_stale, "{app} should have a fresh same-product input");
        connection.execute("INSERT INTO bars VALUES('TEST','15m','native',?1,100,102,99,101,100)",
            [now - 17 * 60 * NANOS_PER_SECOND]).unwrap();
        assert_eq!(selected_candle_source_for_product(&connection, "TEST", app, product, "15m")
            .unwrap().as_deref(), Some("native"), "{app} keeps native priority when it is fresh");
    }
}

fn insert_dss_history(connection: &Connection, stale_timeframe: Option<&str>) {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(i64::MAX as u128) as i64;
    let source = "tokocrypto_native";
    for definition in DSS_BRESSERT_MTF_TIMEFRAMES {
        let duration_ns = timeframe_duration_ns(definition.timeframe).unwrap();
        connection
            .execute(
                "INSERT INTO series(symbol,interval,source,exchange_id,product_id,
                                    data_provider,timezone,price_basis,instrument_kind)
                 VALUES('BTC-IDR',?1,?2,'tokocrypto','tokocrypto_spot',
                        'tokocrypto','UTC','last_trade','spot')",
                params![definition.timeframe, source],
            )
            .expect("native series metadata should persist");
        let age_ns = if stale_timeframe == Some(definition.timeframe) {
            4 * 3_600 * 1_000_000_000_i64
        } else if definition.timeframe == "15m" {
            duration_ns
        } else {
            2 * duration_ns
        };
        let last_bar_time = now_ns.div_euclid(duration_ns) * duration_ns - age_ns;
        let bar_count = if definition.timeframe == "1h" {
            400
        } else {
            80
        };
        let first_bar_time = last_bar_time - (bar_count - 1) * duration_ns;
        for index in 0..bar_count {
            let close = 100.0 + index as f64 * 0.25 + (index as f64 * 0.7).sin();
            connection
                .execute(
                    "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume)
                     VALUES('BTC-IDR',?1,?2,?3,?4,?5,?6,?7,10)",
                    params![
                        definition.timeframe,
                        source,
                        first_bar_time + index * duration_ns,
                        close,
                        close + 1.0,
                        close - 1.0,
                        close,
                    ],
                )
                .expect("native candle should persist");
        }
    }
}

#[test]
fn two_hour_product_history_rolls_up_the_same_products_hourly_bars() {
    let connection = Connection::open_in_memory().expect("in-memory market database should open");
    connection
        .execute_batch(
            "CREATE TABLE series (
                symbol TEXT NOT NULL, interval TEXT NOT NULL, source TEXT NOT NULL,
                exchange_id TEXT NOT NULL, product_id TEXT NOT NULL,
                data_provider TEXT NOT NULL, timezone TEXT NOT NULL,
                price_basis TEXT NOT NULL, instrument_kind TEXT NOT NULL,
                last_success TEXT, last_attempt TEXT, last_error TEXT,
                PRIMARY KEY(symbol, interval, source)
            );
            CREATE TABLE bars (
                symbol TEXT NOT NULL, interval TEXT NOT NULL, source TEXT NOT NULL,
                ts INTEGER NOT NULL, open REAL NOT NULL, high REAL NOT NULL,
                low REAL NOT NULL, close REAL NOT NULL, volume REAL NOT NULL,
                PRIMARY KEY(symbol, interval, source, ts)
            );",
        )
        .expect("market tables should be ready");
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,
                                data_provider,timezone,price_basis,instrument_kind)
             VALUES('BTC-USD','1h','binance_proxy','pintu','pintu_pro_futures',
                    'binance','UTC','last_trade','perpetual')",
            [],
        )
        .expect("hourly provider identity should persist");
    let hour_ns = 3_600_i64 * 1_000_000_000;
    let two_hour_base_ns = (1_700_000_000_i64 / 7_200 * 7_200) * 1_000_000_000;
    for index in 0..6_i64 {
        let open = 100.0 + index as f64;
        connection
            .execute(
                "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume)
                 VALUES('BTC-USD','1h','binance_proxy',?1,?2,?3,?4,?5,10)",
                params![
                    two_hour_base_ns + index * hour_ns,
                    open,
                    open + 2.0,
                    open - 1.0,
                    open + 0.5,
                ],
            )
            .expect("hourly provider bar should persist");
    }

    assert_eq!(
        selected_candle_source_for_product(
            &connection,
            "BTC-USD",
            "pintu",
            "pintu_pro_futures",
            "2h",
        )
        .expect("product source should resolve")
        .as_deref(),
        Some("binance_proxy")
    );
    let two_hour = load_series_candles_for_product(
        &connection,
        "BTC-USD",
        "2h",
        10,
        "pintu",
        "pintu_pro_futures",
    )
    .expect("2h bars should aggregate from hourly product history");

    assert_eq!(two_hour.len(), 3);
    assert_eq!(two_hour[0].open, 100.0);
    assert_eq!(two_hour[0].high, 103.0);
    assert_eq!(two_hour[0].low, 99.0);
    assert_eq!(two_hour[0].close, 101.5);
    assert_eq!(two_hour[0].volume, 20.0);
    assert!(two_hour[1].time > two_hour[0].time);
}

#[test]
fn two_hour_freshness_uses_the_existing_four_hour_policy() {
    assert_eq!(
        freshness_threshold_seconds("BTC-USD", "2h"),
        freshness_threshold_seconds("BTC-USD", "4h")
    );
}

#[test]
fn dss_annotation_requires_all_fresh_product_timeframes_and_preserves_native_source() {
    let connection = dss_market_connection();
    insert_dss_history(&connection, None);

    let diagnostic =
        build_dss_bressert_mtf_annotation(&connection, "BTC-IDR", "tokocrypto", "tokocrypto_spot");

    assert_eq!(diagnostic["status"], "CURRENT", "{diagnostic}");
    assert_ne!(diagnostic["bias"], "UNAVAILABLE");
    assert!(diagnostic["score"].is_number());
    assert_eq!(diagnostic["timeframes"].as_array().unwrap().len(), 6);
    assert_eq!(diagnostic["timeframes"][0]["data_quality"], "APP_NATIVE");
    assert_eq!(diagnostic["timeframes"][0]["source"], "tokocrypto_native");
}

#[test]
fn stale_timeframe_withholds_the_combined_dss_bias() {
    let connection = dss_market_connection();
    insert_dss_history(&connection, Some("15m"));

    let diagnostic =
        build_dss_bressert_mtf_annotation(&connection, "BTC-IDR", "tokocrypto", "tokocrypto_spot");

    assert_eq!(diagnostic["status"], "STALE");
    assert_eq!(diagnostic["bias"], "UNAVAILABLE");
    assert!(diagnostic["score"].is_null());
    let stale_frame = diagnostic["timeframes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|frame| frame["timeframe"] == "15m")
        .unwrap();
    assert_eq!(stale_frame["availability"], "STALE");
}

#[test]
fn missing_product_history_returns_explicit_incomplete_frames() {
    let connection = dss_market_connection();

    let diagnostic =
        build_dss_bressert_mtf_annotation(&connection, "BTC-IDR", "tokocrypto", "tokocrypto_spot");

    assert_eq!(diagnostic["status"], "INCOMPLETE");
    assert_eq!(diagnostic["bias"], "UNAVAILABLE");
    assert!(diagnostic["score"].is_null());
    assert!(diagnostic["timeframes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|frame| frame["availability"] == "NO_HISTORY"));
}
