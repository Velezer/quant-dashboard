use super::{api_error, query_values, read_post_body, respond_status, RequestContext};
use crate::{reward_risk_rule, wire};
use quant_core::{
    RewardRiskGrowthProjectionConfig, RewardRiskRule, DEFAULT_GROWTH_STARTING_BALANCE_USD,
    DEFAULT_GROWTH_TRADE_SIZE_PCT_OF_EQUITY, DEFAULT_GROWTH_TRADING_DAYS,
    GROWTH_TRADING_DAYS_PER_YEAR, MAX_GROWTH_TRADE_SIZE_PCT_OF_EQUITY, MAX_GROWTH_TRADING_DAYS,
    MAX_SETUP_RISK_REWARD_MOVE_PCT, MAX_SETUP_TRADES_PER_DAY, MAX_SETUP_WIN_RATE_PCT,
    MIN_GROWTH_STARTING_BALANCE_UNITS, MIN_GROWTH_TRADE_SIZE_PCT_OF_EQUITY,
    MIN_GROWTH_TRADING_DAYS, MIN_SETUP_RISK_REWARD_MOVE_PCT, MIN_SETUP_TRADES_PER_DAY,
    MIN_SETUP_WIN_RATE_PCT,
};
use serde_json::{json, Value};

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(), RequestContext<'a>> {
    if request.route != "/api/rule-simulator" {
        return Err(request);
    }
    if !request.remote_is_loopback {
        api_error(
            request.stream,
            "403 Forbidden",
            "rule simulator settings are restricted to the local dashboard",
        );
        return Ok(());
    }
    match request.method {
        "GET" => get_rule(request),
        "POST" => post_rule(request),
        _ => {
            api_error(
                request.stream,
                "405 Method Not Allowed",
                "Use GET or POST for the rule simulator",
            );
            Ok(())
        }
    }
}

fn get_rule(request: RequestContext<'_>) -> Result<(), RequestContext<'_>> {
    let values = query_values(request.path);
    let exchange_id = values.get("exchange_id").map(String::as_str).unwrap_or("");
    let product_id = values
        .get("product_id")
        .map(String::as_str)
        .unwrap_or("all");
    let connection = match crate::open_db_minimal(request.db) {
        Ok(connection) => connection,
        Err(error) => {
            api_error(request.stream, "500 Internal Server Error", error);
            return Ok(());
        }
    };
    let context = match reward_risk_rule::fee_context(&connection, exchange_id, product_id) {
        Ok(context) => context,
        Err(error) => {
            api_error(request.stream, "400 Bad Request", error);
            return Ok(());
        }
    };
    let saved = match reward_risk_rule::load_rule(&connection) {
        Ok(saved) => saved,
        Err(error) => {
            api_error(request.stream, "500 Internal Server Error", error);
            return Ok(());
        }
    };
    let payload = json!({
        "status": "ok",
        "saved": saved.saved,
        "updated_at": saved.updated_at,
        "rule": rule_value(saved.rule),
        "rule_limits": {
            "min_risk_reward_move_pct": MIN_SETUP_RISK_REWARD_MOVE_PCT,
            "max_risk_reward_move_pct": MAX_SETUP_RISK_REWARD_MOVE_PCT,
            "min_win_rate_pct": MIN_SETUP_WIN_RATE_PCT,
            "max_win_rate_pct": MAX_SETUP_WIN_RATE_PCT,
            "min_trades_per_day": MIN_SETUP_TRADES_PER_DAY,
            "max_trades_per_day": MAX_SETUP_TRADES_PER_DAY,
        },
        "growth_projection_defaults": RewardRiskGrowthProjectionConfig::default(),
        "growth_projection_limits": {
            "min_starting_balance_units": MIN_GROWTH_STARTING_BALANCE_UNITS,
            "default_starting_balance_units": DEFAULT_GROWTH_STARTING_BALANCE_USD,
            "min_trade_size_pct_of_equity": MIN_GROWTH_TRADE_SIZE_PCT_OF_EQUITY,
            "default_trade_size_pct_of_equity": DEFAULT_GROWTH_TRADE_SIZE_PCT_OF_EQUITY,
            "max_trade_size_pct_of_equity": MAX_GROWTH_TRADE_SIZE_PCT_OF_EQUITY,
            "default_trading_days": DEFAULT_GROWTH_TRADING_DAYS,
            "min_trading_days": MIN_GROWTH_TRADING_DAYS,
            "max_trading_days": MAX_GROWTH_TRADING_DAYS,
            "trading_days_per_year": GROWTH_TRADING_DAYS_PER_YEAR,
        },
        "context": context,
    });
    respond_payload(request.stream, "200 OK", &payload);
    Ok(())
}

fn post_rule(request: RequestContext<'_>) -> Result<(), RequestContext<'_>> {
    let body = match wire::decode::<Value>(&read_post_body(request.request_text)) {
        Ok(body) => body,
        Err(error) => {
            api_error(request.stream, "400 Bad Request", error);
            return Ok(());
        }
    };
    let action = body.get("action").and_then(Value::as_str).unwrap_or("");
    match action {
        "save" => save_rule(request, &body),
        "simulate" => simulate_rule(request, &body),
        _ => {
            api_error(
                request.stream,
                "400 Bad Request",
                "action must be save or simulate",
            );
            Ok(())
        }
    }
}

fn save_rule<'a>(request: RequestContext<'a>, body: &Value) -> Result<(), RequestContext<'a>> {
    let rule = match parse_rule(body.get("rule")) {
        Ok(rule) => rule,
        Err(error) => {
            api_error(request.stream, "400 Bad Request", error);
            return Ok(());
        }
    };
    let connection = match crate::open_db_minimal(request.db) {
        Ok(connection) => connection,
        Err(error) => {
            api_error(request.stream, "500 Internal Server Error", error);
            return Ok(());
        }
    };
    let updated_at = match reward_risk_rule::save_rule(&connection, rule) {
        Ok(updated_at) => updated_at,
        Err(error) => {
            api_error(request.stream, "500 Internal Server Error", error);
            return Ok(());
        }
    };
    let payload = json!({
        "status": "ok",
        "saved": true,
        "updated_at": updated_at,
        "rule": rule_value(rule),
    });
    respond_payload(request.stream, "200 OK", &payload);
    Ok(())
}

