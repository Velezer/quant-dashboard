use serde::Serialize;

pub const DEFAULT_SETUP_RISK_PCT: f64 = 1.2;
pub const DEFAULT_SETUP_REWARD_PCT: f64 = 2.6;
pub const DEFAULT_SETUP_WIN_RATE_PCT: f64 = 50.0;
pub const DEFAULT_SETUP_MAX_TRADES_PER_DAY: u32 = 5;
pub const MIN_SETUP_RISK_REWARD_MOVE_PCT: f64 = 0.01;
pub const MAX_SETUP_RISK_REWARD_MOVE_PCT: f64 = 99.99;
pub const MIN_SETUP_WIN_RATE_PCT: f64 = 0.0;
pub const MAX_SETUP_WIN_RATE_PCT: f64 = 100.0;
pub const MIN_SETUP_TRADES_PER_DAY: u32 = 1;
pub const MAX_SETUP_TRADES_PER_DAY: u32 = 100;
pub const DEFAULT_GROWTH_STARTING_BALANCE_USD: f64 = 100.0;
pub const DEFAULT_GROWTH_TRADE_SIZE_PCT_OF_EQUITY: f64 = 100.0;
pub const DEFAULT_GROWTH_TRADING_DAYS: u32 = 3_650;
pub const GROWTH_TRADING_DAYS_PER_YEAR: u32 = 365;
pub const MIN_GROWTH_STARTING_BALANCE_UNITS: f64 = 0.01;
pub const MIN_GROWTH_TRADE_SIZE_PCT_OF_EQUITY: f64 = 0.01;
pub const MAX_GROWTH_TRADE_SIZE_PCT_OF_EQUITY: f64 = 100.0;
pub const MIN_GROWTH_TRADING_DAYS: u32 = 1;
pub const MAX_GROWTH_TRADING_DAYS: u32 = 3_650;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RewardRiskRule {
    pub risk_pct: f64,
    pub reward_pct: f64,
    pub win_rate_pct: f64,
    pub max_trades_per_day: u32,
}

impl Default for RewardRiskRule {
    fn default() -> Self {
        Self {
            risk_pct: DEFAULT_SETUP_RISK_PCT,
            reward_pct: DEFAULT_SETUP_REWARD_PCT,
            win_rate_pct: DEFAULT_SETUP_WIN_RATE_PCT,
            max_trades_per_day: DEFAULT_SETUP_MAX_TRADES_PER_DAY,
        }
    }
}

