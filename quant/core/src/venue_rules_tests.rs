use crate::venue_rules::{
    stockbit_candidate_price_eligible, venue_fee_policy, venue_fee_policy_for_category,
    VenueFeePolicy, PINTU_FUTURES_FEE_RULES_URL, PINTU_SPOT_FEE_RULES_URL, PLUANG_FEE_RULES_URL,
    STOCKBIT_FEE_RULES_URL, TOKOCRYPTO_FEE_RULES_URL,
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
