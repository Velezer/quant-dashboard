//! Shared fixed-fractional risk limits used by sizing and
//! historical portfolio compounding.

pub const MAX_RISK_PER_TRADE_PCT: f64 = 0.02;
pub const MAX_CAPITAL_PER_TRADE_PCT: f64 = 0.25;
pub const DEFENSIVE_DRAWDOWN_PCT: f64 = 0.05;
pub const HALT_DRAWDOWN_PCT: f64 = 0.10;

#[derive(Debug, Clone, Copy)]
pub struct DrawdownPolicy {
    pub allowed: bool,
    pub risk_scale: f64,
    pub drawdown_fraction: f64,
}

/// Return the common defensive/halt state. `None` means the equity inputs are
/// invalid, so callers must reject the action rather than assume normal risk.
pub fn drawdown_policy(current_equity: f64, peak_equity: f64) -> Option<DrawdownPolicy> {
    if !current_equity.is_finite()
        || !peak_equity.is_finite()
        || current_equity < 0.0
        || peak_equity <= 0.0
    {
        return None;
    }

    let peak = peak_equity.max(current_equity);
    let drawdown_fraction = ((peak - current_equity) / peak).max(0.0);
    let (allowed, risk_scale) = if drawdown_fraction >= HALT_DRAWDOWN_PCT {
        (false, 0.0)
    } else if drawdown_fraction >= DEFENSIVE_DRAWDOWN_PCT {
        (true, 0.5)
    } else {
        (true, 1.0)
    };

    Some(DrawdownPolicy {
        allowed,
        risk_scale,
        drawdown_fraction,
    })
}
