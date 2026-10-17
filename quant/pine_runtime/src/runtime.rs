use crate::ast::{Program, Statement};
use crate::parser;
use crate::strategy::StrategyCommission;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_SOURCE_BYTES: usize = 24 * 1024;
pub(crate) const MAX_STATEMENTS: usize = 256;
pub const MAX_CANDLES: usize = 1_000;
pub const MAX_STRATEGY_CANDLES: usize = 20_000;
pub const MAX_STRATEGY_MAGNIFIER_CANDLES: usize = 40_000;
pub const MAX_STRATEGY_RETAINED_SERIES_VALUES: usize = 100_000;
pub const MAX_STRATEGY_RETAINED_STRING_BYTES: usize = 1_048_576;
const MAX_STRATEGY_STRING_VALUE_BYTES: usize = 512;
pub const MAX_STRATEGY_SECURITY_CACHE_ENTRIES: usize = 2;
const MAX_PINE_LOOP_ITERATIONS: usize = 1_000;
pub const MAX_HISTORY_OFFSET: usize = 10_000;
pub const MAX_EVALUATION_STEPS: usize = 2_000_000;
pub const MAX_STRATEGY_EVALUATION_STEPS: usize = 20_000_000;
pub const MAX_PLOTS: usize = 16;

#[derive(Clone, Debug, Deserialize)]
pub struct Candle {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    #[serde(default)]
    pub volume: f64,
    #[serde(default)]
    pub time: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl Diagnostic {
    pub(crate) fn error(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            severity: "error".to_string(),
            line,
            column,
            message: message.into(),
        }
    }
    pub(crate) fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: "approximation".to_string(),
            line: 0,
            column: 0,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Validation {
    pub valid: bool,
    pub script_type: String,
    pub version: u8,
    pub name: String,
    pub overlay: bool,
    pub approximations: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlotOutput {
    pub title: String,
    pub overlay: bool,
    pub color: String,
    pub kind: String,
    pub values: Vec<Option<f64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndicatorOutput {
    pub valid: bool,
    pub name: String,
    pub overlay: bool,
    pub plots: Vec<PlotOutput>,
    pub diagnostics: Vec<Diagnostic>,
    pub requested_timeframes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrategyTrade {
    pub entry_id: String,
    pub direction: String,
    pub entry_bar: usize,
    pub exit_bar: usize,
    pub entry_time: Option<i64>,
    pub exit_time: Option<i64>,
    pub entry_price: f64,
    pub exit_price: f64,
    pub quantity: f64,
    pub gross_return_pct: f64,
    pub net_return_pct: f64,
    pub gross_pnl: f64,
    pub net_pnl: f64,
    pub max_runup: f64,
    pub max_drawdown: f64,
    pub max_runup_percent: f64,
    pub max_drawdown_percent: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrategyOutput {
    pub valid: bool,
    pub name: String,
    pub trades: Vec<StrategyTrade>,
    pub closed_trades: usize,
    pub wins: usize,
    pub win_rate_pct: Option<f64>,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub net_profit: f64,
    pub initial_capital: f64,
    pub max_runup: f64,
    pub max_runup_percent: f64,
    pub max_drawdown: f64,
    pub max_drawdown_percent: f64,
    pub max_drawdown_initial_capital_percent: f64,
    pub profit_factor: Option<f64>,
    pub profit_factor_unbounded: bool,
    pub commission: Option<StrategyCommission>,
    pub slippage_ticks: usize,
    pub calc_on_order_fills: bool,
    pub instrument_tick_size: Option<f64>,
    pub backtest_fill_limits_assumption_ticks: usize,
    pub bar_magnifier_enabled: bool,
    pub bar_magnifier_covered_chart_bars: usize,
    pub bar_magnifier_fallback_chart_bars: usize,
    pub equity_curve: Vec<f64>,
    pub open_position_size: f64,
    pub plots: Vec<PlotOutput>,
    pub diagnostics: Vec<Diagnostic>,
    pub input_overrides: HashMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrategyInputDefinition {
    pub title: String,
    pub input_type: String,
    pub default_value: serde_json::Value,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
}

pub fn run_indicator(source: &str, candles: &[Candle]) -> Result<IndicatorOutput, Vec<Diagnostic>> {
    run_indicator_with_timeframes(source, candles, "1d", &HashMap::new())
}

pub fn run_indicator_with_timeframes(
    source: &str,
    candles: &[Candle],
    chart_timeframe: &str,
    available_timeframes: &HashMap<String, Vec<Candle>>,
) -> Result<IndicatorOutput, Vec<Diagnostic>> {
    run_indicator_with_inputs(
        source,
        candles,
        chart_timeframe,
        available_timeframes,
        &HashMap::new(),
    )
}

pub fn run_indicator_with_inputs(
    source: &str,
    candles: &[Candle],
    chart_timeframe: &str,
    available_timeframes: &HashMap<String, Vec<Candle>>,
    input_sources: &HashMap<String, Vec<Option<f64>>>,
) -> Result<IndicatorOutput, Vec<Diagnostic>> {
    if let Some(result) = crate::compatibility::run_luxalgo_nwe(source, candles) {
        return result;
    }
    if let Some(result) = crate::compatibility::run_hpotter_dss(
        source,
        candles,
        chart_timeframe,
        available_timeframes,
    ) {
        return result;
    }
    let program = parser::parse(source)?;
    if program.is_strategy {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "This source declares `strategy()`; run it with the Pine backtest runtime.",
        )]);
    }
    if candles.len() > MAX_CANDLES {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("Indicator input exceeds the {MAX_CANDLES} candle limit."),
        )]);
    }
    if let Some((name, _)) = input_sources
        .iter()
        .find(|(_, values)| values.len() != candles.len())
    {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("Pine source input `{name}` must align with the selected chart candle series."),
        )]);
    }
    if let Some(name) = input_sources.iter().find_map(|(name, values)| {
        values
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
            .then_some(name)
    }) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("Pine source input `{name}` contains a non-finite value."),
        )]);
    }
    if let Some((timeframe, _)) = available_timeframes
        .iter()
        .find(|(_, bars)| bars.len() > MAX_CANDLES)
    {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("Timeframe {timeframe} exceeds the {MAX_CANDLES} candle limit."),
        )]);
    }
    if candles.iter().any(|bar| {
        ![bar.open, bar.high, bar.low, bar.close, bar.volume]
            .iter()
            .all(|value| value.is_finite())
    }) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Candle input contains a non-finite value.",
        )]);
    }
    if program.validation.overlay && candles.iter().any(|bar| bar.close <= 0.0) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Overlay calculation requires positive closing prices.",
        )]);
    }
    let mut state = RuntimeState::new(
        &program,
        candles,
        available_timeframes,
        chart_timeframe,
        input_sources,
        chart_timeframe,
        MAX_CANDLES,
    );
    state.evaluate(&program, None)?;
    let mut requested_timeframes = state
        .requested_timeframes
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    requested_timeframes.sort();
    Ok(IndicatorOutput {
        valid: true,
        name: program.validation.name.clone(),
        overlay: program.validation.overlay,
        plots: state.plots,
        diagnostics: state.diagnostics,
        requested_timeframes,
    })
}

