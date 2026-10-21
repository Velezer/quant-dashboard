use super::*;

#[path = "../database/schema.rs"]
mod database_schema;

fn connection() -> Connection {
    let connection = Connection::open_in_memory().expect("memory database opens");
    connection.execute_batch(
        "ATTACH DATABASE ':memory:' AS app;
         PRAGMA foreign_keys=ON;
         CREATE TABLE app.instruments (
             instrument_id TEXT PRIMARY KEY,exchange_id TEXT NOT NULL,product_id TEXT NOT NULL,
             asset_category_id TEXT NOT NULL,symbol TEXT NOT NULL,storage_symbol TEXT NOT NULL,
             venue_verified INTEGER NOT NULL,listing_status TEXT NOT NULL,venue_symbol TEXT NOT NULL,
             quote_currency TEXT NOT NULL,underlying_symbol TEXT NOT NULL,price_source_policy TEXT NOT NULL,
             execution_supported INTEGER NOT NULL,is_active INTEGER NOT NULL
         );
         CREATE TABLE app.instrument_exclusions (
             exchange_id TEXT,asset_category_id TEXT,symbol TEXT,is_active INTEGER
         );
         CREATE TABLE app.exchange_products (
             exchange_id TEXT,product_id TEXT,display_name TEXT,status TEXT
         );
         INSERT INTO app.exchange_products VALUES
             ('pintu','pintu_pro_spot','Pro Spot','enabled'),
             ('pintu','pintu_pro_futures','Pro Futures','enabled'),
             ('stockbit','stockbit_regular','IDX Stocks','enabled');"
    ).expect("minimal registry fixture initializes");
    connection.execute_batch(database_schema::MARKET_SCHEMA).unwrap();
    connection.execute_batch(database_schema::APP_RUNTIME_SCHEMA).unwrap();
    connection
}

fn insert(conn: &Connection, id: &str, app: &str, product: &str, symbol: &str, verified: bool) {
    let category = if app == "stockbit" { "idx_stock" } else { "crypto_spot" };
    conn.execute(
        "INSERT INTO app.instruments VALUES(?1,?2,?3,?4,?5,?6,?7,'venue_active',?5,'USD',?5,'research_proxy',0,1)",
        params![id,app,product,category,symbol,format!("{symbol}-USD"),verified],
    ).unwrap();
}

fn report(conn: &Connection, id: &str, timeframe: &str) -> Value {
    json!({
        "schema_version":1,"protocol_id":super::super::PROTOCOL_ID,
        "instrument":instrument(conn,id).unwrap().unwrap(),"timeframe":timeframe,"computed_at":1234,
        "status":"BACKTEST_COMPLETE","source":{"id":"pintu_native","is_proxy":false},
        "history":{"bars":1500,"data_fingerprint":"fixture-fingerprint"},
        "models":[{
            "kind":"nwe","label":"NWE","score_model_id":"nwe-model-version",
            "entry_validation_status":"VALIDATION_PENDING","sizing_eligible":false,
            "backtest":{"selected_exit_bars":8,"holdout":{"trades":20,"win_rate_pct":75,"mean_return_pct":1.5}},
            "blocking_reasons":["Execution costs are incomplete"]
        },{
            "kind":"dss","label":"DSS Bressert","score_model_id":"dss-model-version",
            "entry_validation_status":"VALIDATION_PENDING","sizing_eligible":false,
            "backtest":{"selected_exit_bars":4,"holdout":{"trades":15,"win_rate_pct":60,"mean_return_pct":0.5}},
            "blocking_reasons":["Execution costs are incomplete"]
        }],
        "sizing":{"status":"BLOCKED"},"sizing_eligible":true
    })
}