fn simulate_rule<'a>(request: RequestContext<'a>, body: &Value) -> Result<(), RequestContext<'a>> {
    let rule = match parse_rule(body.get("rule")) {
        Ok(rule) => rule,
        Err(error) => {
            api_error(request.stream, "400 Bad Request", error);
            return Ok(());
        }
    };
    let exchange_id = body
        .get("exchange_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let product_id = body
        .get("product_id")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let connection = match crate::open_db_minimal(request.db) {
        Ok(connection) => connection,
        Err(error) => {
            api_error(request.stream, "500 Internal Server Error", error);
            return Ok(());
        }
    };
    let context = match reward_risk_rule::fee_context(&connection, exchange_id, product_id) {
        Ok(context) => context,
        Err(error) => {
            api_error(request.stream, "400 Bad Request", error);
            return Ok(());
        }
    };
    let direction = body.get("direction").and_then(Value::as_str).unwrap_or("");
    if direction == "SHORT" && context["direction_policy"].as_str() != Some("long_and_short") {
        api_error(
            request.stream,
            "400 Bad Request",
            "SHORT direction is not supported for the selected app product",
        );
        return Ok(());
    }
    let buy_fee_pct = optional_number(body, "buy_fee_pct");
    let sell_fee_pct = optional_number(body, "sell_fee_pct");
    let projection_config = match parse_projection_config(body.get("growth_projection")) {
        Ok(config) => config,
        Err(error) => {
            api_error(request.stream, "400 Bad Request", error);
            return Ok(());
        }
    };
    let (simulation, account_growth) = match reward_risk_rule::simulate(
        rule,
        buy_fee_pct,
        sell_fee_pct,
        direction,
        projection_config,
    ) {
        Ok(result) => result,
        Err(error) => {
            api_error(request.stream, "400 Bad Request", error);
            return Ok(());
        }
    };
    let payload = json!({
        "status": "ok",
        "simulation": simulation,
        "account_growth": account_growth,
        "context": context,
    });
    respond_payload(request.stream, "200 OK", &payload);
    Ok(())
}

fn parse_rule(value: Option<&Value>) -> Result<RewardRiskRule, String> {
    let value = value.ok_or_else(|| "rule values are required".to_string())?;
    let max_trades_per_day = value
        .get("max_trades_per_day")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| "max_trades_per_day must be an integer".to_string())?;
    RewardRiskRule {
        risk_pct: required_number(value, "risk_pct")?,
        reward_pct: required_number(value, "reward_pct")?,
        win_rate_pct: required_number(value, "win_rate_pct")?,
        max_trades_per_day,
    }
    .validate()
    .map_err(str::to_string)
}

fn parse_projection_config(
    value: Option<&Value>,
) -> Result<RewardRiskGrowthProjectionConfig, String> {
    let value = value.ok_or_else(|| "growth_projection settings are required".to_string())?;
    let trading_days = value
        .get("trading_days")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| "trading_days must be an integer".to_string())?;
    RewardRiskGrowthProjectionConfig {
        starting_balance_units: required_number(value, "starting_balance_units")?,
        trade_size_pct_of_equity: required_number(value, "trade_size_pct_of_equity")?,
        trading_days,
    }
    .validate()
    .map_err(str::to_string)
}

fn required_number(value: &Value, name: &str) -> Result<f64, String> {
    value
        .get(name)
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("{name} must be a finite number"))
}

fn optional_number(value: &Value, name: &str) -> Option<f64> {
    value.get(name).and_then(Value::as_f64)
}

fn rule_value(rule: RewardRiskRule) -> Value {
    json!({
        "risk_pct": rule.risk_pct,
        "reward_pct": rule.reward_pct,
        "win_rate_pct": rule.win_rate_pct,
        "max_trades_per_day": rule.max_trades_per_day,
    })
}

fn respond_payload(stream: std::net::TcpStream, status: &str, payload: &Value) {
    respond_status(stream, wire::encode(payload).unwrap_or_default(), status);
}
