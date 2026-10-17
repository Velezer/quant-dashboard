//! Bounded discovery of the public IDX listed-company universe used for the
//! Stockbit app instrument registry. IDX provenance is kept separate from the
//! selected app identity and from research-only candle providers.

use std::collections::HashSet;

pub const LISTED_COMPANY_DIRECTORY_URL: &str =
    "https://www.idx.id/en/listed-companies/company-profiles/";
pub const MIN_LISTING_SYMBOLS: usize = 500;
pub(crate) const MAX_LISTING_SYMBOLS: usize = 5_000;
const MAX_DIRECTORY_BYTES: usize = 4 * 1024 * 1024;
const NUXT_STATE_START: &str = "window.__NUXT__=(function(";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub symbol: String,
}

pub fn fetch_all() -> Result<Vec<Listing>, String> {
    let body = super::tokocrypto::get_public_document(
        LISTED_COMPANY_DIRECTORY_URL,
        "30",
        "text/html,application/xhtml+xml",
    )?;
    parse_directory(&body)
}

pub fn parse_directory(body: &[u8]) -> Result<Vec<Listing>, String> {
    if body.is_empty() || body.len() > MAX_DIRECTORY_BYTES {
        return Err("IDX company directory was empty or exceeded 4 MiB".into());
    }
    let html =
        std::str::from_utf8(body).map_err(|_| "IDX company directory was not UTF-8".to_string())?;
    if !html.to_ascii_lowercase().contains("</html>") {
        return Err("IDX company directory response was truncated before </html>".into());
    }

    let nuxt_start = html
        .find(NUXT_STATE_START)
        .ok_or_else(|| "IDX response did not contain the company-list page state".to_string())?;
    let script_end = html[nuxt_start..]
        .find("</script>")
        .map(|offset| nuxt_start + offset)
        .ok_or_else(|| "IDX company-list page state was truncated".to_string())?;
    let table_start = html[nuxt_start..script_end]
        .find("tableData:{")
        .map(|offset| nuxt_start + offset)
        .ok_or_else(|| "IDX page state omitted the listed-company table".to_string())?;
    let expected_count = parse_declared_total(html, table_start, nuxt_start, script_end)?;
    if !(MIN_LISTING_SYMBOLS..=MAX_LISTING_SYMBOLS).contains(&expected_count) {
        return Err(format!(
            "IDX company directory declared {expected_count} records; expected {MIN_LISTING_SYMBOLS}..={MAX_LISTING_SYMBOLS}"
        ));
    }

    let data_start = html[table_start..script_end]
        .find("data:[")
        .map(|offset| table_start + offset + "data:".len())
        .ok_or_else(|| "IDX listed-company table omitted its record array".to_string())?;
    let data_end = matching_array_end(html, data_start)
        .ok_or_else(|| "IDX listed-company record array was incomplete".to_string())?;
    let listings = parse_tickers(&html[data_start + 1..data_end])?;
    if listings.len() != expected_count {
        return Err(format!(
            "IDX company directory declared {expected_count} records but contained {} unique tickers",
            listings.len()
        ));
    }
    validate_complete(&listings)?;
    Ok(listings)
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
            return Err(format!("IDX listing has invalid ticker {symbol}"));
        }
        if !seen.insert(symbol.clone()) {
            return Err(format!(
                "IDX listed-company directory repeats ticker {symbol}"
            ));
        }
    }
    Ok(())
}

