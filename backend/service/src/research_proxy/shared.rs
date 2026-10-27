//! Identity of research references shared by registered products in one category.
//! Venue-native candles never use this namespace.
pub(crate) const SOURCE_EXCHANGE_ID: &str = "research_reference";

// Suffix, provider, quote/price basis, market family. Provider identities remain
// distinct even when two reference prices happen to match.
const REFERENCES: [(&str, &str, &str, &str); 7] = [
    (
        "binance_spot_proxy",
        "binance",
        "binance_usdt_reference",
        "spot",
    ),
    (
        "binance_usdm_futures_proxy",
        "binance",
        "binance_usdt_reference",
        "derivatives",
    ),
    (
        "coinbase_spot_proxy",
        "coinbase",
        "coinbase_usd_reference",
        "spot",
    ),
    ("bybit_spot_proxy", "bybit", "bybit_usdt_reference", "spot"),
    (
        "bybit_linear_proxy",
        "bybit",
        "bybit_usdt_reference",
        "derivatives",
    ),
    ("okx_spot_proxy", "okx", "okx_usdt_reference", "spot"),
    ("yfinance_fallback", "yfinance", "yahoo_unadjusted", "all"),
];

pub(crate) fn app_source_prefix<'a>(exchange: &'a str, product: &'a str) -> Option<&'a str> {
    if product.is_empty() {
        return matches!(exchange, "stockbit" | "pintu" | "pluang" | "tokocrypto")
            .then_some(exchange);
    }
    match (exchange, product) {
        ("stockbit", "stockbit_regular")
        | ("tokocrypto", "tokocrypto_spot" | "tokocrypto_tokenized_stock")
        | ("pintu", "pintu_retail_spot" | "pintu_pro_spot")
        | ("pluang", "pluang_spot" | "pluang_perpetual") => Some(product),
        ("pintu", "pintu_pro_futures") => Some("pintu_futures"),
        _ => None,
    }
}

pub(crate) fn category_for_product(exchange: &str, product: &str) -> Option<&'static str> {
    app_source_prefix(exchange, product)?;
    match product {
        "tokocrypto_spot" | "pintu_retail_spot" | "pintu_pro_spot" | "pluang_spot" => {
            Some("crypto_spot")
        }
        "pintu_pro_futures" | "pluang_perpetual" => Some("crypto_perpetual"),
        _ => None,
    }
}

pub(crate) fn canonical_source(category: &str, suffix: &str) -> Option<String> {
    let spot = category == "crypto_spot";
    let derivatives = matches!(category, "crypto_perpetual" | "crypto_futures");
    let supported = REFERENCES.iter().any(|(name, _, _, family)| {
        *name == suffix
            && match *family {
                "spot" => spot,
                "derivatives" => derivatives,
                _ => spot || derivatives,
            }
    });
    supported.then(|| format!("shared_{category}_{suffix}"))
}

pub(crate) fn source_identities_json(category: &str) -> String {
    serde_json::Value::Array(
        REFERENCES
            .iter()
            .filter_map(|(suffix, provider, basis, _)| {
                canonical_source(category, suffix).map(|source| {
                    serde_json::json!({
                        "source":source,"provider":provider,"price_basis":basis,
                    })
                })
            })
            .collect(),
    )
    .to_string()
}

pub(crate) fn is_shared_source_for_category(source: &str, category: &str) -> bool {
    source
        .strip_prefix(&format!("shared_{category}_"))
        .and_then(|suffix| canonical_source(category, suffix))
        .is_some_and(|canonical| canonical == source)
}

pub(crate) fn is_shared_source(source: &str) -> bool {
    ["crypto_spot", "crypto_perpetual", "crypto_futures"]
        .iter()
        .any(|category| is_shared_source_for_category(source, category))
}

/// Only consolidate known provider/quote identities; an app or provider label
/// alone is insufficient evidence that two stored price series are equivalent.
pub(crate) fn legacy_shared_source(
    source: &str,
    exchange: &str,
    product: &str,
    category: &str,
    provider: &str,
    price_basis: &str,
) -> Option<String> {
    if exchange == "stockbit" || (exchange == "tokocrypto" && category != "crypto_spot") {
        return None;
    }
    let prefix = app_source_prefix(exchange, product)?;
    if !product.is_empty() && category_for_product(exchange, product)? != category {
        return None;
    }
    let suffix = source.strip_prefix(prefix)?.strip_prefix('_')?;
    let (_, expected_provider, expected_basis, _) =
        REFERENCES.iter().find(|entry| entry.0 == suffix)?;
    (*expected_provider == provider && *expected_basis == price_basis)
        .then(|| canonical_source(category, suffix))
        .flatten()
}