pub fn run_strategy(
    source: &str,
    candles: &[Candle],
    buy_fee_pct: f64,
    sell_fee_pct: f64,
) -> Result<StrategyOutput, Vec<Diagnostic>> {
    run_strategy_with_inputs(source, candles, buy_fee_pct, sell_fee_pct, &HashMap::new())
}

pub fn strategy_uses_bar_magnifier(source: &str) -> Result<bool, Vec<Diagnostic>> {
    let program = parser::parse(source)?;
    if !program.is_strategy {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Backtest source must declare `strategy()`.",
        )]);
    }
    Ok(
        crate::strategy::StrategyConfig::from_program(&program, 0.0, 0.0, None)?
            .uses_bar_magnifier(),
    )
}

pub fn strategy_input_definitions(
    source: &str,
) -> Result<Vec<StrategyInputDefinition>, Vec<Diagnostic>> {
    let program = parser::parse(source)?;
    if !program.is_strategy {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Backtest source must declare `strategy()`.",
        )]);
    }
    let mut definitions = Vec::new();
    collect_strategy_inputs(&program.statements, &mut definitions);
    Ok(definitions)
}

fn collect_strategy_inputs(
    statements: &[Statement],
    definitions: &mut Vec<StrategyInputDefinition>,
) {
    for statement in statements {
        match statement {
            Statement::Assign {
                value: crate::ast::Expr::Call { name, arguments },
                ..
            } if matches!(
                name.as_str(),
                "input.int" | "input.float" | "input.bool" | "input.string"
            ) =>
            {
                let default = arguments
                    .iter()
                    .find(|arg| arg.name.as_deref() == Some("defval"))
                    .or_else(|| arguments.iter().find(|arg| arg.name.is_none()));
                let title = arguments
                    .iter()
                    .find(|arg| arg.name.as_deref() == Some("title"))
                    .or_else(|| arguments.iter().filter(|arg| arg.name.is_none()).nth(1));
                let (Some(default), Some(title)) = (default, title) else {
                    continue;
                };
                let (Some(default_value), Some(title_value)) = (
                    literal_json_value(&default.value),
                    literal_json_value(&title.value),
                ) else {
                    continue;
                };
                let Some(title) = title_value.as_str() else {
                    continue;
                };
                let numeric_argument = |name: &str| {
                    arguments
                        .iter()
                        .find(|arg| arg.name.as_deref() == Some(name))
                        .and_then(|arg| literal_json_value(&arg.value))
                        .and_then(|value| value.as_f64())
                };
                definitions.push(StrategyInputDefinition {
                    title: title.to_string(),
                    input_type: name.trim_start_matches("input.").to_string(),
                    default_value,
                    min_value: numeric_argument("minval"),
                    max_value: numeric_argument("maxval"),
                });
            }
            Statement::If {
                body, otherwise, ..
            } => {
                collect_strategy_inputs(body, definitions);
                collect_strategy_inputs(otherwise, definitions);
            }
            Statement::For { body, .. } => collect_strategy_inputs(body, definitions),
            _ => {}
        }
    }
}

