use super::catalog_string;
use serde_json::Value;
use std::collections::HashSet;

pub fn catalog_array_contains(record: &Value, key: &str, expected: &str) -> bool {
    record
        .get(key)
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
}

pub(super) fn master_data_id_set(
    catalog: &Value,
    section: &str,
) -> Result<HashSet<String>, String> {
    let records = catalog
        .get(section)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("master data section '{section}' must be an array"))?;
    let mut ids = HashSet::with_capacity(records.len());
    let id_field = if section == "assets" {
        "instrument_id"
    } else {
        "id"
    };
    for record in records {
        let id = catalog_string(record, id_field)
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| format!("every '{section}' record must have a non-empty {id_field}"))?;
        if !ids.insert(id.to_string()) {
            return Err(format!(
                "duplicate id '{id}' in master data section '{section}'"
            ));
        }
    }
    Ok(ids)
}

pub(super) fn validate_master_data_refs(
    record: &Value,
    section: &str,
    field: &str,
    allowed_ids: &HashSet<String>,
) -> Result<(), String> {
    let references = record
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("'{section}' records must contain array field '{field}'"))?;
    for reference in references {
        let id = reference
            .as_str()
            .ok_or_else(|| format!("'{section}.{field}' references must be strings"))?;
        if !allowed_ids.contains(id) {
            return Err(format!("'{section}.{field}' references unknown id '{id}'"));
        }
    }
    Ok(())
}

pub fn validate_master_data_catalog(catalog: &Value) -> Result<(), String> {
    let exchange_ids = master_data_id_set(catalog, "exchanges")?;
    let category_ids = master_data_id_set(catalog, "asset_categories")?;
    let style_ids = master_data_id_set(catalog, "trade_styles")?;
    let indicator_ids = master_data_id_set(catalog, "indicators")?;
    let signal_ids = master_data_id_set(catalog, "signal_strategies")?;
    let profile_ids = master_data_id_set(catalog, "plan_profiles")?;
    master_data_id_set(catalog, "assets")?;

    for app in master_data_records_from(catalog, "exchanges")? {
        validate_master_data_refs(app, "exchanges", "asset_category_ids", &category_ids)?;
        if let Some(default_category) = catalog_string(app, "default_asset_category_id") {
            if !category_ids.contains(default_category) {
                return Err(format!(
                    "exchange references unknown default category '{default_category}'"
                ));
            }
        }
        if let Some(default_profile) = catalog_string(app, "default_research_profile_id") {
            if !profile_ids.contains(default_profile) {
                return Err(format!(
                    "exchange references unknown default profile '{default_profile}'"
                ));
            }
        }
    }
    for category in master_data_records_from(catalog, "asset_categories")? {
        validate_master_data_refs(category, "asset_categories", "exchange_ids", &exchange_ids)?;
    }
    for model in master_data_records_from(catalog, "signal_strategies")? {
        validate_master_data_refs(model, "signal_strategies", "indicator_ids", &indicator_ids)?;
        if model.get("required_confirmation_ids").is_some() {
            validate_master_data_refs(
                model,
                "signal_strategies",
                "required_confirmation_ids",
                &indicator_ids,
            )?;
        }
        validate_master_data_refs(
            model,
            "signal_strategies",
            "asset_category_ids",
            &category_ids,
        )?;
    }
    for profile in master_data_records_from(catalog, "plan_profiles")? {
        let app_id = catalog_string(profile, "exchange_id")
            .ok_or_else(|| "plan profiles must define exchange_id".to_string())?;
        let category_id = catalog_string(profile, "asset_category_id")
            .ok_or_else(|| "plan profiles must define asset_category_id".to_string())?;
        let style_id = catalog_string(profile, "trade_style_id")
            .ok_or_else(|| "plan profiles must define trade_style_id".to_string())?;
        let signal_id = catalog_string(profile, "signal_strategy_id")
            .ok_or_else(|| "plan profiles must define signal_strategy_id".to_string())?;
        if !exchange_ids.contains(app_id)
            || !category_ids.contains(category_id)
            || !style_ids.contains(style_id)
            || !signal_ids.contains(signal_id)
        {
            return Err(format!(
                "plan profile '{}' contains an unknown relationship",
                catalog_string(profile, "id").unwrap_or("<missing>")
            ));
        }
        let app_supports_category = master_data_records_from(catalog, "asset_categories")?
            .iter()
            .find(|category| catalog_string(category, "id") == Some(category_id))
            .and_then(|category| category.get("exchange_ids"))
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(app_id)));
        let model_supports_category = master_data_records_from(catalog, "signal_strategies")?
            .iter()
            .find(|model| catalog_string(model, "id") == Some(signal_id))
            .and_then(|model| model.get("asset_category_ids"))
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(category_id)));
        if !app_supports_category || !model_supports_category {
            return Err(format!(
                "plan profile '{}' combines an unsupported app, asset category, or signal model",
                catalog_string(profile, "id").unwrap_or("<missing>")
            ));
        }
    }
    for asset in master_data_records_from(catalog, "assets")? {
        let app_id = catalog_string(asset, "exchange_id")
            .ok_or_else(|| "master assets must define exchange_id".to_string())?;
        let category_id = catalog_string(asset, "asset_category_id")
            .ok_or_else(|| "master assets must define asset_category_id".to_string())?;
        if !exchange_ids.contains(app_id) || !category_ids.contains(category_id) {
            return Err(format!(
                "asset '{}' references an unknown app or asset category",
                catalog_string(asset, "instrument_id").unwrap_or("<missing>")
            ));
        }
    }
    Ok(())
}

pub(super) fn master_data_records_from<'a>(
    catalog: &'a Value,
    section: &str,
) -> Result<&'a [Value], String> {
    catalog
        .get(section)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("master data section '{section}' must be an array"))
}
