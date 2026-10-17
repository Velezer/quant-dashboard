use super::*;
use std::sync::{Condvar, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarketConnectionKind {
    Regular,
    SignalScan,
}

#[derive(Default)]
struct MarketConnectionCounts {
    regular: usize,
    signal_scan: usize,
}

pub struct MarketConnectionGate {
    active: Mutex<MarketConnectionCounts>,
    pub ready: Condvar,
}

impl MarketConnectionGate {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(MarketConnectionCounts::default()),
            ready: Condvar::new(),
        }
    }

    #[cfg(test)]
    pub fn active_counts(&self) -> (usize, usize) {
        let active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        (active.regular, active.signal_scan)
    }

    pub fn acquire(&self, kind: MarketConnectionKind) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while !self.has_capacity(&active, kind) {
            active = self
                .ready
                .wait(active)
                .unwrap_or_else(|error| error.into_inner());
        }
        match kind {
            MarketConnectionKind::Regular => active.regular += 1,
            MarketConnectionKind::SignalScan => active.signal_scan += 1,
        }
    }

    fn has_capacity(&self, active: &MarketConnectionCounts, kind: MarketConnectionKind) -> bool {
        let total_active = active.regular + active.signal_scan;
        match kind {
            MarketConnectionKind::Regular => {
                active.regular < MAX_OPEN_REGULAR_MARKET_DATABASES
                    && total_active < MAX_OPEN_MARKET_DATABASES_TOTAL
            }
            MarketConnectionKind::SignalScan => {
                active.signal_scan < SIGNAL_SCAN_DATABASE_RESERVE
                    && total_active < MAX_OPEN_MARKET_DATABASES_TOTAL
            }
        }
    }

    pub fn release(&self, kind: MarketConnectionKind) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match kind {
            MarketConnectionKind::Regular => active.regular = active.regular.saturating_sub(1),
            MarketConnectionKind::SignalScan => {
                active.signal_scan = active.signal_scan.saturating_sub(1)
            }
        }
        self.ready.notify_all();
    }
}
