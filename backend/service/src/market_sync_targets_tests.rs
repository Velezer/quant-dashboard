use super::rank_intraday_targets_by_recent_volume;
use crate::main_test_support::*;
use crate::{now_millis, open_db_minimal, StaleSignalSyncPriority};
use rusqlite::params;

const FIFTEEN_MINUTES_NS: i64 = 15 * 60 * 1_000_000_000;
const VOLUME_BASELINE_BARS: i64 = 20;

type Target = (String, String, String, String, String);

fn target(symbol: &str) -> Target {
    (
        "pintu".into(),
        symbol.into(),
        "pintu_retail_spot".into(),
        symbol.into(),
        "crypto_spot".into(),
    )
}

fn insert_history(
    connection: &rusqlite::Connection,
    symbol: &str,
    prior_volume: f64,
    last_volume: f64,
    last_bar_open_ns: i64,
) {
    let source = "pintu_volume_test_proxy";
    connection
        .execute(
            "INSERT INTO series(
                 symbol,interval,source,exchange_id,product_id,data_provider,
                 timezone,price_basis,instrument_kind
             ) VALUES(?1,'15m',?2,'pintu','pintu_retail_spot','test','UTC','unadjusted','crypto')",
            params![symbol, source],
        )
        .expect("test source metadata should persist");
    for prior_index in 1..=VOLUME_BASELINE_BARS {
        let ts = last_bar_open_ns - prior_index * FIFTEEN_MINUTES_NS;
        connection
            .execute(
                "INSERT INTO bars(
                     symbol,interval,source,ts,open,high,low,close,volume,ingested_at
                 ) VALUES(?1,'15m',?2,?3,100,101,99,100,?4,'test')",
                params![symbol, source, ts, prior_volume],
            )
            .expect("prior 15m volume bar should persist");
    }
    connection
        .execute(
            "INSERT INTO bars(
                 symbol,interval,source,ts,open,high,low,close,volume,ingested_at
             ) VALUES(?1,'15m',?2,?3,100,101,99,100,?4,'test')",
            params![symbol, source, last_bar_open_ns, last_volume],
        )
        .expect("latest 15m volume bar should persist");
}

fn latest_completed_bar_open(now_ns: i64) -> i64 {
    now_ns.div_euclid(FIFTEEN_MINUTES_NS) * FIFTEEN_MINUTES_NS - FIFTEEN_MINUTES_NS
}

#[test]
fn latest_relative_15m_quote_volume_moves_active_target_to_front_of_due_batch() {
    let path = temporary_database_path("volume-ranked-sync");
    let connection = open_db_minimal(&path).expect("temporary app and market database should open");
    let now_ns = now_millis().saturating_mul(1_000_000);
    let latest_bar = latest_completed_bar_open(now_ns);
    insert_history(&connection, "NORMAL-USD", 100.0, 100.0, latest_bar);
    insert_history(&connection, "ACTIVE-USD", 100.0, 500.0, latest_bar);

    let ranked = rank_intraday_targets_by_recent_volume(
        vec![target("NORMAL-USD"), target("ACTIVE-USD")],
        &path,
        None,
        &[],
        now_ns,
    );

    assert_eq!(
        ranked.first().map(|candidate| candidate.1.as_str()),
        Some("ACTIVE-USD")
    );
    assert_eq!(
        ranked.last().map(|candidate| candidate.1.as_str()),
        Some("NORMAL-USD")
    );

    drop(connection);
    remove_temporary_database(&path);
}

#[test]
fn stale_signal_priority_stays_ahead_of_higher_volume_candidates() {
    let path = temporary_database_path("volume-ranked-stale-priority");
    let connection = open_db_minimal(&path).expect("temporary app and market database should open");
    let now_ns = now_millis().saturating_mul(1_000_000);
    let latest_bar = latest_completed_bar_open(now_ns);
    insert_history(&connection, "NORMAL-USD", 100.0, 100.0, latest_bar);
    insert_history(&connection, "ACTIVE-USD", 100.0, 500.0, latest_bar);
    let stale_priority = StaleSignalSyncPriority {
        symbol: "NORMAL-USD".into(),
        exchange_id: "pintu".into(),
        product_id: "pintu_retail_spot".into(),
    };

    let ranked = rank_intraday_targets_by_recent_volume(
        vec![target("ACTIVE-USD"), target("NORMAL-USD")],
        &path,
        None,
        &[stale_priority],
        now_ns,
    );

    assert_eq!(
        ranked.first().map(|candidate| candidate.1.as_str()),
        Some("NORMAL-USD")
    );

    drop(connection);
    remove_temporary_database(&path);
}
