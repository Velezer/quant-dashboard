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

#[test]
fn stockbit_forecast_universe_includes_only_active_regular_idx_listings() {
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
         );"
    ).unwrap();
    for (instrument_id, product, category, active, status) in [
        ("stockbit.stockbit_regular.bbca", "stockbit_regular", "idx_stock", true, "public_listed_snapshot"),
        ("stockbit.stockbit_regular.pending", "stockbit_regular", "idx_stock", true, "reconcile_pending"),
        ("stockbit.stockbit_regular.inactive", "stockbit_regular", "idx_stock", false, "public_listed_snapshot"),
        ("stockbit.other.bbca", "other_product", "idx_stock", true, "public_listed_snapshot"),
        ("tokocrypto.tokenized_stock.tsla", "tokocrypto_tokenized_stock", "tokenized_stock", true, "venue_active"),
    ] {
        let symbol = instrument_id.rsplit('.').next().unwrap().to_ascii_uppercase();
        connection.execute(
            "INSERT INTO app.instruments VALUES(?1,'stockbit',?2,?3,?4,?4,1,?5,?4,'IDR',?4,'research_proxy',0,?6)",
            params![instrument_id, product, category, symbol, status, active],
        ).unwrap();
    }
    let instruments = store::verified_stockbit_gainer_universe(&connection, UNIVERSE_LIMIT).unwrap();
    assert_eq!(instruments.len(), 1);
    assert_eq!(instruments[0]["instrument_id"], "stockbit.stockbit_regular.bbca");
}

fn candidate(symbol: &str, forecast_return_pct: f64) -> Candidate {
    Candidate {
        instrument_id: format!("tokocrypto.spot.{symbol}"),
        symbol: symbol.to_owned(),
        venue_symbol: format!("{symbol}IDR"),
        quote_currency: "IDR".to_owned(),
        forecast_return_pct,
        source_family: SOURCE,
        data_provider: "tokocrypto",
        price_basis: "venue_native",
        reference_bar_time: 3 * FOUR_HOUR_NS / NANOS_PER_SECOND,
        reference_close: 110.0,
    }
}

