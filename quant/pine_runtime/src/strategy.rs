use crate::{
    ast::{Expr, Program, Statement},
    Diagnostic, PlotOutput, StrategyOutput, StrategyTrade,
};

mod commission;
mod execution;
mod oca;
mod trailing;
use commission::parse as parse_commission;
pub use commission::{StrategyCommission, StrategyCommissionType};

const MAX_STRATEGY_PYRAMIDING: usize = 100;
const MAX_STRATEGY_SLIPPAGE_TICKS: usize = 10_000;
const MAX_LIMIT_VERIFICATION_TICKS: usize = 10_000;

#[derive(Clone)]
pub(crate) struct StrategyConfig {
    initial_capital: f64,
    default_qty_type: String,
    default_qty_value: f64,
    pyramiding: usize,
    process_orders_on_close: bool,
    calc_on_every_tick: bool,
    calc_on_order_fills: bool,
    use_bar_magnifier: bool,
    close_entries_rule: CloseEntriesRule,
    buy_fee_pct: f64,
    sell_fee_pct: f64,
    commission: Option<StrategyCommission>,
    pub(crate) slippage_ticks: usize,
    pub(crate) tick_size: Option<f64>,
    pub(crate) limit_verification_ticks: usize,
}

impl StrategyConfig {
    pub(crate) fn from_program(
        program: &Program,
        buy_fee_pct: f64,
        sell_fee_pct: f64,
        tick_size: Option<f64>,
    ) -> Result<Self, Vec<Diagnostic>> {
        let Some(arguments) = program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::Expression(Expr::Call { name, arguments }) if name == "strategy" => {
                    Some(arguments)
                }
                _ => None,
            })
        else {
            return Err(vec![Diagnostic::error(
                1,
                1,
                "The Pine strategy declaration could not be read.",
            )]);
        };
        if arguments.iter().filter(|item| item.name.is_none()).count() > 1 {
            return Err(vec![Diagnostic::error(1, 1, "Only the strategy title may be supplied positionally; name simulation properties explicitly." )]);
        }
        let number = |name: &str, default: f64| -> Result<f64, Vec<Diagnostic>> {
            let value = match arguments
                .iter()
                .find(|item| item.name.as_deref() == Some(name))
            {
                Some(item) => match item.value {
                    Expr::Number(value) => value,
                    _ => {
                        return Err(vec![Diagnostic::error(
                            1,
                            1,
                            format!("strategy.{name} must be numeric."),
                        )])
                    }
                },
                None => default,
            };
            if value.is_finite() && value > 0.0 {
                Ok(value)
            } else {
                Err(vec![Diagnostic::error(
                    1,
                    1,
                    format!("strategy.{name} must be a finite positive number."),
                )])
            }
        };
        let initial_capital = number("initial_capital", 1_000_000.0)?;
        let default_qty_value = number("default_qty_value", 1.0)?;
        let default_qty_type = match arguments
            .iter()
            .find(|item| item.name.as_deref() == Some("default_qty_type"))
        {
            Some(item) => match &item.value {
                Expr::Name(value) => value.clone(),
                _ => {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        "strategy.default_qty_type must use a supported strategy constant.",
                    )])
                }
            },
            None => "strategy.fixed".to_string(),
        };
        if !matches!(
            default_qty_type.as_str(),
            "strategy.fixed" | "strategy.cash" | "strategy.percent_of_equity"
        ) {
            return Err(vec![Diagnostic::error(1, 1, "This runtime supports strategy.fixed, strategy.cash in chart currency, and strategy.percent_of_equity default order sizing." )]);
        }
        let currency = arguments
            .iter()
            .find(|item| item.name.as_deref() == Some("currency"));
        if currency.is_some_and(
            |item| !matches!(&item.value, Expr::Name(value) if value == "currency.NONE"),
        ) {
            return Err(vec![Diagnostic::error(1, 1, "Only currency.NONE is supported; foreign account currencies require historical FX conversion." )]);
        }
        let process_orders_on_close = match arguments
            .iter()
            .find(|item| item.name.as_deref() == Some("process_orders_on_close"))
        {
            Some(item) => match item.value {
                Expr::Name(ref value) if value == "true" => true,
                Expr::Name(ref value) if value == "false" => false,
                _ => {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        "strategy.process_orders_on_close must be a boolean literal.",
                    )])
                }
            },
            None => false,
        };
        let calc_on_every_tick = match arguments
            .iter()
            .find(|item| item.name.as_deref() == Some("calc_on_every_tick"))
        {
            Some(item) => match item.value {
                Expr::Name(ref value) if value == "true" => true,
                Expr::Name(ref value) if value == "false" => false,
                _ => {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        "strategy.calc_on_every_tick must be a boolean literal.",
                    )])
                }
            },
            None => false,
        };
        let calc_on_every_history_tick = match arguments
            .iter()
            .find(|item| item.name.as_deref() == Some("calc_on_every_history_tick"))
        {
            Some(item) => match item.value {
                Expr::Name(ref value) if value == "true" => true,
                Expr::Name(ref value) if value == "false" => false,
                _ => {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        "strategy.calc_on_every_history_tick must be a boolean literal.",
                    )])
                }
            },
            None => false,
        };
        if calc_on_every_history_tick {
            return Err(vec![Diagnostic::error(1, 1, "`calc_on_every_history_tick=true` requires historical tick data; this backtest uses stored candle OHLC and cannot claim tick-history fidelity." )]);
        }
        let calc_on_order_fills = match arguments
            .iter()
            .find(|item| item.name.as_deref() == Some("calc_on_order_fills"))
        {
            Some(item) => match item.value {
                Expr::Name(ref value) if value == "true" => true,
                Expr::Name(ref value) if value == "false" => false,
                _ => {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        "strategy.calc_on_order_fills must be a boolean literal.",
                    )])
                }
            },
            None => false,
        };
        let use_bar_magnifier = match arguments
            .iter()
            .find(|item| item.name.as_deref() == Some("use_bar_magnifier"))
        {
            Some(item) => match item.value {
                Expr::Name(ref value) if value == "true" => true,
                Expr::Name(ref value) if value == "false" => false,
                _ => {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        "strategy.use_bar_magnifier must be a boolean literal.",
                    )])
                }
            },
            None => false,
        };
        if calc_on_order_fills && process_orders_on_close {
            return Err(vec![Diagnostic::error(1, 1, "`calc_on_order_fills` is supported with chart OHLC market fills; `process_orders_on_close` cannot be combined with it." )]);
        }
        if calc_on_order_fills && use_bar_magnifier {
            return Err(vec![Diagnostic::error(1, 1, "`calc_on_order_fills` currently uses inferred chart OHLC points; Bar Magnifier cannot be combined with it." )]);
        }
        let close_entries_rule = match arguments
            .iter()
            .find(|item| item.name.as_deref() == Some("close_entries_rule"))
        {
            Some(item) => match &item.value {
                Expr::String(value) if value == "FIFO" => CloseEntriesRule::Fifo,
                Expr::String(value) if value == "ANY" => CloseEntriesRule::Any,
                _ => {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        "strategy.close_entries_rule must be the literal string `FIFO` or `ANY`.",
                    )])
                }
            },
            None => CloseEntriesRule::Fifo,
        };
        let pyramiding = match arguments
            .iter()
            .find(|item| item.name.as_deref() == Some("pyramiding"))
        {
            Some(item) => match item.value {
                Expr::Number(value) if value.fract() == 0.0 => value,
                _ => {
                    return Err(vec![Diagnostic::error(
                        1,
                        1,
                        "strategy.pyramiding must be an integer literal.",
                    )])
                }
            },
            None => 1.0,
        };
        if pyramiding.fract() != 0.0
            || !(1.0..=MAX_STRATEGY_PYRAMIDING as f64).contains(&pyramiding)
        {
            return Err(vec![Diagnostic::error(
                1,
                1,
                format!(
                    "`pyramiding` must be a whole number from 1 through {MAX_STRATEGY_PYRAMIDING}."
                ),
            )]);
        }
        let commission = parse_commission(arguments)?;
        let slippage_ticks = match arguments.iter().find(|item| item.name.as_deref() == Some("slippage")) {
            Some(item) => match item.value {
                Expr::Number(value) if value.is_finite() && value.fract() == 0.0 && value >= 0.0 && value <= MAX_STRATEGY_SLIPPAGE_TICKS as f64 => value as usize,
                _ => return Err(vec![Diagnostic::error(1, 1, format!("strategy.slippage must be a whole number of ticks from 0 through {MAX_STRATEGY_SLIPPAGE_TICKS}."))]),
            },
            None => 0,
        };
        let limit_verification_ticks = match arguments.iter().find(|item| item.name.as_deref() == Some("backtest_fill_limits_assumption")) {
            Some(item) => match item.value {
                Expr::Number(value) if value.is_finite() && value.fract() == 0.0 && value >= 0.0 && value <= MAX_LIMIT_VERIFICATION_TICKS as f64 => value as usize,
                _ => return Err(vec![Diagnostic::error(1, 1, format!("strategy.backtest_fill_limits_assumption must be a whole number of ticks from 0 through {MAX_LIMIT_VERIFICATION_TICKS}."))]),
            },
            None => 0,
        };
        if tick_size.is_some_and(|value| !value.is_finite() || value <= 0.0) {
            return Err(vec![Diagnostic::error(
                1,
                1,
                "Instrument tick size must be a finite positive price increment.",
            )]);
        }
        let supported_properties = [
            "overlay",
            "pyramiding",
            "default_qty_type",
            "default_qty_value",
            "initial_capital",
            "process_orders_on_close",
            "calc_on_every_tick",
            "calc_on_every_history_tick",
            "calc_on_order_fills",
            "use_bar_magnifier",
            "close_entries_rule",
            "commission_type",
            "commission_value",
            "currency",
            "slippage",
            "backtest_fill_limits_assumption",
        ];
        if let Some(property) = arguments
            .iter()
            .filter_map(|item| item.name.as_deref())
            .find(|name| !supported_properties.contains(name))
        {
            return Err(vec![Diagnostic::error(1, 1, format!("strategy.{property} is outside the supported broker model; it will not be ignored."))]);
        }
        Ok(Self {
            initial_capital,
            default_qty_type,
            default_qty_value,
            pyramiding: pyramiding as usize,
            process_orders_on_close,
            calc_on_every_tick,
            calc_on_order_fills,
            use_bar_magnifier,
            close_entries_rule,
            buy_fee_pct,
            sell_fee_pct,
            commission,
            slippage_ticks,
            tick_size,
            limit_verification_ticks,
        })
    }

    pub(crate) fn uses_bar_magnifier(&self) -> bool {
        self.use_bar_magnifier
    }

    fn slippage_price(&self) -> f64 {
        self.slippage_ticks as f64 * self.tick_size.unwrap_or(0.0)
    }
    fn limit_verification_distance(&self) -> f64 {
        self.limit_verification_ticks as f64 * self.tick_size.unwrap_or(0.0)
    }

    fn transaction_fee(&self, price: f64, quantity: f64, venue_fee_pct: f64) -> f64 {
        match self.commission {
            Some(StrategyCommission {
                commission_type: StrategyCommissionType::Percent,
                value,
            }) => price * quantity * value / 100.0,
            Some(StrategyCommission {
                commission_type: StrategyCommissionType::CashPerOrder,
                value,
            }) => value,
            Some(StrategyCommission {
                commission_type: StrategyCommissionType::CashPerContract,
                value,
            }) => quantity * value,
            None => price * quantity * venue_fee_pct / 100.0,
        }
    }
}

