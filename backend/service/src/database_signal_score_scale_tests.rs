use super::signal_score_scale::migrate_signal_score_scale_in_connection;
use crate::signal_alerts::{
    create_signal_screener_rows_table, SIGNAL_LAYOUT_VERSION, SIGNAL_SCORE_SCHEMA_VERSION,
};
use rusqlite::Connection;
use serde_json::{json, Value};

fn legacy_signal_database() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory connection should open");
    connection
        .execute_batch(
            "ATTACH DATABASE ':memory:' AS signals;
             CREATE TABLE signals.signal_store_meta (key TEXT PRIMARY KEY,value TEXT NOT NULL);",
        )
        .expect("legacy signal layout should initialize");
    connection
        .execute(
            "INSERT INTO signals.signal_store_meta(key,value)
             VALUES('signal_layout_version',?1)",
            [SIGNAL_LAYOUT_VERSION],
        )
        .expect("legacy layout marker should initialize");
    create_signal_screener_rows_table(
        &connection,
        "signal_screener_rows",
        false,
        quant_core::SIGNAL_STRENGTH_SCORE_SCALE_MAX,
    )
    .expect("legacy ten-point table should initialize");
    connection
        .execute_batch(
            "CREATE INDEX signals.idx_signal_screener_score
                 ON signal_screener_rows(score DESC,last_updated_at DESC);",
        )
        .expect("legacy index should initialize");
    let score_basis = json!({
        "model_id": "nwe_endpoint_band_proximity_dss_rank_v3",
        "status": "qualifying",
    });
    connection
        .execute(
            "INSERT INTO signals.signal_screener_rows (
                exchange_id,product_id,asset_category_id,symbol,storage_symbol,
                signal_strategy_id,signal_strategy_name,timeframe,direction,score,
                strategy_strength,confluence_score,score_basis_json,current_price,
                price_timeframe,price_bar_time,signal_bar_time,data_source,data_provider,
                data_quality,evidence_status,evidence_source,last_updated_at
             ) VALUES (
                'tokocrypto','tokocrypto_spot','crypto_spot','BTC-USD','BTCUSD',
                'crypto_nwe_band_proximity_4h_v1','Crypto 4h NWE','4h','LONG',
                7.4,8.0,6.0,?1,60000.0,'4h',100,100,'test_source','test_provider',
                'RESEARCH_PROXY','VALIDATION_PENDING','test_source',200
             )",
            [score_basis.to_string()],
        )
        .expect("legacy row should persist");
    connection
}

