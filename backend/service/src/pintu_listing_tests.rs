use super::*;
use serde_json::{json, Map};

#[test]
fn partial_or_malformed_listing_is_rejected_before_reconciliation() {
    assert!(parse_retail(br#"{"code":"success","payload":[{"pair":"BTC/IDR"}]}"#).is_err());
    assert!(parse_pro_spot(br#"{"code":0,"data":{"symbols":[]}}"#).is_err());
    assert!(parse_pro_futures(
        br#"{"code":0,"data":{"tickers":{
        "BTCUSDT-PX":{"symbol":"BTCUSDT-PX"}}}}"#
    )
    .is_err());
}

#[test]
fn official_delistings_do_not_reenter_registry_from_the_ticker_feed() {
    // First, middle, and last examples from the two March notices, plus OM
    // and a contract from the February notice.
    let delisted = [
        "MBOXUSDT-PERP", "DYDXUSDT-PERP", "JASMYUSDT-PERP", "ARUSDT-PERP",
        "ALTUSDT-PERP", "1INCHUSDT-PERP", "USTCUSDT-PERP", "OMUSDT-PERP",
        "OXTUSDT-PERP", "ALPHAUSDT-PERP", "HIFIUSDT-PERP", "BAKEUSDT-PERP",
        "FLMUSDT-PERP", "PERPUSDT-PERP", "BEAMXUSDT-PERP", "GHSTUSDT-PERP",
        "QUICKUSDT-PERP", "SXPUSDT-PERP", "VOXELUSDT-PERP",
    ];
    let mut tickers = Map::new();
    for symbol in delisted {
        tickers.insert(symbol.to_string(), json!({"symbol": symbol}));
    }
    tickers.insert("BTCUSDT-PERP".to_string(), json!({"symbol":"BTCUSDT-PERP"}));
    for index in 0..20 {
        let symbol = format!("COIN{index}USDT-PERP");
        tickers.insert(symbol.clone(), json!({"symbol": symbol}));
    }
    let response = json!({"code": 0, "data": {"tickers": tickers}});
    let listings = parse_pro_futures(response.to_string().as_bytes())
        .expect("an otherwise complete ticker feed should parse");

    assert_eq!(listings.len(), 21);
    assert!(listings.iter().any(|listing| listing.venue_symbol == "BTCUSDT-PERP"));
    for symbol in delisted {
        assert!(!listings.iter().any(|listing| listing.venue_symbol == symbol), "{symbol}");
    }
}

#[test]
fn march_notice_exclusions_cover_all_published_contracts() {
    assert_eq!(PINTU_PRO_FUTURES_MARCH_2_2026_DELISTED_SYMBOLS.len(), 50);
    assert_eq!(PINTU_PRO_FUTURES_MARCH_26_2026_DELISTED_SYMBOLS.len(), 37);
    assert_eq!(PINTU_PRO_FUTURES_2025_DELISTED_SYMBOLS.len(), 10);
    for symbol in PINTU_PRO_FUTURES_MARCH_2_2026_DELISTED_SYMBOLS
        .iter()
        .chain(PINTU_PRO_FUTURES_MARCH_26_2026_DELISTED_SYMBOLS)
        .chain(PINTU_PRO_FUTURES_2025_DELISTED_SYMBOLS)
    {
        assert!(is_announced_delisted_pintu_future(symbol), "{symbol}");
    }
}
