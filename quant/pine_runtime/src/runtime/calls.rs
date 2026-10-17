use super::helpers::*;
use super::*;
use crate::ast::Expr;

impl<'a> RuntimeState<'a> {
    pub(super) fn eval_call(
        &mut self,
        name: &str,
        arguments: &[crate::ast::Argument],
        index: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        let value =
            |state: &mut Self, position: usize, named: &str| -> Result<Value, Vec<Diagnostic>> {
                argument_value(arguments, position, named)
                    .map(|expression| state.eval(expression, index, depth + 1))
                    .unwrap_or(Ok(Value::Missing))
            };
        let single_length_overload = matches!(
            name,
            "ta.highest" | "ta.lowest" | "ta.highestbars" | "ta.lowestbars"
        ) && arguments
            .iter()
            .filter(|argument| argument.name.is_none())
            .count()
            == 1
            && !arguments
                .iter()
                .any(|argument| argument.name.as_deref() == Some("source"));
        let period_position = if name == "ta.atr" || single_length_overload {
            0
        } else {
            1
        };
        let period = value_number(arguments, period_position, "length", self, index, depth + 1)
            .unwrap_or(1)
            .clamp(1, self.history_limit as i64) as usize;
        match name {
            "strategy.entry" => {
                validate_strategy_arguments(
                    name,
                    arguments,
                    &[
                        ("id", 0),
                        ("direction", 1),
                        ("qty", 2),
                        ("limit", 3),
                        ("stop", 4),
                        ("oca_name", 5),
                        ("oca_type", 6),
                        ("comment", 7),
                        ("alert_message", 8),
                        ("disable_alert", 9),
                        ("when", usize::MAX),
                    ],
                    10,
                )?;
                let id = value(self, 0, "id")?.display();
                let direction = match value(self, 1, "direction")?.display().as_str() {
                    "LONG" => 1,
                    "SHORT" => -1,
                    _ => 0,
                };
                if id.is_empty() || direction == 0 {
                    return Err(vec![Diagnostic::error(1, 1, "`strategy.entry()` requires an id and strategy.long or strategy.short direction.")]);
                }
                let enabled = argument_value(arguments, 0, "when")
                    .map(|expr| self.eval(expr, index, depth + 1))
                    .transpose()?
                    .and_then(|v| v.boolean())
                    .unwrap_or(true);
                if enabled && index >= self.strategy_evaluation_start_bar {
                    let quantity = match argument_value(arguments, 2, "qty")
                        .map(|expr| self.eval(expr, index, depth + 1))
                        .transpose()?
                    {
                        None | Some(Value::Missing) => None,
                        Some(Value::Number(quantity)) if quantity.is_finite() && quantity > 0.0 => {
                            Some(quantity)
                        }
                        Some(_) => {
                            return Err(vec![Diagnostic::error(
                                1,
                                1,
                                "`strategy.entry()` qty must be a finite positive number or `na`.",
                            )])
                        }
                    };
                    let mut entry_price =
                        |name: &str, position: usize| -> Result<Option<f64>, Vec<Diagnostic>> {
                            match argument_value(arguments, position, name).map(|expression| self.eval(expression, index, depth + 1)).transpose()? {
                            None | Some(Value::Missing) => Ok(None),
                            Some(Value::Number(price)) if price.is_finite() && price > 0.0 => Ok(Some(price)),
                            _ => Err(vec![Diagnostic::error(1, 1, format!("`strategy.entry()` {name} price must be a finite positive number or `na`."))]),
                        }
                        };
                    let stop = entry_price("stop", 4)?;
                    let limit = entry_price("limit", 3)?;
                    self.strategy
                        .as_ref()
                        .ok_or_else(|| {
                            vec![Diagnostic::error(
                                1,
                                1,
                                "Order calls require a Pine `strategy()` declaration.",
                            )]
                        })?
                        .require_market_only_orders(name, stop.is_some() || limit.is_some())?;
                    let (oca_name, oca_type) =
                        strategy_oca_arguments(arguments, self, index, depth, "strategy.entry()")?;
                    self.strategy
                        .as_mut()
                        .ok_or_else(|| {
                            vec![Diagnostic::error(
                                1,
                                1,
                                "Order calls require a Pine `strategy()` declaration.",
                            )]
                        })?
                        .entry(
                            id,
                            direction,
                            quantity,
                            self.bars[index].close,
                            self.bars[index].time,
                            stop,
                            limit,
                            oca_name,
                            oca_type,
                        );
                }
                Ok(Value::Missing)
            }
            "strategy.order" => {
                validate_strategy_arguments(
                    name,
                    arguments,
                    &[
                        ("id", 0),
                        ("direction", 1),
                        ("qty", 2),
                        ("limit", 3),
                        ("stop", 4),
                        ("oca_name", 5),
                        ("oca_type", 6),
                        ("when", usize::MAX),
                    ],
                    7,
                )?;
                let id = value(self, 0, "id")?.display();
                let direction = match value(self, 1, "direction")?.display().as_str() {
                    "LONG" => 1,
                    "SHORT" => -1,
                    _ => 0,
                };
                if id.is_empty() || direction == 0 {
                    return Err(vec![Diagnostic::error(1, 1, "`strategy.order()` requires an id and strategy.long or strategy.short direction.")]);
                }
                let enabled = argument_value(arguments, 0, "when")
                    .map(|expr| self.eval(expr, index, depth + 1))
                    .transpose()?
                    .and_then(|v| v.boolean())
                    .unwrap_or(true);
                if enabled && index >= self.strategy_evaluation_start_bar {
                    let quantity = match argument_value(arguments, 2, "qty")
                        .map(|expr| self.eval(expr, index, depth + 1))
                        .transpose()?
                    {
                        None | Some(Value::Missing) => None,
                        Some(Value::Number(quantity)) if quantity.is_finite() && quantity > 0.0 => {
                            Some(quantity)
                        }
                        Some(_) => {
                            return Err(vec![Diagnostic::error(
                                1,
                                1,
                                "`strategy.order()` qty must be a finite positive number or `na`.",
                            )])
                        }
                    };
                    let mut entry_price =
                        |name: &str, position: usize| -> Result<Option<f64>, Vec<Diagnostic>> {
                            match argument_value(arguments, position, name).map(|expression| self.eval(expression, index, depth + 1)).transpose()? {
                            None | Some(Value::Missing) => Ok(None),
                            Some(Value::Number(price)) if price.is_finite() && price > 0.0 => Ok(Some(price)),
                            _ => Err(vec![Diagnostic::error(1, 1, format!("`strategy.order()` {name} price must be a finite positive number or `na`."))]),
                        }
                        };
                    let stop = entry_price("stop", 4)?;
                    let limit = entry_price("limit", 3)?;
                    self.strategy
                        .as_ref()
                        .ok_or_else(|| {
                            vec![Diagnostic::error(
                                1,
                                1,
                                "Order calls require a Pine `strategy()` declaration.",
                            )]
                        })?
                        .require_market_only_orders(name, stop.is_some() || limit.is_some())?;
                    let (oca_name, oca_type) =
                        strategy_oca_arguments(arguments, self, index, depth, "strategy.order()")?;
                    self.strategy
                        .as_mut()
                        .ok_or_else(|| {
                            vec![Diagnostic::error(
                                1,
                                1,
                                "Order calls require a Pine `strategy()` declaration.",
                            )]
                        })?
                        .order(
                            id,
                            direction,
                            quantity,
                            self.bars[index].close,
                            self.bars[index].time,
                            stop,
                            limit,
                            oca_name,
                            oca_type,
                        );
                }
                Ok(Value::Missing)
            }
            "strategy.cancel" | "strategy.cancel_all" => {
                let id = if name == "strategy.cancel" {
                    validate_strategy_arguments(name, arguments, &[("id", 0)], 1)?;
                    let id = value(self, 0, "id")?.display();
                    if id.is_empty() {
                        return Err(vec![Diagnostic::error(
                            1,
                            1,
                            "`strategy.cancel()` requires an order id.",
                        )]);
                    }
                    Some(id)
                } else {
                    validate_strategy_arguments(name, arguments, &[], 0)?;
                    None
                };
                if index >= self.strategy_evaluation_start_bar {
                    self.strategy
                        .as_mut()
                        .ok_or_else(|| {
                            vec![Diagnostic::error(
                                1,
                                1,
                                "Order calls require a Pine `strategy()` declaration.",
                            )]
                        })?
                        .cancel(id);
                }
                Ok(Value::Missing)
            }
            "strategy.exit" => {
                use crate::strategy::ExitSize;
                if self
                    .strategy
                    .as_ref()
                    .is_some_and(|broker| broker.calc_on_order_fills())
                {
                    return Err(vec![Diagnostic::error(1, 1, "`strategy.exit()` price and bracket orders are not supported with `calc_on_order_fills`; use market closes." )]);
                }
                validate_strategy_arguments(
                    name,
                    arguments,
                    &[
                        ("id", 0),
                        ("from_entry", 1),
                        ("stop", usize::MAX),
                        ("limit", usize::MAX),
                        ("profit", usize::MAX),
                        ("loss", usize::MAX),
                        ("trail_price", usize::MAX),
                        ("trail_points", usize::MAX),
                        ("trail_offset", usize::MAX),
                        ("qty", usize::MAX),
                        ("qty_percent", usize::MAX),
                    ],
                    2,
                )?;
                let id = value(self, 0, "id")?.display();
                if id.is_empty() {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        "`strategy.exit()` requires an exit id.",
                    )]);
                }
                let from_entry = value(self, 1, "from_entry")?.display();
                let exit_price = |value: Option<Value>| -> Result<Option<f64>, Vec<Diagnostic>> {
                    match value {
                        None | Some(Value::Missing) => Ok(None),
                        Some(Value::Number(price)) if price.is_finite() && price > 0.0 => Ok(Some(price)),
                        _ => Err(vec![Diagnostic::error(1, 1, "`strategy.exit()` stop and limit prices must be finite positive numbers or `na`.")]),
                    }
                };
                let stop = exit_price(
                    argument_value(arguments, 2, "stop")
                        .map(|expression| self.eval(expression, index, depth + 1))
                        .transpose()?,
                )?;
                let limit = exit_price(
                    argument_value(arguments, 3, "limit")
                        .map(|expression| self.eval(expression, index, depth + 1))
                        .transpose()?,
                )?;
                let mut exit_ticks = |argument_name: &str| -> Result<Option<f64>, Vec<Diagnostic>> {
                    let Some(expression) = arguments
                        .iter()
                        .find(|argument| argument.name.as_deref() == Some(argument_name))
                        .map(|argument| &argument.value)
                    else {
                        return Ok(None);
                    };
                    match self.eval(expression, index, depth + 1)? {
                        Value::Missing => Ok(None),
                        Value::Number(ticks) if ticks.is_finite() && ticks.fract() == 0.0 && ticks >= 0.0 => Ok(Some(ticks)),
                        _ => Err(vec![Diagnostic::error(1, 1, format!("`strategy.exit()` {argument_name} must be a finite nonnegative tick distance or `na`."))]),
                    }
                };
                let profit_ticks = exit_ticks("profit")?;
                let loss_ticks = exit_ticks("loss")?;
                let trail_points = exit_ticks("trail_points")?;
                let trail_offset = exit_ticks("trail_offset")?;
                let trail_price = exit_price(
                    argument_value(arguments, 6, "trail_price")
                        .map(|expression| self.eval(expression, index, depth + 1))
                        .transpose()?,
                )?;
                let has_trail_activation = trail_price.is_some() || trail_points.is_some();
                if has_trail_activation != trail_offset.is_some() {
                    return Err(vec![Diagnostic::error(1, 1, "`strategy.exit()` trailing stops require `trail_offset` and either `trail_price` or `trail_points`.")]);
                }
                if (profit_ticks.is_some()
                    || loss_ticks.is_some()
                    || trail_points.is_some()
                    || trail_offset.is_some())
                    && self
                        .strategy
                        .as_ref()
                        .is_none_or(|broker| broker.tick_size().is_none())
                {
                    return Err(vec![Diagnostic::error(1, 1, "`strategy.exit()` profit/loss/trailing tick distances require authoritative instrument tick-size metadata.")]);
                }
                let size = if let Some(expression) = arguments
                    .iter()
                    .find(|argument| argument.name.as_deref() == Some("qty"))
                    .map(|argument| &argument.value)
                {
                    match self.eval(expression, index, depth + 1)? {
                        Value::Number(quantity) if quantity.is_finite() && quantity > 0.0 => {
                            ExitSize::Quantity(quantity)
                        }
                        Value::Missing => ExitSize::Full,
                        _ => {
                            return Err(vec![Diagnostic::error(
                                1,
                                1,
                                "`strategy.exit()` qty must be a finite positive number or `na`.",
                            )])
                        }
                    }
                } else if let Some(expression) = arguments
                    .iter()
                    .find(|argument| argument.name.as_deref() == Some("qty_percent"))
                    .map(|argument| &argument.value)
                {
                    match self.eval(expression, index, depth + 1)? {
                        Value::Number(percent) if percent.is_finite() && percent > 0.0 && percent <= 100.0 => ExitSize::Percent(percent),
                        Value::Missing => ExitSize::Full,
                        _ => return Err(vec![Diagnostic::error(1, 1, "`strategy.exit()` qty_percent must be a finite number greater than zero and at most 100, or `na`." )]),
                    }
                } else {
                    ExitSize::Full
                };
                if stop.is_none()
                    && limit.is_none()
                    && loss_ticks.is_none()
                    && profit_ticks.is_none()
                    && trail_offset.is_none()
                {
                    return Err(vec![Diagnostic::error(1, 1, "`strategy.exit()` requires an active stop/limit, loss/profit tick distance, or valid trailing-stop pair.")]);
                }
                if index >= self.strategy_evaluation_start_bar {
                    self.strategy
                        .as_mut()
                        .ok_or_else(|| {
                            vec![Diagnostic::error(
                                1,
                                1,
                                "Order calls require a Pine `strategy()` declaration.",
                            )]
                        })?
                        .exit(
                            id,
                            (!from_entry.is_empty()).then_some(from_entry),
                            stop,
                            limit,
                            loss_ticks,
                            profit_ticks,
                            trail_price,
                            trail_points,
                            trail_offset,
                            size,
                        );
                }
                Ok(Value::Missing)
            }
            "strategy.close" | "strategy.close_all" => {
                use crate::strategy::ExitSize;
                let (allowed, positional) = if name == "strategy.close_all" {
                    (
                        &[
                            ("comment", 0),
                            ("alert_message", 1),
                            ("immediately", 2),
                            ("disable_alert", 3),
                            ("when", usize::MAX),
                        ][..],
                        4,
                    )
                } else {
                    (
                        &[
                            ("id", 0),
                            ("comment", 1),
                            ("qty", 2),
                            ("qty_percent", 3),
                            ("alert_message", 4),
                            ("immediately", 5),
                            ("disable_alert", 6),
                            ("when", usize::MAX),
                        ][..],
                        7,
                    )
                };
                validate_strategy_arguments(name, arguments, allowed, positional)?;
                let enabled = argument_value(arguments, 0, "when")
                    .map(|expr| self.eval(expr, index, depth + 1))
                    .transpose()?
                    .and_then(|v| v.boolean())
                    .unwrap_or(true);
                if enabled && index >= self.strategy_evaluation_start_bar {
                    if name == "strategy.close" && value(self, 0, "id")?.display().is_empty() {
                        return Err(vec![Diagnostic::error(
                            1,
                            1,
                            "`strategy.close()` requires an entry id.",
                        )]);
                    }
                    let id = if name == "strategy.close_all" {
                        None
                    } else {
                        Some(value(self, 0, "id")?.display())
                    };
                    let immediately_position = if name == "strategy.close_all" { 2 } else { 5 };
                    let immediately =
                        argument_value(arguments, immediately_position, "immediately")
                            .map(|expr| self.eval(expr, index, depth + 1))
                            .transpose()?
                            .and_then(|v| v.boolean())
                            .unwrap_or(false);
                    let size = if name == "strategy.close_all" {
                        ExitSize::Full
                    } else if let Some(expression) = argument_value(arguments, 2, "qty") {
                        match self.eval(expression, index, depth + 1)? {
                            Value::Number(quantity) if quantity.is_finite() && quantity > 0.0 => {
                                ExitSize::Quantity(quantity)
                            }
                            Value::Missing => {
                                close_percent_size(arguments, self, index, depth + 1)?
                            }
                            _ => {
                                return Err(vec![Diagnostic::error(
                                1,
                                1,
                                "`strategy.close()` qty must be a finite positive number or `na`.",
                            )])
                            }
                        }
                    } else {
                        close_percent_size(arguments, self, index, depth + 1)?
                    };
                    self.strategy
                        .as_mut()
                        .ok_or_else(|| {
                            vec![Diagnostic::error(
                                1,
                                1,
                                "Order calls require a Pine `strategy()` declaration.",
                            )]
                        })?
                        .close(id, immediately, size);
                }
                Ok(Value::Missing)
            }
            "strategy.opentrades.entry_bar_index" => {
                let trade_index = value(self, 0, "trade_num")?.number().unwrap_or(-1.0);
                Ok(if trade_index >= 0.0 && trade_index.fract() == 0.0 {
                    self.strategy
                        .as_ref()
                        .and_then(|broker| broker.entry_bar(trade_index as usize))
                        .map(|bar| Value::Number(bar as f64))
                        .unwrap_or(Value::Missing)
                } else {
                    Value::Missing
                })
            }
            "strategy.opentrades.entry_id"
            | "strategy.opentrades.entry_price"
            | "strategy.opentrades.entry_time"
            | "strategy.opentrades.size"
            | "strategy.opentrades.commission"
            | "strategy.opentrades.profit"
            | "strategy.opentrades.profit_percent"
            | "strategy.opentrades.max_runup"
            | "strategy.opentrades.max_drawdown"
            | "strategy.closedtrades.max_runup"
            | "strategy.opentrades.max_runup_percent"
            | "strategy.opentrades.max_drawdown_percent"
            | "strategy.closedtrades.max_drawdown"
            | "strategy.closedtrades.max_runup_percent"
            | "strategy.closedtrades.max_drawdown_percent" => {
                validate_strategy_arguments(name, arguments, &[("trade_num", 0)], 1)?;
                let trade_index = value(self, 0, "trade_num")?.number().unwrap_or(-1.0);
                let trade_index = (trade_index >= 0.0 && trade_index.fract() == 0.0)
                    .then_some(trade_index as usize);
                let Some(broker) = self.strategy.as_ref() else {
                    return Ok(Value::Missing);
                };
                Ok(trade_index
                    .and_then(|index| match name {
                        "strategy.opentrades.entry_id" => broker
                            .open_entry_id(index)
                            .map(|id| Value::String(id.to_string())),
                        "strategy.opentrades.entry_price" => {
                            broker.open_entry_price(index).map(Value::Number)
                        }
                        "strategy.opentrades.entry_time" => broker
                            .open_entry_time(index)
                            .map(|time| Value::Number(timestamp_ns_to_pine_ms(time) as f64)),
                        "strategy.opentrades.size" => broker.open_size(index).map(Value::Number),
                        "strategy.opentrades.commission" => {
                            broker.open_commission(index).map(Value::Number)
                        }
                        "strategy.opentrades.profit" => {
                            broker.open_trade_profit(index).map(Value::Number)
                        }
                        "strategy.opentrades.profit_percent" => {
                            broker.open_trade_profit_percent(index).map(Value::Number)
                        }
                        "strategy.opentrades.max_runup" => {
                            broker.open_max_runup(index).map(Value::Number)
                        }
                        "strategy.opentrades.max_drawdown" => {
                            broker.open_max_drawdown(index).map(Value::Number)
                        }
                        "strategy.opentrades.max_runup_percent" => {
                            broker.open_max_runup_percent(index).map(Value::Number)
                        }
                        "strategy.opentrades.max_drawdown_percent" => {
                            broker.open_max_drawdown_percent(index).map(Value::Number)
                        }
                        "strategy.closedtrades.max_runup" => broker
                            .closed_trade(index)
                            .map(|trade| Value::Number(trade.max_runup)),
                        "strategy.closedtrades.max_drawdown" => broker
                            .closed_trade(index)
                            .map(|trade| Value::Number(trade.max_drawdown)),
                        "strategy.closedtrades.max_runup_percent" => broker
                            .closed_trade(index)
                            .map(|trade| Value::Number(trade.max_runup_percent)),
                        "strategy.closedtrades.max_drawdown_percent" => broker
                            .closed_trade(index)
                            .map(|trade| Value::Number(trade.max_drawdown_percent)),
                        _ => None,
                    })
                    .unwrap_or(Value::Missing))
            }
            "strategy.closedtrades.entry_bar_index"
            | "strategy.closedtrades.entry_time"
            | "strategy.closedtrades.exit_bar_index"
            | "strategy.closedtrades.exit_time"
            | "strategy.closedtrades.entry_price"
            | "strategy.closedtrades.exit_price"
            | "strategy.closedtrades.profit"
            | "strategy.closedtrades.profit_percent"
            | "strategy.closedtrades.size"
            | "strategy.closedtrades.commission"
            | "strategy.closedtrades.entry_id" => {
                validate_strategy_arguments(name, arguments, &[("trade_num", 0)], 1)?;
                let trade_index = value(self, 0, "trade_num")?.number().unwrap_or(-1.0);
                let trade = if trade_index >= 0.0 && trade_index.fract() == 0.0 {
                    self.strategy
                        .as_ref()
                        .and_then(|broker| broker.closed_trade(trade_index as usize))
                } else {
                    None
                };
                Ok(trade
                    .map(|trade| match name {
                        "strategy.closedtrades.entry_bar_index" => {
                            Value::Number(trade.entry_bar as f64)
                        }
                        "strategy.closedtrades.entry_time" => trade
                            .entry_time
                            .map(|time| Value::Number(timestamp_ns_to_pine_ms(time) as f64))
                            .unwrap_or(Value::Missing),
                        "strategy.closedtrades.exit_bar_index" => {
                            Value::Number(trade.exit_bar as f64)
                        }
                        "strategy.closedtrades.exit_time" => trade
                            .exit_time
                            .map(|time| Value::Number(timestamp_ns_to_pine_ms(time) as f64))
                            .unwrap_or(Value::Missing),
                        "strategy.closedtrades.entry_price" => Value::Number(trade.entry_price),
                        "strategy.closedtrades.exit_price" => Value::Number(trade.exit_price),
                        "strategy.closedtrades.profit" => Value::Number(trade.net_pnl),
                        "strategy.closedtrades.profit_percent" => {
                            Value::Number(trade.net_return_pct)
                        }
                        "strategy.closedtrades.size" => {
                            Value::Number(if trade.direction == "LONG" {
                                trade.quantity
                            } else {
                                -trade.quantity
                            })
                        }
                        "strategy.closedtrades.commission" => {
                            Value::Number(trade.gross_pnl - trade.net_pnl)
                        }
                        "strategy.closedtrades.entry_id" => Value::String(trade.entry_id.clone()),
                        _ => Value::Missing,
                    })
                    .unwrap_or(Value::Missing))
            }
            "barcolor" | "bgcolor" => {
                let _ = value(self, 0, "color")?;
                Ok(Value::Missing)
            }
            "input.source" => {
                let title = value(self, 1, "title")?.display();
                let Some(values) = self.input_sources.get(&title) else {
                    self.add_approximation_once(
                        "`input.source()` uses its declared default unless a selected-app indicator source is available.",
                    );
                    return value(self, 0, "defval");
                };
                if self.chart_timeframe != self.input_sources_timeframe {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        format!("`input.source()` `{title}` is bound to the selected chart timeframe and cannot be reused inside request.security()."),
                    )]);
                }
                if values.len() != self.bars.len() {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        format!("`input.source()` `{title}` must align with the selected chart candle series."),
                    )]);
                }
                Ok(values
                    .get(index)
                    .copied()
                    .flatten()
                    .map(Value::Number)
                    .unwrap_or(Value::Missing))
            }
            "input.int" | "input.float" | "input.bool" | "input.string" => {
                let default = value(self, 0, "defval")?;
                let title = value(self, 1, "title")?.display();
                let Some(overridden) = self.input_overrides.get(&title) else {
                    self.add_approximation_once("`input.*()` uses declared defaults unless an explicit validated override was supplied.");
                    return Ok(default);
                };
                let parsed = match name {
                    "input.int" => overridden.as_i64().map(|value| Value::Number(value as f64)),
                    "input.float" => overridden
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .map(Value::Number),
                    "input.bool" => overridden.as_bool().map(Value::Bool),
                    "input.string" => overridden
                        .as_str()
                        .map(|value| Value::String(value.to_string())),
                    _ => None,
                }
                .ok_or_else(|| {
                    vec![Diagnostic::error(
                        1,
                        1,
                        format!("Pine input `{title}` override has the wrong type."),
                    )]
                })?;
                if let Some(number) = parsed.number() {
                    let minimum = argument_value(arguments, 0, "minval")
                        .map(|expr| self.eval(expr, index, depth + 1))
                        .transpose()?
                        .and_then(|value| value.number());
                    let maximum = argument_value(arguments, 0, "maxval")
                        .map(|expr| self.eval(expr, index, depth + 1))
                        .transpose()?
                        .and_then(|value| value.number());
                    if minimum.is_some_and(|minimum| number < minimum)
                        || maximum.is_some_and(|maximum| number > maximum)
                    {
                        return Err(vec![Diagnostic::error(
                            1,
                            1,
                            format!("Pine input `{title}` override is outside its declared range."),
                        )]);
                    }
                }
                Ok(parsed)
            }
            "na" => Ok(Value::Bool(matches!(value(self, 0, "x")?, Value::Missing))),
            "nz" | "fixnan" => {
                let first = value(self, 0, "source")?;
                if matches!(first, Value::Missing) {
                    if name == "fixnan" {
                        Ok(Value::Missing)
                    } else {
                        value(self, 1, "replacement").or_else(|_| Ok(Value::Number(0.0)))
                    }
                } else {
                    Ok(first)
                }
            }
            "math.abs" => unary_number(value(self, 0, "number")?, f64::abs),
            "math.floor" => unary_number(value(self, 0, "number")?, f64::floor),
            "math.ceil" => unary_number(value(self, 0, "number")?, f64::ceil),
            "math.round" => unary_number(value(self, 0, "number")?, f64::round),
            "math.sqrt" => unary_number(value(self, 0, "number")?, f64::sqrt),
            "math.log" => unary_number(value(self, 0, "number")?, f64::ln),
            "math.exp" => unary_number(value(self, 0, "number")?, f64::exp),
            "math.max" => binary_number(
                value(self, 0, "number")?,
                value(self, 1, "number")?,
                f64::max,
            ),
            "math.min" => binary_number(
                value(self, 0, "number")?,
                value(self, 1, "number")?,
                f64::min,
            ),
            "math.pow" => binary_number(
                value(self, 0, "number")?,
                value(self, 1, "exponent")?,
                f64::powf,
            ),
            "request.security" => self.request_security(arguments, index, depth + 1),
            "color.new" => {
                let color = value(self, 0, "color")?.display();
                self.add_approximation_once(
                    "`color.new()` alpha transparency is ignored by the canvas renderer.",
                );
                Ok(Value::String(color))
            }
            "ta.sma" | "ta.ema" | "ta.rma" | "ta.wma" | "ta.highest" | "ta.lowest"
            | "ta.highestbars" | "ta.lowestbars" | "ta.stdev" | "ta.dev" | "ta.mom"
            | "ta.change" => {
                let fallback_source = if single_length_overload {
                    Some(Expr::Name(
                        if name == "ta.highest" || name == "ta.highestbars" {
                            "high"
                        } else {
                            "low"
                        }
                        .to_string(),
                    ))
                } else {
                    None
                };
                let source = if single_length_overload {
                    fallback_source.as_ref()
                } else {
                    argument_value(arguments, 0, "source")
                }
                .or_else(|| argument_value(arguments, 0, "length"))
                .ok_or_else(unsupported_arguments)?;
                if name == "ta.ema" || name == "ta.rma" {
                    self.add_approximation_once("`ta.ema()` seeds from the first finite source value and `ta.rma()` from the first full-window SMA; calls in conditional scopes do not retain Pine's per-call historical execution buffers.");
                }
                self.window_function(name, source, index, period, depth + 1)
            }
            "ta.atr" | "ta.tr" => {
                self.add_approximation_once("`ta.atr()` uses a bounded Wilder average seeded from available true-range bars; TradingView warm-up edge cases may differ.");
                self.true_range_function(name, arguments, index, period, depth + 1)
            }
            "ta.rsi" => {
                let source =
                    argument_value(arguments, 0, "source").ok_or_else(unsupported_arguments)?;
                self.rsi(source, index, period, depth + 1)
            }
            "ta.crossover" | "ta.crossunder" | "ta.cross" => {
                self.cross_function(name, arguments, index, depth + 1)
            }
            "ta.barssince" => self.bars_since(
                argument_value(arguments, 0, "condition").ok_or_else(unsupported_arguments)?,
                index,
                depth + 1,
            ),
            "ta.valuewhen" => self.value_when(arguments, index, depth + 1),
            "ta.vwma" => self.vwma(arguments, index, period, depth + 1),
            "ta.macd" | "ta.dmi" | "ta.bb" | "ta.stoch" => Err(vec![Diagnostic::error(
                1,
                1,
                format!("`{name}()` returns multiple series, which are not supported yet."),
            )]),
            "str.tostring" => Ok(Value::String(value(self, 0, "value")?.display())),
            "str.tonumber" => Ok(value(self, 0, "string")?
                .display()
                .parse::<f64>()
                .map(Value::Number)
                .unwrap_or(Value::Missing)),
            _ => Err(vec![Diagnostic::error(
                1,
                1,
                format!("Unsupported function `{name}()`; no approximation was applied."),
            )]),
        }
    }

    pub(super) fn request_security(
        &mut self,
        arguments: &[crate::ast::Argument],
        index: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        let symbol = argument_value(arguments, 0, "symbol").ok_or_else(unsupported_arguments)?;
        if !matches!(symbol, Expr::Name(name) if name == "syminfo.tickerid" || name == "syminfo.ticker")
        {
            return Err(vec![Diagnostic::error(1, 1, "`request.security()` may use only `syminfo.tickerid` for the selected app instrument; cross-symbol data is blocked." )]);
        }
        let timeframe_expression =
            argument_value(arguments, 1, "timeframe").ok_or_else(unsupported_arguments)?;
        let timeframe_value = self.eval(timeframe_expression, index, depth + 1)?.display();
        let requested_timeframe = normalize_request_timeframe(&timeframe_value)
            .ok_or_else(|| vec![Diagnostic::error(1, 1, format!("Requested timeframe `{timeframe_value}` is not available in the selected app history."))])?;
        let chart_timeframe =
            normalize_request_timeframe(self.chart_timeframe).ok_or_else(|| {
                vec![Diagnostic::error(
                    1,
                    1,
                    "Selected chart timeframe is not supported by the Pine history adapter.",
                )]
            })?;
        let requested_duration = timeframe_duration_ns(&requested_timeframe).unwrap_or_default();
        let chart_duration = timeframe_duration_ns(&chart_timeframe).unwrap_or_default();
        if requested_duration < chart_duration {
            return Err(vec![Diagnostic::error(1, 1, "Lower-timeframe `request.security()` behavior is unsupported; no intrabar value was selected or fabricated." )]);
        }
        let lookahead = option_name(arguments, 4, "lookahead").unwrap_or("barmerge.lookahead_off");
        if lookahead == "barmerge.lookahead_on" {
            return Err(vec![Diagnostic::error(1, 1, "`barmerge.lookahead_on` is rejected because it exposes future higher-timeframe values on historical bars." )]);
        }
        if lookahead != "barmerge.lookahead_off" {
            return Err(vec![Diagnostic::error(
                1,
                1,
                format!("Unsupported `request.security()` lookahead value `{lookahead}`."),
            )]);
        }
        let gaps = option_name(arguments, 3, "gaps").unwrap_or("barmerge.gaps_off");
        let gaps_on = match gaps {
            "barmerge.gaps_off" => false,
            "barmerge.gaps_on" => true,
            _ => {
                return Err(vec![Diagnostic::error(
                    1,
                    1,
                    format!("Unsupported `request.security()` gaps value `{gaps}`."),
                )])
            }
        };
        let allowed_named = ["symbol", "timeframe", "expression", "gaps", "lookahead"];
        if arguments
            .iter()
            .filter_map(|argument| argument.name.as_deref())
            .any(|name| !allowed_named.contains(&name))
        {
            return Err(vec![Diagnostic::error(
                1,
                1,
                "Optional `request.security()` arguments beyond gaps/lookahead are unsupported.",
            )]);
        }
        if arguments
            .iter()
            .filter(|argument| argument.name.is_none())
            .count()
            > 5
        {
            return Err(vec![Diagnostic::error(
                1,
                1,
                "Optional `request.security()` arguments beyond gaps/lookahead are unsupported.",
            )]);
        }
        let expression =
            argument_value(arguments, 2, "expression").ok_or_else(unsupported_arguments)?;
        if expression_contains_security(expression) {
            return Err(vec![Diagnostic::error(
                1,
                1,
                "Nested `request.security()` calls are unsupported.",
            )]);
        }
        let expression_key = format!("{requested_timeframe}|{gaps_on}|{expression:?}");
        self.requested_timeframes
            .insert(requested_timeframe.clone());
        self.add_approximation_once("Higher-timeframe values are aligned to confirmed bar closes from stored selected-app history; session, gaps, and realtime edge cases may differ from TradingView.");
        if !self.request_cache.contains_key(&expression_key) {
            if self.request_cache.len() >= MAX_STRATEGY_SECURITY_CACHE_ENTRIES {
                return Err(vec![Diagnostic::error(1, 1, format!("Pine strategy exceeded the bounded {MAX_STRATEGY_SECURITY_CACHE_ENTRIES} distinct request.security() history limit for this backtest."))]);
            }
            let requested_bars = if requested_timeframe == chart_timeframe {
                self.available_timeframes
                    .get(&requested_timeframe)
                    .map(Vec::as_slice)
                    .unwrap_or(self.bars)
            } else {
                self.available_timeframes.get(&requested_timeframe).map(Vec::as_slice).ok_or_else(|| vec![Diagnostic::error(1, 1, format!("No approved stored {} candle series is available for this selected app instrument.", requested_timeframe))])?
            };
            if requested_bars.is_empty() {
                return Err(vec![Diagnostic::error(1, 1, format!("No approved stored {} candle series is available for this selected app instrument.", requested_timeframe))]);
            }
            let mut evaluator = RuntimeState::new(
                self.program,
                requested_bars,
                self.available_timeframes,
                &requested_timeframe,
                self.input_sources,
                self.input_sources_timeframe,
                self.history_limit,
            );
            for name in expression_names(expression) {
                if let Some(values) = self.series.get(&name) {
                    let Some(first) = values
                        .first()
                        .filter(|value| !matches!(value, Value::Missing))
                    else {
                        return Err(vec![Diagnostic::error(1, 1, format!("`request.security()` cannot reuse unavailable calculated series `{name}` in another timeframe."))]);
                    };
                    if values.iter().any(|value| value != first) {
                        return Err(vec![Diagnostic::error(1, 1, format!("`request.security()` cannot reuse changing chart-timeframe series `{name}`; use a built-in source or constant input."))]);
                    }
                    evaluator
                        .series
                        .insert(name, vec![first.clone(); requested_bars.len()]);
                }
            }
            let mut requested_values = Vec::with_capacity(requested_bars.len());
            for requested_index in 0..requested_bars.len() {
                evaluator.prepare_bar(requested_index)?;
                requested_values.push(evaluator.eval(expression, requested_index, 0)?);
            }
            let aligned = align_security_values(
                self.bars,
                requested_bars,
                chart_duration,
                requested_duration,
                requested_values,
                gaps_on,
            )?;
            self.request_cache.insert(expression_key.clone(), aligned);
        }
        Ok(self
            .request_cache
            .get(&expression_key)
            .and_then(|values| values.get(index))
            .cloned()
            .unwrap_or(Value::Missing))
    }

    fn window_function(
        &mut self,
        name: &str,
        source: &Expr,
        index: usize,
        period: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        let start = index.saturating_add(1).saturating_sub(period);
        let mut values = Vec::with_capacity(period);
        for position in start..=index {
            if let Some(value) = self.eval(source, position, depth + 1)?.number() {
                values.push((position, value));
            }
        }
        match name {
            "ta.highest" => Ok(number_value(
                values.iter().map(|(_, value)| *value).reduce(f64::max),
            )),
            "ta.lowest" => Ok(number_value(
                values.iter().map(|(_, value)| *value).reduce(f64::min),
            )),
            "ta.highestbars" => Ok(number_value(
                values
                    .iter()
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(position, _)| -((*position) as isize - index as isize) as f64),
            )),
            "ta.lowestbars" => Ok(number_value(
                values
                    .iter()
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(position, _)| -((*position) as isize - index as isize) as f64),
            )),
            "ta.sma" => {
                if values.len() < period {
                    Ok(Value::Missing)
                } else {
                    Ok(Value::Number(
                        values.iter().map(|(_, value)| value).sum::<f64>() / period as f64,
                    ))
                }
            }
            "ta.wma" => {
                if values.len() < period {
                    Ok(Value::Missing)
                } else {
                    Ok(Value::Number(
                        values
                            .iter()
                            .enumerate()
                            .map(|(offset, (_, value))| (offset + 1) as f64 * value)
                            .sum::<f64>()
                            / ((period * (period + 1) / 2) as f64),
                    ))
                }
            }
            "ta.ema" | "ta.rma" => {
                self.recursive_average(source, index, period, name == "ta.rma", depth + 1)
            }
            "ta.stdev" => {
                if values.len() < period {
                    return Ok(Value::Missing);
                }
                let mean = values.iter().map(|(_, value)| value).sum::<f64>() / period as f64;
                let variance = values
                    .iter()
                    .map(|(_, value)| (value - mean).powi(2))
                    .sum::<f64>()
                    / period as f64;
                Ok(Value::Number(variance.sqrt()))
            }
            "ta.dev" => {
                if values.len() < period {
                    return Ok(Value::Missing);
                }
                let mean = values.iter().map(|(_, value)| value).sum::<f64>() / period as f64;
                Ok(Value::Number(
                    values
                        .iter()
                        .map(|(_, value)| (value - mean).abs())
                        .sum::<f64>()
                        / period as f64,
                ))
            }
            "ta.mom" => {
                let before = index
                    .checked_sub(period)
                    .and_then(|position| self.eval(source, position, depth + 1).ok())
                    .and_then(|value| value.number());
                let now = self.eval(source, index, depth + 1)?.number();
                Ok(number_value(
                    now.zip(before).map(|(now, before)| now - before),
                ))
            }
            "ta.change" => {
                let before = index
                    .checked_sub(period)
                    .and_then(|position| self.eval(source, position, depth + 1).ok())
                    .and_then(|value| value.number());
                let now = self.eval(source, index, depth + 1)?.number();
                Ok(number_value(
                    now.zip(before).map(|(now, before)| now - before),
                ))
            }
            _ => Ok(Value::Missing),
        }
    }

    fn recursive_average(
        &mut self,
        source: &Expr,
        index: usize,
        period: usize,
        rma: bool,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        let mut average = None;
        let alpha = if rma {
            1.0 / period as f64
        } else {
            2.0 / (period as f64 + 1.0)
        };
        let mut initial = Vec::new();
        for position in 0..=index {
            if let Some(value) = self.eval(source, position, depth + 1)?.number() {
                if let Some(previous) = average {
                    average = Some(alpha * value + (1.0 - alpha) * previous);
                } else if rma {
                    initial.push(value);
                    if initial.len() == period {
                        average = Some(initial.iter().sum::<f64>() / period as f64);
                    }
                } else {
                    // TradingView ta.ema seeds from its first finite source value.
                    average = Some(value);
                }
            }
        }
        Ok(number_value(average))
    }

    fn true_range_function(
        &mut self,
        name: &str,
        _arguments: &[crate::ast::Argument],
        index: usize,
        period: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        let true_range =
            |state: &mut Self, position: usize| -> Result<Option<f64>, Vec<Diagnostic>> {
                let bar = &state.bars[position];
                if position == 0 {
                    return Ok(Some(bar.high - bar.low));
                }
                let previous_close = state
                    .eval(&Expr::Name("close".into()), position - 1, depth + 1)?
                    .number();
                Ok(previous_close.map(|close| {
                    (bar.high - bar.low)
                        .max((bar.high - close).abs())
                        .max((bar.low - close).abs())
                }))
            };
        if name == "ta.tr" {
            return Ok(number_value(true_range(self, index)?));
        }
        let mut ranges = Vec::new();
        for position in 0..=index {
            if let Some(range) = true_range(self, position)? {
                ranges.push(range);
            }
        }
        if ranges.is_empty() {
            return Ok(Value::Missing);
        }
        let mut average = None;
        let alpha = 1.0 / period as f64;
        for range in ranges {
            average = Some(
                average
                    .map(|prior| alpha * range + (1.0 - alpha) * prior)
                    .unwrap_or(range),
            );
        }
        Ok(number_value(average))
    }

    fn rsi(
        &mut self,
        source: &Expr,
        index: usize,
        period: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        let mut gains = Vec::new();
        let mut losses = Vec::new();
        for position in 1..=index {
            let current = self.eval(source, position, depth + 1)?.number();
            let previous = self.eval(source, position - 1, depth + 1)?.number();
            if let Some((current, previous)) = current.zip(previous) {
                gains.push((current - previous).max(0.0));
                losses.push((previous - current).max(0.0));
            }
        }
        if gains.len() < period {
            return Ok(Value::Missing);
        }
        let mut gain = gains[..period].iter().sum::<f64>() / period as f64;
        let mut loss = losses[..period].iter().sum::<f64>() / period as f64;
        for step in period..gains.len() {
            gain = (gain * (period - 1) as f64 + gains[step]) / period as f64;
            loss = (loss * (period - 1) as f64 + losses[step]) / period as f64;
        }
        if loss == 0.0 {
            return Ok(Value::Number(100.0));
        }
        Ok(Value::Number(100.0 - 100.0 / (1.0 + gain / loss)))
    }

    fn cross_function(
        &mut self,
        name: &str,
        arguments: &[crate::ast::Argument],
        index: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        if index == 0 {
            return Ok(Value::Bool(false));
        }
        let left = argument_value(arguments, 0, "source1").ok_or_else(unsupported_arguments)?;
        let right = argument_value(arguments, 1, "source2").ok_or_else(unsupported_arguments)?;
        let current = self
            .eval(left, index, depth + 1)?
            .number()
            .zip(self.eval(right, index, depth + 1)?.number());
        let prior = self
            .eval(left, index - 1, depth + 1)?
            .number()
            .zip(self.eval(right, index - 1, depth + 1)?.number());
        let result = match (name, current, prior) {
            ("ta.crossover", Some((a, b)), Some((pa, pb))) => a > b && pa <= pb,
            ("ta.crossunder", Some((a, b)), Some((pa, pb))) => a < b && pa >= pb,
            ("ta.cross", Some((a, b)), Some((pa, pb))) => {
                (a > b && pa <= pb) || (a < b && pa >= pb)
            }
            _ => false,
        };
        Ok(Value::Bool(result))
    }

    fn bars_since(
        &mut self,
        condition: &Expr,
        index: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        for position in (0..=index).rev() {
            if self.eval(condition, position, depth + 1)?.boolean() == Some(true) {
                return Ok(Value::Number((index - position) as f64));
            }
        }
        Ok(Value::Missing)
    }

    fn value_when(
        &mut self,
        arguments: &[crate::ast::Argument],
        index: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        let condition =
            argument_value(arguments, 0, "condition").ok_or_else(unsupported_arguments)?;
        let source = argument_value(arguments, 1, "source").ok_or_else(unsupported_arguments)?;
        let occurrence = argument_value(arguments, 2, "occurrence")
            .map(|value| self.eval(value, index, depth + 1))
            .transpose()?
            .and_then(|value| value.number())
            .unwrap_or(0.0)
            .max(0.0) as usize;
        let mut found = 0;
        for position in (0..=index).rev() {
            if self.eval(condition, position, depth + 1)?.boolean() == Some(true) {
                if found == occurrence {
                    return self.eval(source, position, depth + 1);
                }
                found += 1;
            }
        }
        Ok(Value::Missing)
    }

    fn vwma(
        &mut self,
        arguments: &[crate::ast::Argument],
        index: usize,
        period: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        let source = argument_value(arguments, 0, "source").ok_or_else(unsupported_arguments)?;
        let start = index.saturating_add(1).saturating_sub(period);
        let mut weighted = 0.0;
        let mut total_volume = 0.0;
        for position in start..=index {
            let value = self.eval(source, position, depth + 1)?.number();
            if let (Some(value), Some(volume)) = (
                value,
                self.bars[position]
                    .volume
                    .is_finite()
                    .then_some(self.bars[position].volume),
            ) {
                weighted += value * volume;
                total_volume += volume;
            }
        }
        Ok(number_value(
            (total_volume > 0.0 && index + 1 >= period).then_some(weighted / total_volume),
        ))
    }

    pub(super) fn add_approximation_once(&mut self, message: &str) {
        if !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == message)
        {
            self.diagnostics.push(Diagnostic::warning(message));
        }
    }
}