#[test]
fn pages_visit_all_registered_tickers_beyond_one_hundred_without_merging_products() {
    let conn = connection();
    for index in 0..205 {
        insert(&conn,&format!("pintu.spot.{index:03}"),"pintu","pintu_pro_spot",&format!("T{index:03}"),true);
    }
    insert(&conn,"pintu.futures.same","pintu","pintu_pro_futures","T000",true);
    insert(&conn,"stockbit.same","stockbit","stockbit_regular","T000",true);
    let first = list(&conn,"pintu",None,"15m",None,0,usize::MAX).unwrap();
    assert_eq!(first["total"],206);
    assert_eq!(first["limit"],100);
    assert_eq!(first["rows"].as_array().unwrap().len(),100);
    assert_eq!(first["summary"]["registered"],206);
    assert_eq!(first["has_more"],true);
    let second = list(&conn,"pintu",None,"15m",None,100,100).unwrap();
    let third = list(&conn,"pintu",None,"15m",None,200,100).unwrap();
    let identities = [first,second,third].iter().flat_map(|page| page["rows"].as_array().unwrap().iter())
        .map(|row|row["instrument_id"].as_str().unwrap().to_string()).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(identities.len(),206);
    let spot = list(&conn,"pintu",Some("pintu_pro_spot"),"15m",Some("T000"),0,100).unwrap();
    assert_eq!(spot["total"],1);
    assert_eq!(spot["rows"][0]["instrument_id"],"pintu.spot.000");
    assert_eq!(spot["products"].as_array().unwrap().len(),2);
}

#[test]
fn search_treats_quotes_wildcards_and_backslashes_as_literal_ticker_text() {
    let conn = connection();
    for (id,symbol) in [("quote","Q'_%\\"),("ordinary","QAAA")] {
        insert(&conn,id,"pintu","pintu_pro_spot",symbol,true);
    }
    for search in ["Q'_%\\","'","_","%","\\"] {
        let page = list(&conn,"pintu",None,"1h",Some(search),0,100).unwrap();
        assert_eq!(page["total"],1,"search {search}");
        assert_eq!(page["rows"][0]["instrument_id"],"quote");
    }
    let all = list(&conn,"pintu",None,"1h",Some("' OR 1=1 --"),0,100).unwrap();
    assert_eq!(all["total"],0);
}

#[test]
fn snapshots_upsert_only_the_exact_instrument_and_timeframe_and_preserve_aggregate_evidence() {
    let conn = connection();
    insert(&conn,"pintu.spot.btc","pintu","pintu_pro_spot","BTC",true);
    insert(&conn,"pintu.futures.btc","pintu","pintu_pro_futures","BTC",true);
    assert!(get_report(&conn,"pintu.spot.btc","15m").unwrap().is_none());
    let mut saved = report(&conn,"pintu.spot.btc","15m");
    save_report(&conn,&saved).unwrap();
    assert_eq!(get_report(&conn,"pintu.spot.btc","15m").unwrap(),Some(saved.clone()));
    assert!(get_report(&conn,"pintu.futures.btc","15m").unwrap().is_none());
    assert!(get_report(&conn,"pintu.spot.btc","1h").unwrap().is_none());
    saved["computed_at"] = json!(9999);
    saved["history"]["data_fingerprint"] = json!("updated-fingerprint");
    save_report(&conn,&saved).unwrap();
    let count: usize = conn.query_row("SELECT COUNT(*) FROM app.entry_model_validation_reports",[],|row|row.get(0)).unwrap();
    assert_eq!(count,1);
    assert_eq!(get_report(&conn,"pintu.spot.btc","15m").unwrap().unwrap()["history"]["data_fingerprint"],"updated-fingerprint");
    let page = list(&conn,"pintu",None,"15m",None,0,100).unwrap();
    let summary = page["rows"].as_array().unwrap().iter().find(|row|row["instrument_id"]=="pintu.spot.btc").unwrap()["evidence"].clone();
    assert_eq!(summary["history_bars"],1500);
    assert_eq!(summary["models"][0]["holdout_trades"],20);
    assert_eq!(summary["models"][0]["selected_exit_bars"],8);
    assert_eq!(summary["sizing_eligible"],false);
    assert_eq!(summary["protocol_current"],true);
    assert!(summary.get("history").is_none());
}

