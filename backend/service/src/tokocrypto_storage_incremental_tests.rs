use super::*;

#[test]
fn intraday_refresh_bootstraps_then_fetches_only_the_gap() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE bars(symbol TEXT,interval TEXT,source TEXT,ts INTEGER);")
        .unwrap();
    assert_eq!(
        recent_candle_limit(&db, "BTC", "15m", 720, 501, 900).unwrap(),
        720
    );
    let now = crate::now_secs() as i64;
    for n in 0..501 {
        let ts = (now - 3600 - n * 900) * crate::NANOS_PER_SECOND;
        db.execute(
            "INSERT INTO bars VALUES('BTC','15m',?1,?2)",
            params![SOURCE, ts],
        )
        .unwrap();
    }
    let limit = recent_candle_limit(&db, "BTC", "15m", 720, 501, 900).unwrap();
    assert!(
        (7..=8).contains(&limit),
        "one-hour gap should be fetched with replay margin, got {limit}"
    );
    db.execute(
        "DELETE FROM bars WHERE ts=(SELECT MIN(ts)+?1 FROM bars)",
        [250_i64 * 900 * crate::NANOS_PER_SECOND],
    )
    .unwrap();
    db.execute(
        "INSERT INTO bars VALUES('BTC','15m',?1,?2)",
        params![SOURCE, (now - 3600 - 501 * 900) * crate::NANOS_PER_SECOND],
    )
    .unwrap();
    assert_eq!(
        recent_candle_limit(&db, "BTC", "15m", 720, 501, 900).unwrap(),
        720
    );
    db.execute(
        "UPDATE bars SET ts=ts-?1",
        [1000_i64 * 900 * crate::NANOS_PER_SECOND],
    )
    .unwrap();
    assert_eq!(
        recent_candle_limit(&db, "BTC", "15m", 720, 501, 900).unwrap(),
        720
    );
}

#[test]
fn hourly_refresh_accepts_the_completed_rows_from_a_full_provider_response() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE bars(symbol TEXT,interval TEXT,source TEXT,ts INTEGER);")
        .unwrap();
    let now = crate::now_secs() as i64;
    for n in 0..499 {
        db.execute(
            "INSERT INTO bars VALUES('BTC','1h',?1,?2)",
            params![SOURCE, (now - 3600 - n * 3600) * crate::NANOS_PER_SECOND],
        )
        .unwrap();
    }
    assert!((4..=5).contains(&recent_candle_limit(&db, "BTC", "1h", 500, 499, 3600).unwrap()));
}