fn parse_declared_total(
    html: &str,
    table_start: usize,
    nuxt_start: usize,
    script_end: usize,
) -> Result<usize, String> {
    let total_start = html[table_start..script_end]
        .find("recordsTotal:")
        .map(|offset| table_start + offset + "recordsTotal:".len())
        .ok_or_else(|| "IDX company table omitted its record total".to_string())?;
    let total_tail = &html[total_start..script_end];
    let total_end = total_tail
        .find(',')
        .ok_or_else(|| "IDX company record total was malformed".to_string())?;
    let expression = total_tail[..total_end].trim();
    if let Ok(total) = expression.parse::<usize>() {
        return Ok(total);
    }
    if !is_js_identifier(expression) {
        return Err("IDX company record total was not a supported value".into());
    }

    let parameters_start = nuxt_start + NUXT_STATE_START.len();
    let parameters_end = html[parameters_start..script_end]
        .find("){")
        .map(|offset| parameters_start + offset)
        .ok_or_else(|| "IDX page state had malformed parameters".to_string())?;
    let parameters: Vec<&str> = html[parameters_start..parameters_end]
        .split(',')
        .map(str::trim)
        .collect();
    let parameter_index = parameters
        .iter()
        .position(|parameter| *parameter == expression)
        .ok_or_else(|| "IDX company record total referenced an unknown value".to_string())?;

    let invocation = html[..script_end]
        .rfind("}(")
        .filter(|offset| *offset >= nuxt_start)
        .ok_or_else(|| "IDX page state had no serialized argument list".to_string())?;
    let raw_arguments = html[invocation + 2..script_end].trim();
    let raw_arguments = raw_arguments
        .strip_suffix("));")
        .or_else(|| raw_arguments.strip_suffix("))"))
        .ok_or_else(|| "IDX page state had an incomplete argument list".to_string())?;
    let arguments = split_js_arguments(raw_arguments)?;
    arguments
        .get(parameter_index)
        .and_then(|argument| argument.trim().parse::<usize>().ok())
        .ok_or_else(|| "IDX company record total could not be resolved".to_string())
}

fn split_js_arguments(source: &str) -> Result<Vec<&str>, String> {
    let mut arguments = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (offset, character) in source.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }
        match character {
            '"' | '\'' | '`' => quote = Some(character),
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    "IDX serialized arguments had unbalanced delimiters".to_string()
                })?;
            }
            ',' if depth == 0 => {
                arguments.push(source[start..offset].trim());
                start = offset + character.len_utf8();
            }
            _ => {}
        }
    }
    if quote.is_some() || depth != 0 {
        return Err("IDX serialized arguments were incomplete".into());
    }
    arguments.push(source[start..].trim());
    Ok(arguments)
}

fn is_js_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters.next().is_some_and(|character| {
        character.is_ascii_alphabetic() || character == '_' || character == '$'
    }) && characters
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '$')
}

fn matching_array_end(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (offset, character) in source[open..].char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }
        if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if character == '[' {
            depth += 1;
        } else if character == ']' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(open + offset);
            }
        }
    }
    None
}

fn parse_tickers(records: &str) -> Result<Vec<Listing>, String> {
    let mut listings = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = 0usize;
    while let Some(relative) = records[cursor..].find("KodeEmiten") {
        let key_start = cursor + relative;
        let after_key = key_start + "KodeEmiten".len();
        let mut value_start = after_key;
        while records
            .as_bytes()
            .get(value_start)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            value_start += 1;
        }
        if records.as_bytes().get(value_start) != Some(&b':') {
            cursor = after_key;
            continue;
        }
        value_start += 1;
        while records
            .as_bytes()
            .get(value_start)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            value_start += 1;
        }
        let value_tail = records
            .get(value_start..)
            .ok_or_else(|| "IDX ticker value was not at a UTF-8 boundary".to_string())?;
        let quoted = value_tail
            .strip_prefix('"')
            .ok_or_else(|| "IDX company record had a non-string ticker".to_string())?;
        let value_end = quoted
            .find('"')
            .ok_or_else(|| "IDX company record had an incomplete ticker".to_string())?;
        let symbol = quoted[..value_end].to_ascii_uppercase();
        if !valid_ticker(&symbol) {
            return Err("IDX company directory contained an invalid ticker".into());
        }
        if !seen.insert(symbol.clone()) {
            return Err(format!(
                "IDX listed-company directory repeats ticker {symbol}"
            ));
        }
        listings.push(Listing { symbol });
        if listings.len() > MAX_LISTING_SYMBOLS {
            return Err("IDX company directory exceeded 5000 tickers".into());
        }
        cursor = value_start + 1 + value_end + 1;
    }
    Ok(listings)
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
