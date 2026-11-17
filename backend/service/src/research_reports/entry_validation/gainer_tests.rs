use super::*;

fn report() -> Value {
    let mut metrics = json!({});
    let mut splits = json!({});
    for partition in ["training","validation","holdout"] {
        splits[partition] = json!({"count":0,"first":null,"last":null});
        metrics[partition] = json!({});
        for name in ["top1","top5","grossTop5","universe","momentum","excessUniverse","excessMomentum","positiveFrequency","decilePrecision","rankCorrelation"] {
            metrics[partition][name] = json!({"count":0,"mean":null,"lower":null,"upper":null});
        }
    }
    json!({"protocol_id":GAINER_PROTOCOL,"cutoff":1,"status":"VALIDATED",
        "coverage":{"universe":10,"attempted":0,"failed":0,"missing":0,"short":0,"proxies":0,
            "minimumHistoryDays":1000,"listingAgeUnknown":0,"stopped":true,"excludedCohorts":0,"purged":0},
        "costs":{"mode":"published","buy":null,"sell":null,"spread":0.1,"slippage":0.1},
        "provenance":{"fingerprint":"fnv1a32:1234","sources":{},"policies":{}},
        "splits":splits,"metrics":metrics,"limitations":"Research only"})
}

#[test]
fn gainer_tests_save_only_aggregate_and_never_accept_validation_claims() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("ATTACH DATABASE ':memory:' AS app; CREATE TABLE app.gainer_research_reports(id INTEGER PRIMARY KEY,protocol_id TEXT,computed_at INTEGER,report_json TEXT);").unwrap();
    assert!(gainer_saved(&connection,None).unwrap()["report"].is_null());
    let saved = gainer_saved(&connection,Some(report())).unwrap();
    assert_eq!(saved["report"]["status"],"UNVALIDATED_RESEARCH");
    assert_eq!(gainer_saved(&connection,None).unwrap()["report"],saved["report"]);
    let mut invalid = report(); invalid["observations"] = json!([{"entry":1}]);
    assert!(gainer_saved(&connection,Some(invalid)).is_err());
    let mut nested = report(); nested["coverage"]["trade_rows"] = json!([{"entry":1}]);
    assert!(gainer_saved(&connection,Some(nested)).is_err());
    let mut old = report(); old["protocol_id"] = json!("old");
    assert!(gainer_saved(&connection,Some(old)).is_err());
    assert_eq!(gainer_saved(&connection,None).unwrap()["report"],saved["report"]);
}

#[test]
fn gainer_tests_reject_unknown_or_invalid_costs() {
    for bad in [json!(null),json!(-1),json!(11),json!("unknown")] {
        let mut value = report(); value["costs"]["spread"] = bad;
        assert!(validate_aggregate(&value).is_err());
    }
    let mut manual = report(); manual["costs"]["mode"] = json!("manual");
    assert!(validate_aggregate(&manual).is_err());
}
