use super::helpers::*;
use super::*;
use crate::ast::{Expr, Program, Statement};

impl<'a> RuntimeState<'a> {
    pub(super) fn evaluate(
        &mut self,
        program: &Program,
        progress: Option<&(dyn Fn(usize, usize) -> bool + Sync)>,
    ) -> Result<(), Vec<Diagnostic>> {
        for current in 0..self.bars.len() {
            if progress.is_some_and(|report| !report(current, self.bars.len())) {
                return Err(vec![Diagnostic::error(
                    1,
                    1,
                    "Pine strategy backtest was cancelled by the user.",
                )]);
            }
            self.current_bar = current;
            let filled_at_open = if let Some(strategy) = self.strategy.as_mut() {
                strategy.begin_bar(
                    current,
                    self.bars[current].open,
                    self.bars[current].high,
                    self.bars[current].low,
                    self.bars[current].close,
                    self.bars[current].time,
                )
            } else {
                false
            };
            self.prepare_bar(current)?;
            let calc_on_order_fills = self
                .strategy
                .as_ref()
                .is_some_and(|strategy| strategy.calc_on_order_fills());
            let bar_start_values = calc_on_order_fills.then(|| self.snapshot_bar_values(current));
            let initialized_var_at_bar_start =
                calc_on_order_fills.then(|| self.initialized_var.clone());
            if calc_on_order_fills {
                let bar_start_values = bar_start_values
                    .as_ref()
                    .expect("calc-on-fill baseline is present");
                let initialized_var_at_bar_start = initialized_var_at_bar_start
                    .as_ref()
                    .expect("calc-on-fill initialization baseline is present");
                let mut executed_this_bar = false;
                if filled_at_open {
                    self.evaluate_fill_triggered_statements(
                        &program.statements,
                        current,
                        bar_start_values,
                        initialized_var_at_bar_start,
                        false,
                        self.bars[current].time,
                    )?;
                    executed_this_bar = true;
                    for _ in 0..3 {
                        let filled = self.strategy.as_mut().is_some_and(|strategy| {
                            strategy.advance_history_tick(self.bars[current].time)
                        });
                        if filled {
                            self.evaluate_fill_triggered_statements(
                                &program.statements,
                                current,
                                bar_start_values,
                                initialized_var_at_bar_start,
                                true,
                                self.bars[current].time,
                            )?;
                        }
                    }
                }
                if executed_this_bar {
                    self.rollback_bar_values(
                        current,
                        &bar_start_values,
                        &initialized_var_at_bar_start,
                    );
                }
                self.evaluate_statements(&program.statements, current)?;
            } else {
                self.evaluate_statements(&program.statements, current)?;
            }
            let filled_at_close = self.strategy.as_mut().is_some_and(|strategy| {
                strategy.finish_bar(self.bars[current].close, self.bars[current].time)
            });
            if calc_on_order_fills && filled_at_close {
                self.evaluate_fill_triggered_statements(
                    &program.statements,
                    current,
                    bar_start_values
                        .as_ref()
                        .expect("calc-on-fill baseline is present"),
                    initialized_var_at_bar_start
                        .as_ref()
                        .expect("calc-on-fill initialization baseline is present"),
                    true,
                    self.bars[current].time,
                )?;
                if let Some(strategy) = self.strategy.as_mut() {
                    strategy.finish_bar(self.bars[current].close, self.bars[current].time);
                    strategy.defer_close_orders_to_next_bar_open();
                }
            }
            if progress.is_some_and(|report| !report(current + 1, self.bars.len())) {
                return Err(vec![Diagnostic::error(
                    1,
                    1,
                    "Pine strategy backtest was cancelled by the user.",
                )]);
            }
        }
        Ok(())
    }

    fn snapshot_bar_values(&self, index: usize) -> std::collections::HashMap<String, Value> {
        self.series
            .iter()
            .map(|(name, values)| {
                (
                    name.clone(),
                    values.get(index).cloned().unwrap_or(Value::Missing),
                )
            })
            .collect()
    }

    fn evaluate_fill_triggered_statements(
        &mut self,
        statements: &[Statement],
        index: usize,
        baseline: &std::collections::HashMap<String, Value>,
        initialized_var_baseline: &std::collections::HashSet<String>,
        rollback_before_execution: bool,
        time: Option<i64>,
    ) -> Result<(), Vec<Diagnostic>> {
        let mut should_rollback = rollback_before_execution;
        loop {
            if should_rollback {
                self.rollback_bar_values(index, baseline, initialized_var_baseline);
            }
            self.evaluate_statements(statements, index)?;
            let filled_immediately = self
                .strategy
                .as_mut()
                .is_some_and(|strategy| strategy.fill_immediate_closes_at_current_tick(time));
            if !filled_immediately {
                return Ok(());
            }
            should_rollback = true;
        }
    }

