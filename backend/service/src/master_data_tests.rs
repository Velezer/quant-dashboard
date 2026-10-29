use super::*;

#[test]
fn stockbit_proximity_catalog_retires_previous_strategy_identities() {
    let previous_strategy_id = quant_core::STOCKBIT_NWE_4H_PREVIOUS_CROSS_STRATEGY_ID;
    let previous_score_strategy_id = quant_core::STOCKBIT_NWE_4H_PREVIOUS_SCORE_STRATEGY_ID;
    let previous_binary_strategy_id = quant_core::STOCKBIT_NWE_4H_PREVIOUS_BINARY_CROSS_STRATEGY_ID;
    let previous_ranked_cross_strategy_id =
        quant_core::STOCKBIT_NWE_4H_PREVIOUS_RANKED_CROSS_STRATEGY_ID;
    let mut catalog = serde_json::json!({
        "indicators": [],
        "signal_strategies": [
            {
                "id": previous_strategy_id,
                "label": "Historical upper-band cross",
                "implemented": true
            },
            {
                "id": previous_score_strategy_id,
                "label": "Historical band-position score",
                "implemented": true
            },
            {
                "id": previous_binary_strategy_id,
                "label": "Previous binary cross score",
                "implemented": true
            },
            {
                "id": previous_ranked_cross_strategy_id,
                "label": "Previous ranked cross score",
                "implemented": true
            }
        ]
    });

    assert!(ensure_stockbit_signal_catalog(&mut catalog));

    let strategies = catalog["signal_strategies"].as_array().unwrap();
    for previous_id in [
        previous_strategy_id,
        previous_score_strategy_id,
        previous_binary_strategy_id,
        previous_ranked_cross_strategy_id,
    ] {
        let previous = strategies
            .iter()
            .find(|model| model["id"] == previous_id)
            .unwrap();
        assert_eq!(previous["status"], "retired");
        assert_eq!(previous["validation_status"], "RETIRED");
        assert_eq!(previous["implemented"], false);
        assert_eq!(
            previous["replaced_by"],
            quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID
        );
    }
    let current = strategies
        .iter()
        .find(|model| model["id"] == quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID)
        .unwrap();
    assert_eq!(current["status"], "research_only");
    assert!(current["description"]
        .as_str()
        .unwrap()
        .contains("lower-band cross or near-band state on a completed 4h candle is LONG"));
    assert!(current["description"]
        .as_str()
        .unwrap()
        .contains("upper-band cross or near-band state is SHORT"));
    assert_eq!(
        current["score_model_id"],
        quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID
    );
    assert_eq!(
        current["minimum_history_bars"],
        quant_core::stockbit_nwe_4h_minimum_history_bars()
    );
}