#[test]
fn ranking_keeps_the_coverage_gate_and_returns_the_best_nonpositive_projection() {
    let cutoff = 3 * FOUR_HOUR_NS;
    let insufficient = forecast_payload(
        ForecastScope::TokocryptoSpot,
        cutoff,
        20,
        Coverage {
            forecasted: 15,
            missing_or_stale: 5,
            ..Coverage::default()
        },
        vec![candidate("LOW", 2.0), candidate("HIGH", 50.0)],
    );
    assert_eq!(insufficient["minimum_forecasted_count"], 16);
    assert_eq!(insufficient["status"], "INSUFFICIENT_COVERAGE");
    assert_eq!(insufficient["candidates"], json!([]));
    assert_eq!(insufficient["provisional_candidate"]["symbol"], "HIGH");
    assert_eq!(insufficient["provisional_candidate"]["forecast_return_pct"], 50.0);

    let sufficient = || Coverage {
        forecasted: 10,
        ..Coverage::default()
    };
    let nonpositive = forecast_payload(
        ForecastScope::TokocryptoSpot,
        cutoff,
        10,
        sufficient(),
        vec![candidate("DOWN", -2.5), candidate("FLAT", 0.0)],
    );
    assert_eq!(nonpositive["status"], "READY");
    assert_eq!(nonpositive["candidates"][0]["symbol"], "FLAT");
    assert_eq!(nonpositive["candidates"][0]["forecast_return_pct"], 0.0);
    assert_eq!(nonpositive["candidates"][1]["symbol"], "DOWN");
    assert_eq!(nonpositive["candidates"][1]["forecast_return_pct"], -2.5);
    let ranked = forecast_payload(
        ForecastScope::TokocryptoSpot,
        cutoff,
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
    assert!(ranked["provisional_candidate"].is_null());
    assert_eq!(ranked["validation_status"], "UNVALIDATED_RESEARCH");
    assert_eq!(ranked["as_of"], cutoff / NANOS_PER_SECOND);
    assert_eq!(
        ranked["forecast_until"],
        (cutoff + FORECAST_HORIZON_NS) / NANOS_PER_SECOND
    );
    assert_eq!(
        ranked["forecast_target"],
        "Next 24h close-to-close return (%)"
    );
    assert_eq!(ranked["forecast_horizon_seconds"], 24 * 60 * 60);
    assert_eq!(ranked["model"], "Naive 24h momentum extrapolation");
    let symbols = ranked["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["symbol"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(symbols, ["A", "Z", "E", "D", "C"]);
}

#[test]
fn stockbit_payload_uses_its_own_session_target_and_proxy_identity() {
    let cutoff = 20 * NANOS_PER_DAY;
    let payload = forecast_payload(
        ForecastScope::StockbitIdx,
        cutoff,
        10,
        Coverage { forecasted: 10, ..Coverage::default() },
        vec![Candidate {
            instrument_id: "stockbit.stockbit_regular.bbca".to_owned(),
            symbol: "BBCA".to_owned(),
            venue_symbol: "BBCA.JK".to_owned(),
            quote_currency: "IDR".to_owned(),
            forecast_return_pct: 3.5,
            source_family: STOCKBIT_SOURCE,
            data_provider: "yfinance",
            price_basis: "yahoo_unadjusted",
            reference_bar_time: cutoff - NANOS_PER_DAY,
            reference_close: 10_000.0,
        }],
    );
    assert_eq!(payload["status"], "READY");
    assert_eq!(payload["scope_id"], "stockbit_idx");
    assert_eq!(payload["target_kind"], "next_idx_session");
    assert!(payload["forecast_until"].is_null());
    assert!(payload["forecast_horizon_seconds"].is_null());
    assert_eq!(payload["source_family"], STOCKBIT_SOURCE);
    assert_eq!(payload["data_provider"], "yfinance");
    assert_eq!(payload["price_basis"], "yahoo_unadjusted");
    assert_eq!(payload["candidates"][0]["forecast_return_pct"], 3.5);
}

#[test]
fn stockbit_momentum_uses_two_completed_daily_closes_and_rejects_stale_history() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(
        "CREATE TABLE bars (
            symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,ts INTEGER NOT NULL,
            open REAL NOT NULL,high REAL NOT NULL,low REAL NOT NULL,close REAL NOT NULL,
            volume REAL NOT NULL,PRIMARY KEY(symbol,interval,source,ts));
         CREATE TABLE series (
            symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,
            timezone TEXT NOT NULL,PRIMARY KEY(symbol,interval,source));"
    ).unwrap();
    let cutoff = 12 * NANOS_PER_DAY;
    connection.execute(
        "INSERT INTO series VALUES('BBCA.JK','1d',?1,'Asia/Jakarta')",
        [STOCKBIT_SOURCE],
    ).unwrap();
    for (day, close) in [(10_i64, 100.0), (11_i64, 110.0)] {
        let timestamp = day * NANOS_PER_DAY;
        connection.execute(
            "INSERT INTO bars VALUES('BBCA.JK','1d',?1,?2,?3,?3,?3,?3,1000)",
            params![STOCKBIT_SOURCE, timestamp, close],
        ).unwrap();
    }
    let InstrumentForecast::Forecasted(forecast) =
        forecast_stockbit_history(&connection, "BBCA.JK", cutoff).unwrap()
    else {
        panic!("two fresh daily closes should forecast");
    };
    assert!((forecast.return_pct - 10.0).abs() < 1e-9);
    assert_eq!(forecast.reference_bar_time, 11 * NANOS_PER_DAY);
    assert_eq!(forecast.reference_close, 110.0);

    assert!(matches!(
        forecast_stockbit_history(&connection, "BBCA.JK", 17 * NANOS_PER_DAY).unwrap(),
        InstrumentForecast::MissingOrStale
    ));
}