fn literal_json_value(expression: &crate::ast::Expr) -> Option<serde_json::Value> {
    match expression {
        crate::ast::Expr::Number(value) if value.is_finite() => {
            serde_json::Number::from_f64(*value).map(serde_json::Value::Number)
        }
        crate::ast::Expr::String(value) => Some(serde_json::Value::String(value.clone())),
        crate::ast::Expr::Name(value) if value == "true" => Some(serde_json::Value::Bool(true)),
        crate::ast::Expr::Name(value) if value == "false" => Some(serde_json::Value::Bool(false)),
        _ => None,
    }
}

pub fn run_strategy_with_inputs(
    source: &str,
    candles: &[Candle],
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    input_overrides: &HashMap<String, serde_json::Value>,
) -> Result<StrategyOutput, Vec<Diagnostic>> {
    run_strategy_with_context(
        source,
        candles,
        "1d",
        &HashMap::new(),
        buy_fee_pct,
        sell_fee_pct,
        input_overrides,
    )
}

pub fn run_strategy_with_context(
    source: &str,
    candles: &[Candle],
    chart_timeframe: &str,
    available_timeframes: &HashMap<String, Vec<Candle>>,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    input_overrides: &HashMap<String, serde_json::Value>,
) -> Result<StrategyOutput, Vec<Diagnostic>> {
    run_strategy_with_history_limit(
        source,
        candles,
        chart_timeframe,
        available_timeframes,
        MAX_CANDLES,
        buy_fee_pct,
        sell_fee_pct,
        input_overrides,
    )
}

pub fn run_strategy_with_history_limit(
    source: &str,
    candles: &[Candle],
    chart_timeframe: &str,
    available_timeframes: &HashMap<String, Vec<Candle>>,
    history_limit: usize,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    input_overrides: &HashMap<String, serde_json::Value>,
) -> Result<StrategyOutput, Vec<Diagnostic>> {
    run_strategy_with_history_limit_and_magnifier(
        source,
        candles,
        chart_timeframe,
        available_timeframes,
        &[],
        None,
        history_limit,
        buy_fee_pct,
        sell_fee_pct,
        input_overrides,
    )
}

