use super::*;

fn wib_timestamp(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    let local = chrono::NaiveDate::from_ymd_opt(year, month, day)
        .unwrap()
        .and_hms_opt(hour, minute, 0)
        .unwrap();
    local.and_utc().timestamp() - quant_core::JAKARTA_UTC_OFFSET_SECONDS
}

fn local_day(year: i32, month: u32, day: u32) -> i64 {
    let midnight = chrono::NaiveDate::from_ymd_opt(year, month, day)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    (midnight.and_utc().timestamp() - quant_core::JAKARTA_UTC_OFFSET_SECONDS)
        .saturating_add(quant_core::JAKARTA_UTC_OFFSET_SECONDS)
        .div_euclid(quant_core::SECONDS_PER_DAY)
}

#[test]
fn stockbit_close_waits_for_the_1600_bar_to_finish() {
    let friday_1614 = wib_timestamp(2026, 10, 30, 16, 14);
    let friday_1615 = wib_timestamp(2026, 10, 30, 16, 15);
    let day = local_day(2026, 10, 30);

    assert_eq!(expected_stockbit_closed_session_day(friday_1614), None);
    assert_eq!(expected_stockbit_closed_session_day(friday_1615), Some(day));

    let final_bar = wib_timestamp(2026, 10, 30, 16, 0) * NANOS_PER_SECOND;
    let earlier_bar = wib_timestamp(2026, 10, 30, 15, 45) * NANOS_PER_SECOND;
    assert!(!is_final_stockbit_close_bar(final_bar, day, friday_1614));
    assert!(is_final_stockbit_close_bar(final_bar, day, friday_1615));
    assert!(!is_final_stockbit_close_bar(earlier_bar, day, friday_1615));
    assert!(!is_final_stockbit_close_bar(
        final_bar,
        day - 1,
        friday_1615
    ));
}

#[test]
fn weekends_only_pause_after_fridays_final_candle_exists() {
    let friday = local_day(2026, 10, 30);
    let saturday = wib_timestamp(2026, 10, 31, 9, 0);
    let sunday = wib_timestamp(2026, 11, 1, 9, 0);
    let monday_before_close = wib_timestamp(2026, 11, 2, 16, 14);
    let monday_after_close = wib_timestamp(2026, 11, 2, 16, 15);

    assert_eq!(expected_stockbit_closed_session_day(saturday), Some(friday));
    assert_eq!(expected_stockbit_closed_session_day(sunday), Some(friday));
    assert_eq!(expected_stockbit_closed_session_day(monday_before_close), None);
    assert_eq!(
        expected_stockbit_closed_session_day(monday_after_close),
        Some(local_day(2026, 11, 2))
    );
}
