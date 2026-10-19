use super::*;

#[test]
fn parses_documented_symbol_wrapper_and_routes_type_one() {
    let body = br#"{"code":0,"msg":"success","data":{"list":[{"type":1,"symbol":"BTC_USDT","baseAsset":"BTC","quoteAsset":"USDT","spotTradingEnable":1}]}}"#;
    let symbols = parse_symbols(body).unwrap();
    assert_eq!(symbols.len(), 1);
    assert!(symbols[0].spot_trading_enabled);
    assert_eq!(
        kline_url(&symbols[0], "15m", 500).unwrap(),
        "https://www.tokocrypto.site/api/v3/klines?symbol=BTCUSDT&interval=15m&limit=500"
    );
}

#[test]
fn accepts_nextme_listing_without_optional_spot_flag() {
    let body = br#"{"code":0,"data":{"list":[{"type":3,"symbol":"TSLA_USDT","baseAsset":"TSLA","quoteAsset":"USDT"}]}}"#;
    let symbols = parse_symbols(body).unwrap();
    assert!(symbols[0].spot_trading_enabled);
    assert_eq!(
        kline_url(&symbols[0], "1h", 10).unwrap(),
        "https://cloudme-toko.2meta.app/api/v1/klines?symbol=TSLA_USDT&interval=1h&limit=10"
    );
}

#[test]
fn missing_spot_flag_on_other_symbol_types_is_not_assumed_enabled() {
    let body = br#"{"code":0,"data":{"list":[{"type":"2","symbol":"ABC_IDR","baseAsset":"ABC","quoteAsset":"IDR"}]}}"#;
    let symbols = parse_symbols(body).unwrap();
    assert!(!symbols[0].spot_trading_enabled);
    assert_eq!(symbols[0].symbol_type, 2);
}

#[test]
fn parses_international_listing_names_without_treating_them_as_supported_tickers() {
    let body = r#"{"code":0,"data":{"list":[{"type":1,"symbol":"币安人生_USDT","baseAsset":"币安人生","quoteAsset":"USDT","spotTradingEnable":1}]}}"#.as_bytes();
    let symbols = parse_symbols(body).unwrap();
    assert_eq!(symbols[0].base_asset, "币安人生");
    assert!(kline_url(&symbols[0], "1d", 10).is_err());
}

#[test]
fn rejects_duplicate_symbols_and_invalid_spot_flags() {
    let duplicate = br#"{"list":[{"type":1,"symbol":"BTC_IDR","baseAsset":"BTC","quoteAsset":"IDR","spotTradingEnable":1},{"type":1,"symbol":"BTC_IDR","baseAsset":"BTC","quoteAsset":"IDR","spotTradingEnable":1}]}"#;
    assert!(parse_symbols(duplicate)
        .unwrap_err()
        .contains("repeats BTC_IDR"));
    let invalid = br#"{"list":[{"type":1,"symbol":"BTC_IDR","baseAsset":"BTC","quoteAsset":"IDR","spotTradingEnable":"unknown"}]}"#;
    assert!(parse_symbols(invalid)
        .unwrap_err()
        .contains("invalid spotTradingEnable"));
}

#[test]
fn parses_completed_bars_and_drops_current_bar() {
    let body = br#"{"code":0,"data":[[1000,"2","3","1","2.5","4",1999,"0",0,"0","0","0"],[2000,"2.5","3","2","2.8","5",2999,"0",0,"0","0","0"]]}"#;
    let bars = parse_klines(body, 2, 2500).unwrap();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].open_time_ms, 1000);
    assert_eq!(bars[0].volume, 4.0);
}

#[test]
fn rejects_duplicate_or_invalid_bars() {
    let duplicate = br#"[[1000,"2","3","1","2.5","4",1999,"0",0,"0","0","0"],[1000,"2","3","1","2.5","4",1999,"0",0,"0","0","0"]]"#;
    assert!(parse_klines(duplicate, 2, 3000).is_err());
    let invalid = br#"[[1000,"2","1","1","2.5","4",1999,"0",0,"0","0","0"]]"#;
    assert!(parse_klines(invalid, 1, 3000).is_err());
}

#[test]
fn reports_rate_limit_retry_after() {
    assert_eq!(
        public_provider_status_error(429, Some(30)).as_deref(),
        Some("Public provider HTTP 429 rate limit; Retry-After: 30 seconds")
    );
}

#[test]
fn rejects_provider_errors_and_accepts_success_status() {
    assert_eq!(
        public_provider_status_error(418, None).as_deref(),
        Some("Public provider HTTP 418 rate limit")
    );
    assert_eq!(
        public_provider_status_error(503, None).as_deref(),
        Some("Public provider HTTP 503")
    );
    assert_eq!(public_provider_status_error(200, None), None);
}
