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
fn delisted_ar_futures_contract_is_excluded_even_if_public_ticker_remains() {
    let mut tickers = Map::new();
    tickers.insert("ARUSDT-PERP".to_string(), json!({"symbol": "ARUSDT-PERP"}));
    for index in 0..20 {
        let symbol = format!("COIN{index}USDT-PERP");
        tickers.insert(symbol.clone(), json!({"symbol": symbol}));
    }
    let response = json!({"code": 0, "data": {"tickers": tickers}});
    let listings = parse_pro_futures(response.to_string().as_bytes())
        .expect("the otherwise complete Pintu futures listing should parse");

    assert_eq!(listings.len(), 20);
    assert!(listings
        .iter()
        .all(|listing| listing.venue_symbol != "ARUSDT-PERP"));
}
