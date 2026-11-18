use crate::venue_rules::{
    stockbit_candidate_price_eligible, venue_fee_policy, venue_fee_policy_for_category,
    VenueFeePolicy, PINTU_FUTURES_FEE_RULES_URL, PINTU_SPOT_FEE_RULES_URL,
    PLUANG_CRYPTO_FUTURES_FEE_RULES_URL, PLUANG_CRYPTO_SPOT_FEE_RULES_URL,
    PLUANG_FEE_RULES_URL, PLUANG_CRYPTO_FUTURES_TAKER_FEE_PCT,
    PLUANG_CRYPTO_SPOT_TAKER_FEE_PCT, STOCKBIT_FEE_RULES_URL, TOKOCRYPTO_FEE_RULES_URL,
};

fn official_fee_rules_url(policy: VenueFeePolicy) -> Option<&'static str> {
    match policy {
        VenueFeePolicy::Known {
            official_fee_rules_url,
            ..
        }
        | VenueFeePolicy::Unknown {
            official_fee_rules_url,
            ..
        }
        | VenueFeePolicy::Unsupported {
            official_fee_rules_url,
            ..
        } => official_fee_rules_url,
    }
}

#[test]
fn official_fee_rules_link_follows_selected_app_product() {
    assert_eq!(
        official_fee_rules_url(venue_fee_policy("stockbit", "stockbit_regular")),
        Some(STOCKBIT_FEE_RULES_URL)
    );
    assert_eq!(
        official_fee_rules_url(venue_fee_policy("pintu", "pintu_pro_futures")),
        Some(PINTU_FUTURES_FEE_RULES_URL)
    );
    assert_eq!(
        official_fee_rules_url(venue_fee_policy("pintu", "pintu_retail_spot")),
        Some(PINTU_SPOT_FEE_RULES_URL)
    );
    assert_eq!(
        official_fee_rules_url(venue_fee_policy("pintu", "pintu_pro_spot")),
        Some(PINTU_SPOT_FEE_RULES_URL)
    );
    assert_eq!(
        official_fee_rules_url(venue_fee_policy("tokocrypto", "tokocrypto_spot")),
        Some(TOKOCRYPTO_FEE_RULES_URL)
    );
    assert_eq!(
        official_fee_rules_url(venue_fee_policy("tokocrypto", "tokocrypto_tokenized_stock")),
        Some(TOKOCRYPTO_FEE_RULES_URL)
    );
    assert_eq!(
        official_fee_rules_url(venue_fee_policy("pluang", "pluang_crypto")),
        Some(PLUANG_FEE_RULES_URL)
    );
    assert_eq!(
        official_fee_rules_url(venue_fee_policy("unknown", "all")),
        None
    );
}

#[test]
fn research_fee_policy_resolves_from_the_app_owned_asset_category() {
    assert_eq!(
        venue_fee_policy_for_category("stockbit", "idx_stock"),
        venue_fee_policy("stockbit", "stockbit_regular")
    );
    assert_eq!(
        venue_fee_policy_for_category("pintu", "crypto_perpetual"),
        venue_fee_policy("pintu", "pintu_pro_futures")
    );
    assert_eq!(
        venue_fee_policy_for_category("tokocrypto", "tokenized_stock"),
        venue_fee_policy("tokocrypto", "tokocrypto_tokenized_stock")
    );
    assert!(matches!(
        venue_fee_policy_for_category("tokocrypto", "tokenized_stock"),
        VenueFeePolicy::Unknown { .. }
    ));
}

#[test]
fn stockbit_candidate_floor_is_inclusive_at_rp60() {
    assert!(!stockbit_candidate_price_eligible(59.0));
    assert!(stockbit_candidate_price_eligible(60.0));
    assert!(stockbit_candidate_price_eligible(61.0));
    assert!(!stockbit_candidate_price_eligible(f64::NAN));
}

#[test]
fn tokocrypto_published_quote_policy_preserves_unknown_scopes() {
    use crate::venue_rules::tokocrypto_spot_published_fee_policy;
    assert!(matches!(tokocrypto_spot_published_fee_policy("IDR"), VenueFeePolicy::Known {buy_fee_pct:0.2222,sell_fee_pct:0.4322,..}));
    assert!(matches!(tokocrypto_spot_published_fee_policy("USDT"), VenueFeePolicy::Known {buy_fee_pct:0.4044,sell_fee_pct:0.4044,..}));
    assert!(matches!(tokocrypto_spot_published_fee_policy("OTHER"), VenueFeePolicy::Unknown {..}));
    assert!(matches!(venue_fee_policy("tokocrypto", "tokocrypto_tokenized_stock"), VenueFeePolicy::Unknown {..}));
}

#[test]
fn pluang_crypto_products_use_published_conservative_taker_rates() {
    assert!(matches!(
        venue_fee_policy("pluang", "pluang_perpetual"),
        VenueFeePolicy::Known {
            buy_fee_pct: PLUANG_CRYPTO_FUTURES_TAKER_FEE_PCT,
            sell_fee_pct: PLUANG_CRYPTO_FUTURES_TAKER_FEE_PCT,
            official_fee_rules_url: Some(PLUANG_CRYPTO_FUTURES_FEE_RULES_URL),
            ..
        }
    ));
    assert!(matches!(
        venue_fee_policy("pluang", "pluang_spot"),
        VenueFeePolicy::Known {
            buy_fee_pct: PLUANG_CRYPTO_SPOT_TAKER_FEE_PCT,
            sell_fee_pct: PLUANG_CRYPTO_SPOT_TAKER_FEE_PCT,
            official_fee_rules_url: Some(PLUANG_CRYPTO_SPOT_FEE_RULES_URL),
            ..
        }
    ));
    assert_eq!(
        venue_fee_policy_for_category("pluang", "crypto_perpetual"),
        venue_fee_policy("pluang", "pluang_perpetual")
    );
    assert_eq!(
        venue_fee_policy_for_category("pluang", "crypto_spot"),
        venue_fee_policy("pluang", "pluang_spot")
    );
}
