use super::list_page;
use rusqlite::Connection;

fn registry_connection() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory app database should open");
    connection
        .execute_batch(
            "CREATE TABLE instruments(
                 instrument_id TEXT PRIMARY KEY,
                 exchange_id TEXT NOT NULL,
                 product_id TEXT NOT NULL,
                 asset_category_id TEXT NOT NULL,
                 symbol TEXT NOT NULL,
                 venue_symbol TEXT NOT NULL,
                 underlying_symbol TEXT NOT NULL,
                 storage_symbol TEXT NOT NULL,
                 quote_currency TEXT NOT NULL,
                 listing_status TEXT NOT NULL,
                 venue_verified INTEGER NOT NULL,
                 execution_supported INTEGER NOT NULL,
                 registration_source TEXT NOT NULL,
                 source_url TEXT,
                 source_snapshot_at TEXT,
                 price_source_policy TEXT NOT NULL,
                 contract_metadata_json TEXT NOT NULL,
                 registered_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 is_active INTEGER NOT NULL
             );
             CREATE INDEX idx_instruments_active ON instruments(is_active);
             INSERT INTO instruments VALUES
                 ('stockbit.bbca','stockbit','stockbit_regular','idx_stock','BBCA','BBCA.JK','BBCA','BBCA.JK','IDR','venue_active',1,0,'seed',NULL,NULL,'stored_history','{}',1,1,1),
                 ('stockbit.bca','stockbit','stockbit_regular','idx_stock','BCA','BCA.JK','BCA','BCA.JK','IDR','venue_active',1,0,'seed',NULL,NULL,'stored_history','{}',1,1,1),
                 ('tokocrypto.btc','tokocrypto','tokocrypto_spot','crypto_spot','BTC','BTCIDR','BTC','BTC-IDR','IDR','venue_active',1,0,'seed',NULL,NULL,'research_proxy','{}',1,1,1),
                 ('stockbit.old','stockbit','stockbit_regular','idx_stock','OLD','OLD.JK','OLD','OLD.JK','IDR','venue_delisted',0,0,'seed',NULL,NULL,'stored_history','{}',1,1,0);",
        )
        .expect("registry schema and fixtures should initialize");
    connection
}

#[test]
fn instrument_registry_pages_are_bounded_and_filters_preserve_counts() {
    let connection = registry_connection();
    let first = list_page(&connection, None, None, None, None, None, 0, 2)
        .expect("first registry page should load");
    assert_eq!(first["registered_count"], 3);
    assert_eq!(first["matching_count"], 3);
    assert_eq!(first["offset"], 0);
    assert_eq!(first["next_offset"], 2);
    assert_eq!(first["has_more"], true);
    assert_eq!(first["instruments"].as_array().map(Vec::len), Some(2));

    let second = list_page(&connection, None, None, None, None, None, 2, 2)
        .expect("second registry page should load");
    assert_eq!(second["offset"], 2);
    assert_eq!(second["next_offset"], 3);
    assert_eq!(second["has_more"], false);
    assert_eq!(second["instruments"].as_array().map(Vec::len), Some(1));

    let filtered = list_page(
        &connection,
        Some("stockbit"),
        Some("stockbit_regular"),
        Some("idx_stock"),
        None,
        Some("BCA"),
        0,
        500,
    )
    .expect("filtered registry page should load");
    assert_eq!(filtered["registered_count"], 3);
    assert_eq!(filtered["matching_count"], 2);
    assert_eq!(filtered["instruments"].as_array().map(Vec::len), Some(2));
}

#[test]
fn instrument_registry_page_size_is_capped() {
    let connection = registry_connection();
    let page = list_page(&connection, None, None, None, None, None, 0, usize::MAX)
        .expect("large requested page should be clamped");
    assert_eq!(page["limit"], super::MAX_PAGE_SIZE_ROWS);
}

#[test]
fn active_instrument_count_uses_the_narrow_index() {
    let connection = registry_connection();
    let plan = connection
        .prepare("EXPLAIN QUERY PLAN SELECT COUNT(*) FROM instruments WHERE is_active=1")
        .expect("active-count query plan should prepare")
        .query_map([], |row| row.get::<_, String>(3))
        .expect("active-count plan should run")
        .collect::<Result<Vec<_>, _>>()
        .expect("active-count plan should load");

    assert!(
        plan.iter()
            .any(|detail| detail.contains("COVERING INDEX idx_instruments_active")),
        "expected the narrow active-row index, got {plan:?}"
    );
}
