use super::*;

const MAX_HISTORICAL_ORDER_EVENTS_PER_BAR: usize = 10_000;

impl StrategyBroker {
    pub(crate) fn begin_bar(
        &mut self,
        bar: usize,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        time: Option<i64>,
    ) -> bool {
        self.current_bar = bar;
        self.current_fill_distance = 0.0;
        self.history_tick_index = 0;
        for position in &mut self.positions {
            position.active_from_distance = 0.0;
        }
        self.set_bar_path(open, high, low, close);
        self.update_open_trade_excursions(open);
        let filled_at_open =
            self.fill_pending_market_orders_at_open(open, high, low, close, bar, time);
        if self.config.calc_on_order_fills {
            return filled_at_open;
        }
        if let Some(magnifier_bars) = self
            .config
            .use_bar_magnifier
            .then(|| self.magnifier_bars.get(bar))
            .flatten()
            .filter(|bars| !bars.is_empty())
            .cloned()
        {
            for lower_bar in magnifier_bars {
                self.current_fill_distance = 0.0;
                self.history_tick_index = 0;
                for position in &mut self.positions {
                    position.active_from_distance = 0.0;
                }
                self.set_bar_path(
                    lower_bar.open,
                    lower_bar.high,
                    lower_bar.low,
                    lower_bar.close,
                );
                self.update_open_trade_excursions(lower_bar.open);
                self.process_bar_path(
                    lower_bar.open,
                    lower_bar.high,
                    lower_bar.low,
                    lower_bar.close,
                    lower_bar.time,
                );
                self.update_open_trade_excursions(lower_bar.close);
            }
        } else {
            self.process_bar_path(open, high, low, close, time);
        }
        false
    }

    pub(crate) fn advance_history_tick(&mut self, time: Option<i64>) -> bool {
        if self.history_tick_index + 1 >= self.bar_path.len() {
            return false;
        }
        self.history_tick_index += 1;
        let (price, distance) = self.bar_path[self.history_tick_index];
        self.current_fill_distance = distance;
        self.update_open_trade_excursions(price);
        self.fill_pending_market_orders_at_tick(price, time)
    }

    pub(crate) fn defer_close_orders_to_next_bar_open(&mut self) {
        self.pending.append(&mut self.pending_on_close);
    }

    fn set_bar_path(&mut self, open: f64, high: f64, low: f64, close: f64) {
        let path = inferred_ohlc_path(open, high, low, close);
        self.bar_path = path
            .into_iter()
            .enumerate()
            .scan(0.0, |distance, (index, price)| {
                if index > 0 {
                    *distance += (price - path[index - 1]).abs();
                }
                Some((price, *distance))
            })
            .collect();
    }

    pub(crate) fn process_bar_path(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        time: Option<i64>,
    ) {
        let total_distance = self
            .bar_path
            .last()
            .map(|(_, distance)| *distance)
            .unwrap_or(0.0);
        let mut distance = 0.0;
        let mut event_steps = 0usize;
        while event_steps < MAX_HISTORICAL_ORDER_EVENTS_PER_BAR {
            event_steps += 1;
            let next_bracket = self.next_bracket_fill_distance(open, high, low, close);
            let next_entry = self.next_entry_fill_distance(open, high, low, close);
            let next_event = match (next_bracket, next_entry) {
                (Some(bracket), Some(entry)) => Some(bracket.min(entry)),
                (Some(bracket), None) => Some(bracket),
                (None, Some(entry)) => Some(entry),
                (None, None) => None,
            }
            .filter(|next| *next <= total_distance && *next >= distance);
            let Some(next_event) = next_event else {
                break;
            };
            self.advance_excursions_to(next_event);
            self.current_fill_distance = next_event;
            let positions_before = self
                .positions
                .iter()
                .map(|position| (position.key, position.remaining_quantity))
                .collect::<Vec<_>>();
            let trades_before = self.trades.len();
            let price_entries_before = self.price_entries.len();
            let brackets_before = self.bracket_exits.len();
            self.process_fixed_brackets(open, high, low, close, time, distance, next_event);
            self.fill_price_entries(open, high, low, close, self.current_bar, time, next_event);
            let state_changed = positions_before
                != self
                    .positions
                    .iter()
                    .map(|position| (position.key, position.remaining_quantity))
                    .collect::<Vec<_>>()
                || trades_before != self.trades.len()
                || price_entries_before != self.price_entries.len()
                || brackets_before != self.bracket_exits.len();
            if !state_changed {
                let advanced = self
                    .bar_path
                    .iter()
                    .find(|(_, point_distance)| *point_distance > distance)
                    .map(|(_, point_distance)| *point_distance);
                let Some(advanced) = advanced else {
                    break;
                };
                self.advance_excursions_to(advanced);
                distance = advanced;
            } else {
                distance = next_event;
            }
        }
        let has_pending_event = self
            .next_bracket_fill_distance(open, high, low, close)
            .is_some_and(|distance| distance <= total_distance)
            || self
                .next_entry_fill_distance(open, high, low, close)
                .is_some_and(|distance| distance <= total_distance);
        if event_steps == MAX_HISTORICAL_ORDER_EVENTS_PER_BAR && has_pending_event {
            self.invalid_fill_error.get_or_insert_with(|| "Historical order path exceeded the bounded 10,000 event limit for one candle; this backtest cannot report complete trade accounting.".to_string());
        }
        self.advance_excursions_to(total_distance);
        self.current_fill_distance = total_distance;
        self.fill_bracket_exit(
            open,
            high,
            low,
            close,
            self.current_bar,
            time,
            distance,
            total_distance,
        );
    }

