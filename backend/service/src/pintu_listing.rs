//! Bounded first-party Pintu listing discovery. Responses are validated in
//! memory; only typed listing records are handed to the SQLite registry.
use serde_json::Value;
use std::collections::HashSet;

pub const RETAIL_URL: &str = "https://api.pintu.co.id/v2/trade/price-changes";
pub const PRO_SPOT_URL: &str = "https://api.pintu.pro/v1/public/get-symbols-reference";
pub const PRO_FUTURES_URL: &str = "https://api.pintu.pro/perps/v1/public/get-tickers";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub product_id: &'static str,
    pub base: String,
    pub venue_symbol: String,
    pub quote: &'static str,
    pub category: &'static str,
    pub source_url: &'static str,
}

pub fn fetch_all() -> Result<Vec<Listing>, String> {
    // Fetch and validate every product before the registry starts a transaction.
    let retail = super::tokocrypto::get_public_json(RETAIL_URL, 25)?;
    let spot = super::tokocrypto::get_public_json(PRO_SPOT_URL, 25)?;
    let futures = super::tokocrypto::get_public_json(PRO_FUTURES_URL, 25)?;
    let mut all = parse_retail(&retail)?;
    all.extend(parse_pro_spot(&spot)?);
    all.extend(parse_pro_futures(&futures)?);
    Ok(all)
}

fn valid_base(base: &str) -> bool {
    !base.is_empty()
        && base.len() <= 20
        && base
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

fn checked(list: Vec<Listing>, min_count: usize, max_count: usize) -> Result<Vec<Listing>, String> {
    if list.len() < min_count || list.len() > max_count {
        return Err(format!(
            "Pintu listing count {} outside validated bounds",
            list.len()
        ));
    }
    let mut keys = HashSet::new();
    for item in &list {
        if !valid_base(&item.base) || !keys.insert((item.product_id, item.venue_symbol.as_str())) {
            return Err("Pintu listing has an invalid or duplicate symbol".into());
        }
    }
    Ok(list)
}

pub fn parse_retail(body: &[u8]) -> Result<Vec<Listing>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    if value.get("code").and_then(Value::as_str) != Some("success") {
        return Err("Pintu Retail listing status is not success".into());
    }
    let rows = value
        .get("payload")
        .and_then(Value::as_array)
        .ok_or("Pintu Retail listing payload is missing")?;
    let mut list = Vec::new();
    for row in rows {
        let pair = row
            .get("pair")
            .and_then(Value::as_str)
            .ok_or("Pintu Retail listing pair is missing")?;
        let (base, quote) = pair
            .split_once('/')
            .ok_or("Pintu Retail pair is malformed")?;
        if quote.eq_ignore_ascii_case("idr") {
            let base = base.to_ascii_uppercase();
            if !valid_base(&base) {
                return Err("Pintu Retail base is invalid".into());
            }
            list.push(Listing {
                product_id: "pintu_retail_spot",
                base,
                venue_symbol: pair.to_ascii_uppercase(),
                quote: "IDR",
                category: "crypto_spot",
                source_url: RETAIL_URL,
            });
        }
    }
    checked(list, 30, 2_000)
}

pub fn parse_pro_spot(body: &[u8]) -> Result<Vec<Listing>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    if value.get("code").and_then(Value::as_i64) != Some(0) {
        return Err("Pintu Pro Spot listing status is not success".into());
    }
    let rows = value
        .pointer("/data/symbols")
        .and_then(Value::as_array)
        .ok_or("Pintu Pro Spot symbols are missing")?;
    let mut list = Vec::new();
    for row in rows {
        if row
            .get("is_tokenized")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let quote = row.get("quote_asset").and_then(Value::as_str).unwrap_or("");
        if quote != "IDR" {
            continue;
        }
        let base = row
            .get("base_asset")
            .and_then(Value::as_str)
            .ok_or("Pintu Pro Spot base is missing")?;
        let symbol = row
            .get("symbol")
            .and_then(Value::as_str)
            .ok_or("Pintu Pro Spot symbol is missing")?;
        if !valid_base(base) || symbol != format!("{base}-IDR") {
            return Err("Pintu Pro Spot symbol is malformed".into());
        }
        list.push(Listing {
            product_id: "pintu_pro_spot",
            base: base.to_string(),
            venue_symbol: symbol.to_string(),
            quote: "IDR",
            category: "crypto_spot",
            source_url: PRO_SPOT_URL,
        });
    }
    checked(list, 10, 2_000)
}

