use super::*;

pub(super) fn augment_app_products(catalog: &mut Value, is_embedded_seed: bool) -> bool {
    let mut changed = upgrade_pintu_golden_cross_profile(catalog);
    let Some(exchanges) = catalog.get_mut("exchanges").and_then(Value::as_array_mut) else {
        return false;
    };
    for exchange in exchanges {
        if exchange.get("id").and_then(Value::as_str) == Some("pintu_futures") {
            exchange["id"] = Value::from("pintu");
            exchange["code"] = Value::from("PINTU");
            exchange["name"] = Value::from("Pintu");
            exchange["label"] = Value::from("Pintu");
            changed = true;
        }
        if exchange.get("id").and_then(Value::as_str) == Some("pintu") {
            if exchange.get("label").and_then(Value::as_str) != Some("Pintu") {
                exchange["label"] = Value::from("Pintu");
                changed = true;
            }
            if let Some(aliases) = exchange.get_mut("aliases").and_then(Value::as_array_mut) {
                let before = aliases.len();
                aliases.retain(|value| value.as_str() != Some("pintu_futures"));
                if aliases.len() != before {
                    changed = true;
                }
            }
            if let Some(ids) = exchange
                .get_mut("asset_category_ids")
                .and_then(Value::as_array_mut)
            {
                if !ids.iter().any(|id| id.as_str() == Some("crypto_spot")) {
                    ids.push(Value::from("crypto_spot"));
                    changed = true;
                }
            }
        }
        if exchange.get("id").and_then(Value::as_str) == Some("pluang") {
            if exchange.get("status").and_then(Value::as_str) != Some("enabled") {
                exchange["status"] = Value::from("enabled");
                changed = true;
            }
            if let Some(ids) = exchange
                .get_mut("asset_category_ids")
                .and_then(Value::as_array_mut)
            {
                for category in ["crypto_spot", "crypto_perpetual"] {
                    if !ids.iter().any(|id| id.as_str() == Some(category)) {
                        ids.push(Value::from(category));
                        changed = true;
                    }
                }
            }
        }
    }
    for category in catalog
        .get_mut("asset_categories")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let category_id = category
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(ids) = category
            .get_mut("exchange_ids")
            .and_then(Value::as_array_mut)
        {
            for id in ids.iter_mut() {
                if id.as_str() == Some("pintu_futures") {
                    *id = Value::from("pintu");
                    changed = true;
                }
            }
            if category_id == "crypto_spot" && !ids.iter().any(|id| id.as_str() == Some("pintu")) {
                ids.push(Value::from("pintu"));
                changed = true;
            }
            if matches!(category_id.as_str(), "crypto_spot" | "crypto_perpetual")
                && !ids.iter().any(|id| id.as_str() == Some("pluang"))
            {
                ids.push(Value::from("pluang"));
                changed = true;
            }
        }
    }
    for asset in catalog
        .get_mut("assets")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if asset.get("exchange_id").and_then(Value::as_str) == Some("pintu_futures") {
            asset["exchange_id"] = Value::from("pintu");
        }
    }
    let Some(profiles) = catalog
        .get_mut("plan_profiles")
        .and_then(Value::as_array_mut)
    else {
        return changed;
    };
    for profile in profiles.iter_mut() {
        if profile.get("exchange_id").and_then(Value::as_str) == Some("pintu_futures") {
            profile["exchange_id"] = Value::from("pintu");
            profile["exchange_code"] = Value::from("PINTU");
            profile["product_id"] = Value::from("pintu_pro_futures");
        }
    }
    let futures = profiles
        .iter()
        .find(|p| p.get("id").and_then(Value::as_str) == Some("pintu_futures_confluence"))
        .cloned();
    let spot = profiles
        .iter()
        .find(|p| p.get("id").and_then(Value::as_str) == Some("tokocrypto_spot_day_trade"))
        .cloned();
    for (id, product, app, source) in [
        ("pintu_retail_spot", "pintu_retail_spot", "pintu", &spot),
        ("pintu_pro_spot", "pintu_pro_spot", "pintu", &spot),
        ("pluang_spot", "pluang_spot", "pluang", &spot),
        ("pluang_perpetual", "pluang_perpetual", "pluang", &futures),
    ] {
        if profiles
            .iter()
            .any(|p| p.get("id").and_then(Value::as_str) == Some(id))
        {
            continue;
        }
        if let Some(mut profile) = source.clone() {
            profile["id"] = Value::from(id);
            profile["product_id"] = Value::from(product);
            profile["exchange_id"] = Value::from(app);
            profile["exchange_code"] = Value::from(app.to_ascii_uppercase());
            profile["execution_scope"] = Value::from("research_only");
            profile["research_enabled"] = Value::from(true);
            if id == "pluang_perpetual" {
                profile["label"] = Value::from("Pluang · Perpetual Confluence (Research)");
                profile["signal_strategy_id"] = Value::from("ema_rsi_breakout_volume");
                profile["engine_strategy_id"] = Value::from("pintu_futures_confluence");
                profile["evidence_strategy_id"] = Value::from("pintu_futures_confluence");
                profile["research_label"] =
                    Value::from("EMA + RSI + Breakout Volume · Research only");
            }
            profile["time_window"] =
                Value::from(format!("{product} research · venue validation pending"));
            profiles.push(profile);
            changed = true;
        }
    }
    if is_embedded_seed {
        changed |= upgrade_tokocrypto_confluence_profiles(catalog);
    }
    changed |= ensure_required_signal_confirmation_models(catalog);
    changed |= ensure_stockbit_signal_catalog(catalog);
    changed |= ensure_crypto_signal_catalog(catalog);
    changed
}