#[derive(Clone)]
struct Position {
    key: u64,
    id: String,
    entry_order_bar: usize,
    direction: i8,
    quantity: f64,
    active_from_distance: f64,
    entry_bar: usize,
    entry_price: f64,
    entry_fee: f64,
    entry_time: Option<i64>,
    remaining_quantity: f64,
    exited_quantity: f64,
    exit_notional: f64,
    exit_fees: f64,
    gross_pnl: f64,
    entry_fee_remaining: f64,
    max_runup: f64,
    max_drawdown: f64,
    max_runup_percent: f64,
    max_drawdown_percent: f64,
    realized_equity_on_entry: f64,
    min_closed_equity_before_entry: f64,
    max_closed_equity_before_entry: f64,
    exit_bar: usize,
    exit_time: Option<i64>,
}

#[derive(Clone)]
enum PendingOrder {
    Entry {
        id: String,
        direction: i8,
        quantity: f64,
        created_bar: usize,
        oca_name: Option<String>,
        oca_type: OcaType,
    },
    NetOrder {
        id: String,
        direction: i8,
        quantity: f64,
        created_bar: usize,
        oca_name: Option<String>,
        oca_type: OcaType,
    },
    Close {
        id: Option<String>,
        size: ExitSize,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OcaType {
    None,
    Cancel,
    Reduce,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CloseEntriesRule {
    Fifo,
    Any,
}

#[derive(Clone)]
struct PriceEntryOrder {
    id: String,
    direction: i8,
    quantity: f64,
    created_bar: usize,
    stop: Option<f64>,
    limit: Option<f64>,
    stop_activated: bool,
    net_order: bool,
    oca_name: Option<String>,
    oca_type: OcaType,
}

#[derive(Clone, Copy)]
pub(crate) enum ExitSize {
    Full,
    Quantity(f64),
    Percent(f64),
}

#[derive(Clone)]
struct BracketExit {
    id: String,
    from_entry: Option<String>,
    stop: Option<f64>,
    limit: Option<f64>,
    loss_ticks: Option<f64>,
    profit_ticks: Option<f64>,
    trail_price: Option<f64>,
    trail_points: Option<f64>,
    trail_offset: Option<f64>,
    trailing_states: std::collections::HashMap<u64, trailing::TrailingState>,
    size: ExitSize,
    created_bar: usize,
}

pub(crate) struct StrategyBroker {
    config: StrategyConfig,
    positions: Vec<Position>,
    pending: Vec<PendingOrder>,
    pending_on_close: Vec<PendingOrder>,
    deferred_close_oca_fills: Vec<(String, OcaType, f64)>,
    price_entries: Vec<PriceEntryOrder>,
    bracket_exits: Vec<BracketExit>,
    trades: Vec<StrategyTrade>,
    equity_curve: Vec<f64>,
    equity: f64,
    gross_profit: f64,
    gross_loss: f64,
    net_profit: f64,
    max_runup: f64,
    max_drawdown: f64,
    max_runup_percent: f64,
    max_drawdown_percent: f64,
    min_closed_equity: f64,
    max_closed_equity: f64,
    wins: usize,
    losses: usize,
    current_bar: usize,
    next_position_key: u64,
    mark_price: f64,
    invalid_fill_error: Option<String>,
    current_fill_distance: f64,
    process_orders_on_close: bool,
    bar_path: Vec<(f64, f64)>,
    history_tick_index: usize,
    magnifier_bars: Vec<Vec<crate::runtime::Candle>>,
}

impl StrategyBroker {
    pub(crate) fn new(
        config: StrategyConfig,
        bar_count: usize,
        magnifier_bars: Vec<Vec<crate::runtime::Candle>>,
    ) -> Self {
        let equity = config.initial_capital;
        Self {
            process_orders_on_close: config.process_orders_on_close,
            config,
            positions: Vec::new(),
            pending: Vec::new(),
            pending_on_close: Vec::new(),
            deferred_close_oca_fills: Vec::new(),
            price_entries: Vec::new(),
            bracket_exits: Vec::new(),
            trades: Vec::new(),
            equity_curve: vec![equity; bar_count],
            equity,
            gross_profit: 0.0,
            gross_loss: 0.0,
            net_profit: 0.0,
            max_runup: 0.0,
            max_drawdown: 0.0,
            max_runup_percent: 0.0,
            max_drawdown_percent: 0.0,
            min_closed_equity: equity,
            max_closed_equity: equity,
            wins: 0,
            losses: 0,
            current_bar: 0,
            next_position_key: 0,
            mark_price: 0.0,
            invalid_fill_error: None,
            current_fill_distance: 0.0,
            bar_path: Vec::new(),
            history_tick_index: 0,
            magnifier_bars,
        }
    }

    pub(crate) fn calc_on_order_fills(&self) -> bool {
        self.config.calc_on_order_fills
    }

    pub(crate) fn fill_immediate_closes_at_current_tick(&mut self, time: Option<i64>) -> bool {
        self.fill_immediate_closes_at_current_tick_inner(time)
    }

    pub(crate) fn require_market_only_orders(
        &self,
        function: &str,
        has_price_order: bool,
    ) -> Result<(), Vec<Diagnostic>> {
        if self.config.calc_on_order_fills && has_price_order {
            return Err(vec![Diagnostic::error(1, 1, format!("`{function}` price-based orders are not supported with `calc_on_order_fills`; use next-tick market orders."))]);
        }
        Ok(())
    }

    pub(crate) fn finish_bar(&mut self, close: f64, time: Option<i64>) -> bool {
        self.update_open_trade_excursions(close);
        let filled = self.fill_pending_market_orders_at_close(close, time);
        if self.process_orders_on_close {
            self.fill_bracket_exit(
                close,
                close,
                close,
                close,
                self.current_bar,
                time,
                self.current_fill_distance,
                f64::INFINITY,
            );
        }
        self.mark_price = close;
        let equity = self.equity();
        if let Some(value) = self.equity_curve.get_mut(self.current_bar) {
            *value = equity;
        }
        filled
    }

    pub(crate) fn entry(
        &mut self,
        id: String,
        direction: i8,
        quantity: Option<f64>,
        close: f64,
        time: Option<i64>,
        stop: Option<f64>,
        limit: Option<f64>,
        oca_name: Option<String>,
        oca_type: OcaType,
    ) {
        if direction == 0 || !close.is_finite() || close <= 0.0 {
            return;
        }
        if self
            .positions
            .first()
            .is_some_and(|position| position.direction == direction)
            && self.positions.len() >= self.config.pyramiding
        {
            return;
        }
        if quantity.is_some_and(|qty| !qty.is_finite() || qty <= 0.0) {
            return;
        }
        let qty = quantity
            .filter(|qty| qty.is_finite() && *qty > 0.0)
            .unwrap_or_else(|| {
                if self.config.default_qty_type == "strategy.percent_of_equity" {
                    self.equity.max(0.0) * self.config.default_qty_value / 100.0 / close
                } else if self.config.default_qty_type == "strategy.cash" {
                    self.config.default_qty_value / close
                } else {
                    self.config.default_qty_value
                }
            });
        if !qty.is_finite() || qty <= 0.0 {
            return;
        }
        self.cancel_pending_entry_id(&id);
        if stop.is_some() || limit.is_some() {
            let mut order = PriceEntryOrder {
                id,
                direction,
                quantity: qty,
                created_bar: self.current_bar,
                stop,
                limit,
                stop_activated: stop.is_none(),
                net_order: false,
                oca_name,
                oca_type,
            };
            if self.process_orders_on_close {
                if let Some(price) = entry_fill_on_close(
                    &mut order,
                    close,
                    self.config.limit_verification_distance(),
                ) {
                    self.fill(
                        PendingOrder::Entry {
                            id: order.id,
                            direction: order.direction,
                            quantity: order.quantity,
                            created_bar: order.created_bar,
                            oca_name: order.oca_name.clone(),
                            oca_type: order.oca_type,
                        },
                        price,
                        self.current_bar,
                        time,
                        order.stop.is_some() && order.limit.is_none(),
                        true,
                    );
                    if let Some(name) = order.oca_name {
                        self.deferred_close_oca_fills
                            .push((name, order.oca_type, order.quantity));
                    }
                    return;
                }
            }
            self.price_entries.push(order);
        } else {
            let order = PendingOrder::Entry {
                id,
                direction,
                quantity: qty,
                created_bar: self.current_bar,
                oca_name,
                oca_type,
            };
            if self.process_orders_on_close {
                self.pending_on_close.push(order);
            } else {
                self.pending.push(order);
            }
        }
    }

    pub(crate) fn order(
        &mut self,
        id: String,
        direction: i8,
        quantity: Option<f64>,
        close: f64,
        time: Option<i64>,
        stop: Option<f64>,
        limit: Option<f64>,
        oca_name: Option<String>,
        oca_type: OcaType,
    ) {
        if direction == 0
            || !close.is_finite()
            || close <= 0.0
            || quantity.is_some_and(|qty| !qty.is_finite() || qty <= 0.0)
        {
            return;
        }
        let quantity = self.order_quantity(quantity, close);
        if !quantity.is_finite() || quantity <= 0.0 {
            return;
        }
        self.cancel_pending_entry_id(&id);
        if stop.is_some() || limit.is_some() {
            let mut order = PriceEntryOrder {
                id,
                direction,
                quantity,
                created_bar: self.current_bar,
                stop,
                limit,
                stop_activated: stop.is_none(),
                net_order: true,
                oca_name,
                oca_type,
            };
            if self.process_orders_on_close {
                if let Some(price) = entry_fill_on_close(
                    &mut order,
                    close,
                    self.config.limit_verification_distance(),
                ) {
                    self.apply_net_order(
                        order.id,
                        direction,
                        order.quantity,
                        order.created_bar,
                        price,
                        self.current_bar,
                        time,
                        order.stop.is_some() && order.limit.is_none(),
                    );
                    if let Some(name) = order.oca_name {
                        self.deferred_close_oca_fills
                            .push((name, order.oca_type, order.quantity));
                    }
                    return;
                }
            }
            self.price_entries.push(order);
        } else {
            let order = PendingOrder::NetOrder {
                id,
                direction,
                quantity,
                created_bar: self.current_bar,
                oca_name,
                oca_type,
            };
            if self.process_orders_on_close {
                self.pending_on_close.push(order);
            } else {
                self.pending.push(order);
            }
        }
    }

    fn order_quantity(&self, quantity: Option<f64>, price: f64) -> f64 {
        quantity
            .filter(|qty| qty.is_finite() && *qty > 0.0)
            .unwrap_or_else(|| {
                if self.config.default_qty_type == "strategy.percent_of_equity" {
                    self.equity.max(0.0) * self.config.default_qty_value / 100.0 / price
                } else if self.config.default_qty_type == "strategy.cash" {
                    self.config.default_qty_value / price
                } else {
                    self.config.default_qty_value
                }
            })
    }

    fn cancel_pending_entry_id(&mut self, id: &str) {
        self.pending.retain(|order| !matches!(order, PendingOrder::Entry { id: pending_id, .. } | PendingOrder::NetOrder { id: pending_id, .. } if pending_id == id));
        self.pending_on_close.retain(|order| !matches!(order, PendingOrder::Entry { id: pending_id, .. } | PendingOrder::NetOrder { id: pending_id, .. } if pending_id == id));
        self.price_entries.retain(|order| order.id != id);
    }

    fn apply_oca_fill(&mut self, name: &str, oca_type: OcaType, filled_quantity: f64) {
        self.apply_oca_fill_preserving_same_tick(
            name,
            oca_type,
            filled_quantity,
            &std::collections::HashSet::new(),
        );
    }

    fn apply_oca_fill_preserving_same_tick(
        &mut self,
        name: &str,
        oca_type: OcaType,
        filled_quantity: f64,
        same_tick_orders: &std::collections::HashSet<String>,
    ) {
        if oca_type == OcaType::None {
            return;
        }
        let adjust_pending = |orders: &mut Vec<PendingOrder>| {
            for order in orders.iter_mut() {
                match order {
                    PendingOrder::Entry {
                        quantity,
                        oca_name: Some(group),
                        oca_type: group_type,
                        ..
                    }
                    | PendingOrder::NetOrder {
                        quantity,
                        oca_name: Some(group),
                        oca_type: group_type,
                        ..
                    } if group == name
                        && *group_type == oca_type
                        && oca_type == OcaType::Reduce =>
                    {
                        *quantity = (*quantity - filled_quantity).max(0.0);
                    }
                    _ => {}
                }
            }
            orders.retain(|order| match order {
                PendingOrder::Entry {
                    quantity,
                    oca_name: Some(group),
                    oca_type: group_type,
                    ..
                }
                | PendingOrder::NetOrder {
                    quantity,
                    oca_name: Some(group),
                    oca_type: group_type,
                    ..
                } if group == name && *group_type == oca_type => {
                    oca_type != OcaType::Cancel && *quantity > 1e-12
                }
                _ => true,
            });
        };
        adjust_pending(&mut self.pending);
        adjust_pending(&mut self.pending_on_close);
        for order in &mut self.price_entries {
            if order.oca_name.as_deref() == Some(name)
                && order.oca_type == oca_type
                && oca_type == OcaType::Reduce
                && !same_tick_orders.contains(&order.id)
            {
                order.quantity = (order.quantity - filled_quantity).max(0.0);
            }
        }
        self.price_entries
            .retain(|order| match order.oca_name.as_deref() {
                Some(group) if group == name && order.oca_type == oca_type => {
                    same_tick_orders.contains(&order.id)
                        || (oca_type != OcaType::Cancel && order.quantity > 1e-12)
                }
                _ => true,
            });
    }

    pub(crate) fn cancel(&mut self, id: Option<String>) {
        let matches = |order_id: &str| id.as_ref().is_none_or(|id| id == order_id);
        self.pending.retain(|order| {
            !matches(match order {
                PendingOrder::Entry { id, .. } | PendingOrder::NetOrder { id, .. } => id,
                PendingOrder::Close { id: Some(id), .. } => id,
                PendingOrder::Close { id: None, .. } => "",
            })
        });
        self.pending_on_close.retain(|order| {
            !matches(match order {
                PendingOrder::Entry { id, .. } | PendingOrder::NetOrder { id, .. } => id,
                PendingOrder::Close { id: Some(id), .. } => id,
                PendingOrder::Close { id: None, .. } => "",
            })
        });
        self.price_entries.retain(|order| !matches(&order.id));
        if let Some(id) = id {
            self.bracket_exits.retain(|exit| exit.id != id);
        } else {
            self.bracket_exits.clear();
        }
    }

    pub(crate) fn close(&mut self, id: Option<String>, immediately: bool, size: ExitSize) {
        let order = PendingOrder::Close { id, size };
        if immediately || self.process_orders_on_close {
            self.pending_on_close.push(order);
        } else {
            self.pending.push(order);
        }
    }

    pub(crate) fn exit(
        &mut self,
        id: String,
        from_entry: Option<String>,
        stop: Option<f64>,
        limit: Option<f64>,
        loss_ticks: Option<f64>,
        profit_ticks: Option<f64>,
        trail_price: Option<f64>,
        trail_points: Option<f64>,
        trail_offset: Option<f64>,
        size: ExitSize,
    ) {
        let mut trailing_states = std::collections::HashMap::new();
        if let Some(existing) = self.bracket_exits.iter().find(|existing| existing.id == id) {
            if existing.trail_offset.is_some() && trail_offset.is_some() {
                trailing_states = existing.trailing_states.clone();
            }
        }
        let order = BracketExit {
            id: id.clone(),
            from_entry,
            stop,
            limit,
            loss_ticks,
            profit_ticks,
            trail_price,
            trail_points,
            trail_offset,
            trailing_states,
            size,
            created_bar: self.current_bar,
        };
        if let Some(existing) = self
            .bracket_exits
            .iter_mut()
            .find(|existing| existing.id == id)
        {
            *existing = order;
        } else {
            self.bracket_exits.push(order);
        }
    }

    fn fill_bracket_exit(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        bar: usize,
        time: Option<i64>,
        start_distance: f64,
        through_distance: f64,
    ) {
        let mut candidates = Vec::new();
        let mut reserved = vec![0.0; self.positions.len()];
        let scan_start_distance = start_distance;
        for exit_index in 0..self.bracket_exits.len() {
            let exit = &mut self.bracket_exits[exit_index];
            let mut exit_reserved = 0.0;
            for (position_index, position) in self.positions.iter().enumerate() {
                if exit.from_entry.as_ref().is_some_and(|entry| {
                    entry != &position.id || position.entry_order_bar > exit.created_bar
                }) {
                    continue;
                }
                let available = (position.remaining_quantity - reserved[position_index]).max(0.0);
                let total_available = self
                    .positions
                    .iter()
                    .filter(|candidate| {
                        exit.from_entry.as_ref().is_none_or(|entry| {
                            entry == &candidate.id && candidate.entry_order_bar <= exit.created_bar
                        })
                    })
                    .map(|candidate| candidate.remaining_quantity)
                    .sum::<f64>();
                let size_quantity = match exit.size {
                    ExitSize::Full => total_available,
                    ExitSize::Quantity(quantity) => quantity,
                    ExitSize::Percent(percent) => total_available * percent / 100.0,
                };
                let exit_remaining = (size_quantity - exit_reserved).max(0.0);
                let quantity = available.min(exit_remaining);
                if quantity <= 1e-12 {
                    continue;
                }
                let (stop, limit) = exit_levels(exit, position, self.config.tick_size);
                let fixed_event = bracket_fill_price(
                    position.direction,
                    open,
                    high,
                    low,
                    close,
                    stop,
                    limit,
                    self.config.limit_verification_distance(),
                );
                let trailing_event = trailing_fill_event(
                    exit,
                    position,
                    open,
                    high,
                    low,
                    close,
                    scan_start_distance.max(position.active_from_distance),
                    through_distance,
                    self.config.tick_size,
                )
                .map(|(event, state)| {
                    exit.trailing_states.insert(position.key, state);
                    event.map(|(distance, price)| (distance, price, true))
                })
                .flatten();
                let fill_event = match (fixed_event, trailing_event) {
                    (Some(fixed), Some(trailing))
                        if trailing.0 < fixed.0 || (trailing.0 == fixed.0 && !fixed.2) =>
                    {
                        Some(trailing)
                    }
                    (Some(fixed), _) => Some(fixed),
                    (None, trailing) => trailing,
                };
                if let Some((distance, price, is_stop)) = fill_event.filter(|(distance, _, _)| {
                    *distance >= position.active_from_distance && *distance <= through_distance
                }) {
                    reserved[position_index] += quantity;
                    exit_reserved += quantity;
                    candidates.push((
                        distance,
                        exit_index,
                        position_index,
                        quantity,
                        price,
                        is_stop,
                    ));
                }
            }
        }
        candidates.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then(left.1.cmp(&right.1))
                .then(left.2.cmp(&right.2))
        });
        let mut filled_exit_quantity = vec![0.0; self.bracket_exits.len()];
        let mut filled_exit_indices = std::collections::HashSet::new();
        for (distance, exit_index, position_index, quantity, price, is_stop) in candidates {
            self.current_fill_distance = distance;
            for position in &mut self.positions {
                if distance >= position.active_from_distance {
                    Self::update_position_excursions(position, price);
                }
            }
            self.update_aggregate_excursions();
            let Some(position) = self.positions.get(position_index) else {
                continue;
            };
            let direction = position.direction;
            let execution_price = price
                - f64::from(direction)
                    * if is_stop {
                        self.config.slippage_price()
                    } else {
                        0.0
                    };
            if !self.validate_fill_price(execution_price) {
                continue;
            }
            let Some(position) = self.positions.get_mut(position_index) else {
                continue;
            };
            let quantity = quantity.min(position.remaining_quantity);
            if quantity <= 1e-12 {
                continue;
            }
            let exit_fee_pct = if position.direction > 0 {
                self.config.sell_fee_pct
            } else {
                self.config.buy_fee_pct
            };
            let gross_pnl =
                f64::from(position.direction) * (execution_price - position.entry_price) * quantity;
            let entry_fee = position.entry_fee * quantity / position.quantity;
            let exit_fee = self
                .config
                .transaction_fee(execution_price, quantity, exit_fee_pct);
            position.remaining_quantity -= quantity;
            position.exited_quantity += quantity;
            position.exit_notional += execution_price * quantity;
            position.exit_fees += exit_fee;
            position.entry_fee_remaining -= entry_fee;
            position.gross_pnl += gross_pnl;
            position.exit_bar = bar;
            position.exit_time = time;
            let realized_net = gross_pnl - entry_fee - exit_fee;
            self.net_profit += realized_net;
            self.equity += realized_net;
            filled_exit_quantity[exit_index] += quantity;
            filled_exit_indices.insert(exit_index);
        }
        for (exit_index, quantity) in filled_exit_quantity.into_iter().enumerate() {
            if quantity > 0.0 {
                if let Some(exit) = self.bracket_exits.get_mut(exit_index) {
                    if let ExitSize::Quantity(remaining) = &mut exit.size {
                        *remaining = (*remaining - quantity).max(0.0);
                    }
                }
            }
        }
        let filled_exit_ids = self
            .bracket_exits
            .iter()
            .enumerate()
            .filter_map(|(index, exit)| {
                filled_exit_indices
                    .contains(&index)
                    .then_some(exit.id.clone())
            })
            .collect::<std::collections::HashSet<_>>();
        self.bracket_exits
            .retain(|exit| !filled_exit_ids.contains(&exit.id));
        for position in &mut self.positions {
            if position.remaining_quantity <= 1e-12 {
                position.remaining_quantity = 0.0;
            }
        }
        let mut retained = Vec::new();
        for position in std::mem::take(&mut self.positions) {
            if position.remaining_quantity == 0.0 {
                self.record_closed_position(position);
            } else {
                retained.push(position);
            }
        }
        self.positions = retained;
        let open_position_keys = self
            .positions
            .iter()
            .map(|position| position.key)
            .collect::<std::collections::HashSet<_>>();
        for exit in &mut self.bracket_exits {
            exit.trailing_states
                .retain(|key, _| open_position_keys.contains(key));
        }
        if self.positions.is_empty() {
            self.bracket_exits.retain(|exit| {
                let matches_entry = |id: &str| {
                    exit.from_entry
                        .as_deref()
                        .is_none_or(|from_entry| from_entry == id)
                };
                let pending_market =
                    self.pending
                        .iter()
                        .chain(&self.pending_on_close)
                        .any(|order| match order {
                            PendingOrder::Entry { id, .. } | PendingOrder::NetOrder { id, .. } => {
                                matches_entry(id)
                            }
                            PendingOrder::Close { .. } => false,
                        });
                pending_market
                    || self
                        .price_entries
                        .iter()
                        .any(|order| matches_entry(&order.id))
            });
        }
    }

    fn fill(
        &mut self,
        order: PendingOrder,
        price: f64,
        bar: usize,
        time: Option<i64>,
        apply_slippage: bool,
        price_entry: bool,
    ) {
        match order {
            PendingOrder::Close { id, size } => {
                self.close_entries(id, size, price, bar, time, apply_slippage);
            }
            PendingOrder::Entry {
                id,
                direction,
                quantity,
                created_bar,
                ..
            } => {
                let reversing = self
                    .positions
                    .first()
                    .is_some_and(|position| position.direction != direction);
                if reversing {
                    for position in std::mem::take(&mut self.positions) {
                        let quantity = position.remaining_quantity;
                        self.close_position(position, quantity, price, bar, time, apply_slippage);
                    }
                    self.bracket_exits.clear();
                }
                let price = price
                    + f64::from(direction)
                        * if apply_slippage {
                            self.config.slippage_price()
                        } else {
                            0.0
                        };
                if !self.validate_fill_price(price) {
                    return;
                }
                let same_tick_price_entry = price_entry
                    && self.positions.iter().any(|position| {
                        position.entry_bar == bar
                            && position.active_from_distance == self.current_fill_distance
                    });
                if self.positions.len() < self.config.pyramiding || same_tick_price_entry {
                    let key = self.next_position_key;
                    self.next_position_key += 1;
                    let entry_fee_pct = if direction > 0 {
                        self.config.buy_fee_pct
                    } else {
                        self.config.sell_fee_pct
                    };
                    let entry_fee = self.config.transaction_fee(price, quantity, entry_fee_pct);
                    let mut position = Position {
                        key,
                        id,
                        entry_order_bar: created_bar,
                        direction,
                        quantity,
                        active_from_distance: self.current_fill_distance,
                        remaining_quantity: quantity,
                        entry_bar: bar,
                        entry_price: price,
                        entry_fee,
                        entry_fee_remaining: entry_fee,
                        entry_time: time,
                        exited_quantity: 0.0,
                        exit_notional: 0.0,
                        exit_fees: 0.0,
                        gross_pnl: 0.0,
                        max_runup: 0.0,
                        max_drawdown: 0.0,
                        max_runup_percent: 0.0,
                        max_drawdown_percent: 0.0,
                        realized_equity_on_entry: self.equity,
                        min_closed_equity_before_entry: self.min_closed_equity,
                        max_closed_equity_before_entry: self.max_closed_equity,
                        exit_bar: bar,
                        exit_time: time,
                    };
                    Self::update_position_excursions(&mut position, price);
                    self.positions.push(position);
                }
            }
            PendingOrder::NetOrder {
                id,
                direction,
                quantity,
                created_bar,
                ..
            } => self.apply_net_order(
                id,
                direction,
                quantity,
                created_bar,
                price,
                bar,
                time,
                apply_slippage,
            ),
        }
    }

    fn apply_net_order(
        &mut self,
        id: String,
        direction: i8,
        quantity: f64,
        created_bar: usize,
        price: f64,
        bar: usize,
        time: Option<i64>,
        apply_slippage: bool,
    ) {
        let entry_price = price
            + f64::from(direction)
                * if apply_slippage {
                    self.config.slippage_price()
                } else {
                    0.0
                };
        if !self.validate_fill_price(entry_price) {
            return;
        }
        let mut remaining = quantity;
        if self
            .positions
            .first()
            .is_some_and(|position| position.direction != direction)
        {
            let positions = std::mem::take(&mut self.positions);
            let mut retained = Vec::new();
            for position in positions {
                if remaining > 1e-12 {
                    let close_quantity = remaining.min(position.remaining_quantity);
                    remaining -= close_quantity;
                    self.close_position(position, close_quantity, price, bar, time, apply_slippage);
                } else {
                    retained.push(position);
                }
            }
            retained.append(&mut self.positions);
            self.positions = retained;
            if self.positions.is_empty() {
                self.bracket_exits.clear();
            }
        }
        if remaining > 1e-12 {
            let key = self.next_position_key;
            self.next_position_key += 1;
            let entry_fee_pct = if direction > 0 {
                self.config.buy_fee_pct
            } else {
                self.config.sell_fee_pct
            };
            let entry_fee = self
                .config
                .transaction_fee(entry_price, remaining, entry_fee_pct);
            let mut position = Position {
                key,
                id,
                entry_order_bar: created_bar,
                direction,
                quantity: remaining,
                active_from_distance: self.current_fill_distance,
                remaining_quantity: remaining,
                entry_bar: bar,
                entry_price,
                entry_fee,
                entry_fee_remaining: entry_fee,
                entry_time: time,
                exited_quantity: 0.0,
                exit_notional: 0.0,
                exit_fees: 0.0,
                gross_pnl: 0.0,
                max_runup: 0.0,
                max_drawdown: 0.0,
                max_runup_percent: 0.0,
                max_drawdown_percent: 0.0,
                realized_equity_on_entry: self.equity,
                min_closed_equity_before_entry: self.min_closed_equity,
                max_closed_equity_before_entry: self.max_closed_equity,
                exit_bar: bar,
                exit_time: time,
            };
            Self::update_position_excursions(&mut position, entry_price);
            self.positions.push(position);
        }
    }

    fn close_entries(
        &mut self,
        id: Option<String>,
        size: ExitSize,
        price: f64,
        bar: usize,
        time: Option<i64>,
        apply_slippage: bool,
    ) {
        let matching_quantity = id.as_ref().map(|id| {
            self.positions
                .iter()
                .filter(|position| position.id == *id)
                .map(|position| position.remaining_quantity)
                .sum::<f64>()
        });
        let target_quantity = match &id {
            Some(_) => match size {
                ExitSize::Full => matching_quantity.unwrap_or_default(),
                ExitSize::Quantity(quantity) => quantity,
                ExitSize::Percent(percent) => {
                    matching_quantity.unwrap_or_default() * percent / 100.0
                }
            },
            None => self
                .positions
                .iter()
                .map(|position| position.remaining_quantity)
                .sum::<f64>(),
        };
        let total_quantity = self
            .positions
            .iter()
            .map(|position| position.remaining_quantity)
            .sum::<f64>();
        let mut remaining = target_quantity.min(total_quantity);
        let mut retained = Vec::new();
        for position in std::mem::take(&mut self.positions) {
            let eligible = id.is_none()
                || self.config.close_entries_rule == CloseEntriesRule::Fifo
                || id.as_ref().is_some_and(|id| id == &position.id);
            if eligible && remaining > 1e-12 {
                let quantity = remaining.min(position.remaining_quantity);
                remaining -= quantity;
                self.close_position(position, quantity, price, bar, time, apply_slippage);
            } else {
                retained.push(position);
            }
        }
        retained.append(&mut self.positions);
        self.positions = retained;
        if self.positions.is_empty() {
            self.bracket_exits.clear();
        }
    }

    fn close_position(
        &mut self,
        mut position: Position,
        quantity: f64,
        price: f64,
        bar: usize,
        time: Option<i64>,
        apply_slippage: bool,
    ) {
        let price = price
            - f64::from(position.direction)
                * if apply_slippage {
                    self.config.slippage_price()
                } else {
                    0.0
                };
        if !self.validate_fill_price(price) {
            return;
        }
        Self::update_position_excursions(&mut position, price);
        let sign = f64::from(position.direction);
        let gross_pnl = sign * (price - position.entry_price) * quantity;
        let (exit_fee_pct, _) = if position.direction > 0 {
            (self.config.sell_fee_pct, self.config.buy_fee_pct)
        } else {
            (self.config.buy_fee_pct, self.config.sell_fee_pct)
        };
        let entry_fee = position.entry_fee_remaining * quantity / position.remaining_quantity;
        position.remaining_quantity -= quantity;
        position.exited_quantity += quantity;
        position.exit_notional += price * quantity;
        let exit_fee = self.config.transaction_fee(price, quantity, exit_fee_pct);
        position.exit_fees += exit_fee;
        position.entry_fee_remaining -= entry_fee;
        position.gross_pnl += gross_pnl;
        position.exit_bar = bar;
        position.exit_time = time;
        let realized_net = gross_pnl - entry_fee - exit_fee;
        self.net_profit += realized_net;
        self.equity += realized_net;
        if position.remaining_quantity <= 1e-12 {
            position.remaining_quantity = 0.0;
        }
        if position.remaining_quantity > 1e-12 {
            self.positions.push(position);
        } else {
            self.record_closed_position(position);
        }
    }

    fn record_closed_position(&mut self, position: Position) {
        self.update_aggregate_from_position(&position);
        self.min_closed_equity = self.min_closed_equity.min(self.equity);
        self.max_closed_equity = self.max_closed_equity.max(self.equity);
        let notional = position.entry_price * position.quantity;
        let gross_pnl = position.gross_pnl;
        let net_pnl = gross_pnl - position.entry_fee - position.exit_fees;
        let gross_return_pct = if notional > 0.0 {
            gross_pnl / notional * 100.0
        } else {
            0.0
        };
        let net_return_pct = if notional > 0.0 {
            net_pnl / notional * 100.0
        } else {
            0.0
        };
        if gross_pnl > 0.0 {
            self.gross_profit += gross_pnl;
        } else {
            self.gross_loss += gross_pnl.abs();
        }
        if net_pnl > 0.0 {
            self.wins += 1;
        }
        if net_pnl < 0.0 {
            self.losses += 1;
        }
        if let Some(value) = self.equity_curve.get_mut(position.exit_bar) {
            *value = self.equity;
        }
        let exit_price = if position.exited_quantity > 0.0 {
            position.exit_notional / position.exited_quantity
        } else {
            position.entry_price
        };
        self.trades.push(StrategyTrade {
            entry_id: position.id,
            direction: if position.direction > 0 {
                "LONG".to_string()
            } else {
                "SHORT".to_string()
            },
            entry_bar: position.entry_bar,
            exit_bar: position.exit_bar,
            entry_time: position.entry_time,
            exit_time: position.exit_time,
            entry_price: position.entry_price,
            exit_price,
            quantity: position.quantity,
            gross_return_pct,
            net_return_pct,
            gross_pnl,
            net_pnl,
            max_runup: position.max_runup,
            max_drawdown: position.max_drawdown,
            max_runup_percent: position.max_runup_percent,
            max_drawdown_percent: position.max_drawdown_percent,
        });
    }

    pub(crate) fn position_size(&self) -> f64 {
        self.positions
            .first()
            .map(|position| {
                f64::from(position.direction)
                    * self
                        .positions
                        .iter()
                        .map(|item| item.remaining_quantity)
                        .sum::<f64>()
            })
            .unwrap_or(0.0)
    }
    pub(crate) fn position_avg_price(&self) -> Option<f64> {
        let quantity = self
            .positions
            .iter()
            .map(|position| position.remaining_quantity)
            .sum::<f64>();
        (quantity > 0.0).then(|| {
            self.positions
                .iter()
                .map(|position| position.entry_price * position.remaining_quantity)
                .sum::<f64>()
                / quantity
        })
    }
    pub(crate) fn position_entry_name(&self) -> Option<&str> {
        self.positions.first().map(|position| position.id.as_str())
    }
    pub(crate) fn entry_bar(&self, trade_index: usize) -> Option<usize> {
        self.positions
            .get(trade_index)
            .map(|position| position.entry_bar)
    }
    pub(crate) fn open_entry_id(&self, trade_index: usize) -> Option<&str> {
        self.positions
            .get(trade_index)
            .map(|position| position.id.as_str())
    }
    pub(crate) fn open_entry_price(&self, trade_index: usize) -> Option<f64> {
        self.positions
            .get(trade_index)
            .map(|position| position.entry_price)
    }
    pub(crate) fn open_entry_time(&self, trade_index: usize) -> Option<i64> {
        self.positions
            .get(trade_index)
            .and_then(|position| position.entry_time)
    }
    pub(crate) fn open_size(&self, trade_index: usize) -> Option<f64> {
        self.positions
            .get(trade_index)
            .map(|position| f64::from(position.direction) * position.remaining_quantity)
    }
    pub(crate) fn open_commission(&self, trade_index: usize) -> Option<f64> {
        self.positions
            .get(trade_index)
            .map(|position| position.entry_fee + position.exit_fees)
    }
    pub(crate) fn open_trade_profit(&self, trade_index: usize) -> Option<f64> {
        self.positions.get(trade_index).map(|position| {
            let gross_profit = position.gross_pnl
                + f64::from(position.direction)
                    * (self.mark_price - position.entry_price)
                    * position.remaining_quantity;
            gross_profit - position.entry_fee - position.exit_fees
        })
    }
    pub(crate) fn open_trade_profit_percent(&self, trade_index: usize) -> Option<f64> {
        self.positions.get(trade_index).map(|position| {
            let entry_notional = position.entry_price * position.quantity;
            let gross_profit = position.gross_pnl
                + f64::from(position.direction)
                    * (self.mark_price - position.entry_price)
                    * position.remaining_quantity;
            (gross_profit - position.entry_fee - position.exit_fees) / entry_notional * 100.0
        })
    }
    pub(crate) fn open_max_runup(&self, trade_index: usize) -> Option<f64> {
        self.positions
            .get(trade_index)
            .map(|position| position.max_runup)
    }
    pub(crate) fn open_max_drawdown(&self, trade_index: usize) -> Option<f64> {
        self.positions
            .get(trade_index)
            .map(|position| position.max_drawdown)
    }
    pub(crate) fn open_max_runup_percent(&self, trade_index: usize) -> Option<f64> {
        self.positions
            .get(trade_index)
            .map(|position| position.max_runup_percent)
    }
    pub(crate) fn open_max_drawdown_percent(&self, trade_index: usize) -> Option<f64> {
        self.positions
            .get(trade_index)
            .map(|position| position.max_drawdown_percent)
    }
    pub(crate) fn open_trades(&self) -> usize {
        self.positions.len()
    }
    pub(crate) fn closed_trades(&self) -> usize {
        self.trades.len()
    }
    pub(crate) fn closed_trade(&self, trade_index: usize) -> Option<&StrategyTrade> {
        self.trades.get(trade_index)
    }
    pub(crate) fn wins(&self) -> usize {
        self.wins
    }
    pub(crate) fn losses(&self) -> usize {
        self.losses
    }
    pub(crate) fn gross_profit(&self) -> f64 {
        self.gross_profit
    }
    pub(crate) fn gross_loss(&self) -> f64 {
        self.gross_loss
    }
    pub(crate) fn net_profit(&self) -> f64 {
        self.net_profit
    }
    pub(crate) fn initial_capital(&self) -> f64 {
        self.config.initial_capital
    }
    pub(crate) fn tick_size(&self) -> Option<f64> {
        self.config.tick_size
    }
    pub(crate) fn invalid_fill_error(&self) -> Option<&str> {
        self.invalid_fill_error.as_deref()
    }
    pub(crate) fn gross_profit_percent(&self) -> f64 {
        self.gross_profit / self.config.initial_capital * 100.0
    }
    pub(crate) fn gross_loss_percent(&self) -> f64 {
        self.gross_loss / self.config.initial_capital * 100.0
    }
    pub(crate) fn net_profit_percent(&self) -> f64 {
        self.net_profit / self.config.initial_capital * 100.0
    }
    pub(crate) fn max_runup(&self) -> f64 {
        self.max_runup
    }
    pub(crate) fn max_drawdown(&self) -> f64 {
        self.max_drawdown
    }
    pub(crate) fn max_runup_percent(&self) -> f64 {
        self.max_runup_percent
    }
    pub(crate) fn max_drawdown_percent(&self) -> f64 {
        self.max_drawdown_percent
    }
    pub(crate) fn open_profit(&self) -> f64 {
        self.equity() - self.equity
    }
    pub(crate) fn open_profit_percent(&self) -> Option<f64> {
        (self.equity > 0.0).then(|| self.open_profit() / self.equity * 100.0)
    }
    pub(crate) fn equity(&self) -> f64 {
        self.equity
            + self
                .positions
                .iter()
                .map(|position| {
                    f64::from(position.direction)
                        * (self.mark_price - position.entry_price)
                        * position.remaining_quantity
                        - position.entry_fee_remaining
                })
                .sum::<f64>()
    }

    fn validate_fill_price(&mut self, price: f64) -> bool {
        if price.is_finite() && price > 0.0 {
            return true;
        }
        self.invalid_fill_error.get_or_insert_with(|| "Configured slippage produced a non-finite or nonpositive simulated fill price; this backtest cannot report complete trade accounting.".to_string());
        false
    }

    fn update_open_trade_excursions(&mut self, price: f64) {
        self.mark_price = price;
        for position in &mut self.positions {
            if self.current_fill_distance < position.active_from_distance {
                continue;
            }
            Self::update_position_excursions(position, price);
        }
        self.update_aggregate_excursions();
    }

    fn update_aggregate_excursions(&mut self) {
        let values = self
            .positions
            .iter()
            .map(Self::aggregate_excursion_values)
            .collect::<Vec<_>>();
        for (runup, drawdown, runup_percent, drawdown_percent) in values {
            self.max_runup = self.max_runup.max(runup);
            self.max_drawdown = self.max_drawdown.max(drawdown);
            self.max_runup_percent = self.max_runup_percent.max(runup_percent);
            self.max_drawdown_percent = self.max_drawdown_percent.max(drawdown_percent);
        }
    }

    fn update_aggregate_from_position(&mut self, position: &Position) {
        let (runup, drawdown, runup_percent, drawdown_percent) =
            Self::aggregate_excursion_values(position);
        self.max_runup = self.max_runup.max(runup);
        self.max_drawdown = self.max_drawdown.max(drawdown);
        self.max_runup_percent = self.max_runup_percent.max(runup_percent);
        self.max_drawdown_percent = self.max_drawdown_percent.max(drawdown_percent);
    }

    fn aggregate_excursion_values(position: &Position) -> (f64, f64, f64, f64) {
        let runup = position.realized_equity_on_entry - position.min_closed_equity_before_entry
            + position.max_runup;
        let drawdown = position.max_closed_equity_before_entry - position.realized_equity_on_entry
            + position.max_drawdown;
        let runup_percent = if position.min_closed_equity_before_entry > 0.0 {
            runup / position.min_closed_equity_before_entry * 100.0
        } else {
            0.0
        };
        let drawdown_percent = if position.max_closed_equity_before_entry > 0.0 {
            drawdown / position.max_closed_equity_before_entry * 100.0
        } else {
            0.0
        };
        (runup, drawdown, runup_percent, drawdown_percent)
    }

    fn update_position_excursions(position: &mut Position, price: f64) {
        let gross_profit = position.gross_pnl
            + f64::from(position.direction)
                * (price - position.entry_price)
                * position.remaining_quantity;
        let net_profit = gross_profit - position.entry_fee - position.exit_fees;
        position.max_runup = position.max_runup.max(net_profit);
        position.max_drawdown = position.max_drawdown.max(-net_profit);
        let entry_notional = position.entry_price * position.quantity;
        if entry_notional > 0.0 {
            position.max_runup_percent = position
                .max_runup_percent
                .max(net_profit / entry_notional * 100.0);
            position.max_drawdown_percent = position
                .max_drawdown_percent
                .max(-net_profit / entry_notional * 100.0);
        }
    }

    pub(crate) fn output(
        self,
        name: String,
        plots: Vec<PlotOutput>,
        mut diagnostics: Vec<Diagnostic>,
        input_overrides: std::collections::HashMap<String, serde_json::Value>,
    ) -> StrategyOutput {
        let magnifier_bars = self
            .magnifier_bars
            .iter()
            .filter(|bars| !bars.is_empty())
            .count();
        let magnifier_fallback_bars = self.magnifier_bars.len().saturating_sub(magnifier_bars);
        let fill_model = if !self.config.use_bar_magnifier {
            "Bar Magnifier is disabled by the strategy declaration; fills use chart OHLC. "
                .to_string()
        } else if magnifier_bars > 0 {
            format!("Bar Magnifier used lower-timeframe OHLC that matches chart-bar aggregate OHLC on {magnifier_bars} chart bars; uncovered or aggregate-mismatched bars use chart OHLC. Lower-timeframe interval completeness is not independently verified. ")
        } else {
            "Bar Magnifier is enabled but no aggregate-matching lower-timeframe candles were available, so fills use chart OHLC. ".to_string()
        };
        let slippage_model = if self.config.slippage_ticks == 0 {
            "`strategy.slippage` is 0 ticks.".to_string()
        } else if let Some(tick_size) = self.config.tick_size {
            format!(
                "`strategy.slippage` is {} ticks at a registered price tick of {tick_size}.",
                self.config.slippage_ticks
            )
        } else {
            "Nonzero `strategy.slippage` requires registered instrument tick metadata.".to_string()
        };
        let limit_verification_model = if self.config.limit_verification_ticks == 0 {
            "`backtest_fill_limits_assumption` is 0 ticks (touch eligible).".to_string()
        } else if let Some(tick_size) = self.config.tick_size {
            format!("`backtest_fill_limits_assumption` requires {} favorable ticks at a registered price tick of {tick_size}; verified orders fill at the requested limit.", self.config.limit_verification_ticks)
        } else {
            "Nonzero `backtest_fill_limits_assumption` requires registered instrument tick metadata.".to_string()
        };
        if self.config.calc_on_every_tick {
            diagnostics.push(Diagnostic::warning("`calc_on_every_tick=true` is a realtime-bar setting in TradingView; this historical backtest has no realtime ticks, so the setting does not change candle replay. Historical OHLC remains approximate."));
        }
        let recalculation_model = if self.config.calc_on_order_fills {
            "`calc_on_order_fills=true` recalculates after market fills at the next inferred chart OHLC point (open, then the remaining inferred path points). Price-based entries/orders, `strategy.exit()` brackets, Bar Magnifier, and `process_orders_on_close` are unsupported in this mode. Recalculations see the final OHLC values for their historical bar, which can introduce lookahead bias. Ordinary and `var` series roll back to the last committed bar state; `varip` series persist across intrabar executions. This is a bounded approximation.".to_string()
        } else {
            "`calc_on_order_fills` is disabled; fill-triggered recalculation is not modeled."
                .to_string()
        };
        diagnostics.push(Diagnostic::warning(format!("{fill_model}Historical fills support strategy.entry(), netting strategy.order(), modification of an unfilled entry/net order by shared ID, OCA cancel/reduce groups on entry and net orders, strategy.cancel()/cancel_all(), market closes, and strategy.exit() absolute stop/limit, entry-relative profit/loss tick, and trailing-stop brackets with bounded quantity reservations. OHLC fills use TradingView's inferred open-high-low-close or open-low-high-close path; bracket exits are processed before a price entry reached at the same inferred path distance, while entries at the same distance resolve by submission order and same-tick price entries may exceed the pyramiding cap. Next-open, same-path-point, and process-on-close fills execute before OCA cancel/reduce updates. {slippage_model} Slippage adjusts market and stop fills in the adverse direction; limit fills are unchanged. {limit_verification_model} {recalculation_model} Margin, currency conversion, and liquidation are not modeled, so fills remain approximate.")));
        let closed_trades = self.trades.len();
        let open_position_size = self.position_size();
        if open_position_size != 0.0 {
            diagnostics.push(Diagnostic::warning("The final bar leaves an open position; open-position profit is excluded from closed-trade statistics."));
        }
        let profit_factor_unbounded = self.gross_profit > 0.0 && self.gross_loss == 0.0;
        let profit_factor = (self.gross_loss > 0.0).then_some(self.gross_profit / self.gross_loss);
        StrategyOutput {
            valid: true,
            name,
            trades: self.trades,
            closed_trades,
            wins: self.wins,
            win_rate_pct: (closed_trades > 0)
                .then_some(self.wins as f64 / closed_trades as f64 * 100.0),
            gross_profit: self.gross_profit,
            gross_loss: self.gross_loss,
            net_profit: self.net_profit,
            initial_capital: self.config.initial_capital,
            max_runup: self.max_runup,
            max_runup_percent: self.max_runup_percent,
            max_drawdown: self.max_drawdown,
            max_drawdown_percent: self.max_drawdown_percent,
            max_drawdown_initial_capital_percent: self.max_drawdown / self.config.initial_capital
                * 100.0,
            profit_factor,
            profit_factor_unbounded,
            commission: self.config.commission,
            slippage_ticks: self.config.slippage_ticks,
            calc_on_order_fills: self.config.calc_on_order_fills,
            instrument_tick_size: self.config.tick_size,
            backtest_fill_limits_assumption_ticks: self.config.limit_verification_ticks,
            bar_magnifier_enabled: self.config.use_bar_magnifier,
            bar_magnifier_covered_chart_bars: if self.config.use_bar_magnifier {
                magnifier_bars
            } else {
                0
            },
            bar_magnifier_fallback_chart_bars: if self.config.use_bar_magnifier {
                magnifier_fallback_bars
            } else {
                0
            },
            equity_curve: self.equity_curve,
            open_position_size,
            plots,
            diagnostics,
            input_overrides,
        }
    }
}

fn order_oca(order: &PendingOrder) -> Option<(String, OcaType, f64)> {
    match order {
        PendingOrder::Entry {
            quantity,
            oca_name: Some(name),
            oca_type,
            ..
        }
        | PendingOrder::NetOrder {
            quantity,
            oca_name: Some(name),
            oca_type,
            ..
        } if *oca_type != OcaType::None => Some((name.clone(), *oca_type, *quantity)),
        _ => None,
    }
}

fn exit_levels(
    exit: &BracketExit,
    position: &Position,
    tick_size: Option<f64>,
) -> (Option<f64>, Option<f64>) {
    let tick_size = tick_size.unwrap_or(0.0);
    let relative_stop = exit
        .loss_ticks
        .map(|ticks| position.entry_price - f64::from(position.direction) * ticks * tick_size);
    let relative_limit = exit
        .profit_ticks
        .map(|ticks| position.entry_price + f64::from(position.direction) * ticks * tick_size);
    let stop = match (exit.stop, relative_stop) {
        (Some(absolute), Some(relative)) if position.direction > 0 => Some(absolute.max(relative)),
        (Some(absolute), Some(relative)) => Some(absolute.min(relative)),
        (absolute, relative) => absolute.or(relative),
    };
    let limit = match (exit.limit, relative_limit) {
        (Some(absolute), Some(relative)) if position.direction > 0 => Some(absolute.min(relative)),
        (Some(absolute), Some(relative)) => Some(absolute.max(relative)),
        (absolute, relative) => absolute.or(relative),
    };
    (stop, limit)
}

fn trailing_fill_event(
    exit: &BracketExit,
    position: &Position,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    start_distance: f64,
    through_distance: f64,
    tick_size: Option<f64>,
) -> Option<(Option<(f64, f64)>, trailing::TrailingState)> {
    let tick_size = tick_size?;
    let offset = exit.trail_offset? * tick_size;
    let activation = match (exit.trail_price, exit.trail_points) {
        (Some(absolute), points) => {
            let relative = points.map(|ticks| {
                position.entry_price + f64::from(position.direction) * ticks * tick_size
            });
            relative.map_or(absolute, |relative| {
                if position.direction > 0 {
                    absolute.min(relative)
                } else {
                    absolute.max(relative)
                }
            })
        }
        (None, Some(points)) => {
            position.entry_price + f64::from(position.direction) * points * tick_size
        }
        (None, None) => return None,
    };
    let state = exit
        .trailing_states
        .get(&position.key)
        .copied()
        .unwrap_or_default();
    Some(trailing::fill_event(
        position.direction,
        open,
        high,
        low,
        close,
        activation,
        offset,
        start_distance,
        through_distance,
        state,
    ))
}

fn bracket_fill_price(
    direction: i8,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    stop: Option<f64>,
    limit: Option<f64>,
    limit_verification_distance: f64,
) -> Option<(f64, f64, bool)> {
    let verified_limit =
        limit.map(|price| price + f64::from(direction) * limit_verification_distance);
    if direction > 0 {
        if stop.is_some_and(|price| open <= price) {
            return Some((0.0, open, true));
        }
        if verified_limit.is_some_and(|price| open >= price) {
            return Some((0.0, open, false));
        }
    } else {
        if stop.is_some_and(|price| open >= price) {
            return Some((0.0, open, true));
        }
        if verified_limit.is_some_and(|price| open <= price) {
            return Some((0.0, open, false));
        }
    }

    let path = inferred_ohlc_path(open, high, low, close);
    let mut travelled = 0.0;
    for segment in path.windows(2) {
        let from = segment[0];
        let to = segment[1];
        let crossed_stop = stop
            .filter(|price| price.is_finite() && from.min(to) <= *price && *price <= from.max(to));
        let crossed_limit = verified_limit
            .filter(|price| price.is_finite() && from.min(to) <= *price && *price <= from.max(to));
        match (crossed_stop, crossed_limit) {
            (Some(stop_price), Some(limit_price)) => {
                if (stop_price - from).abs() <= (limit_price - from).abs() {
                    return Some((travelled + (stop_price - from).abs(), stop_price, true));
                }
                return Some((
                    travelled + (limit_price - from).abs(),
                    limit.expect("verified limit has an original level"),
                    false,
                ));
            }
            (Some(price), None) => return Some((travelled + (price - from).abs(), price, true)),
            (None, Some(price)) => {
                return Some((
                    travelled + (price - from).abs(),
                    limit.expect("verified limit has an original level"),
                    false,
                ))
            }
            (None, None) => {}
        }
        travelled += (to - from).abs();
    }
    None
}

fn entry_fill_on_close(
    order: &mut PriceEntryOrder,
    close: f64,
    limit_verification_distance: f64,
) -> Option<f64> {
    let is_long = order.direction > 0;
    let stop_reached = order.stop.is_some_and(|stop| {
        if is_long {
            close >= stop
        } else {
            close <= stop
        }
    });
    if stop_reached {
        order.stop_activated = true;
    }
    if !order.stop_activated {
        return None;
    }
    match order.limit {
        Some(limit)
            if (is_long && close <= limit - limit_verification_distance)
                || (!is_long && close >= limit + limit_verification_distance) =>
        {
            Some(close)
        }
        Some(_) => None,
        None => Some(close),
    }
}

fn entry_fill_event(
    order: &PriceEntryOrder,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    limit_verification_distance: f64,
    through_distance: f64,
) -> (Option<(f64, f64)>, bool) {
    let stop = order.stop;
    let limit = order.limit;
    let is_long = order.direction > 0;
    let stop_reached = |price: f64| {
        stop.is_some_and(|stop| {
            if is_long {
                price >= stop
            } else {
                price <= stop
            }
        })
    };
    let verified_limit =
        limit.map(|level| level - f64::from(order.direction) * limit_verification_distance);
    let limit_marketable = |price: f64, level: f64| {
        if is_long {
            price <= level
        } else {
            price >= level
        }
    };
    let mut stop_activated = order.stop_activated;

    // A scan can discover several stop-market orders at the same path point.
    // After the first fills, the broker rescans the remaining orders; preserve
    // their original crossing event instead of treating activation as a
    // completed fill.
    if stop_activated && limit.is_none() {
        let stop = stop.expect("activated stop-market orders have a stop price");
        if stop_reached(open) && 0.0 <= through_distance {
            return (Some((0.0, open)), stop_activated);
        }
        let path = inferred_ohlc_path(open, high, low, close);
        let mut travelled = 0.0;
        for segment in path.windows(2) {
            let crossed = if is_long {
                segment[0] < stop && segment[1] >= stop
            } else {
                segment[0] > stop && segment[1] <= stop
            };
            let distance = travelled + (stop - segment[0]).abs();
            if crossed && distance <= through_distance {
                return (Some((distance, stop)), stop_activated);
            }
            travelled += (segment[1] - segment[0]).abs();
        }
        return (None, stop_activated);
    }

    if let Some(limit) = limit {
        if order.stop.is_none() {
            if limit_marketable(
                open,
                verified_limit.expect("limit order has a verification level"),
            ) && 0.0 <= through_distance
            {
                return (Some((0.0, open)), stop_activated);
            }
            let path = inferred_ohlc_path(open, high, low, close);
            let mut travelled = 0.0;
            for segment in path.windows(2) {
                let verification = verified_limit.expect("limit order has a verification level");
                let crosses = if is_long {
                    segment[0] > verification && segment[1] <= verification
                } else {
                    segment[0] < verification && segment[1] >= verification
                };
                let distance = travelled + (verification - segment[0]).abs();
                if crosses && distance <= through_distance {
                    return (Some((distance, limit)), stop_activated);
                }
                travelled += (segment[1] - segment[0]).abs();
            }
            return (None, stop_activated);
        }
    }

    let mut travelled = 0.0;
    if stop_activated {
        if limit.is_some() {
            if limit_marketable(
                open,
                verified_limit.expect("stop-limit order has a verification level"),
            ) && 0.0 <= through_distance
            {
                return (Some((0.0, open)), stop_activated);
            }
        } else if stop_reached(open) {
            return (Some((0.0, open)), stop_activated);
        }
    } else if stop_reached(open) {
        stop_activated = true;
        if limit.is_some() {
            if limit_marketable(
                open,
                verified_limit.expect("stop-limit order has a verification level"),
            ) && 0.0 <= through_distance
            {
                return (Some((0.0, open)), stop_activated);
            }
        } else {
            return (Some((0.0, open)), stop_activated);
        }
    }

    let path = inferred_ohlc_path(open, high, low, close);
    for segment in path.windows(2) {
        let from = segment[0];
        let to = segment[1];
        if !stop_activated {
            let stop = stop.expect("unactivated orders have a stop price");
            let crossed_stop = if is_long {
                from < stop && to >= stop
            } else {
                from > stop && to <= stop
            };
            if !crossed_stop {
                travelled += (to - from).abs();
                continue;
            }
            let activation_distance = travelled + (stop - from).abs();
            if activation_distance > through_distance {
                return (None, stop_activated);
            }
            stop_activated = true;
            if limit.is_none() {
                return (Some((activation_distance, stop)), stop_activated);
            }
            if limit_marketable(
                stop,
                verified_limit.expect("stop-limit order has a verification level"),
            ) {
                return (Some((activation_distance, stop)), stop_activated);
            }
            travelled += (to - from).abs();
            continue;
        }
        if let Some(limit) = limit {
            let verification = verified_limit.expect("stop-limit order has a verification level");
            let crosses_limit = if is_long {
                from > verification && to <= verification
            } else {
                from < verification && to >= verification
            };
            let distance = travelled + (verification - from).abs();
            if crosses_limit && distance <= through_distance {
                return (Some((distance, limit)), stop_activated);
            }
        }
        travelled += (to - from).abs();
    }
    (None, stop_activated)
}

fn inferred_ohlc_path(open: f64, high: f64, low: f64, close: f64) -> [f64; 4] {
    if (open - high).abs() < (open - low).abs() {
        [open, high, low, close]
    } else {
        [open, low, high, close]
    }
}
