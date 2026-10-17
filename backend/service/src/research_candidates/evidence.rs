use super::*;

pub(crate) fn research_source_label(source: &str, instrument_kind: &str) -> &'static str {
    let is_proxy = is_research_proxy_source(source) || instrument_kind.contains("proxy");
    if instrument_kind.contains("tokenized_stock") && is_proxy {
        "yfinance fallback proxy · Tokocrypto tokenized-stock identity"
    } else if source.ends_with("_binance_spot_proxy") {
        "Binance Spot research proxy"
    } else if source.ends_with("_binance_usdm_futures_proxy") {
        "Binance USDⓈ-M Futures research proxy"
    } else if source.ends_with("_coinbase_spot_proxy") {
        "Coinbase Spot research proxy"
    } else if source.ends_with("_bybit_spot_proxy") {
        "Bybit Spot research proxy"
    } else if source.ends_with("_bybit_linear_proxy") {
        "Bybit Linear research proxy"
    } else if source.ends_with("_okx_spot_proxy") {
        "OKX Spot research proxy"
    } else if source == "yfinance"
        || source == "yfinance_fallback"
        || source.ends_with("_yfinance_fallback")
    {
        "yfinance fallback proxy"
    } else if is_proxy {
        "research price proxy"
    } else if source.eq_ignore_ascii_case("tokocrypto")
        || source.eq_ignore_ascii_case("tokocrypto_native")
    {
        "Tokocrypto native history"
    } else if source.eq_ignore_ascii_case("pintu")
        || source.eq_ignore_ascii_case("pintu_futures")
        || source.eq_ignore_ascii_case("pintu_futures_native")
    {
        "Pintu native history"
    } else if source.eq_ignore_ascii_case("stockbit")
        || source.eq_ignore_ascii_case("stockbit_native")
    {
        "Stockbit native history"
    } else if source.eq_ignore_ascii_case("pluang") || source.eq_ignore_ascii_case("pluang_native")
    {
        "Pluang native history"
    } else {
        "local stored history"
    }
}

pub(crate) fn finite_json_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
}

pub(crate) fn has_validated_holdout_cagr(stats: &Value) -> bool {
    stats.get("cagr_validated").and_then(Value::as_bool) == Some(true)
        && stats.get("cagr_validation_status").and_then(Value::as_str)
            == Some("VALIDATED_UNTOUCHED_HOLDOUT")
        && finite_json_number(stats.get("holdout_cagr_pct")).is_some()
        && stats
            .get("holdout_trades")
            .and_then(Value::as_u64)
            .is_some_and(|trades| trades >= MIN_EVIDENCE_SAMPLE_TRADES as u64)
        && stats
            .get("holdout_sample_days")
            .and_then(Value::as_i64)
            .is_some_and(|days| days >= MIN_EVIDENCE_SAMPLE_DAYS)
}

pub(crate) fn has_validated_holdout_calendar_month_return(stats: &Value) -> bool {
    stats
        .get("calendar_month_return_validated")
        .and_then(Value::as_bool)
        == Some(true)
        && stats
            .get("calendar_month_return_validation_status")
            .and_then(Value::as_str)
            == Some("VALIDATED_UNTOUCHED_HOLDOUT")
        && finite_json_number(stats.get("holdout_max_calendar_month_return_pct")).is_some()
        && stats
            .get("holdout_completed_calendar_months")
            .and_then(Value::as_u64)
            .is_some_and(|months| months > 0)
}