#[test]
fn score_scale_migration_preserves_rows_and_marks_old_scores_stale_on_ten_points() {
    let connection = legacy_signal_database();

    let result = migrate_signal_score_scale_in_connection(&connection)
        .expect("legacy score rows should migrate transactionally");

    assert!(result.contains("complete (1 rows across 1 symbols"));
    let (score, strategy_strength, confluence_score, basis_json): (f64, f64, f64, String) =
        connection
            .query_row(
                "SELECT score,strategy_strength,confluence_score,score_basis_json
                   FROM signals.signal_screener_rows",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("migrated row should remain readable");
    let basis: Value = serde_json::from_str(&basis_json).expect("score basis should stay JSON");
    assert_eq!(score, 7.4);
    assert_eq!(strategy_strength, 8.0);
    assert_eq!(confluence_score, 6.0);
    assert_eq!(basis["model_id"], "nwe_endpoint_band_proximity_dss_rank_v3");
    assert_eq!(basis["score_scale_max"], 10.0);
    assert_eq!(basis["minimum_score_out_of_10"], 5.0);
    assert_eq!(basis["component_score_scale_max"], 10.0);
    assert_eq!(basis["score_model_status"], "legacy");
    assert_eq!(basis["strategy_weight"], 0.7);
    assert_eq!(basis["confluence_weight"], 0.3);
    assert_eq!(basis["status"], "stale");

    let (schema_version, source_hash, counts_json): (String, String, String) = connection
        .query_row(
            "SELECT
                (SELECT value FROM signals.signal_store_meta WHERE key='signal_score_schema_version'),
                (SELECT value FROM signals.signal_store_meta WHERE key='signal_score_scale_v3_source_hash'),
                (SELECT value FROM signals.signal_store_meta WHERE key='signal_score_scale_v3_migrated_row_counts_json')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("migration provenance should be stored");
    let counts: Value = serde_json::from_str(&counts_json).expect("row counts should be JSON");
    assert_eq!(schema_version, SIGNAL_SCORE_SCHEMA_VERSION);
    assert!(source_hash.starts_with("fnv1a64:"));
    assert_eq!(counts["signal_screener_rows"], 1);
    assert_eq!(counts["symbols"], 1);

    let result = migrate_signal_score_scale_in_connection(&connection)
        .expect("repeat migration should be idempotent");
    assert_eq!(result, "already complete (score schema v3)");
    let row_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM signals.signal_screener_rows",
            [],
            |row| row.get(0),
        )
        .expect("the single current row should remain");
    assert_eq!(row_count, 1);
}

#[test]
fn score_scale_v3_migration_preserves_twenty_point_rows_as_legacy() {
    let connection = legacy_signal_database();
    connection
        .execute_batch(
            "DROP INDEX signals.idx_signal_screener_score;
             DROP TABLE signals.signal_screener_rows;",
        )
        .expect("the prior score table should be replaceable in the fixture");
    create_signal_screener_rows_table(
        &connection,
        "signal_screener_rows",
        false,
        quant_core::NWE_DSS_SIGNAL_SCORE_SCALE_MAX,
    )
    .expect("the prior twenty-point table should initialize");
    connection
        .execute_batch(
            "CREATE INDEX signals.idx_signal_screener_score
                 ON signal_screener_rows(score DESC,last_updated_at DESC);
             INSERT INTO signals.signal_store_meta(key,value)
                 VALUES('signal_score_schema_version','2');",
        )
        .expect("v2 score layout should be marked");
    let score_basis = json!({
        "model_id": "nwe_dss_relative_volume_estimated_delta_rank_v5",
        "score_scale_max": 20,
        "minimum_score_out_of_20": 1,
        "component_score_scale_max": 10,
        "status": "qualifying",
    });
    connection
        .execute(
            "INSERT INTO signals.signal_screener_rows (
                exchange_id,product_id,asset_category_id,symbol,storage_symbol,
                signal_strategy_id,signal_strategy_name,timeframe,direction,score,
                strategy_strength,confluence_score,score_basis_json,current_price,
                price_timeframe,price_bar_time,signal_bar_time,data_source,data_provider,
                data_quality,evidence_status,evidence_source,last_updated_at
             ) VALUES (
                'stockbit','stockbit_regular','idx_stock','AADI.JK','AADI.JK',
                'stockbit_nwe_band_proximity_4h_v3','Stockbit 4h NWE','4h','LONG',
                12.5,7.0,5.5,?1,6000.0,'4h',100,100,'test_source','test_provider',
                'RESEARCH_PROXY','VALIDATION_PENDING','test_source',200
             )",
            [score_basis.to_string()],
        )
        .expect("v2 score row should persist");

    migrate_signal_score_scale_in_connection(&connection)
        .expect("v2 rows should migrate and be marked stale");

    let (score, basis_json): (f64, String) = connection
        .query_row(
            "SELECT score,score_basis_json FROM signals.signal_screener_rows",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("v2 row should remain readable");
    let basis: Value = serde_json::from_str(&basis_json).unwrap();
    assert_eq!(score, 12.5);
    assert_eq!(basis["score_scale_max"], 20);
    assert_eq!(
        basis["model_id"],
        "nwe_dss_relative_volume_estimated_delta_rank_v5"
    );
    assert_eq!(basis["score_model_status"], "legacy");
    assert_eq!(basis["status"], "stale");
}