/// Contracts Pintu's 2026-02-04 notice says were delisted effective 2026-02-11.
/// The public ticker feed still includes these symbols, so they are excluded
/// from the current Pintu Futures listing until Pintu confirms a relisting.
/// Source: <https://www.pintu.co.id/blog/pengumuman-delisting-50-kontrak-futures>.
const PINTU_PRO_FUTURES_DELISTED_SYMBOLS: &[&str] = &[
    "OXTUSDT-PERP",
    "CHRUSDT-PERP",
    "ETHWUSDT-PERP",
    "ARPAUSDT-PERP",
    "CKBUSDT-PERP",
    "GASUSDT-PERP",
    "BNTUSDT-PERP",
    "CTSIUSDT-PERP",
    "MAGICUSDT-PERP",
    "HFTUSDT-PERP",
    "MOVRUSDT-PERP",
    "OGNUSDT-PERP",
    "KNCUSDT-PERP",
    "MTLUSDT-PERP",
    "ONEUSDT-PERP",
    "RIFUSDT-PERP",
    "POWRUSDT-PERP",
    "STEEMUSDT-PERP",
    "RVNUSDT-PERP",
    "RLCUSDT-PERP",
    "WAXPUSDT-PERP",
    "STGUSDT-PERP",
    "NKNUSDT-PERP",
    "EGLDUSDT-PERP",
    "YFIUSDT-PERP",
    "CYBERUSDT-PERP",
    "TUSDT-PERP",
    "ANKRUSDT-PERP",
    "MASKUSDT-PERP",
    "ILVUSDT-PERP",
    "BANDUSDT-PERP",
    "PEOPLEUSDT-PERP",
    "RUNEUSDT-PERP",
    "C98USDT-PERP",
    "1UMAUSDT-PERP",
    "SFPUSDT-PERP",
    "CELRUSDT-PERP",
    "ZILUSDT-PERP",
    "IDUSDT-PERP",
    "GMXUSDT-PERP",
    "ONGUSDT-PERP",
    "LRCUSDT-PERP",
    "ICXUSDT-PERP",
    "AGLDUSDT-PERP",
    "ONTUSDT-PERP",
    "LQTYUSDT-PERP",
    "BLURUSDT-PERP",
    "FLOWUSDT-PERP",
    "MANAUSDT-PERP",
    "GTCUSDT-PERP",
];

/// Pintu's February 23, 2026 notice delisted ARUSDT-PERP effective March 2,
/// 2026. The public ticker response may retain it after the contract delists.
/// Source: https://pintu.co.id/blog/pengumuman-delisting-50-kontrak-futures-march26.
const PINTU_PRO_FUTURES_MARCH_2026_DELISTED_SYMBOLS: &[&str] = &["ARUSDT-PERP"];

pub fn parse_pro_futures(body: &[u8]) -> Result<Vec<Listing>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    if value.get("code").and_then(Value::as_i64) != Some(0) {
        return Err("Pintu Pro Futures listing status is not success".into());
    }
    let rows = value
        .pointer("/data/tickers")
        .and_then(Value::as_object)
        .ok_or("Pintu Pro Futures tickers are missing")?;
    let mut list = Vec::new();
    for (key, row) in rows {
        let symbol = row
            .get("symbol")
            .and_then(Value::as_str)
            .ok_or("Pintu Pro Futures symbol is missing")?;
        if key != symbol {
            return Err("Pintu Pro Futures key mismatch".into());
        }
        if PINTU_PRO_FUTURES_DELISTED_SYMBOLS.contains(&symbol)
            || PINTU_PRO_FUTURES_MARCH_2026_DELISTED_SYMBOLS.contains(&symbol)
        {
            continue;
        }
        if let Some(base) = symbol.strip_suffix("USDT-PX") {
            if !valid_base(base) || !rows.contains_key(&format!("{base}USDT-PERP")) {
                return Err("Pintu PX listing has no matching perpetual contract".into());
            }
            continue;
        }
        let base = symbol
            .strip_suffix("USDT-PERP")
            .ok_or("Pintu Pro Futures contract suffix is invalid")?;
        if !valid_base(base) {
            return Err("Pintu Pro Futures base is invalid".into());
        }
        list.push(Listing {
            product_id: "pintu_pro_futures",
            base: base.to_string(),
            venue_symbol: symbol.to_string(),
            quote: "USDT",
            category: "crypto_perpetual",
            source_url: PRO_FUTURES_URL,
        });
    }
    checked(list, 20, 2_000)
}

#[cfg(test)]
#[path = "pintu_listing_tests.rs"]
mod tests;
