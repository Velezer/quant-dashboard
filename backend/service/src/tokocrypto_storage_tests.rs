use super::*;

fn storage_fixture() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app")
        .unwrap();
    connection
        .execute_batch(
            "CREATE TABLE instruments(
                instrument_id TEXT PRIMARY KEY,exchange_id TEXT,product_id TEXT,
                asset_category_id TEXT,symbol TEXT,venue_symbol TEXT,underlying_symbol TEXT,
                storage_symbol TEXT,quote_currency TEXT,listing_status TEXT,venue_verified INTEGER,
                execution_supported INTEGER,registration_source TEXT,source_url TEXT,
                source_snapshot_at TEXT,price_source_policy TEXT,contract_metadata_json TEXT,
                registered_at INTEGER,updated_at INTEGER,is_active INTEGER);
             CREATE TABLE series(
                symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,
                exchange_id TEXT NOT NULL,product_id TEXT,instrument_id TEXT,
                data_provider TEXT NOT NULL,timezone TEXT NOT NULL,price_basis TEXT NOT NULL,
                instrument_kind TEXT NOT NULL,last_success TEXT,last_attempt TEXT,last_error TEXT,
                PRIMARY KEY(symbol,interval,source));
             CREATE TABLE bars(
                symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,ts INTEGER NOT NULL,
                open REAL NOT NULL,high REAL NOT NULL,low REAL NOT NULL,close REAL NOT NULL,
                volume REAL NOT NULL,adj_close REAL,dividends REAL,splits REAL,ingested_at TEXT NOT NULL,
                PRIMARY KEY(symbol,interval,source,ts));
             CREATE TABLE ingestion_runs(
                id INTEGER PRIMARY KEY,symbol TEXT,interval TEXT,source TEXT,exchange_id TEXT,
                product_id TEXT,data_provider TEXT,attempted_at TEXT,status TEXT,
                rows_received INTEGER,error TEXT);",
        )
        .unwrap();
    crate::master_data::initialize(&connection)
        .expect("test database should initialize canonical reference data");
    connection
}

#[test]
fn tokenized_selection_prefers_the_b_suffixed_venue_asset() {
    let symbols = vec![
        VenueSymbol {
            symbol: "MSTR_USDT".into(),
            symbol_type: 1,
            base_asset: "MSTR".into(),
            quote_asset: "USDT".into(),
            spot_trading_enabled: true,
        },
        VenueSymbol {
            symbol: "MSTRB_USDT".into(),
            symbol_type: 1,
            base_asset: "MSTRB".into(),
            quote_asset: "USDT".into(),
            spot_trading_enabled: true,
        },
    ];
    let (selected, tokenized) = select_symbol(&symbols, "MSTR", true).unwrap();
    assert!(tokenized);
    assert_eq!(selected.symbol, "MSTRB_USDT");
}

#[test]
fn spot_selection_prefers_idr_and_falls_back_to_usdt() {
    let _connection = storage_fixture();
    let markets = vec![
        VenueSymbol {
            symbol: "BTC_USDT".into(),
            symbol_type: 1,
            base_asset: "BTC".into(),
            quote_asset: "USDT".into(),
            spot_trading_enabled: true,
        },
        VenueSymbol {
            symbol: "BTC_IDR".into(),
            symbol_type: 3,
            base_asset: "BTC".into(),
            quote_asset: "IDR".into(),
            spot_trading_enabled: true,
        },
    ];
    let (selected, tokenized) = select_symbol(&markets, "BTC", false).unwrap();
    assert!(!tokenized);
    assert_eq!(selected.symbol, "BTC_IDR");

    let usdt_only = vec![markets[0].clone()];
    let (selected, tokenized) = select_symbol(&usdt_only, "BTC", false).unwrap();
    assert!(!tokenized);
    assert_eq!(selected.symbol, "BTC_USDT");
}

#[test]
fn spot_selection_accepts_main_next_and_nextme_types() {
    let _connection = storage_fixture();
    for symbol_type in [1, 2, 3] {
        let market = VenueSymbol {
            symbol: format!("ABC_{symbol_type}"),
            symbol_type,
            base_asset: "ABC".into(),
            quote_asset: "IDR".into(),
            spot_trading_enabled: true,
        };
        let (selected, tokenized) = select_symbol(&[market], "ABC", false).unwrap();
        assert!(!tokenized);
        assert_eq!(selected.symbol_type, symbol_type);
    }
}

#[test]
fn persists_native_bar_rows_without_a_file_payload() {
    let connection = storage_fixture();
    let bars = vec![MarketBar {
        open_time_ms: 1_700_000_000_000,
        open: 10.0,
        high: 11.0,
        low: 9.0,
        close: 10.5,
        volume: 3.0,
    }];
    persist_series(
        &connection,
        "TOKOCRYPTO:SPOT:BTC_IDR",
        "15m",
        "crypto_spot",
        &bars,
    )
    .unwrap();
    let row: (String, i64, f64, String) = connection
        .query_row(
            "SELECT s.source,b.ts,b.close,s.data_provider FROM series s
              JOIN bars b ON b.symbol=s.symbol AND b.interval=s.interval AND b.source=s.source
             WHERE s.symbol='TOKOCRYPTO:SPOT:BTC_IDR' AND s.interval='15m'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(row.0, SOURCE);
    assert_eq!(row.1, 1_700_000_000_000_000_000);
    assert_eq!(row.2, 10.5);
    assert_eq!(row.3, "tokocrypto");
}

#[test]
fn hourly_failure_preserves_successfully_fetched_fifteen_minute_candles() {
    let connection = storage_fixture();
    let symbol = "TOKOCRYPTO:SPOT:BTC_USDT";
    connection.execute(
        "INSERT INTO instruments(instrument_id,exchange_id,product_id,asset_category_id,storage_symbol,listing_status,venue_verified,is_active)
         VALUES('btc','tokocrypto','tokocrypto_spot','crypto_spot',?1,'venue_active',1,1)", [symbol],
    ).unwrap();
    let venue = VenueSymbol {
        symbol: "BTC_USDT".into(),
        symbol_type: 1,
        base_asset: "BTC".into(),
        quote_asset: "USDT".into(),
        spot_trading_enabled: true,
    };
    let result =
        sync_intraday_market_with_fetch(&connection, &venue, false, symbol, |_, interval, _| {
            if interval == "1h" {
                return Err("hourly request failed".into());
            }
            Ok(vec![MarketBar {
                open_time_ms: 1_700_000_000_000,
                open: 10.0,
                high: 11.0,
                low: 9.0,
                close: 10.5,
                volume: 3.0,
            }])
        });
    assert_eq!(result.unwrap_err(), "hourly request failed");
    let rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE symbol=?1 AND interval='15m'",
            [symbol],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1);
}
