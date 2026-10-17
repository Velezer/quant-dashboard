use crate::{
    ast::{Argument, Expr},
    Diagnostic,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyCommissionType {
    Percent,
    CashPerOrder,
    CashPerContract,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct StrategyCommission {
    pub commission_type: StrategyCommissionType,
    pub value: f64,
}

pub(super) fn parse(arguments: &[Argument]) -> Result<Option<StrategyCommission>, Vec<Diagnostic>> {
    let specified = arguments.iter().any(|item| {
        matches!(
            item.name.as_deref(),
            Some("commission_type" | "commission_value")
        )
    });
    if !specified {
        return Ok(None);
    }

    let commission_type = match arguments.iter().find(|item| item.name.as_deref() == Some("commission_type")) {
        Some(item) => match &item.value {
            Expr::Name(value) if value == "strategy.commission.percent" => StrategyCommissionType::Percent,
            Expr::Name(value) if value == "strategy.commission.cash_per_contract" => StrategyCommissionType::CashPerContract,
            Expr::Name(value) if value == "strategy.commission.cash_per_order" => StrategyCommissionType::CashPerOrder,
            _ => return Err(vec![Diagnostic::error(1, 1, "strategy.commission_type must use strategy.commission.percent, strategy.commission.cash_per_order, or strategy.commission.cash_per_contract." )]),
        },
        None => StrategyCommissionType::Percent,
    };
    let value = match arguments
        .iter()
        .find(|item| item.name.as_deref() == Some("commission_value"))
    {
        Some(item) => match item.value {
            Expr::Number(value) if value.is_finite() && value >= 0.0 => value,
            _ => {
                return Err(vec![Diagnostic::error(
                    1,
                    1,
                    "strategy.commission_value must be a finite nonnegative amount.",
                )])
            }
        },
        None => 0.0,
    };
    Ok(Some(StrategyCommission {
        commission_type,
        value,
    }))
}
