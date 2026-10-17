use super::annotate_product_identity;
use serde_json::json;

#[test]
fn app_product_does_not_replace_trade_strategy_identity() {
    let mut plan = json!({"Strategy_ID":"tokocrypto_spot_day_trade"});
    annotate_product_identity(&mut plan, "tokocrypto_spot");
    assert_eq!(plan["Product_ID"], "tokocrypto_spot");
    assert_eq!(plan["Strategy_ID"], "tokocrypto_spot_day_trade");
}
