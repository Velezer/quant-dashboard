use super::*;

#[test]
fn retail_listing_confirms_independently_and_same_day_reconciliation_delists() {
    use super::super::pintu_listing::{Listing, PRO_SPOT_URL, RETAIL_URL};

    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("ATTACH DATABASE ':memory:' AS app; ATTACH DATABASE ':memory:' AS signals;")
        .unwrap();
    crate::database::initialize_app_storage_schema(&conn).unwrap();
    conn.execute_batch(
        "CREATE TABLE ingestion_runs(
            symbol TEXT, exchange_id TEXT, source TEXT, interval TEXT, attempted_at TEXT
        )",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO instruments(instrument_id,exchange_id,product_id,asset_category_id,
                symbol,venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                listing_status,venue_verified,execution_supported,registration_source,
                price_source_policy,contract_metadata_json,registered_at,updated_at,is_active)
             VALUES('pintu.pintu_pro_futures.eth','pintu','pintu_pro_futures',
                'crypto_perpetual','ETH','ETHUSDT-PERP','ETHUSDT','ETH-USD','USDT',
                'regulator_approved',0,0,'pintu_regulator_approved_code_list',
                'research_proxy','',1,1,1)",
        [],
    )
    .unwrap();
    let retail = Listing {
        product_id: "pintu_retail_spot",
        base: "BTC".into(),
        venue_symbol: "BTC/IDR".into(),
        quote: "IDR",
        category: "crypto_spot",
        source_url: RETAIL_URL,
    };
    let pro = Listing {
        product_id: "pintu_pro_spot",
        base: "BTC".into(),
        venue_symbol: "BTC-IDR".into(),
        quote: "IDR",
        category: "crypto_spot",
        source_url: PRO_SPOT_URL,
    };
    assert!(reconcile_pintu(&conn, &[retail.clone(), pro])
        .error
        .is_none());
    let confirmed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM instruments WHERE symbol='BTC' AND venue_verified=1
                AND listing_status='venue_active'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(confirmed, 2);
    let backlog: String = conn
        .query_row(
            "SELECT value FROM instrument_registry_meta WHERE key='daily_backlog_remaining'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(backlog, "2");
    assert!(reconcile_pintu(&conn, &[retail]).error.is_none());
    let pro_status: (String, i64) = conn
        .query_row(
            "SELECT listing_status,venue_verified FROM instruments
                WHERE instrument_id='pintu.pintu_pro_spot.btc'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(pro_status, ("venue_delisted".into(), 0));
    let backlog: String = conn
        .query_row(
            "SELECT value FROM instrument_registry_meta WHERE key='daily_backlog_remaining'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(backlog, "1");
    let regulator_status: (String, i64) = conn
        .query_row(
            "SELECT listing_status,venue_verified FROM instruments
                WHERE instrument_id='pintu.pintu_pro_futures.eth'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(regulator_status, ("regulator_approved".into(), 0));
}

#[test]
fn migration_is_transactional_idempotent_and_keeps_bar_provenance() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
            CREATE TABLE instrument_registry_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE instruments(
                instrument_id TEXT PRIMARY KEY,exchange_id TEXT NOT NULL,
                asset_category_id TEXT NOT NULL,symbol TEXT NOT NULL,
                venue_symbol TEXT NOT NULL,underlying_symbol TEXT NOT NULL,
                storage_symbol TEXT NOT NULL,quote_currency TEXT NOT NULL,
                listing_status TEXT NOT NULL,venue_verified INTEGER NOT NULL,
                execution_supported INTEGER NOT NULL,registration_source TEXT NOT NULL,
                source_url TEXT,source_snapshot_at TEXT,price_source_policy TEXT NOT NULL,
                contract_metadata_json TEXT NOT NULL,registered_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,is_active INTEGER NOT NULL,
                UNIQUE(exchange_id,asset_category_id,symbol));
            CREATE TABLE series(exchange_id TEXT,source TEXT);
            CREATE TABLE ingestion_runs(exchange_id TEXT,source TEXT);
            CREATE TABLE trade_plans(id INTEGER PRIMARY KEY,plan_json TEXT);
            CREATE TABLE bars(symbol TEXT,interval TEXT,source TEXT,ts INTEGER);
            INSERT INTO instruments VALUES(
                'pintu_futures.crypto_perpetual.btc','pintu_futures','crypto_perpetual',
                'BTC','BTCUSDT-PERP','BTCUSDT','BTC-USD','USDT','regulator_approved',0,0,
                'pintu_regulator_approved_code_list',NULL,NULL,'research_proxy','',1,1,1);
            INSERT INTO series VALUES('pintu_futures','pintu_futures_binance_futures');
            INSERT INTO ingestion_runs VALUES('pintu_futures','pintu_futures_binance_futures');
            INSERT INTO bars VALUES('BTC-USD','15m','pintu_futures_binance_futures',1);
        "#,
    )
    .unwrap();
    let plan = super::super::wire::encode(&json!({
        "Exchange":"PINTU_FUTURES","Data_Symbol":"BTC-USD","Plan_ID":"old"
    }))
    .unwrap();
    conn.execute("INSERT INTO trade_plans(id,plan_json) VALUES(1,?1)", [plan])
        .unwrap();
    assert_eq!(migrate_app_products(&conn).unwrap(), 1);
    assert_eq!(migrate_app_products(&conn).unwrap(), 0);
    let identity: (String, String, String) = conn
        .query_row(
            "SELECT instrument_id,exchange_id,product_id FROM instruments",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        identity,
        (
            "pintu.pintu_pro_futures.btc".into(),
            "pintu".into(),
            "pintu_pro_futures".into()
        )
    );
    let source: String = conn
        .query_row("SELECT source FROM bars", [], |row| row.get(0))
        .unwrap();
    assert_eq!(source, "pintu_futures_binance_futures");
    let migrated: String = conn
        .query_row("SELECT plan_json FROM trade_plans", [], |row| row.get(0))
        .unwrap();
    let migrated: Value = super::super::wire::decode(&migrated).unwrap();
    assert_eq!(migrated["Exchange"], "PINTU");
    assert_eq!(migrated["Product_ID"], "pintu_pro_futures");
}
