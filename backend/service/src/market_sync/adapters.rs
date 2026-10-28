use super::*;

pub(super) struct TokocryptoNativeAdapter;
pub(super) struct PintuNativeAdapter;

static PINTU_NATIVE_CAPABILITIES: CandleProviderCapabilities = CandleProviderCapabilities {
    exchange_ids: &["pintu"],
    product_ids: &["pintu_pro_futures"],
    asset_categories: &["crypto_perpetual"],
    intervals: &["15m", "1h", "1d"],
};

impl CandleSyncAdapter for PintuNativeAdapter {
    fn provider_id(&self) -> &'static str {
        "pintu"
    }

    fn source_class(&self) -> CandleSourceClass {
        CandleSourceClass::VenueNative
    }

    fn capabilities(&self) -> &'static CandleProviderCapabilities {
        &PINTU_NATIVE_CAPABILITIES
    }

    fn supports_symbol(&self, instrument: RegisteredCandleInstrument<'_>) -> bool {
        instrument.underlying_symbol.ends_with("USDT")
            && instrument.underlying_symbol.len() > "USDT".len()
    }

    fn sync(
        &self,
        connection: &Connection,
        request: &CandleSyncRequest<'_>,
    ) -> CandleProviderReport {
        let instrument = request.instrument;
        let outcomes = match request.window {
            CandleSyncWindow::Intraday => {
                match crate::pintu_storage::sync_intraday_asset(
                    connection,
                    instrument.storage_symbol,
                    instrument.product_id,
                    instrument.asset_category_id,
                ) {
                    Ok((rows_15m, rows_1h)) => vec![
                        CandleProviderReport::interval("15m", Ok(rows_15m)),
                        CandleProviderReport::interval("1h", Ok(rows_1h)),
                    ],
                    Err(error) => {
                        vec![CandleProviderReport::interval("intraday", Err(error))]
                    }
                }
            }
            CandleSyncWindow::Daily => vec![CandleProviderReport::interval(
                "1d",
                crate::pintu_storage::sync_daily_asset(
                    connection,
                    instrument.storage_symbol,
                    instrument.product_id,
                    instrument.asset_category_id,
                ),
            )],
            CandleSyncWindow::SingleInterval(interval) => vec![CandleProviderReport::interval(
                interval,
                Err("venue-native adapter requires the complete scheduled window".into()),
            )],
        };
        CandleProviderReport { outcomes }
    }
}

static TOKOCRYPTO_NATIVE_CAPABILITIES: CandleProviderCapabilities = CandleProviderCapabilities {
    exchange_ids: &["tokocrypto"],
    product_ids: &["tokocrypto_spot", "tokocrypto_tokenized_stock"],
    asset_categories: &["crypto_spot", "tokenized_stock"],
    intervals: &["15m", "1h", "1d"],
};

impl CandleSyncAdapter for TokocryptoNativeAdapter {
    fn provider_id(&self) -> &'static str {
        "tokocrypto"
    }

    fn source_class(&self) -> CandleSourceClass {
        CandleSourceClass::VenueNative
    }

    fn capabilities(&self) -> &'static CandleProviderCapabilities {
        &TOKOCRYPTO_NATIVE_CAPABILITIES
    }

    fn supports_symbol(&self, instrument: RegisteredCandleInstrument<'_>) -> bool {
        matches!(
            (instrument.product_id, instrument.asset_category_id),
            ("tokocrypto_spot", "crypto_spot") | ("tokocrypto_tokenized_stock", "tokenized_stock")
        ) && !instrument.underlying_symbol.trim().is_empty()
            && instrument.tokocrypto_market.is_some()
    }

    fn sync(
        &self,
        connection: &Connection,
        request: &CandleSyncRequest<'_>,
    ) -> CandleProviderReport {
        let instrument = request.instrument;
        let tokenized = instrument.asset_category_id == "tokenized_stock";
        let outcomes = match request.window {
            CandleSyncWindow::Intraday => match instrument.tokocrypto_market {
                Some(market) => match tokocrypto_storage::sync_registered_intraday_asset(
                    connection,
                    instrument.storage_symbol,
                    market.venue_symbol,
                    market.quote_currency,
                    market.contract_metadata_json,
                    tokenized,
                ) {
                    Ok(result) => vec![
                        CandleProviderReport::interval("15m", Ok(result.rows_15m)),
                        CandleProviderReport::interval("1h", Ok(result.rows_1h)),
                    ],
                    Err(error) => vec![CandleProviderReport::interval("intraday", Err(error))],
                },
                None => vec![CandleProviderReport::interval(
                    "intraday",
                    Err("verified Tokocrypto market metadata is unavailable".into()),
                )],
            },
            CandleSyncWindow::Daily => vec![CandleProviderReport::interval(
                "1d",
                match instrument.tokocrypto_market {
                    Some(market) => tokocrypto_storage::sync_registered_daily_asset(
                        connection,
                        instrument.storage_symbol,
                        market.venue_symbol,
                        market.quote_currency,
                        market.contract_metadata_json,
                        tokenized,
                    ),
                    None => Err("verified Tokocrypto market metadata is unavailable".into()),
                },
            )],
            CandleSyncWindow::SingleInterval(interval) => vec![CandleProviderReport::interval(
                interval,
                Err("venue-native adapter requires the complete scheduled window".into()),
            )],
        };
        CandleProviderReport { outcomes }
    }
}