#[test]
fn stockbit_momentum_uses_chart_derived_daily_closes_from_hourly_history() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(
        "CREATE TABLE bars (
            symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,ts INTEGER NOT NULL,
            open REAL NOT NULL,high REAL NOT NULL,low REAL NOT NULL,close REAL NOT NULL,
            volume REAL NOT NULL,PRIMARY KEY(symbol,interval,source,ts));
         CREATE TABLE series (
            symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,
            timezone TEXT NOT NULL,exchange_id TEXT NOT NULL,product_id TEXT NOT NULL,
            instrument_id TEXT NOT NULL,data_provider TEXT NOT NULL,price_basis TEXT NOT NULL,
            PRIMARY KEY(symbol,interval,source));
         INSERT INTO series VALUES(
            'BBCA.JK','1h','stockbit_regular_yfinance_fallback','Asia/Jakarta',
            'stockbit','stockbit_regular','stockbit.stockbit_regular.bbca','yfinance','yahoo_unadjusted');"
    ).unwrap();
    let cutoff = 13 * NANOS_PER_DAY
        - quant_core::JAKARTA_UTC_OFFSET_SECONDS * NANOS_PER_SECOND;
    let jakarta_offset = quant_core::JAKARTA_UTC_OFFSET_SECONDS * NANOS_PER_SECOND;
    let hour_ns = 3_600 * NANOS_PER_SECOND;
    for (day, close) in [(10_i64, 100.0), (11_i64, 110.0)] {
        for hour in [9_i64, 15_i64] {
            let timestamp = day * NANOS_PER_DAY + hour * hour_ns - jakarta_offset;
            connection.execute(
                "INSERT INTO bars VALUES('BBCA.JK','1h',?1,?2,?3,?3,?3,?3,1000)",
                params![STOCKBIT_SOURCE, timestamp, close],
            ).unwrap();
        }
    }

    let InstrumentForecast::Forecasted(forecast) =
        forecast_stockbit_history(&connection, "BBCA.JK", cutoff).unwrap()
    else {
        panic!("hourly history should roll into two fresh daily closes");
    };
    assert!(forecast_source_metadata_matches(
        &connection,
        ForecastScope::StockbitIdx,
        "BBCA.JK",
        "stockbit.stockbit_regular.bbca",
    ).unwrap());
    assert!((forecast.return_pct - 10.0).abs() < 1e-9);
    assert_eq!(
        forecast.reference_bar_time,
        11 * NANOS_PER_DAY - jakarta_offset
    );
    assert_eq!(forecast.reference_close, 110.0);
}

#[test]
fn stockbit_outcome_uses_latest_version_of_next_observed_session_bar() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(
        "CREATE TABLE bars (
            symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,ts INTEGER NOT NULL,
            open REAL NOT NULL,high REAL NOT NULL,low REAL NOT NULL,close REAL NOT NULL,
            volume REAL NOT NULL,PRIMARY KEY(symbol,interval,source,ts));
         CREATE TABLE series (
            symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,
            timezone TEXT NOT NULL,PRIMARY KEY(symbol,interval,source));
         INSERT INTO series VALUES('BBCA.JK','1d','stockbit_regular_yfinance_fallback','Asia/Jakarta');"
    ).unwrap();
    let reference_time = 11 * SECONDS_PER_DAY;
    let target_time = 12 * SECONDS_PER_DAY;
    let hour_seconds = HOUR_NS / NANOS_PER_SECOND;
    for (timestamp, close) in [
        (target_time, 120.0),
        (target_time + hour_seconds, 125.0),
        (target_time + SECONDS_PER_DAY, 130.0),
    ] {
        connection.execute(
            "INSERT INTO bars VALUES('BBCA.JK','1d',?1,?2,?3,?3,?3,?3,1000)",
            params![STOCKBIT_SOURCE, timestamp * NANOS_PER_SECOND, close],
        ).unwrap();
    }

    let target = next_stockbit_target_candle(
        &connection,
        "BBCA.JK",
        reference_time,
        target_time + SECONDS_PER_DAY + 2 * hour_seconds,
    ).unwrap().unwrap();
    assert_eq!(
        target,
        (
            (target_time + hour_seconds) * NANOS_PER_SECOND,
            125.0,
        ),
    );
    assert_eq!(
        next_stockbit_target_candle(
            &connection,
            "BBCA.JK",
            reference_time,
            target_time - 1,
        ).unwrap(),
        None,
    );
}

