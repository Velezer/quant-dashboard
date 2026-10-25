use super::super::{api_error, query_values, respond, RequestContext};
use crate::*;

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(), RequestContext<'a>> {
    let RequestContext {
        stream, db, path, ..
    } = request;
    let values = query_values(path);
    let connection = match open_db_minimal(db) {
        Ok(connection) => connection,
        Err(error) => {
            api_error(stream, "500 Internal Server Error", error);
            return Ok(());
        }
    };
    let storage_symbol = if let Some(requested) = values
        .get("storage_symbol")
        .filter(|value| !value.trim().is_empty())
    {
        let exchange = values
            .get("exchange")
            .or_else(|| values.get("venue"))
            .map(String::as_str)
            .unwrap_or("");
        let product_id = values.get("product_id").map(String::as_str).unwrap_or("");
        match resolve_registered_candle_storage_symbol(
            &connection,
            exchange,
            product_id,
            requested,
        ) {
            Ok(Some(resolved)) => Some(resolved),
            Ok(None) => {
                api_error(
                    stream,
                    "400 Bad Request",
                    "storage_symbol is not an active registered ticker for this app",
                );
                return Ok(());
            }
            Err(error) => {
                api_error(stream, "500 Internal Server Error", error);
                return Ok(());
            }
        }
    } else {
        match values.get("symbol") {
            Some(symbol) if !symbol.trim().is_empty() => {
                let exchange = values.get("exchange").or_else(|| values.get("venue"));
                let asset_class = values.get("asset_class").map(String::as_str);
                let resolved = if let Some(exchange) = exchange.filter(|value| !value.is_empty()) {
                    resolve_candle_storage_symbol(&connection, exchange, symbol, asset_class)
                } else {
                    resolve_candle_storage_symbol_any_exchange(&connection, symbol, asset_class)
                };
                match resolved {
                    Ok(storage_symbol) => Some(storage_symbol),
                    Err(error) => {
                        api_error(stream, "400 Bad Request", error);
                        return Ok(());
                    }
                }
            }
            _ => None,
        }
    };
    match load_storage_stats(&connection, db, storage_symbol.as_deref()) {
        Ok(payload) => respond(
            stream,
            crate::wire::encode(&payload).unwrap_or_default(),
            "200 OK",
        ),
        Err(error) => api_error(stream, "500 Internal Server Error", error),
    }
    return Ok(());
}
