use super::*;

const SYMBOL: &str = "SHAREDTEST-USD";
const SPOT_SOURCE: &str = "shared_crypto_spot_binance_spot_proxy";
const PERPETUAL_SOURCE: &str = "shared_crypto_perpetual_binance_usdm_futures_proxy";

fn insert_series(
    connection: &Connection,
    source: &str,
    exchange: &str,
    product: &str,
    category: &str,
    provider: &str,
) {
    let price_basis = if provider == "binance" {
        "binance_usdt_reference"
    } else {
        "venue_native"
    };
    connection
        .execute(
            "INSERT INTO series(symbol,interval,source,exchange_id,product_id,
                data_provider,timezone,price_basis,instrument_kind,last_success,last_attempt)
             VALUES(?1,'15m',?2,?3,?4,?5,'UTC',?6,?7,?8,?8)",
            params![
                SYMBOL,
                source,
                exchange,
                product,
                provider,
                price_basis,
                category,
                format_timestamp(now_secs())
            ],
        )
        .unwrap();
    let completed_open = (now_secs() as i64).div_euclid(900) * 900 - 900;
    connection
        .execute(
            "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,ingested_at)
             VALUES(?1,'15m',?2,?3,100,102,99,101,100,'test')",
            params![SYMBOL, source, completed_open * NANOS_PER_SECOND],
        )
        .unwrap();
}

fn insert_shared(connection: &Connection, source: &str, category: &str) {
    insert_series(
        connection,
        source,
        research_proxy::shared::SOURCE_EXCHANGE_ID,
        "",
        category,
        "binance",
    );
}

fn selected(connection: &Connection, exchange: &str, product: &str) -> Option<String> {
    selected_candle_source_for_product(connection, SYMBOL, exchange, product, "15m").unwrap()
}

#[test]
fn matching_spot_apps_read_one_canonical_series_and_perpetuals_use_their_own() {
    let connection = open_db_minimal(":memory:").unwrap();
    insert_shared(&connection, SPOT_SOURCE, "crypto_spot");
    for (exchange, product) in [
        ("pintu", "pintu_retail_spot"),
        ("pintu", "pintu_pro_spot"),
        ("pluang", "pluang_spot"),
        ("tokocrypto", "tokocrypto_spot"),
    ] {
        assert_eq!(
            selected(&connection, exchange, product).as_deref(),
            Some(SPOT_SOURCE)
        );
        let candles =
            load_series_candles_for_product(&connection, SYMBOL, "15m", 20, exchange, product)
                .unwrap();
        assert_eq!(candles.len(), 1);
        assert_eq!(candles[0].close, 101.0);
    }
    assert_eq!(selected(&connection, "pintu", "pintu_pro_futures"), None);
    assert_eq!(selected(&connection, "pluang", "pluang_perpetual"), None);

    insert_shared(&connection, PERPETUAL_SOURCE, "crypto_perpetual");
    for (exchange, product) in [
        ("pintu", "pintu_pro_futures"),
        ("pluang", "pluang_perpetual"),
    ] {
        assert_eq!(
            selected(&connection, exchange, product).as_deref(),
            Some(PERPETUAL_SOURCE)
        );
    }
    let stored_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE symbol=?1",
            [SYMBOL],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_rows, 2);
}

#[test]
fn fresh_exact_native_history_precedes_shared_history_and_stale_native_recovers() {
    let connection = open_db_minimal(":memory:").unwrap();
    insert_shared(&connection, SPOT_SOURCE, "crypto_spot");
    insert_series(
        &connection,
        "tokocrypto_native",
        "tokocrypto",
        "tokocrypto_spot",
        "crypto_spot",
        "tokocrypto",
    );
    assert_eq!(
        selected(&connection, "tokocrypto", "tokocrypto_spot").as_deref(),
        Some("tokocrypto_native")
    );
    assert_eq!(
        selected(&connection, "pintu", "pintu_pro_spot").as_deref(),
        Some(SPOT_SOURCE)
    );

    let stale_age = freshness_threshold_seconds(SYMBOL, "15m") + 1_800;
    connection
        .execute(
            "UPDATE bars SET ts=ts-?1 WHERE source='tokocrypto_native'",
            [stale_age * NANOS_PER_SECOND],
        )
        .unwrap();
    assert_eq!(
        selected(&connection, "tokocrypto", "tokocrypto_spot").as_deref(),
        Some(SPOT_SOURCE)
    );
}

