use super::{Candle, Diagnostic, RuntimeState, Value};
use crate::ast::Expr;
use std::collections::HashSet;

pub(super) fn timestamp_ns_to_pine_ms(timestamp_ns: i64) -> i64 {
    timestamp_ns.div_euclid(quant_core::NANOS_PER_MILLISECOND)
}

pub(super) fn argument_value<'a>(
    arguments: &'a [crate::ast::Argument],
    position: usize,
    name: &str,
) -> Option<&'a Expr> {
    arguments
        .iter()
        .find(|argument| argument.name.as_deref() == Some(name))
        .map(|argument| &argument.value)
        .or_else(|| {
            arguments
                .iter()
                .filter(|argument| argument.name.is_none())
                .nth(position)
                .map(|argument| &argument.value)
        })
}

pub(super) fn option_name<'a>(
    arguments: &'a [crate::ast::Argument],
    position: usize,
    name: &str,
) -> Option<&'a str> {
    argument_value(arguments, position, name).and_then(|expression| match expression {
        Expr::Name(value) | Expr::String(value) => Some(value.as_str()),
        _ => None,
    })
}

pub(super) fn normalize_request_timeframe(value: &str) -> Option<String> {
    timeframe_definition(value).map(|(canonical, _)| canonical.to_string())
}

pub(super) fn timeframe_duration_ns(timeframe: &str) -> Option<i64> {
    let (canonical, seconds) = timeframe_definition(timeframe)?;
    (canonical == timeframe).then_some(seconds * quant_core::NANOS_PER_SECOND)
}

fn timeframe_definition(value: &str) -> Option<(&'static str, i64)> {
    match value.trim().to_ascii_lowercase().as_str() {
        "15" | "15m" => Some(("15m", 15 * quant_core::SECONDS_PER_MINUTE)),
        "60" | "1h" => Some(("1h", quant_core::SECONDS_PER_HOUR)),
        "240" | "4h" => Some(("4h", 4 * quant_core::SECONDS_PER_HOUR)),
        "d" | "1d" => Some(("1d", quant_core::SECONDS_PER_DAY)),
        _ => None,
    }
}

pub(super) fn align_security_values(
    chart_bars: &[Candle],
    requested_bars: &[Candle],
    chart_duration: i64,
    requested_duration: i64,
    requested_values: Vec<Value>,
    gaps_on: bool,
) -> Result<Vec<Value>, Vec<Diagnostic>> {
    if requested_values.len() != requested_bars.len()
        || chart_duration <= 0
        || requested_duration <= 0
    {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "The requested timeframe series could not be aligned.",
        )]);
    }
    for bars in [chart_bars, requested_bars] {
        if bars.iter().any(|bar| bar.time.is_none())
            || bars.windows(2).any(|pair| pair[0].time >= pair[1].time)
        {
            return Err(vec![Diagnostic::error(
                1,
                1,
                "Timeframe requests require ordered candle timestamps from stored app history.",
            )]);
        }
    }
    let mut aligned = Vec::with_capacity(chart_bars.len());
    let mut requested_index = 0usize;
    let mut latest = Value::Missing;
    for chart_bar in chart_bars {
        let chart_end = chart_bar
            .time
            .unwrap_or_default()
            .checked_add(chart_duration)
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    1,
                    1,
                    "Chart candle time exceeds the timeframe alignment range.",
                )]
            })?;
        let mut advanced = false;
        while let Some(requested_bar) = requested_bars.get(requested_index) {
            let requested_end = requested_bar
                .time
                .unwrap_or_default()
                .checked_add(requested_duration)
                .ok_or_else(|| {
                    vec![Diagnostic::error(
                        1,
                        1,
                        "Requested candle time exceeds the timeframe alignment range.",
                    )]
                })?;
            if requested_end > chart_end {
                break;
            }
            latest = requested_values[requested_index].clone();
            requested_index += 1;
            advanced = true;
        }
        aligned.push(if gaps_on && !advanced {
            Value::Missing
        } else {
            latest.clone()
        });
    }
    Ok(aligned)
}

pub(super) fn expression_names(expression: &Expr) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_expression_names(expression, &mut names);
    names
}

fn collect_expression_names(expression: &Expr, names: &mut HashSet<String>) {
    match expression {
        Expr::Name(name) => {
            names.insert(name.clone());
        }
        Expr::Unary { value, .. } | Expr::Index { value, .. } => {
            collect_expression_names(value, names)
        }
        Expr::Binary { left, right, .. } => {
            collect_expression_names(left, names);
            collect_expression_names(right, names);
        }
        Expr::Conditional { condition, yes, no } => {
            collect_expression_names(condition, names);
            collect_expression_names(yes, names);
            collect_expression_names(no, names);
        }
        Expr::Call { arguments, .. } => {
            for argument in arguments {
                collect_expression_names(&argument.value, names);
            }
        }
        Expr::Number(_) | Expr::String(_) => {}
    }
}

