use serde::de::DeserializeOwned;
use serde::Serialize;

/// Structured application messages use ordinary JSON text.
pub(super) fn encode<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

pub(super) fn decode<T: DeserializeOwned>(body: &str) -> Result<T, String> {
    serde_json::from_str(body.trim()).map_err(|error| error.to_string())
}
