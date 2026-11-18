//! Venue assumptions and user-defined instrument eligibility rules.

/// Pintu Futures maker-fee assumption as a decimal rate per side.
pub const PINTU_MAKER_FEE_RATE: f64 = 0.001755;
/// The same Pintu Futures maker-fee assumption expressed as percent.
pub const PINTU_MAKER_FEE_PCT: f64 = PINTU_MAKER_FEE_RATE * 100.0;
/// Pintu Futures taker-fee assumption as a decimal rate per side.
pub const PINTU_TAKER_FEE_RATE: f64 = 0.002865;
/// The same Pintu Futures taker-fee assumption expressed as percent.
pub const PINTU_TAKER_FEE_PCT: f64 = PINTU_TAKER_FEE_RATE * 100.0;
/// Pintu Futures taker fee for both sides of a round trip, expressed as percent.
pub const PINTU_TAKER_FEE_ROUND_TRIP_PCT: f64 = PINTU_TAKER_FEE_PCT * 2.0;
pub const PINTU_TAKER_FEE_ASSUMPTION_LABEL: &str = "Pintu Futures taker estimate: 0.2865% per side";

/// Pluang Crypto Futures taker fee published as an all-in rate per side.
/// Official schedule effective September 25, 2026.
pub const PLUANG_CRYPTO_FUTURES_TAKER_FEE_PCT: f64 = 0.2865;
pub const PLUANG_CRYPTO_FUTURES_FEE_ASSUMPTION_LABEL: &str =
    "Pluang Crypto Futures published all-in taker fee: 0.2865% per side; funding excluded";

/// Pluang Crypto spot taker estimate including its published CFX charge.
pub const PLUANG_CRYPTO_SPOT_TAKER_FEE_PCT: f64 = 0.15555;
pub const PLUANG_CRYPTO_SPOT_FEE_ASSUMPTION_LABEL: &str =
    "Pluang Crypto PRO taker estimate: 0.15555% per side including CFX; spread and applicable taxes excluded";

/// Stockbit buy-side fee assumption expressed as percent.
pub const STOCKBIT_BUY_FEE_PCT: f64 = 0.15;
/// Stockbit sell-side fee assumption expressed as percent.
pub const STOCKBIT_SELL_FEE_PCT: f64 = 0.25;
pub const STOCKBIT_FEE_ASSUMPTION_LABEL: &str =
    "Stockbit published stock rates: 0.15% buy / 0.25% sell; conditional stamp duty, Live Data Fee, slippage, and market depth excluded";
pub const TOKOCRYPTO_SPOT_UNKNOWN_FEE_LABEL: &str =
    "Tokocrypto spot fee unavailable; proxy metric excludes fees";
pub const TOKOCRYPTO_TOKENIZED_STOCK_UNKNOWN_FEE_LABEL: &str =
    "Tokocrypto tokenized-stock venue fee unavailable; proxy metric excludes fees";

/// Apply entry and exit fees to a return measured against the entry notional.
/// The exit fee scales with the exit transaction's notional value.
pub(crate) fn net_return_after_transaction_fees(
    gross_return_pct: f64,
    exit_value_multiple: f64,
    entry_fee_pct: f64,
    exit_fee_pct: f64,
) -> f64 {
    gross_return_pct - entry_fee_pct.max(0.0) - exit_fee_pct.max(0.0) * exit_value_multiple
}

pub const STOCKBIT_FEE_RULES_URL: &str = "https://help.stockbit.com/id/article/transaksi-saham-berapa-biaya-trading-di-stockbit-sekuritas-1lbkyq9/";
pub const PINTU_FUTURES_FEE_RULES_URL: &str = "https://pintu.co.id/faq/fees-limits-security";
pub const PINTU_SPOT_FEE_RULES_URL: &str =
    "https://www.pintu.co.id/blog/pengumuman-update-biaya-cfx-turun-2026";
pub const TOKOCRYPTO_FEE_RULES_URL: &str = "https://support.tokocrypto.com/hc/id/articles/360004044591-Informasi-Biaya-Transaksi-di-Tokocrypto";
pub const PLUANG_FEE_RULES_URL: &str = "https://pluang.com/id/biaya";
pub const PLUANG_CRYPTO_FUTURES_FEE_RULES_URL: &str = "https://pluang.com/biaya/crypto-futures";
pub const PLUANG_CRYPTO_SPOT_FEE_RULES_URL: &str = "https://pluang.com/biaya/crypto";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VenueFeePolicy {
    Known {
        buy_fee_pct: f64,
        sell_fee_pct: f64,
        fee_model: &'static str,
        official_fee_rules_url: Option<&'static str>,
    },
    Unknown {
        fee_model: &'static str,
        official_fee_rules_url: Option<&'static str>,
    },
    Unsupported {
        fee_model: &'static str,
        official_fee_rules_url: Option<&'static str>,
    },
}

