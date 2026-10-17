use super::{
    catalog_string, ensure_required_signal_confirmation_models, validate_master_data_catalog,
};
use quant_core::{
    tokocrypto_confluence_indicator_ids, TokocryptoConfluenceConfig,
    TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID, TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID,
    TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) const TOKOCRYPTO_CONFLUENCE_MIGRATION_ID: &str =
    "registered_profile_strategy_bindings_v2";

/// Apply the explicit app-catalog update for existing SQLite databases.
/// Stored signal/evidence history is untouched; only reference model and
/// profile rows are updated inside one SQLite transaction.
pub(crate) fn migrate_tokocrypto_confluence_catalog(conn: &Connection) -> Result<usize, String> {
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let encoded = transaction
        .query_row(
            "SELECT payload_json FROM reference_catalogs WHERE catalog_id='master_data'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    let source_hash = catalog_content_hash(&encoded);

    let completed = transaction
        .query_row(
            "SELECT destination_hash FROM reference_catalog_migrations WHERE migration_id=?1",
            [TOKOCRYPTO_CONFLUENCE_MIGRATION_ID],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(destination_hash) = completed {
        if destination_hash != source_hash {
            return Err("Tokocrypto catalog changed after its recorded migration".into());
        }
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(0);
    }

    let mut catalog: Value = crate::wire::decode(&encoded).map_err(|error| error.to_string())?;
    let before = catalog.clone();
    let changed = upgrade_tokocrypto_confluence_profiles(&mut catalog)
        | ensure_required_signal_confirmation_models(&mut catalog);
    validate_master_data_catalog(&catalog)?;
    if changed {
        catalog["updated_on"] = Value::from("2026-10-03");
    }
    let updated = crate::wire::encode(&catalog).map_err(|error| error.to_string())?;
    let strategy_rows = changed_catalog_record_count(&before, &catalog, "signal_strategies");
    let profile_rows = changed_catalog_record_count(&before, &catalog, "plan_profiles");
    let row_counts = serde_json::json!({
        "signal_strategies": strategy_rows,
        "plan_profiles": profile_rows,
    });
    let destination_hash = catalog_content_hash(&updated);
    transaction
        .execute(
            "UPDATE reference_catalogs SET payload_json=?1 WHERE catalog_id='master_data'",
            [&updated],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO reference_catalog_migrations(
                 migration_id,source_hash,destination_hash,migrated_row_counts_json,completed_at
             ) VALUES(?1,?2,?3,?4,datetime('now'))",
            params![
                TOKOCRYPTO_CONFLUENCE_MIGRATION_ID,
                source_hash,
                destination_hash,
                row_counts.to_string(),
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(strategy_rows + profile_rows)
}

pub(super) fn catalog_content_hash(content: &str) -> String {
    let hash = content
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!("fnv1a64:{hash:016x}")
}

fn changed_catalog_record_count(before: &Value, after: &Value, section: &str) -> usize {
    let before_rows = catalog_record_index(before, section);
    let after_rows = catalog_record_index(after, section);
    let ids = before_rows
        .keys()
        .chain(after_rows.keys())
        .cloned()
        .collect::<HashSet<_>>();
    ids.into_iter()
        .filter(|id| before_rows.get(id).copied() != after_rows.get(id).copied())
        .count()
}

fn catalog_record_index<'a>(catalog: &'a Value, section: &str) -> HashMap<String, &'a Value> {
    catalog
        .get(section)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| Some((record.get("id")?.as_str()?.to_owned(), record)))
        .collect()
}

pub(super) fn upgrade_tokocrypto_confluence_profiles(catalog: &mut Value) -> bool {
    let config = TokocryptoConfluenceConfig::default();
    let indicator_ids = tokocrypto_confluence_indicator_ids()
        .iter()
        .map(|id| Value::from(*id))
        .collect::<Vec<_>>();
    let indicator_count = indicator_ids.len();
    let label = format!(
        "Tokocrypto Confluence · {} of {} · Research",
        config.minimum_agreeing_indicators, indicator_count
    );
    let description = format!(
        "Long-only 15m Tokocrypto crypto-spot and tokenized-stock research model. Emit a long signal when at least {} of {} valid directional votes are long and outnumber short votes: EMA {} / {} trend, RSI {} long band {:.0}–{:.0}, prior {}-bar range breakout, and relative volume at least {:.1}× its prior {}-bar mean with matching candle direction. Missing components do not vote. Fixed {:.2}% target and {:.2}% stop. Research only; unvalidated. Crypto proxy candles are not Tokocrypto IDR execution evidence, and underlying equity candles are not token execution evidence.",
        config.minimum_agreeing_indicators,
        indicator_count,
        config.indicator_config.fast_ema_period,
        config.indicator_config.slow_ema_period,
        config.indicator_config.rsi_period,
        config.indicator_config.rsi_long_min,
        config.indicator_config.rsi_long_max,
        config.indicator_config.breakout_lookback,
        config.indicator_config.relative_volume_min,
        config.indicator_config.relative_volume_period,
        config.target_pct * 100.0,
        config.stop_loss_pct * 100.0
    );
    let versioned_model = serde_json::json!({
        "id": TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID,
        "label": label,
        "short_label": format!(
            "Tokocrypto Confluence · {}-of-{}",
            config.minimum_agreeing_indicators, indicator_count
        ),
        "icon": "📈",
        "description": description,
        "status": "research_only",
        "validation_status": "UNVALIDATED",
        "research_only": true,
        "implemented": true,
        "minimum_agreeing_indicators": config.minimum_agreeing_indicators,
        "indicator_ids": indicator_ids,
        "asset_category_ids": ["crypto_spot", "tokenized_stock"]
    });
    let Some(models) = catalog
        .get_mut("signal_strategies")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    if let Some(model) = models
        .iter_mut()
        .find(|model| catalog_string(model, "id") == Some(TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID))
    {
        if *model != versioned_model {
            *model = versioned_model;
            changed = true;
        }
    } else {
        models.push(versioned_model);
        changed = true;
    }

    let Some(profiles) = catalog
        .get_mut("plan_profiles")
        .and_then(Value::as_array_mut)
    else {
        return changed;
    };
    for (profile_id, engine_id, profile_label, research_label) in [
        (
            "tokocrypto_spot_day_trade",
            TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
            format!(
                "Tokocrypto · Crypto Spot Confluence {}-of-{} (Research)",
                config.minimum_agreeing_indicators, indicator_count
            ),
            format!(
                "Tokocrypto Confluence · {}-of-{} · Research only",
                config.minimum_agreeing_indicators, indicator_count
            ),
        ),
        (
            "tokocrypto_tokenized_stock_day_trade",
            TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID,
            format!(
                "Tokocrypto · Tokenized Stock Confluence {}-of-{} (Research)",
                config.minimum_agreeing_indicators, indicator_count
            ),
            format!(
                "Tokenized Stock Confluence · {}-of-{} · Research only",
                config.minimum_agreeing_indicators, indicator_count
            ),
        ),
    ] {
        let Some(profile) = profiles
            .iter_mut()
            .find(|profile| catalog_string(profile, "id") == Some(profile_id))
        else {
            continue;
        };
        for (key, value) in [
            ("label", Value::from(profile_label.as_str())),
            (
                "signal_strategy_id",
                Value::from(TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID),
            ),
            ("engine_strategy_id", Value::from(engine_id)),
            ("evidence_strategy_id", Value::from(engine_id)),
            ("research_label", Value::from(research_label.as_str())),
            ("validation_status", Value::from("UNVALIDATED")),
            ("execution_scope", Value::from("research_only")),
        ] {
            if profile.get(key) != Some(&value) {
                profile[key] = value;
                changed = true;
            }
        }
    }
    changed | upgrade_inherited_profile_strategies(catalog)
}

// Product profiles were cloned from these catalog-owned strategy templates.
// Retire unsupported bindings without changing app/product or evidence history.
fn upgrade_inherited_profile_strategies(catalog: &mut Value) -> bool {
    let Some(profiles) = catalog
        .get_mut("plan_profiles")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let spot = profiles
        .iter()
        .find(|profile| profile["id"] == "tokocrypto_spot_day_trade")
        .cloned();
    let futures = profiles
        .iter()
        .find(|profile| profile["id"] == "pintu_futures_confluence")
        .cloned();
    let mut changed = false;
    for profile in profiles {
        let engine = catalog_string(profile, "engine_strategy_id").unwrap_or("");
        let source = if engine == "tokocrypto_spot_day_trade" {
            spot.as_ref()
        } else if quant_core::PINTU_GOLDEN_CROSS_LEGACY_STRATEGY_IDS.contains(&engine) {
            futures.as_ref()
        } else {
            None
        };
        let Some(source) = source else {
            continue;
        };
        for key in [
            "engine_strategy_id",
            "signal_strategy_id",
            "evidence_strategy_id",
            "research_label",
            "validation_status",
            "execution_scope",
        ] {
            if let Some(value) = source.get(key) {
                if profile.get(key) != Some(value) {
                    profile[key] = value.clone();
                    changed = true;
                }
            }
        }
    }
    changed
}
