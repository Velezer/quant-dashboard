use super::{instrument_registry_exchange_id, selected_signal_is_registered};
use rusqlite::Connection;

#[test]
fn canonical_pintu_app_id_maps_to_the_registry_app_key() {
    assert_eq!(instrument_registry_exchange_id("pintu_futures"), "pintu");
    assert_eq!(instrument_registry_exchange_id("tokocrypto"), "tokocrypto");
}

#[test]
fn selected_signal_refresh_finds_a_registered_pintu_contract() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "ATTACH DATABASE ':memory:' AS app;
             CREATE TABLE app.instruments (
                 exchange_id TEXT NOT NULL,
                 product_id TEXT NOT NULL,
                 asset_category_id TEXT NOT NULL,
                 symbol TEXT NOT NULL,
                 storage_symbol TEXT NOT NULL,
                 is_active INTEGER NOT NULL,
                 venue_verified INTEGER NOT NULL,
                 listing_status TEXT NOT NULL
             );
             INSERT INTO app.instruments VALUES (
                 'pintu', 'pintu_pro_futures', 'crypto_perpetual',
                 'BTCUSDT', 'BTC-USD', 1, 1, 'venue_active'
             );",
        )
        .unwrap();

    let registered = selected_signal_is_registered(
        &connection,
        "pintu_futures",
        "pintu_pro_futures",
        "crypto_perpetual",
        "BTCUSDT",
        "BTC-USD",
    )
    .unwrap();

    assert!(registered);
}