    fn rollback_bar_values(
        &mut self,
        index: usize,
        baseline: &std::collections::HashMap<String, Value>,
        initialized_var_baseline: &std::collections::HashSet<String>,
    ) {
        let varip_values = self
            .persistent_varip
            .iter()
            .map(|name| {
                (
                    name.clone(),
                    self.series
                        .get(name)
                        .and_then(|values| values.get(index))
                        .cloned()
                        .unwrap_or(Value::Missing),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        for (name, values) in &mut self.series {
            if values.len() <= index {
                values.resize(index + 1, Value::Missing);
            }
            values[index] = baseline.get(name).cloned().unwrap_or(Value::Missing);
        }
        for (name, value) in varip_values {
            let values = self.series.entry(name).or_default();
            if values.len() <= index {
                values.resize(index + 1, Value::Missing);
            }
            values[index] = value;
        }
        self.retained_string_bytes = self
            .series
            .values()
            .flatten()
            .map(|value| match value {
                Value::String(text) => text.len(),
                _ => 0,
            })
            .sum();
        self.initialized_var.clone_from(initialized_var_baseline);
    }

    fn evaluate_statements(
        &mut self,
        statements: &[Statement],
        index: usize,
    ) -> Result<(), Vec<Diagnostic>> {
        for statement in statements {
            match statement {
                Statement::Assign {
                    name,
                    value,
                    declaration,
                } => {
                    let already_initialized = match declaration {
                        crate::ast::DeclarationMode::Ordinary => false,
                        crate::ast::DeclarationMode::Var => self.initialized_var.contains(name),
                        crate::ast::DeclarationMode::Varip => self.initialized_varip.contains(name),
                    };
                    if already_initialized {
                        continue;
                    }
                    let evaluated = self.eval(value, index, 0)?;
                    self.write_series(name, index, evaluated)?;
                    match declaration {
                        crate::ast::DeclarationMode::Ordinary => {}
                        crate::ast::DeclarationMode::Var => {
                            self.initialized_var.insert(name.clone());
                        }
                        crate::ast::DeclarationMode::Varip => {
                            self.initialized_varip.insert(name.clone());
                        }
                    }
                }
                Statement::Expression(expression) => {
                    self.evaluate_expression_statement(expression, index)?
                }
                Statement::If {
                    condition,
                    body,
                    otherwise,
                } => {
                    if self.eval(condition, index, 0)?.boolean() == Some(true) {
                        self.evaluate_statements(body, index)?;
                    } else {
                        self.evaluate_statements(otherwise, index)?;
                    }
                }
                Statement::For {
                    name,
                    start,
                    end,
                    body,
                } => {
                    let first = self.eval(start, index, 0)?.number().unwrap_or(0.0);
                    let last = self.eval(end, index, 0)?.number().unwrap_or(-1.0);
                    if !first.is_finite()
                        || !last.is_finite()
                        || first.fract() != 0.0
                        || last.fract() != 0.0
                    {
                        return Err(vec![Diagnostic::error(
                            1,
                            1,
                            "Pine `for` loop bounds must be finite integers.",
                        )]);
                    }
                    let first = first as i64;
                    let last = last as i64;
                    let count = last.saturating_sub(first).saturating_add(1);
                    if count > MAX_PINE_LOOP_ITERATIONS as i64 {
                        return Err(vec![Diagnostic::error(
                            1,
                            1,
                            "Pine `for` loop exceeds the bounded 1,000-iteration limit.",
                        )]);
                    }
                    for iteration in first..=last {
                        self.write_series(name, index, Value::Number(iteration as f64))?;
                        self.evaluate_statements(body, index)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn prepare_bar(&mut self, index: usize) -> Result<(), Vec<Diagnostic>> {
        let additional_values = self
            .series
            .values()
            .filter(|values| values.len() == index)
            .count();
        if self
            .retained_series_values
            .saturating_add(additional_values)
            > MAX_STRATEGY_RETAINED_SERIES_VALUES
        {
            return Err(vec![Diagnostic::error(1, 1, format!("Pine strategy exceeded the bounded {} retained series-value limit for this backtest.", MAX_STRATEGY_RETAINED_SERIES_VALUES))]);
        }
        let additional_string_bytes = self
            .series
            .iter()
            .filter(|(name, values)| values.len() == index && self.persistent.contains(*name))
            .map(|(_, values)| match values.last() {
                Some(Value::String(text)) => text.len(),
                _ => 0,
            })
            .sum::<usize>();
        if self
            .retained_string_bytes
            .saturating_add(additional_string_bytes)
            > MAX_STRATEGY_RETAINED_STRING_BYTES
        {
            return Err(vec![Diagnostic::error(1, 1, format!("Pine strategy exceeded the bounded {} retained string-byte limit for this backtest.", MAX_STRATEGY_RETAINED_STRING_BYTES))]);
        }
        for (name, values) in &mut self.series {
            let carried = if self.persistent.contains(name) {
                values.last().cloned().unwrap_or(Value::Missing)
            } else {
                Value::Missing
            };
            if values.len() == index {
                values.push(carried);
            }
        }
        self.retained_series_values = self
            .retained_series_values
            .saturating_add(additional_values);
        self.retained_string_bytes = self
            .retained_string_bytes
            .saturating_add(additional_string_bytes);
        Ok(())
    }

    fn write_series(
        &mut self,
        name: &str,
        index: usize,
        value: Value,
    ) -> Result<(), Vec<Diagnostic>> {
        let current_len = self.series.get(name).map_or(0, Vec::len);
        let additional_values = index.saturating_add(1).saturating_sub(current_len);
        let previous_string_bytes = self
            .series
            .get(name)
            .and_then(|values| values.get(index))
            .map_or(0, |stored| match stored {
                Value::String(text) => text.len(),
                _ => 0,
            });
        let next_string_bytes = match &value {
            Value::String(text) => text.len(),
            _ => 0,
        };
        if next_string_bytes > MAX_STRATEGY_STRING_VALUE_BYTES {
            return Err(vec![Diagnostic::error(1, 1, format!("Pine strategy series strings are limited to {MAX_STRATEGY_STRING_VALUE_BYTES} bytes per value."))]);
        }
        let retained_string_bytes = self
            .retained_string_bytes
            .saturating_sub(previous_string_bytes)
            .saturating_add(next_string_bytes);
        if retained_string_bytes > MAX_STRATEGY_RETAINED_STRING_BYTES {
            return Err(vec![Diagnostic::error(1, 1, format!("Pine strategy exceeded the bounded {} retained string-byte limit for this backtest.", MAX_STRATEGY_RETAINED_STRING_BYTES))]);
        }
        if self
            .retained_series_values
            .saturating_add(additional_values)
            > MAX_STRATEGY_RETAINED_SERIES_VALUES
        {
            return Err(vec![Diagnostic::error(1, 1, format!("Pine strategy exceeded the bounded {} retained series-value limit for this backtest.", MAX_STRATEGY_RETAINED_SERIES_VALUES))]);
        }
        let values = self.series.entry(name.to_string()).or_default();
        if values.len() <= index {
            values.resize(index + 1, Value::Missing);
            self.retained_series_values = self
                .retained_series_values
                .saturating_add(additional_values);
        }
        values[index] = value;
        self.retained_string_bytes = retained_string_bytes;
        Ok(())
    }

    fn evaluate_expression_statement(
        &mut self,
        expression: &Expr,
        index: usize,
    ) -> Result<(), Vec<Diagnostic>> {
        let Expr::Call { name, arguments } = expression else {
            let _ = self.eval(expression, index, 0)?;
            return Ok(());
        };
        match name.as_str() {
            "indicator" => Ok(()),
            "strategy" => Ok(()),
            "plot" | "plotshape" | "plotchar" | "hline" => {
                self.evaluate_plot(name, arguments, index)
            }
            "fill" => {
                self.add_approximation_once(
                    "`fill()` is accepted but chart fill regions are not rendered yet.",
                );
                Ok(())
            }
            "alertcondition" => {
                self.add_approximation_once("`alertcondition()` is informational only; this runtime does not create alerts.");
                Ok(())
            }
            _ => {
                let _ = self.eval(expression, index, 0)?;
                Ok(())
            }
        }
    }

    fn evaluate_plot(
        &mut self,
        function: &str,
        arguments: &[crate::ast::Argument],
        index: usize,
    ) -> Result<(), Vec<Diagnostic>> {
        let source = argument_value(arguments, 0, "series").ok_or_else(|| {
            vec![Diagnostic::error(
                1,
                1,
                format!("`{function}()` requires a source value."),
            )]
        })?;
        let value = self.eval(source, index, 0)?;
        let title = argument_value(arguments, 1, "title")
            .map(|expr| self.eval(expr, index, 0))
            .transpose()?
            .unwrap_or(Value::String(function.to_string()))
            .display();
        let color = argument_value(arguments, 2, "color")
            .map(|expr| self.eval(expr, index, 0))
            .transpose()?
            .map(|value| value.display())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "#2962ff".to_string());
        let color = safe_color(&color);
        if function == "plotshape" || function == "plotchar" {
            self.add_approximation_once("`plotshape()`/`plotchar()` location, glyph, text, and size are drawn as a small close-price marker.");
        }
        if arguments.iter().any(|argument| {
            matches!(
                argument.name.as_deref(),
                Some("linewidth" | "style" | "offset" | "transp")
            )
        }) {
            self.add_approximation_once("Plot linewidth, style, offset, and transparency options are not preserved by the current canvas renderer.");
        }
        let (numeric, kind) = if function == "hline" {
            (value.numeric_output(), "line")
        } else if function == "plot" {
            (value.numeric_output(), "line")
        } else {
            (
                if value.boolean().unwrap_or(false) {
                    Some(self.bars[index].close)
                } else {
                    None
                },
                "shape",
            )
        };
        let overlay = self.program.validation.overlay
            || arguments
                .iter()
                .find(|argument| argument.name.as_deref() == Some("force_overlay"))
                .is_some_and(|argument| {
                    matches!(self.eval(&argument.value, index, 0), Ok(Value::Bool(true)))
                });
        let key = format!("{function}:{title}");
        if !self.plot_lookup.contains_key(&key) && self.plots.len() >= MAX_PLOTS {
            return Err(vec![Diagnostic::error(
                1,
                1,
                "Indicator output exceeds the 16 plot limit.",
            )]);
        }
        let plot_index = *self.plot_lookup.entry(key.clone()).or_insert_with(|| {
            self.plots.push(PlotOutput {
                title,
                overlay,
                color,
                kind: kind.to_string(),
                values: vec![None; self.bars.len()],
            });
            self.plots.len() - 1
        });
        if let Some(plot) = self.plots.get_mut(plot_index) {
            plot.values[index] = numeric;
        }
        Ok(())
    }

    pub(super) fn eval(
        &mut self,
        expression: &Expr,
        index: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        self.evaluation_steps = self.evaluation_steps.saturating_add(1);
        if self.evaluation_steps > self.evaluation_step_limit {
            return Err(vec![Diagnostic::error(
                1,
                1,
                "Pine calculation exceeded the bounded evaluation step budget.",
            )]);
        }
        if depth > 128 {
            return Err(vec![Diagnostic::error(
                1,
                1,
                "Expression nesting exceeded the runtime limit.",
            )]);
        }
        match expression {
            Expr::Number(value) => Ok(Value::Number(*value)),
            Expr::String(value) => Ok(Value::String(value.clone())),
            Expr::Name(name) => Ok(self.resolve_name(name, index)),
            Expr::Unary { operator, value } => {
                let value = self.eval(value, index, depth + 1)?;
                match operator.as_str() {
                    "-" => Ok(value
                        .number()
                        .map(|number| Value::Number(-number))
                        .unwrap_or(Value::Missing)),
                    "+" => Ok(value.number().map(Value::Number).unwrap_or(Value::Missing)),
                    "not" => Ok(value
                        .boolean()
                        .map(|boolean| Value::Bool(!boolean))
                        .unwrap_or(Value::Missing)),
                    _ => Ok(Value::Missing),
                }
            }
            Expr::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.eval(left, index, depth + 1)?;
                if operator == "and" || operator == "&&" {
                    return Ok(Value::Bool(
                        left.boolean() == Some(true)
                            && self.eval(right, index, depth + 1)?.boolean() == Some(true),
                    ));
                }
                if operator == "or" || operator == "||" {
                    return Ok(Value::Bool(
                        left.boolean() == Some(true)
                            || self.eval(right, index, depth + 1)?.boolean() == Some(true),
                    ));
                }
                let right = self.eval(right, index, depth + 1)?;
                eval_binary(left, operator, right)
            }
            Expr::Conditional { condition, yes, no } => {
                if self.eval(condition, index, depth + 1)?.boolean() == Some(true) {
                    self.eval(yes, index, depth + 1)
                } else {
                    self.eval(no, index, depth + 1)
                }
            }
            Expr::Index { value, offset } => {
                let offset = self
                    .eval(offset, index, depth + 1)?
                    .number()
                    .unwrap_or(-1.0);
                if !offset.is_finite()
                    || offset < 0.0
                    || offset.fract() != 0.0
                    || offset as usize > MAX_HISTORY_OFFSET
                {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        "History offset must be a nonnegative integer within the runtime limit.",
                    )]);
                }
                self.eval_at_offset(value, index, offset as usize, depth + 1)
            }
            Expr::Call { name, arguments } => self.eval_call(name, arguments, index, depth + 1),
        }
    }

    fn eval_at_offset(
        &mut self,
        expression: &Expr,
        index: usize,
        offset: usize,
        depth: usize,
    ) -> Result<Value, Vec<Diagnostic>> {
        if offset > index {
            return Ok(Value::Missing);
        }
        match expression {
            Expr::Name(name) if self.series.contains_key(name) => Ok(self
                .series
                .get(name)
                .and_then(|values| values.get(index - offset))
                .cloned()
                .unwrap_or(Value::Missing)),
            Expr::Name(name) if is_series_builtin(name) => {
                Ok(self.resolve_name(name, index - offset))
            }
            _ => self.eval(expression, index - offset, depth + 1),
        }
    }

    fn resolve_name(&self, name: &str, index: usize) -> Value {
        if let Some(value) = self.resolve_strategy_name(name) {
            return value;
        }
        let bar = self.bars.get(index);
        match name {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "na" => Value::Missing,
            "open" => bar
                .map(|bar| Value::Number(bar.open))
                .unwrap_or(Value::Missing),
            "high" => bar
                .map(|bar| Value::Number(bar.high))
                .unwrap_or(Value::Missing),
            "low" => bar
                .map(|bar| Value::Number(bar.low))
                .unwrap_or(Value::Missing),
            "close" => bar
                .map(|bar| Value::Number(bar.close))
                .unwrap_or(Value::Missing),
            "volume" => bar
                .map(|bar| Value::Number(bar.volume))
                .unwrap_or(Value::Missing),
            "hl2" => bar
                .map(|bar| Value::Number((bar.high + bar.low) / 2.0))
                .unwrap_or(Value::Missing),
            "hlc3" => bar
                .map(|bar| Value::Number((bar.high + bar.low + bar.close) / 3.0))
                .unwrap_or(Value::Missing),
            "ohlc4" => bar
                .map(|bar| Value::Number((bar.open + bar.high + bar.low + bar.close) / 4.0))
                .unwrap_or(Value::Missing),
            "bar_index" => Value::Number(index as f64),
            "time" => bar
                .and_then(|bar| bar.time)
                .map(|time| Value::Number(timestamp_ns_to_pine_ms(time) as f64))
                .unwrap_or(Value::Missing),
            "barstate.isfirst" => Value::Bool(index == 0),
            "barstate.islast" => Value::Bool(index + 1 == self.bars.len()),
            "barstate.isconfirmed" => Value::Bool(true),
            "barstate.isrealtime" => Value::Bool(false),
            "syminfo.tickerid" | "syminfo.ticker" => {
                Value::String("selected-app-instrument".to_string())
            }
            "timeframe.period" => Value::String(self.chart_timeframe.to_string()),
            "barmerge.gaps_off"
            | "barmerge.gaps_on"
            | "barmerge.lookahead_off"
            | "barmerge.lookahead_on" => Value::String(name.to_string()),
            "color.black" => Value::String("#000000".to_string()),
            "color.white" => Value::String("#ffffff".to_string()),
            "color.red" => Value::String("#f23645".to_string()),
            "color.green" => Value::String("#089981".to_string()),
            "color.lime" => Value::String("#00e676".to_string()),
            "color.blue" => Value::String("#2962ff".to_string()),
            "color.orange" => Value::String("#ff9800".to_string()),
            "color.yellow" => Value::String("#ffeb3b".to_string()),
            "color.aqua" => Value::String("#00bcd4".to_string()),
            "color.fuchsia" => Value::String("#e040fb".to_string()),
            "color.purple" => Value::String("#9c27b0".to_string()),
            "color.gray" => Value::String("#787b86".to_string()),
            "color.maroon" => Value::String("#800000".to_string()),
            "color.navy" => Value::String("#000080".to_string()),
            "color.olive" => Value::String("#808000".to_string()),
            "color.silver" => Value::String("#c0c0c0".to_string()),
            "color.teal" => Value::String("#008080".to_string()),
            "math.pi" => Value::Number(std::f64::consts::PI),
            "math.e" => Value::Number(std::f64::consts::E),
            "plot.style_line" | "display.all" | "format.price" => Value::String(name.to_string()),
            _ => self
                .series
                .get(name)
                .and_then(|values| values.get(index))
                .cloned()
                .unwrap_or(Value::Missing),
        }
    }
}
