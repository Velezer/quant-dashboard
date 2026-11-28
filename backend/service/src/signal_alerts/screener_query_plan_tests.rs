use super::{
    initialize_screener_schema, signal_screener_display_order_sql,
    signal_screener_exchange_display_order_sql,
    signal_screener_exchange_timeframe_display_order_sql,
};
use rusqlite::Connection;

#[test]
fn signal_screener_page_order_uses_display_index_without_temporary_sort() {
    let connection = Connection::open_in_memory().expect("in-memory database should open");
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS signals;")
        .expect("signal database should attach");
    initialize_screener_schema(&connection).expect("screener schema should initialize");

    let query = format!(
        "EXPLAIN QUERY PLAN
         SELECT timeframe, score, last_updated_at, exchange_id, product_id, symbol,
                signal_strategy_id
           FROM signals.signal_screener_rows
          ORDER BY {}
          LIMIT 50 OFFSET 0",
        signal_screener_display_order_sql()
    );
    let plan = connection
        .prepare(&query)
        .expect("query plan should prepare")
        .query_map([], |row| row.get::<_, String>(3))
        .expect("query plan should run")
        .collect::<Result<Vec<_>, _>>()
        .expect("query plan rows should load");

    assert!(
        plan.iter()
            .any(|detail| detail.contains("idx_signal_screener_display_order")),
        "expected display-order index in query plan, got {plan:?}"
    );
    assert!(
        plan.iter()
            .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")),
        "expected the index to avoid a temporary sort, got {plan:?}"
    );
}

#[test]
fn exchange_scoped_screener_uses_exchange_order_index() {
    let connection = Connection::open_in_memory().expect("in-memory database should open");
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS signals;")
        .expect("signal database should attach");
    initialize_screener_schema(&connection).expect("screener schema should initialize");

    let query = format!(
        "EXPLAIN QUERY PLAN
         SELECT timeframe, score, last_updated_at, exchange_id, product_id, symbol,
                signal_strategy_id
           FROM signals.signal_screener_rows
                INDEXED BY idx_signal_screener_exchange_display_order
          WHERE exchange_id='stockbit'
          ORDER BY {}",
        signal_screener_exchange_display_order_sql()
    );
    let plan = connection
        .prepare(&query)
        .expect("scoped query plan should prepare")
        .query_map([], |row| row.get::<_, String>(3))
        .expect("scoped query plan should run")
        .collect::<Result<Vec<_>, _>>()
        .expect("scoped query plan rows should load");

    assert!(
        plan.iter()
            .any(|detail| detail.contains("idx_signal_screener_exchange_display_order")),
        "expected exchange-prefixed display index, got {plan:?}"
    );
    assert!(
        plan.iter()
            .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")),
        "expected the exchange index to preserve display order, got {plan:?}"
    );
}

#[test]
fn exchange_and_timeframe_scoped_screener_uses_compound_order_index() {
    let connection = Connection::open_in_memory().expect("in-memory database should open");
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS signals;")
        .expect("signal database should attach");
    initialize_screener_schema(&connection).expect("screener schema should initialize");

    let query = format!(
        "EXPLAIN QUERY PLAN
         SELECT timeframe, score, last_updated_at, exchange_id, product_id, symbol,
                signal_strategy_id
           FROM signals.signal_screener_rows
                INDEXED BY idx_signal_screener_exchange_timeframe_display_order
          WHERE exchange_id='stockbit' AND timeframe='4h'
          ORDER BY {}",
        signal_screener_exchange_timeframe_display_order_sql()
    );
    let plan = connection
        .prepare(&query)
        .expect("scoped query plan should prepare")
        .query_map([], |row| row.get::<_, String>(3))
        .expect("scoped query plan should run")
        .collect::<Result<Vec<_>, _>>()
        .expect("scoped query plan rows should load");

    assert!(
        plan.iter().any(|detail| {
            detail.contains("idx_signal_screener_exchange_timeframe_display_order")
        }),
        "expected exchange/timeframe-prefixed display index, got {plan:?}"
    );
    assert!(
        plan.iter()
            .all(|detail| !detail.contains("USE TEMP B-TREE FOR ORDER BY")),
        "expected the compound index to preserve display order, got {plan:?}"
    );
}
