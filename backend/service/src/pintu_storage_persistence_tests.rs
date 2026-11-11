use super::{persist_sync, PintuCandle, SOURCE};
use crate::main_test_support::{remove_temporary_database, temporary_database_path};
use rusqlite::Connection;

const SYMBOL: &str = "BTC-USD";
const PRODUCT: &str = "pintu_pro_futures";
const CATEGORY: &str = "crypto_perpetual";

struct Stores {
    market: String,
    app: String,
}

impl Stores {
    fn new(label: &str) -> Self {
        Self {
            market: temporary_database_path(&format!("pintu-{label}-market")),
            app: temporary_database_path(&format!("pintu-{label}-app")),
        }
    }

    fn open(&self, journal: &str) -> (Connection, Connection) {
        let app = Connection::open(&self.app).unwrap();
        app.execute_batch(&format!("PRAGMA journal_mode={journal};"))
            .unwrap();
        app.execute_batch(
            "CREATE TABLE instruments (
                instrument_id TEXT, exchange_id TEXT, product_id TEXT,
                asset_category_id TEXT, storage_symbol TEXT, listing_status TEXT,
                venue_verified INTEGER, is_active INTEGER
             );
             INSERT INTO instruments VALUES(
                 'pintu-btc', 'pintu', 'pintu_pro_futures', 'crypto_perpetual',
                 'BTC-USD', 'venue_active', 1, 1
             );",
        )
        .unwrap();
        let market = Connection::open(&self.market).unwrap();
        market
            .execute_batch(&format!("PRAGMA journal_mode={journal};"))
            .unwrap();
        crate::database::initialize_empty_market_schema(&market).unwrap();
        market
            .execute("ATTACH DATABASE ?1 AS app", [&self.app])
            .unwrap();
        for connection in [&market, &app] {
            connection
                .busy_timeout(std::time::Duration::from_millis(50))
                .unwrap();
        }
        (market, app)
    }
}

impl Drop for Stores {
    fn drop(&mut self) {
        for path in [&self.market, &self.app] {
            remove_temporary_database(path);
            let _ = std::fs::remove_file(format!("{path}-journal"));
        }
    }
}

fn candle(open_time_ms: i64) -> PintuCandle {
    PintuCandle {
        open_time_ms,
        open: 100.0,
        high: 102.0,
        low: 99.0,
        close: 101.0,
        volume: 10.0,
    }
}

#[test]
fn native_candle_pages_do_not_reserve_the_attached_app_writer() {
    for journal in ["WAL", "DELETE"] {
        let files = Stores::new(&format!("concurrent-{journal}"));
        let (market, app_writer) = files.open(journal);
        app_writer
            .execute_batch("BEGIN IMMEDIATE; UPDATE instruments SET is_active=1;")
            .unwrap();
        let intraday = [candle(1_700_000_000_000), candle(1_700_000_900_000)];
        let hourly = [candle(1_700_000_000_000)];

        persist_sync(
            &market,
            SYMBOL,
            PRODUCT,
            CATEGORY,
            &[("15m", &intraday), ("1h", &hourly)],
        )
        .expect("ticker writes must succeed while the shared app writer is active");

        assert!(!app_writer.is_autocommit());
        let bars: i64 = market
            .query_row(
                "SELECT COUNT(*) FROM main.bars WHERE source=?1",
                [SOURCE],
                |row| row.get(0),
            )
            .unwrap();
        let series: i64 = market
            .query_row(
                "SELECT COUNT(*) FROM main.series
                 WHERE instrument_id='pintu-btc' AND product_id=?1",
                [PRODUCT],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(bars, 3);
        assert_eq!(series, 2);
        app_writer.execute_batch("ROLLBACK;").unwrap();
    }
}

#[test]
fn invalid_candle_rolls_back_the_complete_native_page() {
    let files = Stores::new("atomic-page");
    let (market, _app) = files.open("WAL");
    let candles = [candle(1_700_000_000_000), candle(i64::MAX)];
    let error = persist_sync(
        &market,
        SYMBOL,
        PRODUCT,
        CATEGORY,
        &[("15m", &candles)],
    )
    .unwrap_err();
    assert!(error.contains("timestamp overflow"));
    assert!(market.is_autocommit());
    for table in ["series", "bars", "ingestion_runs"] {
        let count: i64 = market
            .query_row(&format!("SELECT COUNT(*) FROM main.{table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} must not retain a partial candle page");
    }
}

#[test]
fn unverified_listing_cannot_persist_native_candles() {
    let files = Stores::new("unverified");
    let (market, app) = files.open("DELETE");
    app.execute("UPDATE instruments SET venue_verified=0", [])
        .unwrap();
    let candles = [candle(1_700_000_000_000)];
    let error = persist_sync(
        &market,
        SYMBOL,
        PRODUCT,
        CATEGORY,
        &[("15m", &candles)],
    )
    .unwrap_err();
    assert!(error.contains("one active verified listing"));
    assert!(market.is_autocommit());
    let count: i64 = market
        .query_row("SELECT COUNT(*) FROM main.bars", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