#[test]
fn screener_reference_payload_exposes_app_identity_without_strategy_plans() {
    let mut catalog: Value = super::super::wire::decode(MASTER_DATA_JSON).unwrap();
    assert!(augment_app_products(&mut catalog, true));
    let reference = signal_screener_master_data_from_catalog(&catalog);
    let mut fields = reference
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    fields.sort_unstable();

    assert_eq!(
        fields,
        ["asset_categories", "exchanges", "signal_score_models"]
    );
    assert!(reference.get("signal_strategies").is_none());
    assert!(reference.get("trade_styles").is_none());
    assert!(reference.get("plan_profiles").is_none());
    let models = reference["signal_score_models"].as_array().unwrap();
    assert_eq!(
        models.len(),
        quant_core::SIGNAL_SCREENER_TIMEFRAMES.len() * 4
    );
    for timeframe in quant_core::SIGNAL_SCREENER_TIMEFRAMES {
        let expected_ids = [
            quant_core::stockbit_nwe_signal_strategy_id(timeframe),
            quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe),
            quant_core::crypto_nwe_signal_strategy_id(timeframe),
            quant_core::crypto_dss_bressert_signal_strategy_id(timeframe),
        ];
        for expected_id in expected_ids.into_iter().flatten() {
            let model = models
                .iter()
                .find(|model| model["id"] == expected_id)
                .expect("each indicator and timeframe should have its own score model");
            assert_eq!(model["timeframe"], *timeframe);
            assert_eq!(
                model["score_scale_max"],
                quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX
            );
            assert_eq!(
                model["minimum_score_out_of_40"],
                quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
            );
            assert_eq!(
                model["component_score_scale_max"],
                quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX
            );
        }
    }
    let stockbit_model = models
        .iter()
        .find(|model| model["id"] == quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID)
        .expect("the Stockbit 4h NWE model should remain available");
    assert_eq!(
        stockbit_model["id"],
        quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID
    );
    assert_eq!(
        stockbit_model["score_model_id"],
        quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID
    );
    assert_eq!(
        stockbit_model["score_scale_max"],
        quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX
    );
    assert_eq!(
        stockbit_model["minimum_score_out_of_40"],
        quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
    );
    assert_eq!(
        stockbit_model["component_score_scale_max"],
        quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX
    );
    assert_eq!(
        stockbit_model["indicator_component_score_scale_max"],
        quant_core::FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX
    );
    assert!(stockbit_model.get("strategy_weight").is_none());
    assert!(stockbit_model.get("confluence_weight").is_none());
    assert_eq!(
        stockbit_model["minimum_history_bars"],
        quant_core::stockbit_nwe_4h_minimum_history_bars()
    );
    assert!(stockbit_model["score_basis_description"]
        .as_str()
        .unwrap()
        .contains("lower-band cross or near-band state on a completed 4h candle is LONG"));
    assert!(stockbit_model["score_basis_description"]
        .as_str()
        .unwrap()
        .contains("upper-band cross or near-band state is SHORT"));
    assert!(stockbit_model["score_basis_description"]
        .as_str()
        .unwrap()
        .contains("four equal-weight factors: NWE, DSS Bressert"));
    assert!(stockbit_model["score_basis_description"]
        .as_str()
        .unwrap()
        .contains("estimated OHLCV volume delta"));
    assert!(stockbit_model["score_basis_description"]
        .as_str()
        .unwrap()
        .contains("1.0/40"));
    assert!(stockbit_model["score_basis_description"]
        .as_str()
        .unwrap()
        .contains("within 50% of the full envelope width"));

    let dss_model = models
        .iter()
        .find(|model| model["id"] == quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID)
        .expect("the DSS near-cross model should be available in the screener catalog");
    assert_eq!(
        dss_model["score_model_id"],
        quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID
    );
    assert_eq!(
        dss_model["minimum_history_bars"],
        quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS
    );
    assert_eq!(
        dss_model["minimum_score_out_of_40"],
        quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40
    );
    assert_eq!(
        dss_model["score_scale_max"],
        quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX
    );
    assert_eq!(
        dss_model["component_score_scale_max"],
        quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX
    );
    assert_eq!(
        dss_model["strategy_score_label"],
        "Price evidence (NWE + DSS)"
    );
    assert_eq!(
        dss_model["confluence_score_label"],
        "Volume evidence (relative volume + delta proxy)"
    );
    assert!(dss_model["score_basis_description"]
        .as_str()
        .unwrap()
        .contains("approaching near-cross"));
    assert!(dss_model["score_basis_description"]
        .as_str()
        .unwrap()
        .contains("four equal-weight factors: NWE, DSS Bressert"));

    let crypto_dss_model = models
        .iter()
        .find(|model| model["id"] == quant_core::CRYPTO_DSS_BRESSERT_4H_SIGNAL_STRATEGY_ID)
        .expect("the crypto 4h DSS near-cross model should be published");
    assert_eq!(
        crypto_dss_model["score_model_id"],
        quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID
    );
    assert_eq!(
        crypto_dss_model["minimum_history_bars"],
        quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS
    );
    assert!(crypto_dss_model["asset_category_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|category| category == "crypto_spot"));
    assert!(!crypto_dss_model["asset_category_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|category| category == "tokenized_stock"));
}

#[test]
fn golden_cross_catalog_migration_publishes_the_active_rules_and_indicators() {
    let mut catalog = serde_json::json!({
        "signal_strategies": [{
            "id": "ema_20_50_golden_cross_long",
            "label": "legacy",
            "indicator_ids": ["ema_20_50", "rsi_14", "relative_volume"]
        }],
        "indicators": [{
            "id": "dss_bressert",
            "signal_strategy": false
        }],
        "plan_profiles": [{
            "id": "pintu_futures_confluence",
            "signal_strategy_id": "ema_20_50_golden_cross_long",
            "engine_strategy_id": "pintu_futures_golden_cross_v1",
            "evidence_strategy_id": "pintu_futures_golden_cross_v1"
        }]
    });

    assert!(upgrade_pintu_golden_cross_profile(&mut catalog));

    let model = catalog["signal_strategies"][0].clone();
    let description = model["description"].as_str().unwrap_or_default();
    assert_eq!(model["id"], PINTU_GOLDEN_CROSS_SIGNAL_STRATEGY_ID);
    assert!(description.contains("prior 23 completed 1h"));
    assert!(description.contains("below 5"));
    assert!(description.contains("Fixed 1% stop"));
    assert!(description.contains("3R gross"));
    assert!(!description.contains("RSI"));
    assert!(!description.contains("relative volume"));
    assert_eq!(
        model["indicator_ids"],
        serde_json::json!(["ema_20_50", "dss_bressert", "market_structure_shift_15m"])
    );
    assert_eq!(model["validation_status"], "UNVALIDATED");

    let profile = &catalog["plan_profiles"][0];
    assert_eq!(
        profile["engine_strategy_id"],
        PINTU_GOLDEN_CROSS_STRATEGY_ID
    );
    assert_eq!(
        profile["evidence_strategy_id"],
        PINTU_GOLDEN_CROSS_STRATEGY_ID
    );
    assert_eq!(
        catalog["indicators"][0]["signal_strategy"],
        serde_json::json!(true)
    );
    assert_eq!(catalog["indicators"][1]["id"], "market_structure_shift_15m");
}

#[test]
fn required_confirmations_version_profiles_without_merging_evidence_identity() {
    let mut catalog = serde_json::json!({
        "indicators": [{"id":"nadaraya_watson_midline_retest"}],
        "signal_strategies": [{
            "id": "idx_open_low_rvol",
            "label": "Open-Low + Relative Volume",
            "description": "Existing registered signal model.",
            "indicator_ids": ["open_low_structure"],
            "asset_category_ids": ["idx_stock"],
            "implemented": true
        }, {
            "id": "idx_open_low_rvol_dss_bressert_nwe_v1",
            "label": "Open-Low + Relative Volume · DSS/Bressert/NWE v1",
            "description": "Previous versioned signal model.",
            "indicator_ids": ["open_low_structure"],
            "required_confirmation_ids": ["dss_bressert_near_cross", "nadaraya_watson_midline_retest"],
            "asset_category_ids": ["idx_stock"],
            "implemented": true
        }],
        "plan_profiles": [
            {
                "id": "stockbit_day_trade",
                "signal_strategy_id": "idx_open_low_rvol_dss_bressert_nwe_v1",
                "evidence_strategy_id": "idx_stockbit_day_trade_dss_bressert_nadaraya_watson_v1",
                "research_label": "Stockbit day trade"
            },
            {
                "id": "stockbit_multibagger",
                "signal_strategy_id": "idx_open_low_rvol",
                "evidence_strategy_id": "multibagger",
                "research_label": "Stockbit multibagger"
            }
        ]
    });

    assert!(ensure_required_signal_confirmation_models(&mut catalog));
    let day_trade_id = quant_core::historical_evidence_profile("idx_stockbit_day_trade")
        .expect("day trade evidence profile should exist")
        .strategy_id;
    let multibagger_id = quant_core::historical_evidence_profile("multibagger")
        .expect("multibagger evidence profile should exist")
        .strategy_id;
    assert_ne!(day_trade_id, multibagger_id);
    assert_eq!(
        catalog["plan_profiles"][0]["evidence_strategy_id"],
        day_trade_id
    );
    assert_eq!(
        catalog["plan_profiles"][0]["signal_strategy_id"],
        "idx_open_low_rvol_dss_extreme_nwe_touch_v2"
    );
    assert_eq!(
        catalog["plan_profiles"][1]["evidence_strategy_id"],
        multibagger_id
    );
    assert_ne!(
        catalog["plan_profiles"][0]["signal_strategy_id"],
        "idx_open_low_rvol"
    );

    let versioned_model = catalog["signal_strategies"]
        .as_array()
        .and_then(|models| {
            models
                .iter()
                .find(|model| model["id"] == catalog["plan_profiles"][0]["signal_strategy_id"])
        })
        .expect("versioned signal model should be registered");
    assert_eq!(
        versioned_model["required_confirmation_ids"],
        serde_json::json!(quant_core::REQUIRED_SIGNAL_CONFIRMATION_IDS)
    );
    let retired_model = catalog["signal_strategies"]
        .as_array()
        .and_then(|models| {
            models
                .iter()
                .find(|model| model["id"] == "idx_open_low_rvol_dss_bressert_nwe_v1")
        })
        .expect("previous model identity should remain available for history");
    assert_eq!(retired_model["status"], "retired");
    assert_eq!(retired_model["validation_status"], "RETIRED");
    assert!(retired_model.get("required_confirmation_ids").is_none());
    assert!(retired_model["description"]
        .as_str()
        .unwrap()
        .contains("retired from current signal generation"));
    assert!(!catalog["indicators"]
        .as_array()
        .unwrap()
        .iter()
        .any(|indicator| indicator["id"] == "nadaraya_watson_midline_retest"));
    assert!(!ensure_required_signal_confirmation_models(&mut catalog));
}

#[test]
fn embedded_catalog_seed_binds_inherited_products_to_supported_engines() {
    let mut catalog: Value = super::super::wire::decode(MASTER_DATA_JSON).unwrap();

    assert!(augment_app_products(&mut catalog, true));
    let profiles = catalog["plan_profiles"].as_array().unwrap();
    let spot = profiles
        .iter()
        .find(|profile| profile["id"] == "tokocrypto_spot_day_trade")
        .unwrap();
    let tokenized = profiles
        .iter()
        .find(|profile| profile["id"] == "tokocrypto_tokenized_stock_day_trade")
        .unwrap();
    let pintu_spot = profiles
        .iter()
        .find(|profile| profile["id"] == "pintu_retail_spot")
        .unwrap();

    assert!(spot["signal_strategy_id"]
        .as_str()
        .unwrap()
        .starts_with(TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID));
    assert_eq!(
        spot["engine_strategy_id"],
        TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID
    );
    assert_eq!(
        tokenized["engine_strategy_id"],
        TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID
    );
    assert_eq!(
        pintu_spot["engine_strategy_id"],
        TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID
    );
    assert_eq!(spot["validation_status"], "UNVALIDATED");

    let current_model = catalog["signal_strategies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["id"] == TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID)
        .unwrap();
    assert!(current_model["label"].as_str().unwrap().contains("2 of 4"));
    assert_eq!(current_model["validation_status"], "UNVALIDATED");
}

#[test]
fn existing_tokocrypto_profiles_wait_for_the_explicit_catalog_migration() {
    let mut catalog: Value = super::super::wire::decode(MASTER_DATA_JSON).unwrap();

    let _ = augment_app_products(&mut catalog, false);
    let profiles = catalog["plan_profiles"].as_array().unwrap();
    let spot = profiles
        .iter()
        .find(|profile| profile["id"] == "tokocrypto_spot_day_trade")
        .unwrap();
    let tokenized = profiles
        .iter()
        .find(|profile| profile["id"] == "tokocrypto_tokenized_stock_day_trade")
        .unwrap();

    assert_eq!(spot["signal_strategy_id"], "ema_rsi_breakout_volume");
    assert_eq!(spot["engine_strategy_id"], "tokocrypto_spot_day_trade");
    assert_eq!(tokenized["engine_strategy_id"], "tokenized_stock_day_trade");
}

#[test]
fn explicit_tokocrypto_catalog_migration_is_transactional_provenance_preserving_and_idempotent() {
    let mut seed: Value = super::super::wire::decode(MASTER_DATA_JSON).unwrap();
    augment_app_products(&mut seed, false);
    let seed_encoded = super::super::wire::encode(&seed).unwrap();
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .unwrap();
    initialize_schema(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO app.reference_catalogs(catalog_id,payload_json,source_label)
             VALUES('master_data',?1,'embedded_release_seed')",
            [seed_encoded],
        )
        .unwrap();

    let changed_rows = migrate_tokocrypto_confluence_catalog(&connection).unwrap();
    assert!(changed_rows > 0);
    assert_eq!(
        migrate_tokocrypto_confluence_catalog(&connection).unwrap(),
        0
    );

    let encoded = connection
        .query_row(
            "SELECT payload_json FROM app.reference_catalogs WHERE catalog_id='master_data'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    let catalog: Value = super::super::wire::decode(&encoded).unwrap();
    validate_master_data_catalog(&catalog).unwrap();
    let spot = catalog["plan_profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["id"] == "tokocrypto_spot_day_trade")
        .unwrap();
    assert_eq!(
        spot["engine_strategy_id"],
        TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID
    );
    assert!(spot["signal_strategy_id"]
        .as_str()
        .unwrap()
        .starts_with(TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID));

    for id in [
        "pintu_retail_spot",
        "pintu_pro_spot",
        "pluang_spot",
        "pluang_perpetual",
    ] {
        let profile = catalog["plan_profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|profile| profile["id"] == id)
            .unwrap();
        let source_id = if id == "pluang_perpetual" {
            "pintu_futures_confluence"
        } else {
            "tokocrypto_spot_day_trade"
        };
        let source = catalog["plan_profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|profile| profile["id"] == source_id)
            .unwrap();
        for key in [
            "engine_strategy_id",
            "signal_strategy_id",
            "evidence_strategy_id",
        ] {
            assert_eq!(
                profile[key], source[key],
                "{id} must use its owned template's {key}"
            );
        }
        let original = seed["plan_profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|profile| profile["id"] == id)
            .unwrap();
        for key in [
            "exchange_id",
            "exchange_code",
            "product_id",
            "asset_category_id",
        ] {
            assert_eq!(profile[key], original[key], "{id} must preserve {key}");
        }
    }

    let (source_label, marker): (String, (String, String)) = (
        connection
            .query_row(
                "SELECT source_label FROM app.reference_catalogs WHERE catalog_id='master_data'",
                [],
                |row| row.get(0),
            )
            .unwrap(),
        connection
            .query_row(
                "SELECT source_hash,destination_hash FROM app.reference_catalog_migrations
                 WHERE migration_id=?1",
                [TOKOCRYPTO_CONFLUENCE_MIGRATION_ID],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap(),
    );
    assert_eq!(source_label, "embedded_release_seed");
    assert!(marker.0.starts_with("fnv1a64:"));
    assert_eq!(
        marker.1,
        super::tokocrypto_catalog::catalog_content_hash(&encoded)
    );

    let row_counts: String = connection
        .query_row(
            "SELECT migrated_row_counts_json FROM app.reference_catalog_migrations
             WHERE migration_id=?1",
            [TOKOCRYPTO_CONFLUENCE_MIGRATION_ID],
            |row| row.get(0),
        )
        .unwrap();
    let row_counts: Value = super::super::wire::decode(&row_counts).unwrap();
    assert!(row_counts["signal_strategies"].as_u64().unwrap() > 0);
    assert!(row_counts["plan_profiles"].as_u64().unwrap() > 0);
}
