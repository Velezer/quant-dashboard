use super::{
    augment_app_products, signal_screener_master_data_with_registered_products, MASTER_DATA_JSON,
};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;

#[test]
fn crypto_screener_models_include_registered_products_without_tokenized_stock() {
    let mut catalog: Value =
        serde_json::from_str(MASTER_DATA_JSON).expect("embedded master data should parse");
    augment_app_products(&mut catalog, true);

    let connection = Connection::open_in_memory().expect("app database should open");
    connection
        .execute_batch(
            "ATTACH DATABASE ':memory:' AS app;
             CREATE TABLE app.exchange_products(
                 exchange_id TEXT NOT NULL,
                 product_id TEXT NOT NULL,
                 asset_category_id TEXT NOT NULL,
                 status TEXT NOT NULL
             );
             INSERT INTO app.exchange_products VALUES
                 ('tokocrypto','tokocrypto_spot','crypto_spot','enabled'),
                 ('tokocrypto','tokocrypto_tokenized_stock','tokenized_stock','enabled'),
                 ('pintu_futures','pintu_pro_spot','crypto_perpetual','enabled');",
        )
        .expect("registered products should initialize");

    let payload = signal_screener_master_data_with_registered_products(&connection, &catalog)
        .expect("registered crypto product scope should load");
    let models = payload["signal_score_models"]
        .as_array()
        .expect("screener models should be available");
    let crypto_models = models
        .iter()
        .filter(|model| {
            model["id"] == quant_core::CRYPTO_NWE_4H_SIGNAL_STRATEGY_ID
                || model["id"] == quant_core::CRYPTO_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID
        })
        .collect::<Vec<_>>();

    assert_eq!(crypto_models.len(), 2);
    for model in crypto_models {
        let products = model["product_ids"]
            .as_array()
            .expect("model product scope should be available");
        let product_ids = products
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        let unique_product_ids = product_ids.iter().copied().collect::<HashSet<_>>();

        assert!(product_ids.contains(&"tokocrypto_spot"));
        assert!(product_ids.contains(&"pintu_pro_spot"));
        assert!(!product_ids.contains(&"tokocrypto_tokenized_stock"));
        assert_eq!(product_ids.len(), unique_product_ids.len());
    }
}