fn close_percent_size(
    arguments: &[crate::ast::Argument],
    state: &mut RuntimeState<'_>,
    index: usize,
    depth: usize,
) -> Result<crate::strategy::ExitSize, Vec<Diagnostic>> {
    use crate::strategy::ExitSize;
    match argument_value(arguments, 3, "qty_percent").map(|expression| state.eval(expression, index, depth)).transpose()? {
        None | Some(Value::Missing) => Ok(ExitSize::Full),
        Some(Value::Number(percent)) if percent.is_finite() && percent > 0.0 && percent <= 100.0 => Ok(ExitSize::Percent(percent)),
        _ => Err(vec![Diagnostic::error(1, 1, "`strategy.close()` qty_percent must be finite, greater than zero, and at most 100, or `na`." )]),
    }
}

fn strategy_oca_arguments(
    arguments: &[crate::ast::Argument],
    state: &mut RuntimeState<'_>,
    index: usize,
    depth: usize,
    function: &str,
) -> Result<(Option<String>, crate::strategy::OcaType), Vec<Diagnostic>> {
    use crate::strategy::OcaType;
    let oca_name = argument_value(arguments, 5, "oca_name")
        .map(|expression| state.eval(expression, index, depth + 1))
        .transpose()?
        .map(|value| value.display())
        .filter(|value| !value.is_empty());
    let oca_value = match argument_value(arguments, 6, "oca_type")
        .map(|expression| state.eval(expression, index, depth + 1))
        .transpose()?
    {
        None | Some(Value::Missing) => "strategy.oca.none".to_string(),
        Some(Value::String(value)) if matches!(value.as_str(), "strategy.oca.none" | "strategy.oca.cancel" | "strategy.oca.reduce") => value,
        Some(value) => return Err(vec![Diagnostic::error(1, 1, format!("`{function}` oca_type must be strategy.oca.cancel, strategy.oca.reduce, or strategy.oca.none; received `{}`.", value.display()))]),
    };
    let oca_type = match oca_value.as_str() {
        "strategy.oca.none" => OcaType::None,
        "strategy.oca.cancel" => OcaType::Cancel,
        "strategy.oca.reduce" => OcaType::Reduce,
        _ => unreachable!(),
    };
    if oca_type != OcaType::None && oca_name.is_none() {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("`{function}` OCA cancel/reduce requires a nonempty oca_name."),
        )]);
    }
    Ok((oca_name, oca_type))
}

fn validate_strategy_arguments(
    function: &str,
    arguments: &[crate::ast::Argument],
    supported_names: &[(&str, usize)],
    max_positional: usize,
) -> Result<(), Vec<Diagnostic>> {
    let positional_count = arguments
        .iter()
        .filter(|argument| argument.name.is_none())
        .count();
    if positional_count > max_positional {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!(
                "`{function}()` has positional arguments outside the supported market-order model."
            ),
        )]);
    }
    if let Some(name) = arguments
        .iter()
        .filter_map(|argument| argument.name.as_deref())
        .find(|name| {
            !supported_names
                .iter()
                .any(|(supported, _)| supported == name)
        })
    {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!(
                "`{function}()` argument `{name}` is not supported by the current broker model."
            ),
        )]);
    }
    if supported_names.iter().any(|(name, position)| {
        *position != usize::MAX
            && arguments
                .iter()
                .filter(|argument| argument.name.as_deref() == Some(name))
                .count()
                > 0
            && positional_count > *position
    }) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("`{function}()` supplies the same argument both positionally and by name."),
        )]);
    }
    Ok(())
}