#[test]
fn other_apps_native_history_never_becomes_a_shared_research_reference() {
    let connection = open_db_minimal(":memory:").unwrap();
    insert_series(
        &connection,
        "tokocrypto_native",
        "tokocrypto",
        "tokocrypto_spot",
        "crypto_spot",
        "tokocrypto",
    );
    assert_eq!(selected(&connection, "pintu", "pintu_pro_spot"), None);
    assert_eq!(selected(&connection, "pluang", "pluang_spot"), None);
    assert_eq!(
        selected(&connection, "tokocrypto", "tokocrypto_spot").as_deref(),
        Some("tokocrypto_native")
    );
}

#[test]
fn malformed_shared_family_category_provider_and_quote_are_ineligible() {
    for (source, category, provider, price_basis) in [
        (
            "shared_crypto_spot_binance_usdm_futures_proxy",
            "crypto_spot",
            "binance",
            "binance_usdt_reference",
        ),
        (
            SPOT_SOURCE,
            "crypto_perpetual",
            "binance",
            "binance_usdt_reference",
        ),
        (
            SPOT_SOURCE,
            "crypto_spot",
            "coinbase",
            "coinbase_usd_reference",
        ),
        (
            SPOT_SOURCE,
            "crypto_spot",
            "binance",
            "binance_idr_reference",
        ),
    ] {
        let connection = open_db_minimal(":memory:").unwrap();
        insert_series(
            &connection,
            source,
            research_proxy::shared::SOURCE_EXCHANGE_ID,
            "",
            category,
            provider,
        );
        connection
            .execute(
                "UPDATE series SET price_basis=?1 WHERE source=?2",
                params![price_basis, source],
            )
            .unwrap();
        assert_eq!(
            selected(&connection, "pintu", "pintu_pro_spot"),
            None,
            "must reject {source} with {category}/{provider}/{price_basis}"
        );
        let preferred = selected_candle_source_metadata_for_product_with_preferred_source(
            &connection,
            SYMBOL,
            "pintu",
            "pintu_pro_spot",
            "15m",
            Some(source),
        )
        .unwrap();
        assert_eq!(
            preferred, None,
            "preferred source must retain the same identity checks"
        );
    }
}

#[test]
fn preferred_source_and_chart_product_aliases_retain_category_boundaries() {
    let connection = open_db_minimal(":memory:").unwrap();
    insert_shared(&connection, SPOT_SOURCE, "crypto_spot");
    insert_shared(&connection, PERPETUAL_SOURCE, "crypto_perpetual");
    let preferred = selected_candle_source_metadata_for_product_with_preferred_source(
        &connection,
        SYMBOL,
        "pintu",
        "pintu_pro_spot",
        "15m",
        Some(PERPETUAL_SOURCE),
    )
    .unwrap();
    assert_eq!(preferred, Some((SPOT_SOURCE.into(), "binance".into())));
    for (alias, expected) in [
        ("pintu_retail_spot", SPOT_SOURCE),
        ("pintu_pro_spot", SPOT_SOURCE),
        ("pluang_spot", SPOT_SOURCE),
        ("tokocrypto_spot", SPOT_SOURCE),
        ("pintu_futures", PERPETUAL_SOURCE),
        ("pintu_pro_futures", PERPETUAL_SOURCE),
        ("pluang_perpetual", PERPETUAL_SOURCE),
    ] {
        assert_eq!(
            selected_candle_source_for_timeframe(&connection, SYMBOL, Some(alias), "15m",)
                .unwrap()
                .as_deref(),
            Some(expected),
            "wrong source for chart alias {alias}"
        );
    }
}

