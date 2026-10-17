use super::*;

fn registry_fixture() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "ATTACH DATABASE ':memory:' AS app;
             CREATE TABLE app.instruments (
                 exchange_id TEXT NOT NULL,
                 asset_category_id TEXT NOT NULL,
                 is_active INTEGER NOT NULL,
                 listing_status TEXT NOT NULL,
                 venue_verified INTEGER NOT NULL,
                 storage_symbol TEXT NOT NULL,
                 symbol TEXT NOT NULL,
                 venue_symbol TEXT NOT NULL,
                 underlying_symbol TEXT NOT NULL
             );",
        )
        .unwrap();
    connection
}

fn insert_instrument(
    connection: &Connection,
    exchange: &str,
    category: &str,
    storage_symbol: &str,
    symbol: &str,
    venue_symbol: &str,
    verified: bool,
) {
    connection
        .execute(
            "INSERT INTO app.instruments VALUES(?1, ?2, 1, 'verified', ?3, ?4, ?5, ?6, ?5)",
            params![
                exchange,
                category,
                verified,
                storage_symbol,
                symbol,
                venue_symbol
            ],
        )
        .unwrap();
}

#[test]
fn ticker_alias_resolves_to_the_registered_storage_symbol() {
    let connection = registry_fixture();
    insert_instrument(
        &connection,
        "stockbit",
        "idx_stock",
        "BBCA.JK",
        "BBCA",
        "BBCA",
        true,
    );

    assert_eq!(
        storage_symbol_for_exchange_category(&connection, "stockbit", "idx_stock", "BBCA")
            .unwrap()
            .as_deref(),
        Some("BBCA.JK")
    );
}

#[test]
fn identical_aliases_across_products_share_one_storage_symbol() {
    let connection = registry_fixture();
    for product_storage in ["BTC-USD", "BTC-USD"] {
        insert_instrument(
            &connection,
            "tokocrypto",
            "crypto_spot",
            product_storage,
            "BTC",
            "BTC/IDR",
            true,
        );
    }

    assert_eq!(
        storage_symbol_for_exchange_category(&connection, "tokocrypto", "crypto_spot", "BTC/IDR")
            .unwrap()
            .as_deref(),
        Some("BTC-USD")
    );
}

#[test]
fn ambiguous_alias_is_rejected_instead_of_selecting_a_ticker_file() {
    let connection = registry_fixture();
    insert_instrument(
        &connection,
        "tokocrypto",
        "crypto_spot",
        "ABC-IDR",
        "ABC",
        "ABC/IDR",
        true,
    );
    insert_instrument(
        &connection,
        "tokocrypto",
        "crypto_spot",
        "ABC-USDT",
        "ABC",
        "ABC/USDT",
        true,
    );

    assert!(
        storage_symbol_for_exchange_category(&connection, "tokocrypto", "crypto_spot", "ABC")
            .is_err()
    );
}

#[test]
fn unverified_product_listing_does_not_resolve() {
    let connection = registry_fixture();
    insert_instrument(
        &connection,
        "pintu",
        "crypto_perpetual",
        "BTC-USD",
        "BTC",
        "BTCUSDT-PERP",
        false,
    );

    assert_eq!(
        storage_symbol_for_exchange_category(&connection, "pintu", "crypto_perpetual", "BTC")
            .unwrap(),
        None
    );
}

#[test]
fn research_eligibility_keeps_venue_verification_policy_in_the_registry_boundary() {
    assert_eq!(
        research_eligibility("pintu", "regulator_approved", false),
        "pending_verification"
    );
    assert_eq!(
        research_eligibility("pluang", "public_listed_snapshot", false),
        "pending_verification"
    );
    assert_eq!(research_eligibility("pintu", "verified", true), "eligible");
    assert_eq!(
        research_eligibility("stockbit", "verified", false),
        "eligible"
    );
    assert_eq!(
        research_eligibility("tokocrypto", "venue_delisted", true),
        "excluded"
    );
}
