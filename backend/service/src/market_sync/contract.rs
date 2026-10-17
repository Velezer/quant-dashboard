use rusqlite::Connection;

#[derive(Clone, Copy)]
pub(super) struct RegisteredCandleInstrument<'a> {
    pub(super) exchange_id: &'a str,
    pub(super) storage_symbol: &'a str,
    pub(super) product_id: &'a str,
    pub(super) underlying_symbol: &'a str,
    pub(super) asset_category_id: &'a str,
    pub(super) tokocrypto_market: Option<RegisteredTokocryptoMarket<'a>>,
}

#[derive(Clone, Copy)]
pub(super) struct RegisteredTokocryptoMarket<'a> {
    pub(super) venue_symbol: &'a str,
    pub(super) quote_currency: &'a str,
    pub(super) contract_metadata_json: &'a str,
}

pub(super) struct PreparedTokocryptoMarket {
    pub(super) venue_symbol: String,
    pub(super) quote_currency: String,
    pub(super) contract_metadata_json: String,
}

pub(super) struct PreparedCandleSyncTarget {
    pub(super) exchange_id: String,
    pub(super) storage_symbol: String,
    pub(super) product_id: String,
    pub(super) underlying_symbol: String,
    pub(super) asset_category_id: String,
    pub(super) tokocrypto_market: Option<PreparedTokocryptoMarket>,
}

#[derive(Clone, Copy)]
pub(super) enum CandleSyncWindow {
    Intraday,
    Daily,
}

impl CandleSyncWindow {
    pub(super) fn intervals(self) -> &'static [&'static str] {
        match self {
            Self::Intraday => &["15m", "1h"],
            Self::Daily => &["1d"],
        }
    }
}

pub(super) struct CandleSyncRequest<'a> {
    pub(super) instrument: RegisteredCandleInstrument<'a>,
    pub(super) window: CandleSyncWindow,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CandleSourceClass {
    VenueNative,
    ResearchProxy,
}

pub(super) fn should_continue_candle_provider_chain(
    source_class: CandleSourceClass,
    succeeded: bool,
) -> bool {
    source_class == CandleSourceClass::VenueNative && !succeeded
}

pub(super) struct CandleProviderCapabilities {
    pub(super) exchange_ids: &'static [&'static str],
    pub(super) product_ids: &'static [&'static str],
    pub(super) asset_categories: &'static [&'static str],
    pub(super) intervals: &'static [&'static str],
}

impl CandleProviderCapabilities {
    pub(super) fn supports(&self, request: &CandleSyncRequest<'_>) -> bool {
        let instrument = request.instrument;
        self.exchange_ids.contains(&instrument.exchange_id)
            && self.product_ids.contains(&instrument.product_id)
            && self
                .asset_categories
                .contains(&instrument.asset_category_id)
            && request
                .window
                .intervals()
                .iter()
                .all(|interval| self.intervals.contains(interval))
    }
}

#[derive(Debug)]
pub(super) struct CandleProviderFailure {
    pub(super) message: String,
}

pub(super) struct CandleIntervalOutcome {
    pub(super) interval: &'static str,
    pub(super) result: Result<usize, CandleProviderFailure>,
}

pub(super) struct CandleProviderReport {
    pub(super) outcomes: Vec<CandleIntervalOutcome>,
}

impl CandleProviderReport {
    pub(super) fn interval(
        interval: &'static str,
        result: Result<usize, String>,
    ) -> CandleIntervalOutcome {
        CandleIntervalOutcome {
            interval,
            result: result.map_err(|message| CandleProviderFailure { message }),
        }
    }

    pub(super) fn all_succeeded(&self) -> bool {
        !self.outcomes.is_empty() && self.outcomes.iter().all(|outcome| outcome.result.is_ok())
    }

    pub(super) fn error_summary(&self) -> String {
        self.outcomes
            .iter()
            .filter_map(|outcome| {
                outcome
                    .result
                    .as_ref()
                    .err()
                    .map(|error| format!("{}: {}", outcome.interval, error.message))
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

pub(super) trait CandleSyncAdapter: Sync {
    fn provider_id(&self) -> &'static str;
    fn source_class(&self) -> CandleSourceClass;
    fn capabilities(&self) -> &'static CandleProviderCapabilities;
    fn supports_symbol(&self, instrument: RegisteredCandleInstrument<'_>) -> bool;
    fn sync(
        &self,
        connection: &Connection,
        request: &CandleSyncRequest<'_>,
    ) -> CandleProviderReport;

    fn supports(&self, request: &CandleSyncRequest<'_>) -> bool {
        self.capabilities().supports(request) && self.supports_symbol(request.instrument)
    }
}
