use super::*;

const HOUR_NS: i64 = 3_600 * NANOS_PER_SECOND;
const FOUR_HOUR_NS: i64 = 4 * HOUR_NS;

fn hours() -> Vec<Candle> {
    (0..12)
        .map(|index| {
            let open = 100.0 + index as f64;
            new_candle(
                index * HOUR_NS,
                open,
                open + 2.0,
                open - 1.0,
                open + 1.0,
                10.0,
            )
        })
        .collect()
}

#[test]
fn complete_four_hour_buckets_preserve_ohlcv_and_stop_at_gaps() {
    let cutoff = 3 * FOUR_HOUR_NS;
    let mut native = hours();
    let result = complete_four_hour_suffix(&native, cutoff, HOUR_NS, FOUR_HOUR_NS);
    assert_eq!(result.len(), 3);
    let latest = result.last().unwrap();
    assert_eq!(latest.time, cutoff - FOUR_HOUR_NS);
    assert_eq!(
        (
            latest.open,
            latest.high,
            latest.low,
            latest.close,
            latest.volume
        ),
        (108.0, 113.0, 107.0, 112.0, 40.0),
    );
    native.remove(6);
    let result = complete_four_hour_suffix(&native, cutoff, HOUR_NS, FOUR_HOUR_NS);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].time, cutoff - FOUR_HOUR_NS);
    native.remove(8);
    assert!(complete_four_hour_suffix(&native, cutoff, HOUR_NS, FOUR_HOUR_NS).is_empty());
}

#[test]
fn older_or_invalid_hours_cannot_take_the_place_of_a_completed_bucket() {
    let cutoff = 3 * FOUR_HOUR_NS;
    let mut native = hours();
    assert!(complete_four_hour_suffix(&native[..11], cutoff, HOUR_NS, FOUR_HOUR_NS).is_empty());
    native[11].volume = f64::INFINITY;
    assert!(complete_four_hour_suffix(&native, cutoff, HOUR_NS, FOUR_HOUR_NS).is_empty());
}

#[test]
fn forecast_universe_requires_verified_active_spot_and_honors_exclusions() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(
        "ATTACH DATABASE ':memory:' AS app;
         CREATE TABLE app.instruments (
             instrument_id TEXT, exchange_id TEXT, product_id TEXT, asset_category_id TEXT,
             symbol TEXT, storage_symbol TEXT, venue_verified INTEGER, listing_status TEXT,
             venue_symbol TEXT, quote_currency TEXT, underlying_symbol TEXT, price_source_policy TEXT,
             execution_supported INTEGER, is_active INTEGER
         );
         CREATE TABLE instrument_exclusions (
             exchange_id TEXT, asset_category_id TEXT, symbol TEXT, is_active INTEGER
         );
         INSERT INTO instrument_exclusions VALUES ('tokocrypto','crypto_spot','EXCLUDED',1);"
    ).unwrap();
    for (symbol, product, category, active, verified, status) in [
        (
            "VALID",
            "tokocrypto_spot",
            "crypto_spot",
            true,
            true,
            "venue_active",
        ),
        (
            "UNVERIFIED",
            "tokocrypto_spot",
            "crypto_spot",
            true,
            false,
            "venue_active",
        ),
        (
            "INACTIVE",
            "tokocrypto_spot",
            "crypto_spot",
            false,
            true,
            "venue_active",
        ),
        (
            "DELISTED",
            "tokocrypto_spot",
            "crypto_spot",
            true,
            true,
            "venue_delisted",
        ),
        (
            "EXCLUDED",
            "tokocrypto_spot",
            "crypto_spot",
            true,
            true,
            "venue_active",
        ),
        (
            "TOKENIZED",
            "tokocrypto_tokenized_stock",
            "tokenized_stock",
            true,
            true,
            "venue_active",
        ),
    ] {
        connection
            .execute(
                "INSERT INTO app.instruments VALUES
             (?1,'tokocrypto',?2,?3,?1,?1,?4,?5,?1,'IDR',?1,'venue_native',0,?6)",
                params![symbol, product, category, verified, status, active],
            )
            .unwrap();
    }
    let instruments = store::verified_gainer_universe(&connection, UNIVERSE_LIMIT).unwrap();
    assert_eq!(instruments.len(), 1);
    assert_eq!(instruments[0]["instrument_id"], "VALID");
    assert!(store::verified_gainer_universe(&connection, 0).is_err());
}

fn candidate(symbol: &str, score: f64) -> Candidate {
    Candidate {
        instrument_id: format!("tokocrypto.spot.{symbol}"),
        symbol: symbol.to_owned(),
        venue_symbol: format!("{symbol}IDR"),
        quote_currency: "IDR".to_owned(),
        score,
        source_family: SOURCE,
    }
}

#[test]
fn rank_is_bounded_and_withheld_until_cohort_coverage_is_sufficient() {
    let cutoff = 3 * FOUR_HOUR_NS;
    let insufficient = forecast_payload(
        cutoff,
        FOUR_HOUR_NS,
        20,
        Coverage {
            scored: 15,
            missing_or_stale: 5,
            ..Coverage::default()
        },
        vec![candidate("A", 50.0)],
    );
    assert_eq!(insufficient["minimum_scored_count"], 16);
    assert_eq!(insufficient["status"], "INSUFFICIENT_COVERAGE");
    assert_eq!(insufficient["candidates"], json!([]));

    let sufficient = || Coverage {
        scored: 10,
        ..Coverage::default()
    };
    let empty = forecast_payload(cutoff, FOUR_HOUR_NS, 10, sufficient(), Vec::new());
    assert_eq!(empty["status"], "NO_POSITIVE_CANDIDATES");
    let ranked = forecast_payload(
        cutoff,
        FOUR_HOUR_NS,
        10,
        sufficient(),
        vec![
            candidate("Z", 5.0),
            candidate("A", 5.0),
            candidate("B", 1.0),
            candidate("C", 2.0),
            candidate("D", 3.0),
            candidate("E", 4.0),
        ],
    );
    assert_eq!(ranked["status"], "READY");
    assert_eq!(ranked["validation_status"], "UNVALIDATED_RESEARCH");
    assert_eq!(ranked["as_of"], cutoff / NANOS_PER_SECOND);
    assert_eq!(
        ranked["forecast_until"],
        (cutoff + FOUR_HOUR_NS) / NANOS_PER_SECOND
    );
    let symbols = ranked["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["symbol"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(symbols, ["A", "Z", "E", "D", "C"]);
}
