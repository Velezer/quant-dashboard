use super::{begin_signal_store_write, publish_signal_screener_app_batch_attempt};
use crate::main_test_support::{remove_temporary_database, temporary_database_path};
use rusqlite::Connection;

struct Stores {
    app: String,
    signals: String,
}

impl Stores {
    fn new(label: &str) -> Self {
        let stores = Self {
            app: temporary_database_path(&format!("{label}-app")),
            signals: temporary_database_path(&format!("{label}-signals")),
        };
        let setup = stores.open();
        setup
            .execute_batch(
                "PRAGMA app.journal_mode=WAL;
                 PRAGMA signals.journal_mode=WAL;
                 CREATE TABLE app.provider_marker(value INTEGER);
                 INSERT INTO app.provider_marker VALUES(1);",
            )
            .unwrap();
        crate::signal_alerts::initialize_screener_schema(&setup).unwrap();
        setup
            .execute_batch(
                "INSERT INTO signals.signal_screener_status(
                     id,last_attempt_at,last_success_at,last_attempt_status,signal_count
                 ) VALUES(1,0,0,'ready',0);",
            )
            .unwrap();
        stores
    }

    fn open(&self) -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute("ATTACH ?1 AS app", [&self.app]).unwrap();
        connection
            .execute("ATTACH ?1 AS signals", [&self.signals])
            .unwrap();
        connection
            .busy_timeout(std::time::Duration::from_millis(20))
            .unwrap();
        connection
    }
}

impl Drop for Stores {
    fn drop(&mut self) {
        remove_temporary_database(&self.app);
        remove_temporary_database(&self.signals);
    }
}

fn publish_empty_app(connection: &Connection) -> Result<(), rusqlite::Error> {
    publish_signal_screener_app_batch_attempt(
        connection,
        "pintu",
        &[],
        true,
        &[],
        &[],
        &[],
        &[],
    )
}

#[test]
fn snapshot_publication_does_not_reserve_the_attached_app_writer() {
    let stores = Stores::new("snapshot-independent-app-writer");
    let provider = stores.open();
    provider
        .execute_batch("BEGIN DEFERRED; UPDATE app.provider_marker SET value=2;")
        .unwrap();
    let publisher = stores.open();

    publish_empty_app(&publisher).unwrap();

    let status: String = publisher
        .query_row(
            "SELECT last_attempt_status FROM signals.signal_screener_app_status
             WHERE exchange_id='pintu'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "scanning");
    assert_eq!(
        publisher
            .query_row("SELECT value FROM app.provider_marker", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        1
    );
    assert!(!provider.is_autocommit());
    provider.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn snapshot_reservation_serializes_signal_writers_and_rolls_back_on_failure() {
    let stores = Stores::new("snapshot-signal-writer-reservation");
    let existing_writer = stores.open();
    existing_writer
        .execute_batch(
            "BEGIN DEFERRED;
             UPDATE signals.signal_screener_status SET signal_count=7 WHERE id=1;",
        )
        .unwrap();
    let publisher = stores.open();

    let error = begin_signal_store_write(&publisher).unwrap_err();
    assert!(matches!(error, rusqlite::Error::SqliteFailure(code, _)
        if code.code == rusqlite::ErrorCode::DatabaseBusy));
    assert!(publisher.is_autocommit());
    // A failed signals reservation must not retain any app writer lock.
    publisher
        .execute("UPDATE app.provider_marker SET value=3", [])
        .unwrap();

    existing_writer.execute_batch("COMMIT").unwrap();
    publish_empty_app(&publisher).unwrap();
    assert!(publisher.is_autocommit());
    assert_eq!(
        publisher
            .query_row(
                "SELECT signal_count FROM signals.signal_screener_status WHERE id=1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}
