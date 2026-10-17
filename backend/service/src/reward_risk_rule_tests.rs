use rusqlite::Connection;

#[test]
fn default_pintu_exchange_scope_resolves_canonical_futures_fee_policy() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .unwrap();
    crate::master_data::initialize(&connection).unwrap();
    let context = crate::reward_risk_rule::fee_context(&connection, "pintu", "all").unwrap();

    assert_eq!(context["exchange_id"], "pintu");
    assert_eq!(context["product_id"], "pintu_pro_futures");
    assert_eq!(context["fee_policy"]["status"], "known");
    assert!((context["fee_policy"]["buy_fee_pct"].as_f64().unwrap() - 0.2865).abs() < 1e-12);
    assert_eq!(
        context["fee_policy"]["official_fee_rules_url"],
        "https://pintu.co.id/faq/fees-limits-security"
    );
}

#[test]
fn default_stockbit_exchange_scope_resolves_idx_stock_fee_policy() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .unwrap();
    crate::master_data::initialize(&connection).unwrap();
    let context = crate::reward_risk_rule::fee_context(&connection, "stockbit", "all").unwrap();

    assert_eq!(context["exchange_id"], "stockbit");
    assert_eq!(context["asset_category_id"], "idx_stock");
    assert_eq!(context["asset_category_label"], "IDX Stock");
    assert_eq!(context["fee_policy"]["status"], "known");
    assert_eq!(context["fee_policy"]["buy_fee_pct"], 0.15);
    assert_eq!(context["fee_policy"]["sell_fee_pct"], 0.25);
    assert_eq!(
        context["fee_policy"]["official_fee_rules_url"],
        "https://help.stockbit.com/id/article/transaksi-saham-berapa-biaya-trading-di-stockbit-sekuritas-1lbkyq9/"
    );
}

#[test]
fn registered_stockbit_product_resolves_fee_context_without_a_plan_profile_product_id() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "ATTACH DATABASE ':memory:' AS app;
             CREATE TABLE app.exchange_products(
                 exchange_id TEXT NOT NULL,
                 product_id TEXT NOT NULL,
                 asset_category_id TEXT NOT NULL,
                 status TEXT NOT NULL DEFAULT 'enabled'
             );
             INSERT INTO app.exchange_products(exchange_id,product_id,asset_category_id)
             VALUES('stockbit','stockbit_regular','idx_stock');",
        )
        .unwrap();
    crate::master_data::initialize(&connection).unwrap();
    let context =
        crate::reward_risk_rule::fee_context(&connection, "stockbit", "stockbit_regular").unwrap();

    assert_eq!(context["product_id"], "stockbit_regular");
    assert_eq!(context["asset_category_id"], "idx_stock");
    assert_eq!(context["direction_policy"], "long_only");
    assert_eq!(context["fee_policy"]["status"], "known");
}
