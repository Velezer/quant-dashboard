use super::*;
use std::{
    collections::HashSet,
    sync::{Condvar, Mutex},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarketConnectionKind {
    Regular,
    SignalScan,
}

#[derive(Default)]
struct MarketConnectionCounts {
    regular: usize,
    signal_scan: usize,
    active_symbols: HashSet<String>,
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
        self.acquire_inner(kind, None);
    }

    pub fn acquire_ticker(&self, kind: MarketConnectionKind, storage_symbol: &str) {
        self.acquire_inner(kind, Some(storage_symbol));
    }

    fn acquire_inner(&self, kind: MarketConnectionKind, storage_symbol: Option<&str>) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while storage_symbol.is_some_and(|symbol| active.active_symbols.contains(symbol))
            || !self.has_capacity(&active, kind)
        {
            active = self
                .ready
                .wait(active)
                .unwrap_or_else(|error| error.into_inner());
        }
        if let Some(symbol) = storage_symbol {
            active.active_symbols.insert(symbol.to_string());
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
        self.release_inner(kind, None);
    }

    pub fn release_ticker(&self, kind: MarketConnectionKind, storage_symbol: &str) {
        self.release_inner(kind, Some(storage_symbol));
    }

    fn release_inner(&self, kind: MarketConnectionKind, storage_symbol: Option<&str>) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(symbol) = storage_symbol {
            active.active_symbols.remove(symbol);
        }
        match kind {
            MarketConnectionKind::Regular => active.regular = active.regular.saturating_sub(1),
            MarketConnectionKind::SignalScan => {
                active.signal_scan = active.signal_scan.saturating_sub(1)
            }
        }
        self.ready.notify_all();
    }
}
