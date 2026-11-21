use super::{
    parse_api_page, validate_complete, Listing, LISTED_COMPANY_DIRECTORY_URL, MAX_LISTING_SYMBOLS,
    MIN_LISTING_SYMBOLS,
};

fn ticker(index: usize) -> String {
    format!("T{index:03}")
}

fn api_page(symbols: &[String], total: usize) -> Vec<u8> {
    let rows = symbols
        .iter()
        .map(|symbol| {
            serde_json::json!({
                "EfekEmiten_Saham": true,
                "KodeEmiten": symbol,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&serde_json::json!({
        "recordsTotal": total,
        "data": rows,
    }))
    .unwrap()
}

#[test]
fn stockbit_listings_use_the_official_paginated_json_endpoint() {
    assert_eq!(
        LISTED_COMPANY_DIRECTORY_URL,
        "https://www.idx.id/primary/ListedCompany/GetCompanyProfiles"
    );
}

#[test]
fn api_page_keeps_source_row_count_separate_from_usable_tickers() {
    let mut symbols: Vec<String> = (0..MIN_LISTING_SYMBOLS).map(ticker).collect();
    symbols.push("TEST-SHM".to_owned());
    let body = api_page(&symbols, symbols.len());

    let page = parse_api_page(&body).unwrap();

    assert_eq!(page.record_count, MIN_LISTING_SYMBOLS + 1);
    assert_eq!(page.listings.len(), MIN_LISTING_SYMBOLS);
    validate_complete(&page.listings).unwrap();
}

#[test]
fn invalid_or_duplicate_listing_symbols_are_rejected_before_reconciliation() {
    let mut listings: Vec<Listing> = (0..MIN_LISTING_SYMBOLS)
        .map(|index| Listing {
            symbol: ticker(index),
        })
        .collect();
    listings[1].symbol = listings[0].symbol.clone();
    let duplicate = validate_complete(&listings).unwrap_err();
    assert!(duplicate.contains("repeats ticker T000"));

    listings[1].symbol = "TEST-SHM".to_owned();
    let invalid = validate_complete(&listings).unwrap_err();
    assert!(invalid.contains("invalid symbol TEST-SHM"));
}

#[test]
fn too_small_or_too_large_universes_are_rejected() {
    let too_small: Vec<Listing> = (0..MIN_LISTING_SYMBOLS - 1)
        .map(|index| Listing {
            symbol: ticker(index),
        })
        .collect();
    assert!(validate_complete(&too_small)
        .unwrap_err()
        .contains("expected 500"));

    let too_large: Vec<Listing> = (0..=MAX_LISTING_SYMBOLS)
        .map(|index| Listing {
            symbol: format!("T{index:04}"),
        })
        .collect();
    assert!(validate_complete(&too_large)
        .unwrap_err()
        .contains("expected 500..=5000"));
}

#[test]
fn api_response_with_a_non_string_stock_ticker_is_rejected() {
    let mut rows: Vec<serde_json::Value> = (0..MIN_LISTING_SYMBOLS)
        .map(|index| {
            serde_json::json!({
                "EfekEmiten_Saham": true,
                "KodeEmiten": ticker(index),
            })
        })
        .collect();
    rows[0]["KodeEmiten"] = serde_json::json!(123);
    let body = serde_json::to_vec(&serde_json::json!({
        "recordsTotal": MIN_LISTING_SYMBOLS,
        "data": rows,
    }))
    .unwrap();

    let error = match parse_api_page(&body) {
        Ok(_) => panic!("a numeric ticker cannot be treated as a registered symbol"),
        Err(error) => error,
    };

    assert!(error.contains("omitted a string KodeEmiten"));
}
