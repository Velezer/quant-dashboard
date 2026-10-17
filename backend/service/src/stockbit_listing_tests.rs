use super::{parse_directory, validate_complete, Listing, MIN_LISTING_SYMBOLS};

fn ticker(index: usize) -> String {
    format!("T{index:03}")
}

fn directory_page(symbols: &[String], declared_total: usize) -> Vec<u8> {
    let mut html = String::from(
        "<html><script>window.__NUXT__=(function(total){return {tableData:{recordsTotal:total,data:[",
    );
    for (index, symbol) in symbols.iter().enumerate() {
        if index != 0 {
            html.push(',');
        }
        html.push_str(&format!("{{KodeEmiten:\"{symbol}\"}}"));
    }
    html.push_str(&format!("]}}}};}}({declared_total}));</script></html>"));
    html.into_bytes()
}

#[test]
fn complete_idx_company_directory_is_normalized_and_count_checked() {
    let symbols: Vec<String> = (0..MIN_LISTING_SYMBOLS).map(ticker).collect();
    let listings = parse_directory(&directory_page(&symbols, symbols.len())).unwrap();

    assert_eq!(listings.len(), MIN_LISTING_SYMBOLS);
    assert_eq!(listings[0].symbol, "T000");
    assert_eq!(listings.last().unwrap().symbol, "T499");
}

#[test]
fn record_total_mismatch_cannot_be_used_to_delist_tickers() {
    let symbols: Vec<String> = (0..MIN_LISTING_SYMBOLS - 1).map(ticker).collect();
    let error = parse_directory(&directory_page(&symbols, MIN_LISTING_SYMBOLS)).unwrap_err();
    assert!(error.contains("declared 500 records"));
}

#[test]
fn a_small_complete_page_is_rejected_as_a_partial_registry_refresh() {
    let symbols: Vec<String> = (0..MIN_LISTING_SYMBOLS - 1).map(ticker).collect();
    let error = parse_directory(&directory_page(&symbols, symbols.len())).unwrap_err();
    assert!(error.contains("expected 500"));
}

#[test]
fn duplicate_tickers_in_the_directory_are_rejected() {
    let mut symbols: Vec<String> = (0..MIN_LISTING_SYMBOLS).map(ticker).collect();
    symbols[1] = symbols[0].clone();
    let error = parse_directory(&directory_page(&symbols, symbols.len())).unwrap_err();
    assert!(error.contains("repeats ticker T000"));
}

#[test]
fn missing_page_terminator_cannot_be_treated_as_complete() {
    let symbols: Vec<String> = (0..MIN_LISTING_SYMBOLS).map(ticker).collect();
    let mut html = directory_page(&symbols, symbols.len());
    let end = html
        .windows(b"</html>".len())
        .position(|bytes| bytes == b"</html>")
        .unwrap();
    html.truncate(end);
    let error = parse_directory(&html).unwrap_err();
    assert!(error.contains("truncated"));
}

#[test]
fn reconciliation_guard_rejects_duplicate_or_invalid_direct_input() {
    let mut listings: Vec<Listing> = (0..MIN_LISTING_SYMBOLS)
        .map(|index| Listing {
            symbol: ticker(index),
        })
        .collect();
    listings[1].symbol = listings[0].symbol.clone();
    let error = validate_complete(&listings).unwrap_err();
    assert!(error.contains("repeats ticker T000"));
}

#[test]
fn non_directory_html_is_rejected() {
    let error = parse_directory(b"<html><body>Access denied</body></html>").unwrap_err();
    assert!(error.contains("company-list page state"));
}
