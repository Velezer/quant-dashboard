use super::{
    project_reward_risk_account_growth, RewardRiskGrowthProjectionConfig, RewardRiskRule,
    TradeDirection, DEFAULT_GROWTH_STARTING_BALANCE_USD, DEFAULT_GROWTH_TRADING_DAYS,
    GROWTH_TRADING_DAYS_PER_YEAR, MAX_GROWTH_TRADING_DAYS,
};

#[test]
fn growth_projection_defaults_to_one_hundred_usd() {
    assert_eq!(DEFAULT_GROWTH_STARTING_BALANCE_USD, 100.0);
    assert_eq!(
        RewardRiskGrowthProjectionConfig::default().starting_balance_units,
        100.0
    );
    assert_eq!(DEFAULT_GROWTH_TRADING_DAYS, 3_650);
    assert_eq!(GROWTH_TRADING_DAYS_PER_YEAR, 365);
    assert_eq!(
        DEFAULT_GROWTH_TRADING_DAYS / GROWTH_TRADING_DAYS_PER_YEAR,
        10
    );
}

#[test]
fn fee_adjusted_expected_balance_compounds_at_the_daily_trade_cap() {
    let rule = RewardRiskRule::default();
    let config = RewardRiskGrowthProjectionConfig {
        starting_balance_units: 10_000.0,
        trade_size_pct_of_equity: 100.0,
        trading_days: 2,
    };

    let (simulation, projection) = project_reward_risk_account_growth(
        rule,
        Some(0.2865),
        Some(0.2865),
        TradeDirection::Long,
        config,
    )
    .unwrap();

    let probability = rule.win_rate_pct / 100.0;
    let trade_multiplier = probability * (1.0 + simulation.net_reward_pct.unwrap() / 100.0)
        + (1.0 - probability) * (1.0 + simulation.net_loss_pct.unwrap() / 100.0);
    let one_day_multiplier = trade_multiplier.powi(rule.max_trades_per_day as i32);
    assert!(simulation.fee_adjusted);
    assert_eq!(projection.trades_per_day, rule.max_trades_per_day);
    assert_eq!(projection.points.len(), 3);
    assert_eq!(projection.points[0].expected_balance_units, 10_000.0);
    assert!(
        (projection.points[1].expected_balance_units - 10_000.0 * one_day_multiplier).abs() < 1e-8
    );
    assert!(
        (projection.expected_ending_balance_units - 10_000.0 * one_day_multiplier.powi(2)).abs()
            < 1e-8
    );
}

#[test]
fn reducing_position_size_reduces_expected_compound_growth() {
    let rule = RewardRiskRule::default();
    let fees = (Some(0.2865), Some(0.2865));
    let full_size = project_reward_risk_account_growth(
        rule,
        fees.0,
        fees.1,
        TradeDirection::Long,
        RewardRiskGrowthProjectionConfig {
            trading_days: 30,
            ..RewardRiskGrowthProjectionConfig::default()
        },
    )
    .unwrap()
    .1;
    let half_size = project_reward_risk_account_growth(
        rule,
        fees.0,
        fees.1,
        TradeDirection::Long,
        RewardRiskGrowthProjectionConfig {
            trade_size_pct_of_equity: 50.0,
            trading_days: 30,
            ..RewardRiskGrowthProjectionConfig::default()
        },
    )
    .unwrap()
    .1;

    assert!(half_size.expected_ending_balance_units > half_size.starting_balance_units);
    assert!(half_size.expected_ending_balance_units < full_size.expected_ending_balance_units);
}

#[test]
fn unknown_fees_produce_a_gross_only_growth_projection() {
    let (_, projection) = project_reward_risk_account_growth(
        RewardRiskRule::default(),
        None,
        None,
        TradeDirection::Long,
        RewardRiskGrowthProjectionConfig {
            trading_days: 1,
            ..RewardRiskGrowthProjectionConfig::default()
        },
    )
    .unwrap();

    assert!(!projection.fee_adjusted);
    assert!(projection.expected_ending_balance_units > projection.starting_balance_units);
}

