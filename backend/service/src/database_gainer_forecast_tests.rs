use super::*;

#[test]
fn tokocrypto_singleton_schedule_migrates_into_scoped_history() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(
        "ATTACH DATABASE ':memory:' AS app;
         CREATE TABLE app.tokocrypto_gainer_forecast_schedule (
             id INTEGER PRIMARY KEY CHECK(id=1),protocol_id TEXT NOT NULL,
             forecast_cutoff INTEGER NOT NULL,status TEXT NOT NULL,attempt_count INTEGER NOT NULL,
             last_attempt_at INTEGER NOT NULL,next_attempt_at INTEGER NOT NULL,last_error TEXT NOT NULL,
             result_json TEXT NOT NULL,updated_at INTEGER NOT NULL
         );
         INSERT INTO app.tokocrypto_gainer_forecast_schedule
         VALUES(1,'tokocrypto_native_momentum_next_24h_return_forecast_v1',1800000000,
             'complete',1,1800000100,1800014700,'','{\"status\":\"READY\"}',1800000200);",
    ).unwrap();

    initialize_app_runtime_storage_schema(&connection).unwrap();

    let schedule: (String, String, i64, String, i64, String) = connection.query_row(
        "SELECT scope_id,protocol_id,forecast_cutoff,status,next_attempt_at,result_json
         FROM app.gainer_forecast_schedule",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).unwrap();
    assert_eq!(schedule.0, "tokocrypto_spot");
    assert_eq!(schedule.1, "tokocrypto_native_momentum_next_24h_return_forecast_v1");
    assert_eq!(schedule.2, 1_800_000_000);
    assert_eq!(schedule.3, "scheduled");
    assert_eq!(schedule.4, 0);
    assert_eq!(schedule.5, "{}");
    let universe_snapshot: String = connection.query_row(
        "SELECT universe_json FROM app.gainer_forecast_schedule WHERE scope_id='tokocrypto_spot'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(universe_snapshot, "null");
    let preserved_runs: i64 = connection.query_row(
        "SELECT COUNT(*) FROM app.gainer_forecast_runs
         WHERE scope_id='tokocrypto_spot' AND result_json=?1",
        [r#"{"status":"READY"}"#],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(preserved_runs, 1);
    let legacy_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM app.sqlite_schema
                       WHERE type='table' AND name='tokocrypto_gainer_forecast_schedule')",
        [],
        |row| row.get(0),
    ).unwrap();
    assert!(!legacy_exists);
}