pub(super) fn expression_contains_security(expression: &Expr) -> bool {
    match expression {
        Expr::Call { name, arguments } => {
            name == "request.security"
                || arguments
                    .iter()
                    .any(|argument| expression_contains_security(&argument.value))
        }
        Expr::Unary { value, .. } | Expr::Index { value, .. } => {
            expression_contains_security(value)
        }
        Expr::Binary { left, right, .. } => {
            expression_contains_security(left) || expression_contains_security(right)
        }
        Expr::Conditional { condition, yes, no } => {
            expression_contains_security(condition)
                || expression_contains_security(yes)
                || expression_contains_security(no)
        }
        Expr::Number(_) | Expr::String(_) | Expr::Name(_) => false,
    }
}

pub(super) fn value_number(
    arguments: &[crate::ast::Argument],
    position: usize,
    name: &str,
    state: &mut RuntimeState<'_>,
    index: usize,
    depth: usize,
) -> Option<i64> {
    argument_value(arguments, position, name)
        .and_then(|value| state.eval(value, index, depth).ok())
        .and_then(|value| value.number())
        .filter(|value| value.is_finite())
        .map(|value| value.round() as i64)
}

pub(super) fn unsupported_arguments() -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        1,
        1,
        "The built-in call is missing a required argument.",
    )]
}
pub(super) fn number_value(value: Option<f64>) -> Value {
    value
        .filter(|value| value.is_finite())
        .map(Value::Number)
        .unwrap_or(Value::Missing)
}
pub(super) fn unary_number(
    value: Value,
    operation: fn(f64) -> f64,
) -> Result<Value, Vec<Diagnostic>> {
    Ok(number_value(value.number().map(operation)))
}
pub(super) fn binary_number(
    left: Value,
    right: Value,
    operation: fn(f64, f64) -> f64,
) -> Result<Value, Vec<Diagnostic>> {
    Ok(number_value(
        left.number()
            .zip(right.number())
            .map(|(left, right)| operation(left, right)),
    ))
}
pub(super) fn is_series_builtin(name: &str) -> bool {
    matches!(
        name,
        "open"
            | "high"
            | "low"
            | "close"
            | "volume"
            | "hl2"
            | "hlc3"
            | "ohlc4"
            | "time"
            | "bar_index"
    )
}

pub(super) fn safe_color(color: &str) -> String {
    let valid_hex = color.strip_prefix('#').is_some_and(|hex| {
        matches!(hex.len(), 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    let valid_name = !color.is_empty()
        && color.len() <= 24
        && color
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    if valid_hex || valid_name {
        color.to_string()
    } else {
        "#2962ff".to_string()
    }
}

pub(super) fn eval_binary(
    left: Value,
    operator: &str,
    right: Value,
) -> Result<Value, Vec<Diagnostic>> {
    if operator == "+" && (matches!(left, Value::String(_)) || matches!(right, Value::String(_))) {
        return Ok(Value::String(format!(
            "{}{}",
            left.display(),
            right.display()
        )));
    }
    let a = left.number();
    let b = right.number();
    match operator {
        "+" => Ok(number_value(a.zip(b).map(|(a, b)| a + b))),
        "-" => Ok(number_value(a.zip(b).map(|(a, b)| a - b))),
        "*" => Ok(number_value(a.zip(b).map(|(a, b)| a * b))),
        "/" => Ok(number_value(
            a.zip(b).filter(|(_, b)| *b != 0.0).map(|(a, b)| a / b),
        )),
        "%" => Ok(number_value(
            a.zip(b).filter(|(_, b)| *b != 0.0).map(|(a, b)| a % b),
        )),
        "==" => Ok(Value::Bool(match (&left, &right) {
            (Value::String(a), Value::String(b)) => a == b,
            _ => a.zip(b).is_some_and(|(a, b)| a == b),
        })),
        "!=" => Ok(Value::Bool(match (&left, &right) {
            (Value::String(a), Value::String(b)) => a != b,
            _ => a.zip(b).is_some_and(|(a, b)| a != b),
        })),
        "<" => Ok(Value::Bool(a.zip(b).is_some_and(|(a, b)| a < b))),
        ">" => Ok(Value::Bool(a.zip(b).is_some_and(|(a, b)| a > b))),
        "<=" => Ok(Value::Bool(a.zip(b).is_some_and(|(a, b)| a <= b))),
        ">=" => Ok(Value::Bool(a.zip(b).is_some_and(|(a, b)| a >= b))),
        _ => Err(vec![Diagnostic::error(
            1,
            1,
            format!("Unsupported operator `{operator}`."),
        )]),
    }
}
