use super::RequestContext;

mod chart_bundle;
mod day_trade_profile;
mod liquidity_risk;
mod markov;
mod storage_stats;

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(), RequestContext<'a>> {
    match request.route {
        "/api/compute/markov" => markov::handle(request),
        "/api/chart/storage/stats" => storage_stats::handle(request),
        "/api/compute/chart-bundle" => chart_bundle::handle(request),
        "/api/compute/day-trade-profile" => day_trade_profile::handle(request),
        "/api/compute/liquidity-risk" => liquidity_risk::handle(request),
        _ => Err(request),
    }
}