impl RewardRiskRule {
    pub fn validate(self) -> Result<Self, &'static str> {
        if !self.risk_pct.is_finite()
            || !(MIN_SETUP_RISK_REWARD_MOVE_PCT..=MAX_SETUP_RISK_REWARD_MOVE_PCT)
                .contains(&self.risk_pct)
        {
            return Err("risk_pct must be between 0.01 and 99.99");
        }
        if !self.reward_pct.is_finite()
            || !(MIN_SETUP_RISK_REWARD_MOVE_PCT..=MAX_SETUP_RISK_REWARD_MOVE_PCT)
                .contains(&self.reward_pct)
        {
            return Err("reward_pct must be between 0.01 and 99.99");
        }
        if !self.win_rate_pct.is_finite()
            || !(MIN_SETUP_WIN_RATE_PCT..=MAX_SETUP_WIN_RATE_PCT).contains(&self.win_rate_pct)
        {
            return Err("win_rate_pct must be between 0 and 100");
        }
        if !(MIN_SETUP_TRADES_PER_DAY..=MAX_SETUP_TRADES_PER_DAY).contains(&self.max_trades_per_day)
        {
            return Err("max_trades_per_day must be between 1 and 100");
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeDirection {
    Long,
    Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RewardRiskSimulation {
    pub gross_reward_pct: f64,
    pub gross_loss_pct: f64,
    pub net_reward_pct: Option<f64>,
    pub net_loss_pct: Option<f64>,
    pub reward_risk_after_fees: Option<f64>,
    pub break_even_win_rate_pct: Option<f64>,
    pub gross_expectancy_per_trade_pct: f64,
    pub expectancy_per_trade_pct: Option<f64>,
    pub expected_wins_at_daily_cap: f64,
    pub gross_expected_return_at_daily_cap_pct: f64,
    pub expected_return_at_daily_cap_pct: Option<f64>,
    pub best_case_at_daily_cap_pct: Option<f64>,
    pub worst_case_at_daily_cap_pct: Option<f64>,
    pub direction: &'static str,
    pub fee_adjusted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RewardRiskGrowthProjectionConfig {
    pub starting_balance_units: f64,
    pub trade_size_pct_of_equity: f64,
    pub trading_days: u32,
}

impl Default for RewardRiskGrowthProjectionConfig {
    fn default() -> Self {
        Self {
            starting_balance_units: DEFAULT_GROWTH_STARTING_BALANCE_USD,
            trade_size_pct_of_equity: DEFAULT_GROWTH_TRADE_SIZE_PCT_OF_EQUITY,
            trading_days: DEFAULT_GROWTH_TRADING_DAYS,
        }
    }
}

impl RewardRiskGrowthProjectionConfig {
    pub fn validate(self) -> Result<Self, &'static str> {
        if !self.starting_balance_units.is_finite()
            || self.starting_balance_units < MIN_GROWTH_STARTING_BALANCE_UNITS
        {
            return Err("starting_balance_units must be finite and at least 0.01");
        }
        if !self.trade_size_pct_of_equity.is_finite()
            || !(MIN_GROWTH_TRADE_SIZE_PCT_OF_EQUITY..=MAX_GROWTH_TRADE_SIZE_PCT_OF_EQUITY)
                .contains(&self.trade_size_pct_of_equity)
        {
            return Err("trade_size_pct_of_equity must be between 0.01 and 100");
        }
        if !(MIN_GROWTH_TRADING_DAYS..=MAX_GROWTH_TRADING_DAYS).contains(&self.trading_days) {
            return Err("trading_days must be between 1 and 3650");
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RewardRiskGrowthPoint {
    pub trading_day: u32,
    pub expected_balance_units: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RewardRiskAccountGrowthProjection {
    pub fee_adjusted: bool,
    pub starting_balance_units: f64,
    pub expected_ending_balance_units: f64,
    pub trade_size_pct_of_equity: f64,
    pub trades_per_day: u32,
    pub trading_days: u32,
    pub expected_balance_growth_per_trade_pct: f64,
    pub points: Vec<RewardRiskGrowthPoint>,
}

pub fn simulate_reward_risk_rule(
    rule: RewardRiskRule,
    buy_fee_pct: Option<f64>,
    sell_fee_pct: Option<f64>,
    direction: TradeDirection,
) -> Result<RewardRiskSimulation, &'static str> {
    let rule = rule.validate()?;
    let fees = match (buy_fee_pct, sell_fee_pct) {
        (Some(buy), Some(sell)) => {
            if !buy.is_finite() || !(0.0..=100.0).contains(&buy) {
                return Err("buy_fee_pct must be between 0 and 100");
            }
            if !sell.is_finite() || !(0.0..=100.0).contains(&sell) {
                return Err("sell_fee_pct must be between 0 and 100");
            }
            Some((buy, sell))
        }
        (None, None) => None,
        _ => return Err("both entry and exit fee percentages are required"),
    };
    let win_probability = rule.win_rate_pct / 100.0;
    let gross_expectancy_per_trade_pct =
        win_probability * rule.reward_pct - (1.0 - win_probability) * rule.risk_pct;
    let max_trades = rule.max_trades_per_day as f64;
    let fee_adjusted_values = fees.map(|(buy_fee_pct, sell_fee_pct)| {
        let (entry_fee_pct, exit_fee_pct, stop_exit_multiple, target_exit_multiple) =
            match direction {
                TradeDirection::Long => (
                    buy_fee_pct,
                    sell_fee_pct,
                    1.0 - rule.risk_pct / 100.0,
                    1.0 + rule.reward_pct / 100.0,
                ),
                TradeDirection::Short => (
                    sell_fee_pct,
                    buy_fee_pct,
                    1.0 + rule.risk_pct / 100.0,
                    1.0 - rule.reward_pct / 100.0,
                ),
            };
        let net_loss_pct = -crate::venue_rules::net_return_after_transaction_fees(
            -rule.risk_pct,
            stop_exit_multiple,
            entry_fee_pct,
            exit_fee_pct,
        );
        let net_reward_pct = crate::venue_rules::net_return_after_transaction_fees(
            rule.reward_pct,
            target_exit_multiple,
            entry_fee_pct,
            exit_fee_pct,
        );
        let expectancy_per_trade_pct =
            win_probability * net_reward_pct - (1.0 - win_probability) * net_loss_pct;
        (
            net_reward_pct,
            -net_loss_pct,
            expectancy_per_trade_pct,
            max_trades * expectancy_per_trade_pct,
            max_trades * net_reward_pct,
            -max_trades * net_loss_pct,
        )
    });

    Ok(RewardRiskSimulation {
        gross_reward_pct: rule.reward_pct,
        gross_loss_pct: -rule.risk_pct,
        net_reward_pct: fee_adjusted_values.map(|values| values.0),
        net_loss_pct: fee_adjusted_values.map(|values| values.1),
        reward_risk_after_fees: fee_adjusted_values
            .filter(|values| values.0 > 0.0)
            .map(|values| values.0 / -values.1),
        break_even_win_rate_pct: fee_adjusted_values
            .filter(|values| values.0 > 0.0)
            .map(|values| -values.1 / (-values.1 + values.0) * 100.0),
        gross_expectancy_per_trade_pct,
        expectancy_per_trade_pct: fee_adjusted_values.map(|values| values.2),
        expected_wins_at_daily_cap: win_probability * max_trades,
        gross_expected_return_at_daily_cap_pct: gross_expectancy_per_trade_pct * max_trades,
        expected_return_at_daily_cap_pct: fee_adjusted_values.map(|values| values.3),
        best_case_at_daily_cap_pct: fee_adjusted_values.map(|values| values.4),
        worst_case_at_daily_cap_pct: fee_adjusted_values.map(|values| values.5),
        direction: match direction {
            TradeDirection::Long => "LONG",
            TradeDirection::Short => "SHORT",
        },
        fee_adjusted: fees.is_some(),
    })
}

pub fn project_reward_risk_account_growth(
    rule: RewardRiskRule,
    buy_fee_pct: Option<f64>,
    sell_fee_pct: Option<f64>,
    direction: TradeDirection,
    config: RewardRiskGrowthProjectionConfig,
) -> Result<(RewardRiskSimulation, RewardRiskAccountGrowthProjection), &'static str> {
    let rule = rule.validate()?;
    let config = config.validate()?;
    let simulation = simulate_reward_risk_rule(rule, buy_fee_pct, sell_fee_pct, direction)?;
    let win_probability = rule.win_rate_pct / 100.0;
    let trade_size_fraction = config.trade_size_pct_of_equity / 100.0;
    let reward_pct = simulation
        .net_reward_pct
        .unwrap_or(simulation.gross_reward_pct);
    let loss_pct = simulation.net_loss_pct.unwrap_or(simulation.gross_loss_pct);
    let equity_after_target = (1.0 + reward_pct / 100.0 * trade_size_fraction).max(0.0);
    let equity_after_stop = (1.0 + loss_pct / 100.0 * trade_size_fraction).max(0.0);
    let expected_trade_multiplier =
        win_probability * equity_after_target + (1.0 - win_probability) * equity_after_stop;
    if !expected_trade_multiplier.is_finite() {
        return Err("expected account growth exceeded the numeric range");
    }

    let mut expected_balance_units = config.starting_balance_units;
    let mut points = Vec::with_capacity(config.trading_days as usize + 1);
    points.push(RewardRiskGrowthPoint {
        trading_day: 0,
        expected_balance_units,
    });
    for trading_day in 1..=config.trading_days {
        for _ in 0..rule.max_trades_per_day {
            expected_balance_units *= expected_trade_multiplier;
            if !expected_balance_units.is_finite() {
                return Err("expected account growth exceeded the numeric range; reduce the horizon or trade size");
            }
        }
        points.push(RewardRiskGrowthPoint {
            trading_day,
            expected_balance_units,
        });
    }

    let projection = RewardRiskAccountGrowthProjection {
        fee_adjusted: simulation.fee_adjusted,
        starting_balance_units: config.starting_balance_units,
        expected_ending_balance_units: expected_balance_units,
        trade_size_pct_of_equity: config.trade_size_pct_of_equity,
        trades_per_day: rule.max_trades_per_day,
        trading_days: config.trading_days,
        expected_balance_growth_per_trade_pct: (expected_trade_multiplier - 1.0) * 100.0,
        points,
    };
    Ok((simulation, projection))
}
