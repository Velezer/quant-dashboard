use super::*;

#[test]
fn native_backfill_cursor_keeps_venue_host_and_requests_only_older_bars() {
    let mut symbol = VenueSymbol { symbol:"BTC_USDT".into(), symbol_type:1,base_asset:"BTC".into(),quote_asset:"USDT".into(),spot_trading_enabled:true };
    let url = historical_kline_url(&symbol,"1h",123456).unwrap();
    assert!(url.starts_with(MAIN_KLINES_URL));
    assert!(url.contains("symbol=BTCUSDT"));
    assert!(url.ends_with("limit=1000&endTime=123455"));
    symbol.symbol_type = 3;
    let next = historical_kline_url(&symbol,"1h",123456).unwrap();
    assert!(next.starts_with(NEXT_KLINES_URL));
    assert!(next.contains("symbol=BTC_USDT"));
    assert!(historical_kline_url(&symbol,"1h",0).is_err());
    symbol.symbol_type = 2;
    assert!(historical_kline_url(&symbol,"1h",123456).is_err());
}