pub fn run_strategy_with_history_limit_and_magnifier(
    source: &str,
    candles: &[Candle],
    chart_timeframe: &str,
    available_timeframes: &HashMap<String, Vec<Candle>>,
    magnifier_bars: &[Candle],
    magnifier_timeframe: Option<&str>,
    history_limit: usize,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    input_overrides: &HashMap<String, serde_json::Value>,
) -> Result<StrategyOutput, Vec<Diagnostic>> {
    run_strategy_with_history_limit_and_magnifier_and_tick_size(
        source,
        candles,
        chart_timeframe,
        available_timeframes,
        magnifier_bars,
        magnifier_timeframe,
        history_limit,
        buy_fee_pct,
        sell_fee_pct,
        input_overrides,
        None,
    )
}

pub fn run_strategy_with_history_limit_and_magnifier_and_tick_size(
    source: &str,
    candles: &[Candle],
    chart_timeframe: &str,
    available_timeframes: &HashMap<String, Vec<Candle>>,
    magnifier_bars: &[Candle],
    magnifier_timeframe: Option<&str>,
    history_limit: usize,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    input_overrides: &HashMap<String, serde_json::Value>,
    tick_size: Option<f64>,
) -> Result<StrategyOutput, Vec<Diagnostic>> {
    run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start(
        source,
        candles,
        chart_timeframe,
        available_timeframes,
        magnifier_bars,
        magnifier_timeframe,
        history_limit,
        buy_fee_pct,
        sell_fee_pct,
        input_overrides,
        tick_size,
        0,
    )
}

pub fn run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start(
    source: &str,
    candles: &[Candle],
    chart_timeframe: &str,
    available_timeframes: &HashMap<String, Vec<Candle>>,
    magnifier_bars: &[Candle],
    magnifier_timeframe: Option<&str>,
    history_limit: usize,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    input_overrides: &HashMap<String, serde_json::Value>,
    tick_size: Option<f64>,
    evaluation_start_bar: usize,
) -> Result<StrategyOutput, Vec<Diagnostic>> {
    run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start_and_progress(
        source,
        candles,
        chart_timeframe,
        available_timeframes,
        magnifier_bars,
        magnifier_timeframe,
        history_limit,
        buy_fee_pct,
        sell_fee_pct,
        input_overrides,
        tick_size,
        evaluation_start_bar,
        None,
    )
}