#[test]
fn app_only_requests_share_retail_and_pro_spot_when_category_is_unambiguous() {
    let connection = open_db_minimal(":memory:").unwrap();
    insert_shared(&connection, SPOT_SOURCE, "crypto_spot");
    insert_shared(&connection, PERPETUAL_SOURCE, "crypto_perpetual");
    let register = |product, category| {
        connection.execute(
            "INSERT INTO app.instruments(instrument_id,exchange_id,product_id,asset_category_id,
                symbol,venue_symbol,underlying_symbol,storage_symbol,quote_currency,listing_status,
                venue_verified,registration_source,price_source_policy,registered_at,updated_at)
             VALUES(?1,'pintu',?1,?2,'SHAREDTEST','SHAREDTEST','SHAREDTEST',?3,'USDT',
                'venue_active',1,'test','research_proxy',1,1)",
            params![product, category, SYMBOL],
        ).unwrap();
    };
    register("pintu_retail_spot", "crypto_spot");
    register("pintu_pro_spot", "crypto_spot");
    assert_eq!(
        selected_candle_source_for_timeframe(&connection, SYMBOL, Some("pintu"), "15m",)
            .unwrap()
            .as_deref(),
        Some(SPOT_SOURCE)
    );
    register("pintu_pro_futures", "crypto_perpetual");
    assert_eq!(
        selected_candle_source_for_timeframe(&connection, SYMBOL, Some("pintu"), "15m",).unwrap(),
        None,
        "an app-only request must not choose between Spot and Perpetual"
    );
    connection
        .execute(
            "UPDATE app.instruments SET is_active=0 WHERE instrument_id='pintu_pro_futures'",
            [],
        )
        .unwrap();
    assert_eq!(
        selected_candle_source_for_timeframe(&connection, SYMBOL, Some("pintu"), "15m",)
            .unwrap()
            .as_deref(),
        Some(SPOT_SOURCE)
    );
}

#[test]
fn shared_metadata_discloses_reference_scope_and_the_requested_venues_native_error() {
    let connection = open_db_minimal(":memory:").unwrap();
    insert_shared(&connection, PERPETUAL_SOURCE, "crypto_perpetual");
    insert_series(
        &connection,
        "pintu_futures_native",
        "pintu",
        "pintu_pro_futures",
        "crypto_perpetual",
        "pintu",
    );
    connection
        .execute(
            "UPDATE bars SET ts=ts-?1 WHERE source='pintu_futures_native'",
            [(freshness_threshold_seconds(SYMBOL, "15m") + 1_800) * NANOS_PER_SECOND],
        )
        .unwrap();
    connection.execute(
        "UPDATE series SET last_error='native rate limited' WHERE source='pintu_futures_native'", [],
    ).unwrap();
    let metadata =
        load_product_candle_metadata(&connection, SYMBOL, "pintu", "pintu_pro_futures", "15m")
            .unwrap();
    assert_eq!(metadata["shared_reference"], true);
    assert_eq!(metadata["source_is_proxy"], true);
    assert_eq!(metadata["exchange_id"], "research_reference");
    assert_eq!(metadata["app_exchange_id"], "pintu");
    assert_eq!(metadata["product_id"], "pintu_pro_futures");
    assert_eq!(metadata["native_fallback_reason"], "native rate limited");
    assert_eq!(metadata["is_stale"], false);
    let chart_metadata =
        load_series_metadata_for_exchange(&connection, SYMBOL, "15m", false, "pintu_futures")
            .unwrap();
    assert_eq!(chart_metadata["app_exchange_id"], "pintu");
    assert_eq!(
        chart_metadata["native_fallback_reason"],
        "native rate limited"
    );

    insert_shared(&connection, SPOT_SOURCE, "crypto_spot");
    let spot_metadata =
        load_product_candle_metadata(&connection, SYMBOL, "pintu", "pintu_pro_spot", "15m")
            .unwrap();
    assert_eq!(spot_metadata["shared_reference"], true);
    assert_eq!(
        spot_metadata["native_fallback_reason"],
        Value::Null,
        "a Futures failure must not be presented as a Spot fallback reason"
    );
    let spot_chart_metadata =
        load_series_metadata_for_exchange(&connection, SYMBOL, "15m", false, "pintu_pro_spot")
            .unwrap();
    assert_eq!(spot_chart_metadata["native_fallback_reason"], Value::Null);
}
