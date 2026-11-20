use super::{query_values, respond_status, serve_static, RequestContext};
use crate::macro_indicators::MacroIndicatorId;

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(), RequestContext<'a>> {
    if request.route == "/macro" && request.method == "GET" {
        serve_static(request.stream, "frontend/dashboard/templates/macro-indicators.html");
        return Ok(());
    }
    if request.route == "/api/macro/indicators" && request.method == "GET" {
        let force_refresh = query_values(request.path)
            .get("refresh")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        match crate::macro_indicators::read(
            MacroIndicatorId::TenYearTipsRealYield,
            force_refresh,
        ) {
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

