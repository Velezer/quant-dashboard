pub mod liquidity;
pub mod policy;
pub mod position;
pub mod reward_risk;
pub mod tail_risk;

pub(super) const MIN_RELATIVE_DISTANCE_PRICE_DENOMINATOR: f64 = 1e-4;

pub use liquidity::{
    compute_liquidity_risk, discover_liquidity_zones, evaluate_liquidity_risk, LiquidityRiskReport,
    LiquidityRiskResult, LiquidityZone,
};
pub use policy::{
    drawdown_policy, DrawdownPolicy, DEFENSIVE_DRAWDOWN_PCT, HALT_DRAWDOWN_PCT,
    MAX_CAPITAL_PER_TRADE_PCT, MAX_RISK_PER_TRADE_PCT,
};
pub use position::{calculate_position_size, PositionSizeRequest, PositionSizeResult};
pub use reward_risk::{
    project_reward_risk_account_growth, simulate_reward_risk_rule,
    RewardRiskAccountGrowthProjection, RewardRiskGrowthPoint, RewardRiskGrowthProjectionConfig,
    RewardRiskRule, RewardRiskSimulation, TradeDirection, DEFAULT_GROWTH_STARTING_BALANCE_USD,
    DEFAULT_GROWTH_TRADE_SIZE_PCT_OF_EQUITY, DEFAULT_GROWTH_TRADING_DAYS,
    GROWTH_TRADING_DAYS_PER_YEAR, MAX_GROWTH_TRADE_SIZE_PCT_OF_EQUITY, MAX_GROWTH_TRADING_DAYS,
    MAX_SETUP_RISK_REWARD_MOVE_PCT, MAX_SETUP_TRADES_PER_DAY, MAX_SETUP_WIN_RATE_PCT,
    MIN_GROWTH_STARTING_BALANCE_UNITS, MIN_GROWTH_TRADE_SIZE_PCT_OF_EQUITY,
    MIN_GROWTH_TRADING_DAYS, MIN_SETUP_RISK_REWARD_MOVE_PCT, MIN_SETUP_TRADES_PER_DAY,
    MIN_SETUP_WIN_RATE_PCT,
};

#[cfg(test)]
mod reward_risk_tests;
pub use tail_risk::{compute_tail_risk, LiquidityZoneInput, TailRiskScoreResult};
