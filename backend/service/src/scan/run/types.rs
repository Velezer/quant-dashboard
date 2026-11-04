use super::super::*;
use super::stockbit_close::StockbitCloseScanCheckpoint;

pub(super) type RegisteredInstrument = (String, String, String, String, String, String);

pub(super) struct ScanCycleOutput {
    pub(super) user_indicator_alerts: Vec<Value>,
    pub(super) running_signal_rows: Vec<Value>,
    pub(super) potential_signal_rows: Vec<Value>,
    pub(super) standalone_signal_rows: Vec<Value>,
    pub(super) readiness: signal_alerts::SignalReadiness,
    pub(super) score_updates: Vec<signal_alerts::StockbitSignalScoreUpdate>,
    pub(super) scanned_instruments: Vec<(String, String, String)>,
    pub(super) preserved_stockbit: std::collections::BTreeSet<(String, String, String)>,
    pub(super) close_checkpoints: Vec<StockbitCloseScanCheckpoint>,
    pub(super) generated_plan_count: usize,
    pub(super) price_data_unavailable_count: usize,
    pub(super) app_workers: Vec<Value>,
}

impl ScanCycleOutput {
    pub(super) fn new() -> Self {
        Self {
            user_indicator_alerts: Vec::new(),
            running_signal_rows: Vec::new(),
            potential_signal_rows: Vec::new(),
            standalone_signal_rows: Vec::new(),
            readiness: signal_alerts::SignalReadiness::new(),
            score_updates: Vec::new(),
            scanned_instruments: Vec::new(),
            preserved_stockbit: std::collections::BTreeSet::new(),
            close_checkpoints: Vec::new(),
            generated_plan_count: 0,
            price_data_unavailable_count: 0,
            app_workers: Vec::new(),
        }
    }

    #[cfg(not(test))]
    pub(super) fn merge(&mut self, mut other: Self) {
        self.user_indicator_alerts
            .append(&mut other.user_indicator_alerts);
        self.running_signal_rows
            .append(&mut other.running_signal_rows);
        self.potential_signal_rows
            .append(&mut other.potential_signal_rows);
        self.standalone_signal_rows
            .append(&mut other.standalone_signal_rows);
        self.readiness.merge(other.readiness);
        self.score_updates.append(&mut other.score_updates);
        self.scanned_instruments
            .append(&mut other.scanned_instruments);
        self.preserved_stockbit.extend(other.preserved_stockbit);
        self.close_checkpoints.append(&mut other.close_checkpoints);
        self.generated_plan_count += other.generated_plan_count;
        self.price_data_unavailable_count += other.price_data_unavailable_count;
        self.app_workers.append(&mut other.app_workers);
    }
}

#[derive(Default)]
pub(super) struct AppScanResult {
    pub(super) fresh_price_count: usize,
    pub(super) unavailable_price_count: usize,
    pub(super) generated_plan_count: usize,
    pub(super) signal_row_count: usize,
    pub(super) paused_market_closed_count: usize,
}

