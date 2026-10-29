use rusqlite::Connection;

#[test]
fn screener_schema_creates_storage_symbol_in_dedicated_database() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app; ATTACH DATABASE ':memory:' AS signals;")
        .unwrap();

    super::super::initialize_screener_schema(&connection)
        .expect("signal snapshot schema should initialize");

    let has_storage_symbol = {
        let mut statement = connection
            .prepare("PRAGMA signals.table_info('signal_screener_rows')")
            .unwrap();
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap();
        columns
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .iter()
            .any(|name| name == "storage_symbol")
    };
    assert!(has_storage_symbol);
}

#[test]
fn alert_history_expires_two_hours_after_creation() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "ATTACH DATABASE ':memory:' AS app;
             CREATE TABLE app.signal_alerts(
                 alert_id INTEGER PRIMARY KEY,
                 created_at INTEGER NOT NULL
             );",
        )
        .unwrap();
    let now = crate::now_secs() as i64;
    let expiry = now.saturating_sub(super::super::SIGNAL_ALERT_HISTORY_RETENTION_SECONDS);
    connection
        .execute(
            "INSERT INTO app.signal_alerts(alert_id,created_at) VALUES(1,?1),(2,?2)",
            rusqlite::params![expiry + 1, expiry],
        )
        .unwrap();

    super::refresh_lifecycles(&connection).unwrap();

    let remaining_alerts = connection
        .query_row("SELECT COUNT(*) FROM app.signal_alerts", [], |row| row.get::<_, i64>(0))
        .unwrap();
    assert_eq!(remaining_alerts, 1);
}