pub fn run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start_and_progress(
    source: &str,
    candles: &[Candle],
    chart_timeframe: &str,
    available_timeframes: &HashMap<String, Vec<Candle>>,
    magnifier_bars: &[Candle],
    magnifier_timeframe: Option<&str>,
    history_limit: usize,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    input_overrides: &HashMap<String, serde_json::Value>,
    tick_size: Option<f64>,
    evaluation_start_bar: usize,
    progress: Option<&(dyn Fn(usize, usize) -> bool + Sync)>,
) -> Result<StrategyOutput, Vec<Diagnostic>> {
    if history_limit == 0 || history_limit > MAX_STRATEGY_CANDLES {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("Strategy history limit must be between 1 and {MAX_STRATEGY_CANDLES} candles."),
        )]);
    }
    if !buy_fee_pct.is_finite()
        || !sell_fee_pct.is_finite()
        || buy_fee_pct < 0.0
        || sell_fee_pct < 0.0
    {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Strategy fees must be finite, nonnegative percentages.",
        )]);
    }
    let program = parser::parse(source)?;
    if !program.is_strategy {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Backtest source must declare `strategy()`.",
        )]);
    }
    if candles.is_empty() || candles.len() > history_limit {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("Strategy history must contain 1 to {history_limit} candles."),
        )]);
    }
    if evaluation_start_bar >= candles.len() {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Strategy evaluation start bar must fall inside the supplied candle history.",
        )]);
    }
    if let Some((timeframe, _)) = available_timeframes
        .iter()
        .find(|(_, bars)| bars.len() > history_limit)
    {
        return Err(vec![Diagnostic::error(1, 1, format!("Strategy context timeframe {timeframe} exceeds the {history_limit} candle history limit."))]);
    }
    if magnifier_bars.len() > MAX_STRATEGY_MAGNIFIER_CANDLES {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!(
                "Bar Magnifier input exceeds the {MAX_STRATEGY_MAGNIFIER_CANDLES} candle limit."
            ),
        )]);
    }
    let magnifier_groups = validate_and_group_magnifier_bars(
        candles,
        chart_timeframe,
        magnifier_bars,
        magnifier_timeframe,
    )?;
    if candles.iter().any(|bar| {
        ![bar.open, bar.high, bar.low, bar.close, bar.volume]
            .iter()
            .all(|value| value.is_finite())
            || bar.open <= 0.0
            || bar.high <= 0.0
            || bar.low <= 0.0
            || bar.close <= 0.0
            || bar.volume < 0.0
            || bar.high < bar.low
            || bar.open < bar.low
            || bar.open > bar.high
            || bar.close < bar.low
            || bar.close > bar.high
    }) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Strategy candles contain invalid or inconsistent OHLCV values.",
        )]);
    }
    if candles.windows(2).any(|pair| {
        pair[0].time.is_none() || pair[1].time.is_none() || pair[0].time >= pair[1].time
    }) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Strategy candle timestamps must be present and strictly increasing.",
        )]);
    }
    if candles[0].time.is_none() {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Strategy candle timestamps must be present and strictly increasing.",
        )]);
    }
    let input_sources = HashMap::new();
    let mut state = RuntimeState::new(
        &program,
        candles,
        available_timeframes,
        chart_timeframe,
        &input_sources,
        chart_timeframe,
        history_limit,
    );
    state.evaluation_step_limit = MAX_STRATEGY_EVALUATION_STEPS;
    state.strategy_evaluation_start_bar = evaluation_start_bar;
    state.input_overrides = input_overrides.clone();
    let strategy_config = crate::strategy::StrategyConfig::from_program(
        &program,
        buy_fee_pct,
        sell_fee_pct,
        tick_size,
    )?;
    if (strategy_config.slippage_ticks > 0 || strategy_config.limit_verification_ticks > 0)
        && strategy_config.tick_size.is_none()
    {
        return Err(vec![Diagnostic::error(1, 1, "This instrument has no authoritative tick-size metadata; nonzero strategy.slippage or backtest_fill_limits_assumption cannot be simulated." )]);
    }
    state.strategy = Some(crate::strategy::StrategyBroker::new(
        strategy_config,
        candles.len(),
        magnifier_groups,
    ));
    state.evaluate(&program, progress)?;
    let broker = state
        .strategy
        .take()
        .expect("strategy broker is initialized");
    if let Some(message) = broker.invalid_fill_error() {
        return Err(vec![Diagnostic::error(1, 1, message)]);
    }
    let name = program.validation.name.clone();
    let plots = std::mem::take(&mut state.plots);
    let diagnostics = std::mem::take(&mut state.diagnostics);
    drop(state);
    Ok(broker.output(name, plots, diagnostics, input_overrides.clone()))
}