pub(super) fn retire_stockbit_nwe_4h_legacy_strategies(catalog: &mut Value) -> bool {
    let Some(strategies) = catalog
        .get_mut("signal_strategies")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for previous_id in [
        quant_core::STOCKBIT_NWE_4H_PREVIOUS_CROSS_STRATEGY_ID,
        quant_core::STOCKBIT_NWE_4H_PREVIOUS_SCORE_STRATEGY_ID,
        quant_core::STOCKBIT_NWE_4H_PREVIOUS_BINARY_CROSS_STRATEGY_ID,
        quant_core::STOCKBIT_NWE_4H_PREVIOUS_RANKED_CROSS_STRATEGY_ID,
    ] {
        let Some(strategy) = strategies
            .iter_mut()
            .find(|strategy| strategy.get("id").and_then(Value::as_str) == Some(previous_id))
        else {
            continue;
        };
        let replacements = [
            ("status", Value::from("retired")),
            ("validation_status", Value::from("RETIRED")),
            ("implemented", Value::from(false)),
            (
                "replaced_by",
                Value::from(quant_core::STOCKBIT_NWE_4H_SIGNAL_STRATEGY_ID),
            ),
            (
                "description",
                Value::from("Retired after the Stockbit 4h NWE rule changed to score crosses and near-band states with a 50% floor. Historical records retain this strategy identity."),
            ),
        ];
        for (key, value) in replacements {
            if strategy.get(key) != Some(&value) {
                strategy[key] = value;
                changed = true;
            }
        }
    }
    changed
}

pub(super) fn upsert_catalog_record(catalog: &mut Value, section: &str, record: Value) -> bool {
    let Some(records) = catalog.get_mut(section).and_then(Value::as_array_mut) else {
        return false;
    };
    let id = record.get("id").and_then(Value::as_str);
    let Some(index) = records
        .iter()
        .position(|existing| existing.get("id").and_then(Value::as_str) == id)
    else {
        records.push(record);
        return true;
    };
    if records[index] == record {
        return false;
    }
    records[index] = record;
    true
}

