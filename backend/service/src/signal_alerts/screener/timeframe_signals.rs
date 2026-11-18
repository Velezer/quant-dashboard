use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(super) struct ScreenerMatch {
    pub rowid: i64,
    pub market_entry: bool,
    pub stale_score: bool,
    pub stale_price: bool,
}

impl ScreenerMatch {
    fn priority(self) -> (bool, bool, bool) {
        (self.market_entry, !self.stale_score, !self.stale_price)
    }
}

type TimeframeIdentity = [Box<str>; 5];

#[derive(Default)]
pub(super) struct ScreenerMatches {
    ordered: Vec<Option<ScreenerMatch>>,
    timeframe_positions: HashMap<TimeframeIdentity, usize>,
}

impl ScreenerMatches {
    pub fn push(&mut self, candidate: ScreenerMatch, signal: &Value, group_by_timeframe: bool) {
        if group_by_timeframe {
            let identity = [
                "exchange_id",
                "product_id",
                "asset_category_id",
                "symbol",
                "timeframe",
            ]
            .map(|field| signal[field].as_str().unwrap_or_default().into());
            let position = self
                .timeframe_positions
                .entry(identity)
                .or_insert(self.ordered.len());
            if let Some(previous) = self.ordered.get_mut(*position) {
                if previous.is_some_and(|previous| previous.priority() >= candidate.priority()) {
                    return;
                }
                *previous = None;
                *position = self.ordered.len();
            }
        }
        self.ordered.push(Some(candidate));
    }

    pub fn into_ordered(self) -> impl Iterator<Item = ScreenerMatch> {
        self.ordered.into_iter().flatten()
    }
}
