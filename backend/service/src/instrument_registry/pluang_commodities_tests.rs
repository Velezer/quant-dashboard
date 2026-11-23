use super::*;

#[test]
fn seeds_verified_pluang_commodities_with_proxy_only_execution_policy() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app; ATTACH DATABASE ':memory:' AS signals;")
        .unwrap();
    crate::database::initialize_app_storage_schema(&connection).unwrap();
    crate::master_data::initialize(&connection).unwrap();
    super::initialize(&connection).unwrap();

    let rows = connection
        .prepare(
            "SELECT symbol,product_id,asset_category_id,storage_symbol,quote_currency,
                    venue_verified,execution_supported,price_source_policy,registration_source,
                    source_url,contract_metadata_json
               FROM app.instruments
              WHERE exchange_id='pluang' AND product_id='pluang_commodities'
              ORDER BY symbol",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, bool>(5)?,
                row.get::<_, bool>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
        ["UKOUSD", "XAGUSD", "XAUUSD"]
    );
    for row in &rows {
        assert_eq!(row.1, "pluang_commodities");
        assert_eq!(row.2, "commodity_derivative");
        assert_eq!(row.3, format!("{}-USD", row.0));
        assert_eq!(row.4, "USD");
        assert!(row.5);
        assert!(!row.6);
        assert_eq!(row.7, "yfinance_research_proxy");
        assert_eq!(row.8, "pluang_official_commodities_catalog");
        assert!(row.9.contains("pluang.com/"));
        let metadata: Value = serde_json::from_str(&row.10).unwrap();
        assert_eq!(metadata["native_price_feed"], false);
        assert_eq!(metadata["execution_supported"], false);
        assert!(metadata["reference_symbol"].as_str().is_some());
    }
}