fn validate_and_group_magnifier_bars(
    chart_bars: &[Candle],
    chart_timeframe: &str,
    magnifier_bars: &[Candle],
    magnifier_timeframe: Option<&str>,
) -> Result<Vec<Vec<Candle>>, Vec<Diagnostic>> {
    if magnifier_bars.is_empty() {
        return Ok(vec![Vec::new(); chart_bars.len()]);
    }
    let Some(magnifier_timeframe) =
        magnifier_timeframe.and_then(helpers::normalize_request_timeframe)
    else {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Bar Magnifier candles require a supported timeframe.",
        )]);
    };
    let chart_timeframe =
        helpers::normalize_request_timeframe(chart_timeframe).ok_or_else(|| {
            vec![Diagnostic::error(
                1,
                1,
                "Chart timeframe is not supported for Bar Magnifier alignment.",
            )]
        })?;
    let chart_duration = helpers::timeframe_duration_ns(&chart_timeframe).unwrap_or_default();
    let magnifier_duration =
        helpers::timeframe_duration_ns(&magnifier_timeframe).unwrap_or_default();
    if magnifier_duration <= 0 || magnifier_duration >= chart_duration {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Bar Magnifier timeframe must be finer than the chart timeframe.",
        )]);
    }
    if chart_bars.iter().any(|bar| bar.time.is_none()) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Chart candle timestamps are required for Bar Magnifier alignment.",
        )]);
    }
    if magnifier_bars.iter().any(|bar| {
        ![bar.open, bar.high, bar.low, bar.close, bar.volume]
            .iter()
            .all(|value| value.is_finite())
            || bar.open <= 0.0
            || bar.high <= 0.0
            || bar.low <= 0.0
            || bar.close <= 0.0
            || bar.volume < 0.0
            || bar.high < bar.low
            || bar.open < bar.low
            || bar.open > bar.high
            || bar.close < bar.low
            || bar.close > bar.high
    }) || magnifier_bars.iter().any(|bar| bar.time.is_none())
    {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Bar Magnifier candles contain invalid OHLCV values or missing timestamps.",
        )]);
    }
    if magnifier_bars
        .windows(2)
        .any(|pair| pair[0].time >= pair[1].time)
    {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Bar Magnifier timestamps must be strictly increasing.",
        )]);
    }
    let mut groups = vec![Vec::new(); chart_bars.len()];
    let mut chart_index = 0;
    for lower_bar in magnifier_bars {
        let time = lower_bar.time.expect("timestamps were validated");
        while chart_index < chart_bars.len()
            && time
                >= chart_bars[chart_index]
                    .time
                    .expect("chart timestamps are validated")
                    + chart_duration
        {
            chart_index += 1;
        }
        if chart_index == chart_bars.len() {
            break;
        }
        let chart_start = chart_bars[chart_index]
            .time
            .expect("chart timestamps are validated");
        if time >= chart_start && time < chart_start + chart_duration {
            groups[chart_index].push(lower_bar.clone());
        }
    }
    for (chart_bar, group) in chart_bars.iter().zip(groups.iter_mut()) {
        let chart_start = chart_bar.time.expect("chart timestamps are validated");
        let aggregates_chart = group
            .first()
            .is_some_and(|first| first.time == Some(chart_start))
            && close_enough(group.first().map(|bar| bar.open), Some(chart_bar.open))
            && close_enough(group.last().map(|bar| bar.close), Some(chart_bar.close))
            && close_enough(
                group.iter().map(|bar| bar.high).reduce(f64::max),
                Some(chart_bar.high),
            )
            && close_enough(
                group.iter().map(|bar| bar.low).reduce(f64::min),
                Some(chart_bar.low),
            );
        if !aggregates_chart {
            group.clear();
        }
    }
    Ok(groups)
}

fn close_enough(left: Option<f64>, right: Option<f64>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            (left - right).abs() <= left.abs().max(right.abs()).max(1.0) * 1e-9
        }
        _ => false,
    }
}

pub(crate) fn align_numeric_series_to_chart(
    chart_bars: &[Candle],
    chart_timeframe: &str,
    requested_bars: &[Candle],
    requested_timeframe: &str,
    values: Vec<Option<f64>>,
) -> Result<Vec<Option<f64>>, Vec<Diagnostic>> {
    let chart_timeframe =
        helpers::normalize_request_timeframe(chart_timeframe).ok_or_else(|| {
            vec![Diagnostic::error(
                1,
                1,
                "Selected chart timeframe is not supported by the Pine history adapter.",
            )]
        })?;
    let requested_timeframe = helpers::normalize_request_timeframe(requested_timeframe)
        .ok_or_else(|| {
            vec![Diagnostic::error(
                1,
                1,
                "Requested timeframe is not available in the Pine history adapter.",
            )]
        })?;
    let chart_duration = helpers::timeframe_duration_ns(&chart_timeframe).unwrap_or_default();
    let requested_duration =
        helpers::timeframe_duration_ns(&requested_timeframe).unwrap_or_default();
    let values = values
        .into_iter()
        .map(|value| match value.filter(|value| value.is_finite()) {
            Some(value) => Value::Number(value),
            None => Value::Missing,
        })
        .collect();
    helpers::align_security_values(
        chart_bars,
        requested_bars,
        chart_duration,
        requested_duration,
        values,
        false,
    )
    .map(|aligned| aligned.into_iter().map(|value| value.number()).collect())
}

