use super::{list_signal_screener_page, screener_tests::screener_connection};
use rusqlite::{params, Connection};
use serde_json::json;

fn insert_model_row(
    connection: &Connection,
    instrument: [&str; 4],
    timeframe: &str,
    model: quant_core::EntryModelKind,
    score: f64,
    reference_only: bool,
) {
    let [exchange, product, category, symbol] = instrument;
    let strategy = match (exchange, model) {
        ("stockbit", quant_core::EntryModelKind::Nwe) => {
            quant_core::stockbit_nwe_signal_strategy_id(timeframe)
        }
        ("stockbit", quant_core::EntryModelKind::Dss) => {
            quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe)
        }
        (_, quant_core::EntryModelKind::Nwe) => {
            quant_core::crypto_nwe_signal_strategy_id(timeframe)
        }
        (_, quant_core::EntryModelKind::Dss) => {
            quant_core::crypto_dss_bressert_signal_strategy_id(timeframe)
        }
    }
    .unwrap();
    let now = crate::now_secs() as i64;
    let price_bar_time = (now - 16 * 60) * crate::NANOS_PER_SECOND;
    let basis = json!({
        "model_id": model.score_model_id(),
        "status": "qualifying",
        "score_scale_max": quant_core::SIX_INDICATOR_SCORE_SCALE_MAX,
        "price_reference_only": reference_only,
    });
    connection.execute(
        "INSERT INTO signals.signal_screener_rows(
            exchange_id,product_id,asset_category_id,symbol,storage_symbol,
            signal_strategy_id,signal_strategy_name,timeframe,direction,score,
            score_basis_json,current_price,price_timeframe,price_bar_time,signal_bar_time,last_updated_at
         ) VALUES(?1,?2,?3,?4,?4,?5,?5,?6,'LONG',?7,?8,100.0,'15m',?9,?9,?10)",
        params![exchange, product, category, symbol, strategy, timeframe, score, basis.to_string(), price_bar_time, now],
    ).unwrap();
}

#[test]
fn stockbit_pairs_use_one_result_per_timeframe_before_pagination() {
    use quant_core::EntryModelKind::{Dss, Nwe};
    let connection = screener_connection();
    crate::database::initialize_app_runtime_storage_schema(&connection).unwrap();
    let agrs = ["stockbit", "stockbit_regular", "idx_stock", "AGRS"];
    insert_model_row(&connection, agrs, "15m", Dss, 29.0, true);
    insert_model_row(&connection, agrs, "15m", Nwe, 29.0, true);
    insert_model_row(&connection, agrs, "1d", Dss, 9.0, true);

    let page = |search, offset| {
        list_signal_screener_page(
            &connection,
            900,
            Some("stockbit"),
            None,
            None,
            Some(search),
            offset,
            1,
        )
        .unwrap()
    };
    let first = page("AGRS", 0);
    let second = page("AGRS", 1);
    assert_eq!(first["summary"]["matching_rows"], 2);
    assert_eq!(first["summary"]["has_more"], true);
    assert_eq!(first["signals"][0]["timeframe"], "15m");
    assert_eq!(
        first["signals"][0]["market_entry_status"],
        "NO_MARKET_ENTRY"
    );
    assert_eq!(
        first["signals"][0]["timeframe_strategy_ids"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(second["signals"][0]["timeframe"], "1d");
    assert_eq!(second["summary"]["has_more"], false);
    assert_eq!(
        page("stockbit_nwe", 0)["signals"][0]["signal_strategy_id"],
        quant_core::stockbit_nwe_signal_strategy_id("15m").unwrap()
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM signals.signal_screener_rows",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
        3
    );
}

#[test]
fn timeframe_result_prefers_a_current_entry_and_counts_its_score_once() {
    use quant_core::EntryModelKind::{Dss, Nwe};
    let connection = screener_connection();
    crate::database::initialize_app_runtime_storage_schema(&connection).unwrap();
    let spot = ["pluang", "pluang_spot", "crypto_spot", "AGRS-USD"];
    let perpetual = ["pluang", "pluang_perpetual", "crypto_perpetual", "AGRS-USD"];
    insert_model_row(&connection, spot, "15m", Dss, 29.0, true);
    insert_model_row(&connection, spot, "15m", Nwe, 15.0, false);
    insert_model_row(&connection, perpetual, "15m", Dss, 19.0, false);
    insert_model_row(&connection, perpetual, "15m", Nwe, 19.0, false);

    let page = list_signal_screener_page(
        &connection,
        900,
        Some("pluang"),
        Some("15m"),
        None,
        None,
        0,
        10,
    )
    .unwrap();
    assert_eq!(page["summary"]["matching_rows"], 2);
    assert_eq!(page["summary"]["stale_price_signal_count"], 0);
    let signals = page["signals"].as_array().unwrap();
    assert_eq!(signals[0]["product_id"], "pluang_perpetual");
    assert_eq!(signals[1]["product_id"], "pluang_spot");
    assert_eq!(
        signals[1]["signal_strategy_id"],
        quant_core::crypto_nwe_signal_strategy_id("15m").unwrap()
    );
    for signal in signals {
        assert_eq!(signal["direction_score"]["long_count"], 1);
        assert_eq!(signal["direction_score"]["short_count"], 0);
        assert_eq!(signal["direction_score"]["net_score"], signal["score"]);
    }
}
