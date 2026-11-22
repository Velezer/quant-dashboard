use super::{query_values, respond_status, serve_static, RequestContext};
use crate::macro_indicators::MacroIndicatorId;

#[cfg(test)]
#[path = "macro_indicators_tests.rs"]
mod tests;

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(), RequestContext<'a>> {
    if request.route == "/macro" && request.method == "GET" {
        serve_static(request.stream, "frontend/dashboard/templates/macro-indicators.html");
        return Ok(());
    }
    if request.route == "/api/macro/indicators" && request.method == "GET" {
        let (id, force_refresh) = match requested_indicator(request.path) {
            Ok(selection) => selection,
            Err(error) => {
                respond_status(
                    request.stream,
                    crate::wire::encode(&serde_json::json!({"error": error}))
                        .unwrap_or_default(),
                    "400 Bad Request",
                );
                return Ok(());
            }
        };
        match crate::macro_indicators::read(id, force_refresh) {
            Ok(payload) => respond_status(
                request.stream,
                crate::wire::encode(&payload).unwrap_or_default(),
                "200 OK",
            ),
            Err(error) => respond_status(
                request.stream,
                crate::wire::encode(&serde_json::json!({"error": error}))
                    .unwrap_or_default(),
                "502 Bad Gateway",
            ),
        }
        return Ok(());
    }
    Err(request)
}

fn requested_indicator(path: &str) -> Result<(MacroIndicatorId, bool), String> {
    let values = query_values(path);
    let id = match values.get("id") {
        Some(value) => MacroIndicatorId::from_application_id(value).ok_or_else(|| {
            "Unknown macro indicator. Supported ids: ten_year_tips_real_yield, two_year_treasury_nominal_yield, usd_idr_monthly_average.".to_string()
        })?,
        None => MacroIndicatorId::TenYearTipsRealYield,
    };
    let force_refresh = values
        .get("refresh")
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
    Ok((id, force_refresh))
}

