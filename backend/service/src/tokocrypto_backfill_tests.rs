use super::*;

#[test]
fn native_backfill_status_is_bounded_and_does_not_mark_fresh_prices_failed() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE ingestion_runs(id INTEGER PRIMARY KEY,symbol TEXT,interval TEXT,source TEXT,exchange_id TEXT,data_provider TEXT,attempted_at TEXT,status TEXT,rows_received INTEGER,error TEXT);").unwrap();
    for _ in 0..10 { record_backfill_state(&connection,"TEST","paging",1000).unwrap(); }
    record_backfill_state(&connection,"TEST","complete",0).unwrap();
    let state:(i64,String,i64) = connection.query_row("SELECT COUNT(*),status,rows_received FROM ingestion_runs",[],|row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(state,(1,"complete".into(),0));
    assert_eq!(connection.query_row("SELECT COUNT(*) FROM ingestion_runs WHERE interval='1h'",[],|row|row.get::<_,i64>(0)).unwrap(),0);
}