pub(super) fn ensure_required_signal_confirmation_models(catalog: &mut Value) -> bool {
    let mut changed = {
        let Some(indicators) = catalog.get_mut("indicators").and_then(Value::as_array_mut) else {
            return false;
        };
        let initial_indicator_count = indicators.len();
        indicators.retain(|record| {
            record.get("id").and_then(Value::as_str) != Some("nadaraya_watson_midline_retest")
        });
        indicators.len() != initial_indicator_count
    };
    let definitions = [
        (
            quant_core::DSS_BRESSERT_EXTREME_TURN_CONFIRMATION_ID,
            "DSS Bressert Extreme Turn",
        ),
        (
            quant_core::NADARAYA_WATSON_ENVELOPE_TOUCH_CONFIRMATION_ID,
            "Nadaraya-Watson Envelope Touch",
        ),
    ];
    if let Some(models) = catalog
        .get_mut("signal_strategies")
        .and_then(Value::as_array_mut)
    {
        for model in models {
            let is_retired_version = catalog_string(model, "id")
                .is_some_and(|id| id.ends_with(RETIRED_SIGNAL_CONFIRMATION_MODEL_SUFFIX));
            if !is_retired_version {
                continue;
            }
            for (key, value) in [
                ("status", Value::from("retired")),
                ("validation_status", Value::from("RETIRED")),
            ] {
                if model.get(key) != Some(&value) {
                    model[key] = value;
                    changed = true;
                }
            }
            if model
                .as_object_mut()
                .is_some_and(|object| object.remove("required_confirmation_ids").is_some())
            {
                changed = true;
            }
            let description = catalog_string(model, "description")
                .unwrap_or_default()
                .to_owned();
            let retirement_note =
                " Retained as a historical strategy identity; it is retired from current signal generation.";
            if !description.contains(retirement_note.trim()) {
                model["description"] = Value::from(format!("{description}{retirement_note}"));
                changed = true;
            }
        }
    }
    for (id, label) in definitions {
        let indicators = catalog
            .get_mut("indicators")
            .and_then(Value::as_array_mut)
            .expect("indicator section was validated at function entry");
        let Some(record) = indicators
            .iter_mut()
            .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
        else {
            indicators.push(serde_json::json!({
                "id": id,
                "label": label,
                "kind": "indicator",
                "signal_strategy": false,
            }));
            changed = true;
            continue;
        };
        for (key, value) in [
            ("label", Value::from(label)),
            ("kind", Value::from("indicator")),
            ("signal_strategy", Value::from(false)),
        ] {
            if record.get(key) != Some(&value) {
                record[key] = value;
                changed = true;
            }
        }
        if record
            .as_object_mut()
            .is_some_and(|object| object.remove("custom_alert_only").is_some())
        {
            changed = true;
        }
    }

    let confirmation_ids = definitions
        .iter()
        .map(|(id, _)| Value::from(*id))
        .collect::<Vec<_>>();
    let profile_versions = catalog
        .get("plan_profiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|profile| {
            let profile_id = catalog_string(profile, "id")?;
            let signal_id = catalog_string(profile, "signal_strategy_id")?;
            let evidence_id = catalog_string(profile, "evidence_strategy_id")?;
            let engine_id = catalog_string(profile, "engine_strategy_id").unwrap_or("");
            let is_legacy_tokocrypto_profile = matches!(
                (profile_id, engine_id),
                ("tokocrypto_spot_day_trade", "tokocrypto_spot_day_trade")
                    | (
                        "tokocrypto_tokenized_stock_day_trade",
                        "tokenized_stock_day_trade"
                    )
            );
            if is_legacy_tokocrypto_profile && signal_id.starts_with("ema_rsi_breakout_volume") {
                return None;
            }
            let base_signal_id = signal_id
                .strip_suffix(REQUIRED_SIGNAL_CONFIRMATION_MODEL_SUFFIX)
                .or_else(|| signal_id.strip_suffix(RETIRED_SIGNAL_CONFIRMATION_MODEL_SUFFIX))
                .unwrap_or(signal_id);
            let signal_version =
                format!("{base_signal_id}{REQUIRED_SIGNAL_CONFIRMATION_MODEL_SUFFIX}");
            let evidence_id_without_retired_suffix = evidence_id
                .strip_suffix(quant_core::RETIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX)
                .unwrap_or(evidence_id);
            let evidence_version =
                quant_core::historical_evidence_profile(evidence_id_without_retired_suffix)
                    .map(|profile| profile.strategy_id.to_string())
                    .unwrap_or_else(|| {
                        let base_id = evidence_id_without_retired_suffix
                            .strip_suffix(quant_core::REQUIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX)
                            .unwrap_or(evidence_id_without_retired_suffix);
                        format!(
                            "{base_id}{}",
                            quant_core::REQUIRED_SIGNAL_CONFIRMATION_EVIDENCE_SUFFIX
                        )
                    });
            Some((
                profile_id.to_string(),
                base_signal_id.to_string(),
                signal_version,
                evidence_version,
            ))
        })
        .collect::<Vec<_>>();

    if let Some(models) = catalog
        .get_mut("signal_strategies")
        .and_then(Value::as_array_mut)
    {
        let base_models = profile_versions
            .iter()
            .filter_map(|(_, base_id, signal_id, _)| {
                let base = models.iter().find(|model| {
                    catalog_string(model, "id") == Some(base_id.as_str())
                })?;
                let mut version = base.clone();
                let label = catalog_string(base, "label").unwrap_or(base_id);
                version["id"] = Value::from(signal_id.as_str());
                version["label"] =
                    Value::from(format!("{label} · DSS extreme + NWE touch"));
                if let Some(short_label) = catalog_string(base, "short_label") {
                    version["short_label"] = Value::from(format!("{short_label} · DSS/NWE"));
                }
                let description = catalog_string(base, "description").unwrap_or("");
                version["description"] = Value::from(format!(
                    "{description} Directional signals require a recent DSS Bressert extreme that turns back and a completed candle whose wick or body touches the matching causal Nadaraya-Watson outer envelope, then closes inside the envelope. DSS cross/near-cross, body versus wick contact, NWE near-line, and reclaim contribute to the displayed ranking score. The endpoint requires {} completed candles. This strategy version is unvalidated until matching chronological evidence is recorded.",
                    quant_core::NadarayaWatsonEnvelopeConfig::default()
                        .minimum_endpoint_envelope_bars()
                ));
                version["required_confirmation_ids"] =
                    Value::Array(confirmation_ids.clone());
                Some((signal_id.clone(), version))
            })
            .collect::<Vec<_>>();
        for (signal_id, model) in base_models {
            if let Some(existing) = models
                .iter_mut()
                .find(|record| catalog_string(record, "id") == Some(signal_id.as_str()))
            {
                if *existing != model {
                    *existing = model;
                    changed = true;
                }
            } else {
                models.push(model);
                changed = true;
            }
        }
    }

    let Some(profiles) = catalog
        .get_mut("plan_profiles")
        .and_then(Value::as_array_mut)
    else {
        return changed;
    };
    for profile in profiles {
        let Some(profile_id) = catalog_string(profile, "id") else {
            continue;
        };
        if let Some((_, _, signal_version, evidence_version)) = profile_versions
            .iter()
            .find(|(id, _, _, _)| id == profile_id)
        {
            for (key, value) in [
                ("signal_strategy_id", signal_version.as_str()),
                ("evidence_strategy_id", evidence_version.as_str()),
            ] {
                if catalog_string(profile, key) != Some(value) {
                    profile[key] = Value::from(value);
                    changed = true;
                }
            }
            if let Some(label) = catalog_string(profile, "research_label") {
                if !label.contains("DSS extreme + NWE touch") {
                    let prior_label = label
                        .strip_suffix(" · DSS Bressert + Nadaraya confirmed")
                        .unwrap_or(label);
                    profile["research_label"] =
                        Value::from(format!("{prior_label} · DSS extreme + NWE touch"));
                    changed = true;
                }
            }
        }
    }
    changed
}