#[test]
fn account_equity_is_floored_at_zero_and_never_becomes_debt() {
    let (_, projection) = project_reward_risk_account_growth(
        RewardRiskRule::default(),
        Some(100.0),
        Some(100.0),
        TradeDirection::Long,
        RewardRiskGrowthProjectionConfig {
            trading_days: 2,
            ..RewardRiskGrowthProjectionConfig::default()
        },
    )
    .unwrap();

    assert_eq!(projection.points[1].expected_balance_units, 0.0);
    assert_eq!(projection.expected_ending_balance_units, 0.0);
}

#[test]
fn projection_rejects_horizons_outside_the_bounded_chart_range() {
    let error = project_reward_risk_account_growth(
        RewardRiskRule::default(),
        None,
        None,
        TradeDirection::Long,
        RewardRiskGrowthProjectionConfig {
            trading_days: MAX_GROWTH_TRADING_DAYS + 1,
            ..RewardRiskGrowthProjectionConfig::default()
        },
    )
    .unwrap_err();

    assert_eq!(error, "trading_days must be between 1 and 3650");
}

#[test]
fn growth_projection_supports_a_ten_year_horizon() {
    let (_, projection) = project_reward_risk_account_growth(
        RewardRiskRule::default(),
        Some(0.2865),
        Some(0.2865),
        TradeDirection::Long,
        RewardRiskGrowthProjectionConfig {
            trading_days: MAX_GROWTH_TRADING_DAYS,
            ..RewardRiskGrowthProjectionConfig::default()
        },
    )
    .unwrap();

    assert_eq!(projection.trading_days, 3_650);
    assert_eq!(projection.points.len(), 3_651);
    assert_eq!(projection.points.first().unwrap().trading_day, 0);
    assert_eq!(
        projection.points.last().unwrap().trading_day,
        MAX_GROWTH_TRADING_DAYS
    );
    assert_eq!(
        projection.points.first().unwrap().expected_balance_units,
        projection.starting_balance_units
    );
    assert_eq!(
        projection.points.last().unwrap().expected_balance_units,
        projection.expected_ending_balance_units
    );
    assert!(projection
        .points
        .iter()
        .all(|point| point.expected_balance_units.is_finite()));
}

#[test]
fn projection_rejects_numeric_overflow_instead_of_returning_non_finite_points() {
    let rule = RewardRiskRule {
        risk_pct: 1.2,
        reward_pct: 99.9,
        win_rate_pct: 100.0,
        max_trades_per_day: 100,
    };
    let error = project_reward_risk_account_growth(
        rule,
        None,
        None,
        TradeDirection::Long,
        RewardRiskGrowthProjectionConfig {
            trading_days: MAX_GROWTH_TRADING_DAYS,
            ..RewardRiskGrowthProjectionConfig::default()
        },
    )
    .unwrap_err();

    assert!(error.contains("numeric range"));
}

#[test]
fn rule_limits_match_the_editable_dashboard_ranges() {
    let rule = RewardRiskRule {
        risk_pct: 99.99,
        reward_pct: 0.01,
        win_rate_pct: 100.0,
        max_trades_per_day: 100,
    };
    assert!(rule.validate().is_ok());
    assert!(RewardRiskRule {
        risk_pct: 0.009,
        ..rule
    }
    .validate()
    .is_err());
    assert!(RewardRiskRule {
        reward_pct: 100.0,
        ..rule
    }
    .validate()
    .is_err());
    assert!(RewardRiskRule {
        win_rate_pct: 100.1,
        ..rule
    }
    .validate()
    .is_err());
    assert!(RewardRiskRule {
        max_trades_per_day: 101,
        ..rule
    }
    .validate()
    .is_err());
}
