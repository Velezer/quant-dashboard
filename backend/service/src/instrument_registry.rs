use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

mod api;
mod catalog;
mod schema;
mod sync;

#[cfg(test)]
pub(super) use api::MAX_PAGE_SIZE_ROWS;
pub(super) use api::{list_page, register_manual, DEFAULT_PAGE_SIZE_ROWS};
#[cfg(test)]
pub(super) use catalog::lookup_for_exchange;
pub(super) use catalog::{
    lookup_tokocrypto_market, research_eligibility, storage_symbol_for_exchange_category,
};
#[cfg(test)]
pub(super) use schema::REGISTRY_SCHEMA;
pub(super) use schema::{initialize, initialize_schema, is_excluded, migrate_app_products};
#[cfg(test)]
pub(super) use sync::listing_status;
pub(super) use sync::{
    listing_status_for_exchange, sync_from_live_sources, sync_pintu, sync_stockbit, sync_tokocrypto,
};
#[cfg(test)]
pub(super) use sync::{reconcile_pintu, reconcile_stockbit, reconcile_tokocrypto};

/// Result of a daily registry sync for a single exchange.
#[derive(Debug)]
pub(super) struct SyncResult {
    pub exchange_id: String,
    pub inserted: usize,
    pub deactivated: usize,
    pub error: Option<String>,
}

#[cfg(test)]
#[path = "instrument_registry_tests.rs"]
mod app_product_tests;

#[cfg(test)]
#[path = "instrument_registry_pagination_tests.rs"]
mod pagination_tests;

#[cfg(test)]
#[path = "instrument_registry_sync_tests.rs"]
mod sync_tests;

#[cfg(test)]
#[path = "listing_status_tests.rs"]
mod listing_status_tests;

#[cfg(test)]
#[path = "instrument_registry_storage_symbol_tests.rs"]
mod storage_symbol_tests;