pub(super) fn upgrade_pintu_golden_cross_profile(catalog: &mut Value) -> bool {
    let signal_model = serde_json::json!({
        "id": PINTU_GOLDEN_CROSS_SIGNAL_STRATEGY_ID,
        "label": "EMA 20/50 + Recent 1h DSS Oversold + 15m DSS Turn + Bullish MSS · Research",
        "short_label": "Golden Cross + 1h/15m DSS + MSS",
        "icon": "📈",
        "description": "Pintu Pro Futures, 15m, long-only: require at least one of the current or prior 23 completed 1h DSS Bressert readings (24 hours) to be below the configured oversold level (22). On the closed 15m EMA 20/50 golden-cross candle, DSS must be rising and must have printed below 5 on that candle or one of the prior three closed 15m candles. Require a fresh bullish 15m market-structure shift: confirmed five-candle swings show lower highs and lower lows, then a closed candle breaks above the latest lower swing high on the cross candle or one of the prior three candles; price must remain above that level. Fixed 1% stop; TP1 at least 3R gross price-distance before fees and slippage. Research only; unvalidated.",
        "status": "research_only",
        "validation_status": "UNVALIDATED",
        "research_only": true,
        "implemented": true,
        "indicator_ids": ["ema_20_50", "dss_bressert", "market_structure_shift_15m"],
        "asset_category_ids": ["crypto_perpetual"]
    });
    let Some(models) = catalog
        .get_mut("signal_strategies")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = match models.iter().position(|model| {
        let Some(id) = model.get("id").and_then(Value::as_str) else {
            return false;
        };
        id == PINTU_GOLDEN_CROSS_SIGNAL_STRATEGY_ID
            || PINTU_GOLDEN_CROSS_LEGACY_SIGNAL_STRATEGY_IDS.contains(&id)
    }) {
        Some(index) if models[index] != signal_model => {
            models[index] = signal_model.clone();
            true
        }
        Some(_) => false,
        None => {
            models.push(signal_model.clone());
            true
        }
    };
    let model_count = models.len();
    models.retain(|model| {
        let id = model.get("id").and_then(Value::as_str);
        id == Some(PINTU_GOLDEN_CROSS_SIGNAL_STRATEGY_ID)
            || !id.is_some_and(|id| PINTU_GOLDEN_CROSS_LEGACY_SIGNAL_STRATEGY_IDS.contains(&id))
    });
    changed |= model_count != models.len();

    if let Some(indicators) = catalog.get_mut("indicators").and_then(Value::as_array_mut) {
        if let Some(dss) = indicators
            .iter_mut()
            .find(|indicator| indicator.get("id").and_then(Value::as_str) == Some("dss_bressert"))
        {
            if dss.get("signal_strategy").and_then(Value::as_bool) != Some(true) {
                dss["signal_strategy"] = Value::Bool(true);
                changed = true;
            }
        }
        if !indicators.iter().any(|indicator| {
            indicator.get("id").and_then(Value::as_str) == Some("market_structure_shift_15m")
        }) {
            indicators.push(serde_json::json!({
                "id": "market_structure_shift_15m",
                "label": "15m Market Structure Shift",
                "kind": "market_structure_rule",
                "signal_strategy": true
            }));
            changed = true;
        }
    }

    let Some(profiles) = catalog
        .get_mut("plan_profiles")
        .and_then(Value::as_array_mut)
    else {
        return changed;
    };
    let Some(profile) = profiles.iter_mut().find(|profile| {
        profile.get("id").and_then(Value::as_str) == Some("pintu_futures_confluence")
    }) else {
        return changed;
    };
    for (key, value) in [
        ("label", "Pintu Futures · Golden Cross + Recent 1h DSS Oversold + 15m DSS Turn + MSS (Research)"),
        ("signal_strategy_id", PINTU_GOLDEN_CROSS_SIGNAL_STRATEGY_ID),
        ("engine_strategy_id", PINTU_GOLDEN_CROSS_STRATEGY_ID),
        ("evidence_strategy_id", PINTU_GOLDEN_CROSS_STRATEGY_ID),
        (
            "time_window",
            "Pintu Pro Futures · 15m golden cross after 1h DSS oversold within 24h and a recent rising 15m DSS extreme · fresh bullish MSS · fixed 1% stop · TP1 minimum 3R",
        ),
        (
            "research_label",
            "Golden Cross + Recent 1h DSS Oversold + 15m DSS Turn + Bullish MSS · Research only",
        ),
        ("validation_status", "UNVALIDATED"),
        ("execution_scope", "research_only"),
    ] {
        if profile.get(key).and_then(Value::as_str) != Some(value) {
            profile[key] = Value::from(value);
            changed = true;
        }
    }
    changed
}