/// Return the selected app product's existing fee policy for R:R simulation.
/// Fee policy remains separate for Tokocrypto spot and tokenized stocks.
pub fn venue_fee_policy(exchange_id: &str, product_id: &str) -> VenueFeePolicy {
    match (exchange_id, product_id) {
        ("stockbit", "stockbit_regular") => VenueFeePolicy::Known {
            buy_fee_pct: STOCKBIT_BUY_FEE_PCT,
            sell_fee_pct: STOCKBIT_SELL_FEE_PCT,
            fee_model: STOCKBIT_FEE_ASSUMPTION_LABEL,
            official_fee_rules_url: Some(STOCKBIT_FEE_RULES_URL),
        },
        ("pintu", "pintu_pro_futures") => VenueFeePolicy::Known {
            buy_fee_pct: PINTU_TAKER_FEE_PCT,
            sell_fee_pct: PINTU_TAKER_FEE_PCT,
            fee_model: PINTU_TAKER_FEE_ASSUMPTION_LABEL,
            official_fee_rules_url: Some(PINTU_FUTURES_FEE_RULES_URL),
        },
        ("pluang", "pluang_perpetual") => VenueFeePolicy::Known {
            buy_fee_pct: PLUANG_CRYPTO_FUTURES_TAKER_FEE_PCT,
            sell_fee_pct: PLUANG_CRYPTO_FUTURES_TAKER_FEE_PCT,
            fee_model: PLUANG_CRYPTO_FUTURES_FEE_ASSUMPTION_LABEL,
            official_fee_rules_url: Some(PLUANG_CRYPTO_FUTURES_FEE_RULES_URL),
        },
        ("pluang", "pluang_spot") => VenueFeePolicy::Known {
            buy_fee_pct: PLUANG_CRYPTO_SPOT_TAKER_FEE_PCT,
            sell_fee_pct: PLUANG_CRYPTO_SPOT_TAKER_FEE_PCT,
            fee_model: PLUANG_CRYPTO_SPOT_FEE_ASSUMPTION_LABEL,
            official_fee_rules_url: Some(PLUANG_CRYPTO_SPOT_FEE_RULES_URL),
        },
        ("pintu", "pintu_retail_spot") => VenueFeePolicy::Unknown {
            fee_model: "Pintu app spot fee is reflected in the final price and spread",
            official_fee_rules_url: Some(PINTU_SPOT_FEE_RULES_URL),
        },
        ("pintu", "pintu_pro_spot") => VenueFeePolicy::Unknown {
            fee_model: "Pintu Spot fee schedule is not configured for this simulator",
            official_fee_rules_url: Some(PINTU_SPOT_FEE_RULES_URL),
        },
        ("tokocrypto", "tokocrypto_spot") => VenueFeePolicy::Unknown {
            fee_model: TOKOCRYPTO_SPOT_UNKNOWN_FEE_LABEL,
            official_fee_rules_url: Some(TOKOCRYPTO_FEE_RULES_URL),
        },
        ("tokocrypto", "tokocrypto_tokenized_stock") => VenueFeePolicy::Unknown {
            fee_model: TOKOCRYPTO_TOKENIZED_STOCK_UNKNOWN_FEE_LABEL,
            official_fee_rules_url: Some(TOKOCRYPTO_FEE_RULES_URL),
        },
        ("pluang", _) => VenueFeePolicy::Unsupported {
            fee_model: "Pluang execution and fee rules are not configured",
            official_fee_rules_url: Some(PLUANG_FEE_RULES_URL),
        },
        _ => VenueFeePolicy::Unsupported {
            fee_model: "No fee policy is configured for this app product",
            official_fee_rules_url: None,
        },
    }
}

/// Resolve the configured fee policy for an app-owned asset category. This
/// keeps research consumers from copying product-to-venue fee mappings.
pub fn venue_fee_policy_for_category(exchange_id: &str, category_id: &str) -> VenueFeePolicy {
    let product_id = match (exchange_id, category_id) {
        ("stockbit", "idx_stock") => "stockbit_regular",
        ("pintu", "crypto_perpetual") | ("pintu_futures", "crypto_perpetual") => {
            "pintu_pro_futures"
        }
        ("tokocrypto", "crypto_spot") => "tokocrypto_spot",
        ("pluang", "crypto_spot") => "pluang_spot",
        ("pluang", "crypto_perpetual") => "pluang_perpetual",
        ("tokocrypto", "tokenized_stock") => "tokocrypto_tokenized_stock",
        _ => {
            return VenueFeePolicy::Unsupported {
                fee_model: "No fee policy is configured for this app category",
                official_fee_rules_url: None,
            }
        }
    };
    let canonical_exchange = if exchange_id == "pintu_futures" {
        "pintu"
    } else {
        exchange_id
    };
    venue_fee_policy(canonical_exchange, product_id)
}

/// Stockbit candidate screening floor selected to exclude very low-priced IDX
/// stocks from plan sizing. This is separate from IDX tick and
/// price-limit rules.
pub const STOCKBIT_MIN_CANDIDATE_PRICE_IDR: f64 = 60.0;

pub fn stockbit_candidate_price_eligible(price: f64) -> bool {
    price.is_finite() && price >= STOCKBIT_MIN_CANDIDATE_PRICE_IDR
}

/// Standard Spot rates published 18 June 2026; no account-specific discounts.
/// Apply to historical research as an explicit current-fee scenario, not a
/// reconstruction of earlier tax/fee schedules. Tokenized stocks stay unknown.
pub fn tokocrypto_spot_published_fee_policy(quote: &str) -> VenueFeePolicy {
    let (buy, sell, label) = match quote {
        "IDR" => (0.2222, 0.4322, "Tokocrypto standard IDR taker totals incl. tax and ICEx; published 2026-06-18; historical current-fee scenario"),
        "USDT" => (0.4044, 0.4044, "Tokocrypto standard USDT/crypto totals incl. tax and ICEx; published 2026-06-18; historical current-fee scenario"),
        _ => return venue_fee_policy("tokocrypto", "tokocrypto_spot"),
    };
    VenueFeePolicy::Known { buy_fee_pct: buy, sell_fee_pct: sell,
        fee_model: label, official_fee_rules_url: Some(TOKOCRYPTO_FEE_RULES_URL) }
}
