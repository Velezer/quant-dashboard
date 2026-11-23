#[test]
fn yahoo_reference_mapping_covers_the_three_pluang_commodities() {
    use super::yahoo_fallback_symbol;

    assert_eq!(
        yahoo_fallback_symbol("pluang", "XAUUSD", "commodity_derivative").as_deref(),
        Some("GC=F")
    );
    assert_eq!(
        yahoo_fallback_symbol("pluang", "XAGUSD", "commodity_derivative").as_deref(),
        Some("SI=F")
    );
    assert_eq!(
        yahoo_fallback_symbol("pluang", "UKOUSD", "commodity_derivative").as_deref(),
        Some("BZ=F")
    );
    assert_eq!(
        yahoo_fallback_symbol("pintu", "XAUUSD", "commodity_derivative"),
        None
    );
}
