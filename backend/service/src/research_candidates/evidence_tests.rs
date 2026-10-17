use super::*;

#[test]
fn source_labels_preserve_proxy_identity_and_native_venue() {
    assert_eq!(
        research_source_label("yfinance_fallback", "tokenized_stock_yfinance_proxy"),
        "yfinance fallback proxy · Tokocrypto tokenized-stock identity"
    );
    assert_eq!(
        research_source_label("tokocrypto_native", "crypto_spot"),
        "Tokocrypto native history"
    );
}

#[test]
fn holdout_validation_requires_provenance_and_minimum_sample() {
    let valid = serde_json::json!({
        "cagr_validated": true,
        "cagr_validation_status": "VALIDATED_UNTOUCHED_HOLDOUT",
        "holdout_cagr_pct": 12.0,
        "holdout_trades": MIN_EVIDENCE_SAMPLE_TRADES,
        "holdout_sample_days": MIN_EVIDENCE_SAMPLE_DAYS,
    });
    assert!(has_validated_holdout_cagr(&valid));

    let insufficient = serde_json::json!({
        "cagr_validated": true,
        "cagr_validation_status": "VALIDATED_UNTOUCHED_HOLDOUT",
        "holdout_cagr_pct": 12.0,
        "holdout_trades": MIN_EVIDENCE_SAMPLE_TRADES - 1,
        "holdout_sample_days": MIN_EVIDENCE_SAMPLE_DAYS,
    });
    assert!(!has_validated_holdout_cagr(&insufficient));
}

#[test]
fn calendar_month_validation_requires_a_validated_completed_month() {
    let valid = serde_json::json!({
        "calendar_month_return_validated": true,
        "calendar_month_return_validation_status": "VALIDATED_UNTOUCHED_HOLDOUT",
        "holdout_max_calendar_month_return_pct": 100.0,
        "holdout_completed_calendar_months": 1,
    });
    assert!(has_validated_holdout_calendar_month_return(&valid));

    let no_completed_month = serde_json::json!({
        "calendar_month_return_validated": true,
        "calendar_month_return_validation_status": "VALIDATED_UNTOUCHED_HOLDOUT",
        "holdout_max_calendar_month_return_pct": 100.0,
        "holdout_completed_calendar_months": 0,
    });
    assert!(!has_validated_holdout_calendar_month_return(
        &no_completed_month
    ));
}