#[test]
fn counts_are_ticker_counts_and_never_infer_validation_from_profitable_backtests() {
    let conn = connection();
    insert(&conn,"one","pintu","pintu_pro_spot","ONE",true);
    insert(&conn,"two","pintu","pintu_pro_spot","TWO",true);
    insert(&conn,"three","pintu","pintu_pro_spot","THREE",true);
    let one = report(&conn,"one","4h");
    save_report(&conn,&one).unwrap();
    let mut two = report(&conn,"two","4h");
    for model in two["models"].as_array_mut().unwrap() {
        model["entry_validation_status"] = json!("VALIDATED_HOLDOUT");
    }
    save_report(&conn,&two).unwrap();
    let mut three = report(&conn,"three","4h");
    three["status"] = json!("NO_HISTORY");
    save_report(&conn,&three).unwrap();
    let page = list(&conn,"pintu",None,"4h",None,0,100).unwrap();
    assert_eq!(page["summary"],json!({"registered":3,"backtested":2,"validated":1}));
    let filtered = list(&conn,"pintu",None,"4h",Some("ONE"),0,100).unwrap();
    assert_eq!(filtered["summary"],json!({"registered":1,"backtested":1,"validated":0}));
    assert_eq!(list(&conn,"pintu",None,"15m",None,0,100).unwrap()["summary"]["backtested"],0);
    conn.execute("UPDATE app.entry_model_validation_reports SET protocol_id='old-protocol' WHERE instrument_id='two'",[]).unwrap();
    let old = list(&conn,"pintu",None,"4h",None,0,100).unwrap();
    assert_eq!(old["summary"]["validated"],0);
    assert_eq!(old["summary"]["backtested"],1);
}

#[test]
fn registry_eligibility_and_verification_remain_distinct_from_saved_evidence() {
    let conn = connection();
    insert(&conn,"pending","pintu","pintu_pro_spot","PENDING",false);
    insert(&conn,"excluded","pintu","pintu_pro_spot","EXCLUDED",true);
    insert(&conn,"stockbit.unverified","stockbit","stockbit_regular","UNVERIFIED",false);
    conn.execute("INSERT INTO app.instrument_exclusions VALUES('pintu','crypto_spot','EXCLUDED',1)",[]).unwrap();
    assert_eq!(instrument(&conn,"pending").unwrap().unwrap()["research_eligibility"],"pending_venue_verification");
    assert_eq!(instrument(&conn,"excluded").unwrap().unwrap()["research_eligibility"],"excluded");
    let stockbit = instrument(&conn,"stockbit.unverified").unwrap().unwrap();
    assert_eq!(stockbit["research_eligibility"],"eligible");
    assert_eq!(stockbit["venue_verified"],false);
    conn.execute("UPDATE app.instruments SET is_active=0 WHERE instrument_id='excluded'",[]).unwrap();
    assert!(instrument(&conn,"excluded").unwrap().is_none());
    assert_eq!(list(&conn,"pintu",None,"15m",None,0,100).unwrap()["total"],1);
}

#[test]
fn saving_rejects_mismatched_registration_protocol_and_oversized_reports() {
    let conn = connection();
    insert(&conn,"one","pintu","pintu_pro_spot","ONE",true);
    let original = report(&conn,"one","15m");
    for (field,value) in [("product_id","pintu_pro_futures"),("exchange_id","stockbit"),("storage_symbol","OTHER")] {
        let mut wrong = original.clone();
        wrong["instrument"][field] = json!(value);
        assert!(save_report(&conn,&wrong).is_err());
    }
    let mut wrong = original.clone();
    wrong["protocol_id"] = json!("old-version");
    assert!(save_report(&conn,&wrong).is_err());
    wrong = original.clone(); wrong["timeframe"] = json!("5m");
    assert!(save_report(&conn,&wrong).is_err());
    wrong = original.clone(); wrong["extra"] = json!("x".repeat(MAX_REPORT_JSON_BYTES));
    assert!(save_report(&conn,&wrong).is_err());
    assert!(get_report(&conn,"one","15m").unwrap().is_none());
}
