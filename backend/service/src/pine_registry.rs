use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Deserialize)]
pub(super) struct RegisterRequest {
    pub name: String,
    pub source: String,
    pub author: String,
    pub source_reference: String,
    pub rights_basis: String,
    pub license_or_permission: String,
    pub attribution: String,
    pub rights_confirmed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct IndicatorRecord {
    pub id: i64,
    pub name: String,
    pub source: String,
    pub author: String,
    pub source_reference: String,
    pub rights_basis: String,
    pub license_or_permission: String,
    pub attribution: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct IndicatorSummary {
    pub id: i64,
    pub name: String,
    pub author: String,
    pub source_reference: String,
    pub rights_basis: String,
    pub license_or_permission: String,
    pub attribution: String,
    pub created_at: i64,
}

pub(super) type StrategyRecord = IndicatorRecord;
pub(super) type StrategySummary = IndicatorSummary;

#[derive(Debug)]
pub(super) enum RegistryError {
    Invalid(String),
    Storage(rusqlite::Error),
}

impl From<rusqlite::Error> for RegistryError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error)
    }
}

pub(super) fn initialize_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS app.pine_indicators (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        source TEXT NOT NULL,
        author TEXT NOT NULL,
        source_reference TEXT NOT NULL,
        rights_basis TEXT NOT NULL CHECK(rights_basis IN ('original','permission','license')),
        license_or_permission TEXT NOT NULL,
        attribution TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS app.idx_pine_indicators_created ON pine_indicators(created_at DESC, id DESC);
    CREATE TABLE IF NOT EXISTS app.pine_strategies (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        source TEXT NOT NULL,
        author TEXT NOT NULL,
        source_reference TEXT NOT NULL,
        rights_basis TEXT NOT NULL CHECK(rights_basis IN ('original','permission','license')),
        license_or_permission TEXT NOT NULL,
        attribution TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS app.idx_pine_strategies_created ON pine_strategies(created_at DESC, id DESC);")
}

pub(super) fn validate_source(source: &str) -> quant_pine_runtime::Validation {
    quant_pine_runtime::validate_source(source)
}

pub(super) fn register(
    connection: &Connection,
    request: RegisterRequest,
) -> Result<IndicatorRecord, RegistryError> {
    validate_source_kind(&request, "indicator")?;
    register_record(connection, request, "pine_indicators")
}

pub(super) fn register_strategy(
    connection: &Connection,
    request: RegisterRequest,
) -> Result<StrategyRecord, RegistryError> {
    validate_source_kind(&request, "strategy")?;
    register_record(connection, request, "pine_strategies")
}

fn validate_source_kind(
    request: &RegisterRequest,
    required_kind: &str,
) -> Result<(), RegistryError> {
    validate_request(request)?;
    let validation = validate_source(&request.source);
    if !validation.valid {
        let message = validation
            .diagnostics
            .iter()
            .map(|diagnostic| {
                format!(
                    "line {}:{} {}",
                    diagnostic.line, diagnostic.column, diagnostic.message
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        return Err(RegistryError::Invalid(message));
    }
    if validation.script_type != required_kind {
        let previous_rule = if required_kind == "indicator" {
            "strategy()"
        } else {
            "indicator()"
        };
        return Err(RegistryError::Invalid(format!("This registration path requires a Pine `{required_kind}()` declaration; `{previous_rule}` source belongs in its own registry.")));
    }
    Ok(())
}

fn register_record(
    connection: &Connection,
    request: RegisterRequest,
    table: &str,
) -> Result<IndicatorRecord, RegistryError> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(RegistryError::Invalid(
            "Pine script name is required.".to_string(),
        ));
    }
    if name.len() > 96 {
        return Err(RegistryError::Invalid(
            "Pine script name must be 96 characters or fewer.".to_string(),
        ));
    }
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64;
    let sql = format!(
        "INSERT INTO app.{table} (name, source, author, source_reference, rights_basis, license_or_permission, attribution, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    );
    connection.execute(
        &sql,
        params![
            name,
            request.source,
            request.author.trim(),
            request.source_reference.trim(),
            request.rights_basis,
            request.license_or_permission.trim(),
            request.attribution.trim(),
            created_at
        ],
    )?;
    let id = connection.last_insert_rowid();
    Ok(IndicatorRecord {
        id,
        name: name.to_string(),
        source: request.source,
        author: request.author.trim().to_string(),
        source_reference: request.source_reference.trim().to_string(),
        rights_basis: request.rights_basis,
        license_or_permission: request.license_or_permission.trim().to_string(),
        attribution: request.attribution.trim().to_string(),
        created_at,
    })
}

fn validate_request(request: &RegisterRequest) -> Result<(), RegistryError> {
    if request.source.len() > quant_pine_runtime::MAX_SOURCE_BYTES {
        return Err(RegistryError::Invalid(
            "Pine source exceeds the runtime registration limit.".to_string(),
        ));
    }
    if !request.rights_confirmed {
        return Err(RegistryError::Invalid("Confirm that you own the source or have rights to adapt and use it before registration.".to_string()));
    }
    if request.author.trim().is_empty() {
        return Err(RegistryError::Invalid(
            "Author is required for rights provenance.".to_string(),
        ));
    }
    if request.source_reference.trim().is_empty() {
        return Err(RegistryError::Invalid(
            "Source URL or provenance reference is required.".to_string(),
        ));
    }
    if request.attribution.trim().is_empty() {
        return Err(RegistryError::Invalid(
            "Record required attribution, or enter `None required by the applicable terms`."
                .to_string(),
        ));
    }
    if !matches!(
        request.rights_basis.as_str(),
        "original" | "permission" | "license"
    ) {
        return Err(RegistryError::Invalid(
            "Rights basis must be original work, permission, or a license.".to_string(),
        ));
    }
    if request.rights_basis != "original" && request.license_or_permission.trim().is_empty() {
        return Err(RegistryError::Invalid(
            "Record the license terms or permission basis.".to_string(),
        ));
    }
    if request.source.trim().is_empty() {
        return Err(RegistryError::Invalid(
            "Pine source is required.".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn list(
    connection: &Connection,
    requested_id: Option<i64>,
) -> Result<Vec<IndicatorRecord>, rusqlite::Error> {
    list_from_table(connection, requested_id, "pine_indicators")
}

pub(super) fn list_strategies(
    connection: &Connection,
    requested_id: Option<i64>,
) -> Result<Vec<StrategyRecord>, rusqlite::Error> {
    list_from_table(connection, requested_id, "pine_strategies")
}

fn list_from_table(
    connection: &Connection,
    requested_id: Option<i64>,
    table: &str,
) -> Result<Vec<IndicatorRecord>, rusqlite::Error> {
    let Some(id) = requested_id else {
        return Ok(Vec::new());
    };
    let sql = format!("SELECT id, name, source, author, source_reference, rights_basis, license_or_permission, attribution, created_at FROM app.{table} WHERE id=?1");
    let record = connection.query_row(&sql, [id], map_row).optional()?;
    Ok(record.into_iter().collect())
}

pub(super) fn list_summaries(
    connection: &Connection,
) -> Result<Vec<IndicatorSummary>, rusqlite::Error> {
    list_summaries_from_table(connection, "pine_indicators")
}

pub(super) fn list_strategy_summaries(
    connection: &Connection,
) -> Result<Vec<StrategySummary>, rusqlite::Error> {
    list_summaries_from_table(connection, "pine_strategies")
}

fn list_summaries_from_table(
    connection: &Connection,
    table: &str,
) -> Result<Vec<IndicatorSummary>, rusqlite::Error> {
    let sql = format!("SELECT id, name, author, source_reference, rights_basis, license_or_permission, attribution, created_at FROM app.{table} ORDER BY created_at DESC, id DESC LIMIT 100");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok(IndicatorSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            author: row.get(2)?,
            source_reference: row.get(3)?,
            rights_basis: row.get(4)?,
            license_or_permission: row.get(5)?,
            attribution: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;
    rows.collect()
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndicatorRecord> {
    Ok(IndicatorRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        source: row.get(2)?,
        author: row.get(3)?,
        source_reference: row.get(4)?,
        rights_basis: row.get(5)?,
        license_or_permission: row.get(6)?,
        attribution: row.get(7)?,
        created_at: row.get(8)?,
    })
}

#[cfg(test)]
#[path = "pine_registry_tests.rs"]
mod tests;
