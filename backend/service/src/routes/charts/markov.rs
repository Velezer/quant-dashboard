use super::super::{api_error, query_values, respond, RequestContext};
use crate::*;

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(), RequestContext<'a>> {
    let RequestContext {
        stream, db, path, ..
    } = request;
    let values = query_values(path);
    let timeframe = values
        .get("timeframe")
        .filter(|value| !value.is_empty())
        .map(String::as_str)
        .unwrap_or("1d");
    let limit = values
        .get("limit")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(MAX_CANDLES)
        .clamp(30, MAX_CANDLES);
    let connection = match open_db_minimal(db) {
        Ok(connection) => connection,
        Err(error) => {
            api_error(stream, "500 Internal Server Error", error);
            return Ok(());
        }
    };
    match load_markov_payload(&connection, db, timeframe, limit, None, None, None) {
        Ok(payload) => {
            let response = match payload {
                Value::Object(mut object) => {
                    object.insert("status".into(), Value::String("ok".into()));
                    object.insert("service".into(), Value::String("rust_quant_engine".into()));
                    object.insert("version".into(), Value::String(SERVICE_VERSION.into()));
                    object.insert("timeframe".into(), Value::String(timeframe.into()));
                    Value::Object(object)
                }
                other => {
                    serde_json::json!({"status":"ok","service":"rust_quant_engine","version":SERVICE_VERSION,"timeframe":timeframe,"payload":other})
                }
            };
            // Keep the response shape a single object so the Python/API
            // gateway can forward it without a second transformation.
            respond(
                stream,
                crate::wire::encode(&response).unwrap_or_default(),
                "200 OK",
            );
        }
        Err(error) => api_error(stream, "500 Internal Server Error", error),
    }
    return Ok(());
}
