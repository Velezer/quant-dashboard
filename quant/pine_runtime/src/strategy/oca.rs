use super::*;

impl StrategyBroker {
    pub(super) fn fill_immediate_closes_at_current_tick_inner(
        &mut self,
        time: Option<i64>,
    ) -> bool {
        let immediate = std::mem::take(&mut self.pending_on_close);
        let bar = self.current_bar;
        let (oca_fills, filled) =
            self.fill_order_batch(immediate, self.mark_price, bar, time, true, Vec::new());
        for (name, oca_type, quantity) in oca_fills {
            self.apply_oca_fill(&name, oca_type, quantity);
        }
        filled
    }

    pub(super) fn fill_pending_market_orders_at_tick(
        &mut self,
        price: f64,
        time: Option<i64>,
    ) -> bool {
        let pending = std::mem::take(&mut self.pending);
        let bar = self.current_bar;
        let (oca_fills, filled) =
            self.fill_order_batch(pending, price, bar, time, true, Vec::new());
        for (name, oca_type, quantity) in oca_fills {
            self.apply_oca_fill(&name, oca_type, quantity);
        }
        filled
    }

    pub(super) fn fill_pending_market_orders_at_open(
        &mut self,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        bar: usize,
        time: Option<i64>,
    ) -> bool {
        let pending = std::mem::take(&mut self.pending);
        let (oca_fills, filled) = self.fill_order_batch(pending, open, bar, time, true, Vec::new());
        let same_tick_price_orders = self
            .price_entries
            .iter()
            .filter_map(|candidate| {
                let (event, _) = entry_fill_event(
                    candidate,
                    open,
                    high,
                    low,
                    close,
                    self.config.limit_verification_distance(),
                    f64::INFINITY,
                );
                event
                    .filter(|(distance, _)| *distance == 0.0)
                    .map(|_| candidate.id.clone())
            })
            .collect::<std::collections::HashSet<_>>();
        for (name, oca_type, quantity) in oca_fills {
            self.apply_oca_fill_preserving_same_tick(
                &name,
                oca_type,
                quantity,
                &same_tick_price_orders,
            );
        }
        filled
    }

    pub(super) fn fill_pending_market_orders_at_close(
        &mut self,
        close: f64,
        time: Option<i64>,
    ) -> bool {
        let deferred = std::mem::take(&mut self.deferred_close_oca_fills);
        let pending_on_close = std::mem::take(&mut self.pending_on_close);
        let bar = self.current_bar;
        let (oca_fills, filled) =
            self.fill_order_batch(pending_on_close, close, bar, time, true, deferred);
        for (name, oca_type, quantity) in oca_fills {
            self.apply_oca_fill(&name, oca_type, quantity);
        }
        filled
    }

    fn fill_order_batch(
        &mut self,
        orders: Vec<PendingOrder>,
        price: f64,
        bar: usize,
        time: Option<i64>,
        apply_slippage: bool,
        mut oca_fills: Vec<(String, OcaType, f64)>,
    ) -> (Vec<(String, OcaType, f64)>, bool) {
        let mut filled = false;
        for order in orders {
            let oca = order_oca(&order);
            let position_state = self
                .positions
                .iter()
                .map(|position| (position.key, position.remaining_quantity))
                .collect::<Vec<_>>();
            let closed_trades = self.trades.len();
            self.fill(order, price, bar, time, apply_slippage, false);
            let order_executed = self.trades.len() != closed_trades
                || self
                    .positions
                    .iter()
                    .map(|position| (position.key, position.remaining_quantity))
                    .collect::<Vec<_>>()
                    != position_state;
            filled |= order_executed;
            if order_executed {
                if let Some((name, oca_type, quantity)) = oca {
                    oca_fills.push((name, oca_type, quantity));
                }
            }
        }
        (oca_fills, filled)
    }
}
