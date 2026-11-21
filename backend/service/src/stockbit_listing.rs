//! Bounded discovery of the public IDX listed-company universe used for the
//! Stockbit app instrument registry. IDX provenance is kept separate from the
//! selected app identity and from research-only candle providers.

use std::collections::HashSet;

use serde_json::Value;

pub const LISTED_COMPANY_DIRECTORY_URL: &str =
    "https://www.idx.id/primary/ListedCompany/GetCompanyProfiles";
const LISTED_COMPANY_REFERER: &str =
    "https://www.idx.id/en/listed-companies/company-profiles/";
pub const MIN_LISTING_SYMBOLS: usize = 500;
pub(crate) const MAX_LISTING_SYMBOLS: usize = 5_000;
const MAX_API_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const API_PAGE_SIZE: usize = 500;
const API_REQUEST_TIMEOUT_SECONDS: u64 = 15;
const IDX_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub symbol: String,
}

pub fn fetch_all() -> Result<Vec<Listing>, String> {
    fetch_from_idx_api()
}

fn fetch_from_idx_api() -> Result<Vec<Listing>, String> {
    let mut start = 0usize;
    let mut expected_total = None;
    let mut listings = Vec::new();
    let mut seen = HashSet::new();

    loop {
        let url = format!(
            "{LISTED_COMPANY_DIRECTORY_URL}?start={start}&length={API_PAGE_SIZE}"
        );
        let response = super::public_http::get_bounded(
            &url,
            super::public_http::RequestOptions {
                connect_timeout: std::time::Duration::from_secs(5),
                total_timeout: std::time::Duration::from_secs(API_REQUEST_TIMEOUT_SECONDS),
                max_body_bytes: MAX_API_RESPONSE_BYTES,
                max_redirects: 0,
                accept: "application/json, text/plain, */*",
                user_agent: Some(IDX_USER_AGENT),
                accept_encoding: None,
                referer: Some(LISTED_COMPANY_REFERER),
            },
        )?;
        if response.status != 200 {
            return Err(format!("IDX company-profile API returned HTTP {}", response.status));
        }
        let page = parse_api_page(&response.body)?;
        if !(MIN_LISTING_SYMBOLS..=MAX_LISTING_SYMBOLS).contains(&page.total) {
            return Err(format!(
                "IDX company-profile API declared {} records; expected {MIN_LISTING_SYMBOLS}..={MAX_LISTING_SYMBOLS}",
                page.total
            ));
        }
        if expected_total.replace(page.total).is_some_and(|total| total != page.total) {
            return Err("IDX company-profile API total changed during pagination".into());
        }
        let expected_page_count = (page.total - start).min(API_PAGE_SIZE);
        if page.record_count != expected_page_count {
            return Err(format!(
                "IDX company-profile API page at {start} returned {} records; expected {expected_page_count}",
                page.record_count
            ));
        }
        for listing in page.listings {
            if !seen.insert(listing.symbol.clone()) {
                return Err(format!(
                    "IDX company-profile API repeated ticker {}",
                    listing.symbol
                ));
            }
            listings.push(listing);
        }
        start = start.saturating_add(expected_page_count);
        if start == page.total {
            break;
        }
    }
    validate_complete(&listings)?;
    Ok(listings)
}

struct ApiPage {
    total: usize,
    record_count: usize,
    listings: Vec<Listing>,
}

fn parse_api_page(body: &[u8]) -> Result<ApiPage, String> {
    if body.is_empty() || body.len() > MAX_API_RESPONSE_BYTES {
        return Err("IDX company-profile API response was empty or exceeded 4 MiB".into());
    }
    let root: Value = serde_json::from_slice(body)
        .map_err(|error| format!("IDX company-profile API JSON was invalid: {error}"))?;
    let total = root
        .get("recordsTotal")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "IDX company-profile API omitted its record total".to_string())?;
    let rows = root
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "IDX company-profile API omitted its data array".to_string())?;
    let mut listings = Vec::with_capacity(rows.len());
    for row in rows {
        // The endpoint can list issuers with other securities as well. Stockbit
        // scans IDX shares, so do not register non-share records as tickers.
        if row.get("EfekEmiten_Saham").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let symbol = row
            .get("KodeEmiten")
            .and_then(Value::as_str)
            .ok_or_else(|| "IDX company-profile record omitted a string KodeEmiten".to_string())?
            .trim()
            .to_ascii_uppercase();
        // IDX's full company-profile directory includes a small number of
        // non-trading/test security identifiers (for example TEST-SHM).
        // They count toward the source page length but cannot be Stockbit
        // ticker identities. Exclude those rows without rejecting the valid
        // listings in the same complete response.
        if valid_ticker(&symbol) {
            listings.push(Listing { symbol });
        }
    }
    Ok(ApiPage {
        total,
        record_count: rows.len(),
        listings,
    })
}

pub fn validate_complete(listings: &[Listing]) -> Result<(), String> {
    if !(MIN_LISTING_SYMBOLS..=MAX_LISTING_SYMBOLS).contains(&listings.len()) {
        return Err(format!(
            "IDX refresh contains {} symbols; expected {MIN_LISTING_SYMBOLS}..={MAX_LISTING_SYMBOLS} for a complete refresh",
            listings.len()
        ));
    }
    let mut seen = HashSet::with_capacity(listings.len());
    for listing in listings {
        let symbol = listing.symbol.to_ascii_uppercase();
        if !valid_ticker(&symbol) {
            return Err(format!("IDX listing has invalid symbol {symbol}"));
        }
        if !seen.insert(symbol.clone()) {
            return Err(format!("IDX company-profile API repeats ticker {symbol}"));
        }
    }
    Ok(())
}

fn valid_ticker(symbol: &str) -> bool {
    (3..=6).contains(&symbol.len())
        && symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

#[cfg(test)]
#[path = "stockbit_listing_tests.rs"]
mod tests;