pub(crate) fn normalize_supported_timeframe(value: &str) -> Option<String> {
    helpers::normalize_request_timeframe(value)
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeCapabilities {
    pub max_source_bytes: usize,
    pub max_statements: usize,
    pub max_candles: usize,
    pub max_strategy_candles: usize,
    pub max_plots: usize,
    pub max_history_offset: usize,
    pub max_evaluation_steps: usize,
    pub max_strategy_evaluation_steps: usize,
    pub realtime_available: bool,
}

pub fn capabilities() -> RuntimeCapabilities {
    RuntimeCapabilities {
        max_source_bytes: MAX_SOURCE_BYTES,
        max_statements: MAX_STATEMENTS,
        max_candles: MAX_CANDLES,
        max_strategy_candles: MAX_STRATEGY_CANDLES,
        max_plots: MAX_PLOTS,
        max_history_offset: MAX_HISTORY_OFFSET,
        max_evaluation_steps: MAX_EVALUATION_STEPS,
        max_strategy_evaluation_steps: MAX_STRATEGY_EVALUATION_STEPS,
        realtime_available: false,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Number(f64),
    Bool(bool),
    String(String),
    Missing,
}

impl Value {
    fn number(&self) -> Option<f64> {
        match self {
            Self::Number(value) if value.is_finite() => Some(*value),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            _ => None,
        }
    }
    fn boolean(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Number(value) if value.is_finite() => Some(*value != 0.0),
            _ => None,
        }
    }
    fn display(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Number(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Missing => String::new(),
        }
    }
    fn numeric_output(&self) -> Option<f64> {
        self.number().filter(|value| value.is_finite())
    }
}

impl RuntimeState<'_> {
    pub(super) fn resolve_strategy_name(&self, name: &str) -> Option<Value> {
        let broker = self.strategy.as_ref()?;
        match name {
            "strategy.long" | "strategy.direction.long" => Some(Value::String("LONG".to_string())),
            "strategy.short" | "strategy.direction.short" => {
                Some(Value::String("SHORT".to_string()))
            }
            "strategy.percent_of_equity" => {
                Some(Value::String("strategy.percent_of_equity".to_string()))
            }
            "strategy.fixed" => Some(Value::String("strategy.fixed".to_string())),
            "strategy.oca.cancel" | "strategy.oca.reduce" | "strategy.oca.none" => {
                Some(Value::String(name.to_string()))
            }
            "strategy.position_size" => Some(Value::Number(broker.position_size())),
            "strategy.position_avg_price" => Some(
                broker
                    .position_avg_price()
                    .map(Value::Number)
                    .unwrap_or(Value::Missing),
            ),
            "strategy.position_entry_name" => Some(
                broker
                    .position_entry_name()
                    .map(|name| Value::String(name.to_string()))
                    .unwrap_or(Value::Missing),
            ),
            "strategy.initial_capital" => Some(Value::Number(broker.initial_capital())),
            "strategy.closedtrades" => Some(Value::Number(broker.closed_trades() as f64)),
            "strategy.wintrades" => Some(Value::Number(broker.wins() as f64)),
            "strategy.losstrades" => Some(Value::Number(broker.losses() as f64)),
            "strategy.grossprofit" => Some(Value::Number(broker.gross_profit())),
            "strategy.grossprofit_percent" => Some(Value::Number(broker.gross_profit_percent())),
            "strategy.grossloss" => Some(Value::Number(broker.gross_loss())),
            "strategy.grossloss_percent" => Some(Value::Number(broker.gross_loss_percent())),
            "strategy.netprofit" => Some(Value::Number(broker.net_profit())),
            "strategy.netprofit_percent" => Some(Value::Number(broker.net_profit_percent())),
            "strategy.openprofit" => Some(Value::Number(broker.open_profit())),
            "strategy.openprofit_percent" => Some(
                broker
                    .open_profit_percent()
                    .map(Value::Number)
                    .unwrap_or(Value::Missing),
            ),
            "strategy.max_runup" => Some(Value::Number(broker.max_runup())),
            "strategy.max_runup_percent" => Some(Value::Number(broker.max_runup_percent())),
            "strategy.max_drawdown" => Some(Value::Number(broker.max_drawdown())),
            "strategy.max_drawdown_percent" => Some(Value::Number(broker.max_drawdown_percent())),
            "strategy.equity" => Some(Value::Number(broker.equity())),
            "strategy.opentrades" => Some(Value::Number(broker.open_trades() as f64)),
            _ => None,
        }
    }
}

