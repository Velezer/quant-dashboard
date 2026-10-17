use super::*;

fn yahoo_response(timestamp: i64) -> Vec<u8> {
    yahoo_response_with_timestamps(&[timestamp])
}

fn yahoo_response_with_timestamps(timestamps: &[i64]) -> Vec<u8> {
    let count = timestamps.len();
    serde_json::json!({
        "chart": {
            "result": [{
                "timestamp": timestamps,
                "indicators": {
                    "quote": [{
                        "open": vec![100.0; count],
                        "high": vec![101.0; count],
                        "low": vec![99.0; count],
                        "close": vec![100.5; count],
                        "volume": vec![10.0; count]
                    }]
                }
            }]
        }
    })
    .to_string()
    .into_bytes()
}

fn parse_one_yahoo_candle(timestamp: i64, duration_seconds: i64) -> Result<Vec<Bar>, String> {
    parse_yahoo_candles(
        &yahoo_response(timestamp),
        timestamp + duration_seconds + 1,
        duration_seconds,
        1,
    )
}

#[test]
fn yahoo_15m_rejects_timestamps_off_the_quarter_hour() {
    let timestamp = 1_800_000_000 + 60;

    let result = parse_one_yahoo_candle(timestamp, FIFTEEN_MINUTE_INTERVAL_SECONDS);

    assert!(result.unwrap_err().contains("not aligned"));
}

#[test]
fn yahoo_15m_accepts_utc_quarter_hour_boundaries() {
    let timestamp = 1_800_000_000;

    let result = parse_one_yahoo_candle(timestamp, FIFTEEN_MINUTE_INTERVAL_SECONDS);

    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn yahoo_15m_skips_an_unaligned_incomplete_live_bar() {
    let completed = 1_800_000_000;
    let partial = completed + FIFTEEN_MINUTE_INTERVAL_SECONDS + 60;
    let now = partial + FIFTEEN_MINUTE_INTERVAL_SECONDS - 1;
    let body = yahoo_response_with_timestamps(&[completed, partial]);

    let bars = parse_yahoo_candles(&body, now, FIFTEEN_MINUTE_INTERVAL_SECONDS, 2)
        .expect("an unaligned open candle is discarded while the completed history is retained");

    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].ts, completed);
}

#[test]
fn yahoo_hourly_accepts_us_equity_half_hour_session_offset() {
    let timestamp = 1_800_000_000 + YAHOO_HOURLY_SESSION_OFFSET_SECONDS;

    let result = parse_one_yahoo_candle(timestamp, ONE_HOUR_INTERVAL_SECONDS);

    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn yahoo_hourly_rejects_irregular_and_subminute_timestamps() {
    let irregular = parse_one_yahoo_candle(1_800_000_000 + 15 * 60, ONE_HOUR_INTERVAL_SECONDS);
    let subminute = parse_one_yahoo_candle(1_800_000_000 + 1, ONE_HOUR_INTERVAL_SECONDS);

    assert!(irregular.unwrap_err().contains("not aligned"));
    assert!(subminute.unwrap_err().contains("not aligned"));
}

#[test]
fn yahoo_daily_accepts_exchange_local_session_offsets() {
    let timestamp = 1_800_000_000 + 14 * 60 * 60;

    let result = parse_one_yahoo_candle(timestamp, ONE_DAY_INTERVAL_SECONDS);

    assert_eq!(result.unwrap().len(), 1);
}