    fn process_fixed_brackets(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        time: Option<i64>,
        start_distance: f64,
        through_distance: f64,
    ) {
        self.fill_bracket_exit(
            open,
            high,
            low,
            close,
            self.current_bar,
            time,
            start_distance,
            through_distance,
        );
    }

    fn advance_excursions_to(&mut self, distance: f64) {
        while self.history_tick_index + 1 < self.bar_path.len()
            && self.bar_path[self.history_tick_index + 1].1 <= distance
        {
            self.history_tick_index += 1;
            let (price, tick_distance) = self.bar_path[self.history_tick_index];
            self.current_fill_distance = tick_distance;
            self.update_open_trade_excursions(price);
        }
    }

    fn next_entry_fill_distance(&self, open: f64, high: f64, low: f64, close: f64) -> Option<f64> {
        self.price_entries
            .iter()
            .filter_map(|order| {
                entry_fill_event(
                    order,
                    open,
                    high,
                    low,
                    close,
                    self.config.limit_verification_distance(),
                    f64::INFINITY,
                )
                .0
                .map(|(distance, _)| distance)
            })
            .min_by(f64::total_cmp)
    }

    fn next_bracket_fill_distance(
        &self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    ) -> Option<f64> {
        let mut next = None;
        for exit in &self.bracket_exits {
            for position in &self.positions {
                if position.remaining_quantity <= 1e-12
                    || exit.from_entry.as_ref().is_some_and(|entry| {
                        entry != &position.id || position.entry_order_bar > exit.created_bar
                    })
                {
                    continue;
                }
                let (stop, limit) = exit_levels(exit, position, self.config.tick_size);
                let fixed_distance = bracket_fill_price(
                    position.direction,
                    open,
                    high,
                    low,
                    close,
                    stop,
                    limit,
                    self.config.limit_verification_distance(),
                )
                .map(|(distance, _, _)| distance);
                let trailing_distance = trailing_fill_event(
                    exit,
                    position,
                    open,
                    high,
                    low,
                    close,
                    self.current_fill_distance
                        .max(position.active_from_distance),
                    f64::INFINITY,
                    self.config.tick_size,
                )
                .and_then(|(event, _)| event.map(|(distance, _)| distance));
                if let Some(distance) = fixed_distance
                    .into_iter()
                    .chain(trailing_distance)
                    .filter(|distance| *distance >= position.active_from_distance)
                    .min_by(f64::total_cmp)
                {
                    next = Some(next.map_or(distance, |previous: f64| previous.min(distance)));
                }
            }
        }
        next
    }

    fn fill_price_entries(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        bar: usize,
        time: Option<i64>,
        through_distance: f64,
    ) {
        loop {
            let calculations = self
                .price_entries
                .iter()
                .map(|order| {
                    entry_fill_event(
                        order,
                        open,
                        high,
                        low,
                        close,
                        self.config.limit_verification_distance(),
                        through_distance,
                    )
                })
                .collect::<Vec<_>>();
            for (order, (_, activated)) in self.price_entries.iter_mut().zip(&calculations) {
                order.stop_activated = *activated;
            }
            let next_distance = calculations
                .iter()
                .filter_map(|(event, _)| event.map(|(distance, _)| distance))
                .min_by(f64::total_cmp);
            let Some(distance) = next_distance else {
                break;
            };
            let same_tick_ids = calculations
                .iter()
                .enumerate()
                .filter_map(|(index, (event, _))| {
                    event
                        .filter(|(event_distance, _)| *event_distance == distance)
                        .map(|_| self.price_entries[index].id.clone())
                })
                .collect::<std::collections::HashSet<_>>();
            let batch = calculations
                .iter()
                .enumerate()
                .filter_map(|(index, (event, _))| {
                    event
                        .filter(|(event_distance, _)| *event_distance == distance)
                        .map(|(_, price)| (index, distance, price))
                })
                .collect::<Vec<_>>();
            let mut orders = batch
                .into_iter()
                .rev()
                .map(|(index, event_distance, price)| {
                    (self.price_entries.remove(index), event_distance, price)
                })
                .collect::<Vec<_>>();
            orders.reverse();
            let mut oca_fills: Vec<(String, OcaType, f64)> = Vec::new();
            for (order, event_distance, price) in orders {
                self.current_fill_distance = distance;
                let position_state = self
                    .positions
                    .iter()
                    .map(|position| (position.key, position.remaining_quantity))
                    .collect::<Vec<_>>();
                let closed_trades = self.trades.len();
                let oca = (order.oca_name.clone(), order.oca_type, order.quantity);
                if order.net_order {
                    self.apply_net_order(
                        order.id,
                        order.direction,
                        order.quantity,
                        order.created_bar,
                        price,
                        bar,
                        time,
                        order.stop.is_some() && order.limit.is_none(),
                    );
                } else {
                    self.fill(
                        PendingOrder::Entry {
                            id: order.id,
                            direction: order.direction,
                            quantity: order.quantity,
                            created_bar: order.created_bar,
                            oca_name: order.oca_name,
                            oca_type: order.oca_type,
                        },
                        price,
                        bar,
                        time,
                        order.stop.is_some() && order.limit.is_none(),
                        true,
                    );
                }
                let order_executed = self.trades.len() != closed_trades
                    || self
                        .positions
                        .iter()
                        .map(|position| (position.key, position.remaining_quantity))
                        .collect::<Vec<_>>()
                        != position_state;
                if order_executed {
                    if let (Some(name), oca_type, quantity) = oca {
                        if oca_type != OcaType::None {
                            oca_fills.push((name, oca_type, quantity));
                        }
                    }
                }
                let _ = event_distance;
            }
            for (name, oca_type, quantity) in oca_fills {
                self.apply_oca_fill_preserving_same_tick(&name, oca_type, quantity, &same_tick_ids);
            }
        }
    }
}