pub(super) struct ResearchProxyAdapter;

static RESEARCH_PROXY_CAPABILITIES: CandleProviderCapabilities = CandleProviderCapabilities {
    exchange_ids: &["stockbit", "tokocrypto", "pintu", "pluang"],
    product_ids: &[
        "stockbit_regular",
        "tokocrypto_spot",
        "tokocrypto_tokenized_stock",
        "pintu_retail_spot",
        "pintu_pro_spot",
        "pintu_pro_futures",
        "pluang_spot",
        "pluang_perpetual",
    ],
    asset_categories: &[
        "idx_stock",
        "crypto_spot",
        "crypto_perpetual",
        "crypto_futures",
        "tokenized_stock",
    ],
    intervals: &["15m", "1h", "1d"],
};

impl CandleSyncAdapter for ResearchProxyAdapter {
    fn provider_id(&self) -> &'static str {
        "research_proxy"
    }

    fn source_class(&self) -> CandleSourceClass {
        CandleSourceClass::ResearchProxy
    }

    fn capabilities(&self) -> &'static CandleProviderCapabilities {
        &RESEARCH_PROXY_CAPABILITIES
    }

    fn supports_symbol(&self, instrument: RegisteredCandleInstrument<'_>) -> bool {
        yahoo_fallback_symbol(
            instrument.exchange_id,
            instrument.underlying_symbol,
            instrument.asset_category_id,
        )
        .is_some()
    }

    fn sync(
        &self,
        connection: &Connection,
        request: &CandleSyncRequest<'_>,
    ) -> CandleProviderReport {
        let instrument = request.instrument;
        let Some(yahoo_symbol) = yahoo_fallback_symbol(
            instrument.exchange_id,
            instrument.underlying_symbol,
            instrument.asset_category_id,
        ) else {
            return CandleProviderReport {
                outcomes: vec![CandleProviderReport::interval(
                    "provider_selection",
                    Err("no approved candle provider mapping".into()),
                )],
            };
        };
        let mut outcomes = Vec::with_capacity(request.window.intervals().len());
        for interval in request.window.intervals() {
            let result = match *interval {
                "15m" => research_proxy::sync_15m_for_exchange(
                    connection,
                    instrument.storage_symbol,
                    instrument.asset_category_id,
                    instrument.exchange_id,
                    instrument.product_id,
                    &yahoo_symbol,
                ),
                "1h" => research_proxy::sync_hourly_for_exchange(
                    connection,
                    instrument.storage_symbol,
                    instrument.asset_category_id,
                    instrument.exchange_id,
                    instrument.product_id,
                    &yahoo_symbol,
                ),
                "1d" => research_proxy::sync_daily_for_exchange(
                    connection,
                    instrument.storage_symbol,
                    instrument.asset_category_id,
                    instrument.exchange_id,
                    instrument.product_id,
                    &yahoo_symbol,
                ),
                _ => Err(format!("unsupported research proxy interval {interval}")),
            };
            outcomes.push(CandleProviderReport::interval(interval, result));
        }
        CandleProviderReport { outcomes }
    }
}

pub(super) static TOKOCRYPTO_NATIVE_ADAPTER: TokocryptoNativeAdapter = TokocryptoNativeAdapter;
pub(super) static PINTU_NATIVE_ADAPTER: PintuNativeAdapter = PintuNativeAdapter;
pub(super) static RESEARCH_PROXY_ADAPTER: ResearchProxyAdapter = ResearchProxyAdapter;