#[test]
fn stockbit_outcomes_roll_up_hourly_history_and_wait_for_session_close() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(
        "CREATE TABLE bars (
            symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,ts INTEGER NOT NULL,
            open REAL NOT NULL,high REAL NOT NULL,low REAL NOT NULL,close REAL NOT NULL,
            volume REAL NOT NULL,PRIMARY KEY(symbol,interval,source,ts));
         CREATE TABLE series (
            symbol TEXT NOT NULL,interval TEXT NOT NULL,source TEXT NOT NULL,
            timezone TEXT NOT NULL,exchange_id TEXT NOT NULL,product_id TEXT NOT NULL,
            instrument_id TEXT NOT NULL,data_provider TEXT NOT NULL,price_basis TEXT NOT NULL,
            PRIMARY KEY(symbol,interval,source));
         INSERT INTO series VALUES(
            'BBCA.JK','1h','stockbit_regular_yfinance_fallback','Asia/Jakarta',
            'stockbit','stockbit_regular','stockbit.stockbit_regular.bbca','yfinance','yahoo_unadjusted');"
    ).unwrap();
    let jakarta_offset = quant_core::JAKARTA_UTC_OFFSET_SECONDS * NANOS_PER_SECOND;
    let hour_ns = 3_600 * NANOS_PER_SECOND;
    for (day, close) in [(10_i64, 100.0), (11_i64, 110.0)] {
        for hour in [9_i64, 15_i64] {
            let timestamp = day * NANOS_PER_DAY + hour * hour_ns - jakarta_offset;
            connection.execute(
                "INSERT INTO bars VALUES('BBCA.JK','1h',?1,?2,?3,?3,?3,?3,1000)",
                params![STOCKBIT_SOURCE, timestamp, close],
            ).unwrap();
        }
    }
    let reference_time = (10 * NANOS_PER_DAY - jakarta_offset) / NANOS_PER_SECOND;
    let target_time = 11 * NANOS_PER_DAY - jakarta_offset;

    assert_eq!(
        next_stockbit_target_candle(
            &connection,
            "BBCA.JK",
            reference_time,
            (target_time + 15 * 60 * 60 * NANOS_PER_SECOND) / NANOS_PER_SECOND,
        ).unwrap(),
        None,
    );
    assert_eq!(
        next_stockbit_target_candle(
            &connection,
            "BBCA.JK",
            reference_time,
            (target_time + 17 * 60 * 60 * NANOS_PER_SECOND) / NANOS_PER_SECOND,
        ).unwrap(),
        Some((target_time, 110.0)),
    );
}

#[test]
fn momentum_forecast_projects_the_prior_24h_percent_return() {
    let closes = [100.0, 101.0, 102.0, 104.0, 106.0, 108.0, 110.0];
    let bars = closes
        .into_iter()
        .enumerate()
        .map(|(index, close)| StrategyBar {
            time: index as i64 * FOUR_HOUR_NS,
            open: close,
            high: close,
            low: close,
            close,
            volume: 1.0,
        })
        .collect::<Vec<_>>();
    assert!((forecast_return_pct(&bars, 6).unwrap() - 10.0).abs() < f64::EPSILON * 100.0);
    assert_eq!(forecast_return_pct(&bars, 7), None);
}
