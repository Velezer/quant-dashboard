use super::*;
use quant_core::{
    project_reward_risk_account_growth, RewardRiskGrowthProjectionConfig, RewardRiskRule,
    TradeDirection, VenueFeePolicy,
};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SavedRule {
    pub rule: RewardRiskRule,
    pub saved: bool,
    pub updated_at: Option<i64>,
}

pub(crate) fn load_rule(connection: &Connection) -> Result<SavedRule, rusqlite::Error> {
    let stored = connection
        .query_row(
            "SELECT risk_pct,reward_pct,win_rate_pct,max_trades_per_day,updated_at
             FROM app.saved_reward_risk_rule WHERE id=1",
            [],
            |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?;
    match stored {
        Some((risk_pct, reward_pct, win_rate_pct, max_trades, updated_at)) => {
            let max_trades_per_day = u32::try_from(max_trades).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Integer,
                    Box::new(error),
                )
            })?;
            let rule = RewardRiskRule {
                risk_pct,
                reward_pct,
                win_rate_pct,
                max_trades_per_day,
            }
            .validate()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Real,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
                )
            })?;
            Ok(SavedRule {
                rule,
                saved: true,
                updated_at: Some(updated_at),
            })
        }
        None => Ok(SavedRule {
            rule: RewardRiskRule::default(),
            saved: false,
            updated_at: None,
        }),
    }
}

pub(crate) fn save_rule(
    connection: &Connection,
    rule: RewardRiskRule,
) -> Result<i64, rusqlite::Error> {
    let rule = rule
        .validate()
        .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
    let updated_at = now_secs() as i64;
    connection.execute(
        "INSERT INTO app.saved_reward_risk_rule
         (id,risk_pct,reward_pct,win_rate_pct,max_trades_per_day,updated_at)
         VALUES (1,?1,?2,?3,?4,?5)
         ON CONFLICT(id) DO UPDATE SET
           risk_pct=excluded.risk_pct,reward_pct=excluded.reward_pct,
           win_rate_pct=excluded.win_rate_pct,
           max_trades_per_day=excluded.max_trades_per_day,updated_at=excluded.updated_at",
        params![
            rule.risk_pct,
            rule.reward_pct,
            rule.win_rate_pct,
            rule.max_trades_per_day,
            updated_at
        ],
    )?;
    Ok(updated_at)
}

pub(crate) fn fee_context(
    connection: &Connection,
    exchange_input: &str,
    product_input: &str,
) -> Result<Value, String> {
    let exchange = master_data::exchange_by_alias(exchange_input)
        .ok_or_else(|| "Choose a supported app exchange".to_string())?;
    let exchange_id = master_data::catalog_string(exchange, "id")
        .ok_or_else(|| "Selected app exchange has no canonical identity".to_string())?;
    let profile = if product_input == "all" {
        master_data::default_profile_for_exchange(exchange_id)
    } else {
        master_data::profile_for_app_product(exchange_id, product_input)
    };
    let registered_category = if product_input == "all" {
        None
    } else {
        connection
            .query_row(
                "SELECT asset_category_id FROM app.exchange_products
                 WHERE exchange_id=?1 AND product_id=?2 AND status='enabled'
                   AND asset_category_id<>''",
                rusqlite::params![exchange_id, product_input],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("Could not load the selected app product: {error}"))?
    };
    let asset_category_id = registered_category.as_deref().or_else(|| {
        profile.and_then(|value| master_data::catalog_string(value, "asset_category_id"))
    });
    if product_input != "all" && asset_category_id.is_none() {
        return Err(
            "Choose a registered product supported by the selected app exchange".to_string(),
        );
    }
    let product_id = if product_input == "all" {
        profile
            .and_then(|value| master_data::catalog_string(value, "product_id"))
            .unwrap_or(product_input)
    } else {
        product_input
    };
    let exchange_label = master_data::catalog_string(exchange, "label").unwrap_or(exchange_id);
    let category = asset_category_id.and_then(master_data::category_by_id_or_research_alias);
    let direction_policy = category
        .and_then(|category| master_data::catalog_string(category, "direction_policy"))
        .unwrap_or("long_only");
    let fee_policy = match if product_input == "all" && product_id == "all" {
        asset_category_id
            .map(|category| quant_core::venue_fee_policy_for_category(exchange_id, category))
            .unwrap_or_else(|| quant_core::venue_fee_policy(exchange_id, product_id))
    } else {
        quant_core::venue_fee_policy(exchange_id, product_id)
    } {
        VenueFeePolicy::Known {
            buy_fee_pct,
            sell_fee_pct,
            fee_model,
            official_fee_rules_url,
        } => json!({
            "status": "known",
            "buy_fee_pct": buy_fee_pct,
            "sell_fee_pct": sell_fee_pct,
            "fee_model": fee_model,
            "official_fee_rules_url": official_fee_rules_url,
        }),
        VenueFeePolicy::Unknown {
            fee_model,
            official_fee_rules_url,
        } => json!({
            "status": "unknown",
            "buy_fee_pct": null,
            "sell_fee_pct": null,
            "fee_model": fee_model,
            "official_fee_rules_url": official_fee_rules_url,
        }),
        VenueFeePolicy::Unsupported {
            fee_model,
            official_fee_rules_url,
        } => json!({
            "status": "unsupported",
            "buy_fee_pct": null,
            "sell_fee_pct": null,
            "fee_model": fee_model,
            "official_fee_rules_url": official_fee_rules_url,
        }),
    };
    let asset_category_label =
        category.and_then(|category| master_data::catalog_string(category, "label"));
    Ok(json!({
        "exchange_id": exchange_id,
        "exchange_label": exchange_label,
        "product_id": product_id,
        "asset_category_id": asset_category_id,
        "asset_category_label": asset_category_label,
        "direction_policy": direction_policy,
        "fee_policy": fee_policy,
    }))
}

pub(crate) fn simulate(
    rule: RewardRiskRule,
    buy_fee_pct: Option<f64>,
    sell_fee_pct: Option<f64>,
    direction: &str,
    projection_config: RewardRiskGrowthProjectionConfig,
) -> Result<(Value, Value), &'static str> {
    let direction = match direction {
        "LONG" => TradeDirection::Long,
        "SHORT" => TradeDirection::Short,
        _ => return Err("direction must be LONG or SHORT"),
    };
    let (simulation, projection) = project_reward_risk_account_growth(
        rule,
        buy_fee_pct,
        sell_fee_pct,
        direction,
        projection_config,
    )?;
    let simulation =
        serde_json::to_value(simulation).map_err(|_| "could not encode simulation result")?;
    let projection = serde_json::to_value(projection)
        .map_err(|_| "could not encode account growth projection")?;
    Ok((simulation, projection))
}