struct RuntimeState<'a> {
    bars: &'a [Candle],
    program: &'a Program,
    available_timeframes: &'a HashMap<String, Vec<Candle>>,
    chart_timeframe: &'a str,
    input_sources: &'a HashMap<String, Vec<Option<f64>>>,
    input_sources_timeframe: &'a str,
    history_limit: usize,
    series: HashMap<String, Vec<Value>>,
    retained_series_values: usize,
    retained_string_bytes: usize,
    persistent: HashSet<String>,
    persistent_varip: HashSet<String>,
    initialized_var: HashSet<String>,
    initialized_varip: HashSet<String>,
    plots: Vec<PlotOutput>,
    plot_lookup: HashMap<String, usize>,
    diagnostics: Vec<Diagnostic>,
    current_bar: usize,
    evaluation_steps: usize,
    request_cache: HashMap<String, Vec<Value>>,
    requested_timeframes: HashSet<String>,
    evaluation_step_limit: usize,
    strategy_evaluation_start_bar: usize,
    strategy: Option<crate::strategy::StrategyBroker>,
    input_overrides: HashMap<String, serde_json::Value>,
}

impl<'a> RuntimeState<'a> {
    fn new(
        program: &'a Program,
        bars: &'a [Candle],
        available_timeframes: &'a HashMap<String, Vec<Candle>>,
        chart_timeframe: &'a str,
        input_sources: &'a HashMap<String, Vec<Option<f64>>>,
        input_sources_timeframe: &'a str,
        history_limit: usize,
    ) -> Self {
        fn collect_persistent_names(
            statements: &[Statement],
            names: &mut HashSet<String>,
            varip_names: &mut HashSet<String>,
        ) {
            for statement in statements {
                match statement {
                    Statement::Assign {
                        name,
                        declaration: crate::ast::DeclarationMode::Var,
                        ..
                    } => {
                        names.insert(name.clone());
                    }
                    Statement::Assign {
                        name,
                        declaration: crate::ast::DeclarationMode::Varip,
                        ..
                    } => {
                        names.insert(name.clone());
                        varip_names.insert(name.clone());
                    }
                    Statement::If {
                        body, otherwise, ..
                    } => {
                        collect_persistent_names(body, names, varip_names);
                        collect_persistent_names(otherwise, names, varip_names);
                    }
                    Statement::For { body, .. } => {
                        collect_persistent_names(body, names, varip_names)
                    }
                    Statement::Assign { .. } | Statement::Expression(_) => {}
                }
            }
        }
        let mut persistent = HashSet::new();
        let mut persistent_varip = HashSet::new();
        collect_persistent_names(&program.statements, &mut persistent, &mut persistent_varip);
        Self {
            bars,
            program,
            available_timeframes,
            chart_timeframe,
            input_sources,
            input_sources_timeframe,
            history_limit,
            series: HashMap::new(),
            retained_series_values: 0,
            retained_string_bytes: 0,
            persistent,
            persistent_varip,
            initialized_var: HashSet::new(),
            initialized_varip: HashSet::new(),
            plots: Vec::new(),
            plot_lookup: HashMap::new(),
            diagnostics: Vec::new(),
            current_bar: 0,
            evaluation_steps: 0,
            request_cache: HashMap::new(),
            requested_timeframes: HashSet::new(),
            evaluation_step_limit: MAX_EVALUATION_STEPS,
            strategy_evaluation_start_bar: 0,
            strategy: None,
            input_overrides: HashMap::new(),
        }
    }
}

mod calls;
mod evaluation;
mod helpers;
