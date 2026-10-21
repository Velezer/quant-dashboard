use super::{api_error,has_json_content_type,query_values,read_post_body,respond,RequestContext};
use crate::*;
use serde_json::json;
use std::sync::{Mutex,TryLockError};

// One heavy ticker computation at a time; public requests are rejected when busy.
static ENTRY_VALIDATION_WORK: Mutex<()> = Mutex::new(());

fn request_header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().take_while(|line| !line.is_empty()).find_map(|line| {
        let (key,value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then_some(value.trim())
    })
}

fn same_origin_request(request: &str) -> bool {
    if request_header(request,"sec-fetch-site") == Some("cross-site") { return false; }
    let Some(origin) = request_header(request,"origin") else {return true};
    let Some(host) = request_header(request,"host") else {return false};
    let authority = origin.strip_prefix("https://").or_else(|| origin.strip_prefix("http://"));
    authority.is_some_and(|value| value.trim_end_matches('/').eq_ignore_ascii_case(host))
}

pub(super) fn handle<'a>(request: RequestContext<'a>) -> Result<(),RequestContext<'a>> {
    if !matches!(request.route,"/api/entry-validation"|"/api/entry-validation/report"|"/api/entry-validation/run") {
        return Err(request);
    }
    let RequestContext {stream,db,method,path,route,request_text,..} = request;
    if route == "/api/entry-validation/run" {
        if method != "POST" { api_error(stream,"405 Method Not Allowed","Use POST with application/json to run a ticker."); return Ok(()); }
        if !has_json_content_type(request_text) { api_error(stream,"415 Unsupported Media Type","Entry validation requires application/json."); return Ok(()); }
        if !same_origin_request(request_text) { api_error(stream,"403 Forbidden","Run validation from this dashboard origin."); return Ok(()); }
        let payload: Value = match crate::wire::decode(&read_post_body(request_text)) {
            Ok(value) => value,
            Err(error) => {api_error(stream,"400 Bad Request",error);return Ok(())}
        };
        let instrument = payload.get("instrument_id").and_then(Value::as_str).unwrap_or("");
        let timeframe = payload.get("timeframe").and_then(Value::as_str).unwrap_or("");
        if instrument.is_empty() || instrument.len()>300 || entry_validation_timeframe(timeframe).is_err() {
            api_error(stream,"400 Bad Request","An exact registered instrument_id and supported timeframe are required.");return Ok(());
        }
        let _permit = match ENTRY_VALIDATION_WORK.try_lock() {
            Ok(permit) => permit,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
            Err(TryLockError::WouldBlock) => {api_error(stream,"503 Service Unavailable","Another ticker backtest is running. Retry this ticker after it finishes.");return Ok(())}
        };
        let connection = match open_db_minimal(db) {Ok(value)=>value,Err(error)=>{api_error(stream,"500 Internal Server Error",error);return Ok(())}};
        match run_entry_validation(&connection,db,instrument,timeframe) {
            Ok(report) => respond(stream,crate::wire::encode(&json!({"report":report})).unwrap_or_default(),"200 OK"),
            Err(error) => api_error(stream,"400 Bad Request",error),
        }
        return Ok(());
    }
    if method != "GET" {api_error(stream,"405 Method Not Allowed","Use GET to read saved entry evidence.");return Ok(())}
    let values = query_values(path);
    let timeframe = values.get("timeframe").map(String::as_str).unwrap_or("15m");
    if let Err(error) = entry_validation_timeframe(timeframe) {api_error(stream,"400 Bad Request",error);return Ok(())}
    let connection = match open_db_minimal(db) {Ok(value)=>value,Err(error)=>{api_error(stream,"500 Internal Server Error",error);return Ok(())}};
    if route == "/api/entry-validation/report" {
        let instrument = values.get("instrument_id").map(String::as_str).unwrap_or("");
        match entry_validation_saved(&connection,instrument,timeframe) {
            Ok(Some(report)) => respond(stream,crate::wire::encode(&json!({"report":report})).unwrap_or_default(),"200 OK"),
            Ok(None) => api_error(stream,"404 Not Found","This ticker/timeframe has no saved entry backtest yet. Run this ticker to create one."),
            Err(error) => api_error(stream,"400 Bad Request",error),
        }
        return Ok(());
    }
    let parse_page = |name: &str, fallback: usize| -> Result<usize,String> {
        values.get(name).map_or(Ok(fallback),|value| value.parse().map_err(|_| format!("{name} must be a nonnegative integer")))
    };
    let (offset,limit) = match (parse_page("offset",0),parse_page("limit",100)) {
        (Ok(offset),Ok(limit)) => (offset,limit),
        (Err(error),_)|(_,Err(error)) => {api_error(stream,"400 Bad Request",error);return Ok(())}
    };
    match entry_validation_list(&connection,values.get("exchange").map(String::as_str).unwrap_or("stockbit"),
        values.get("product").map(String::as_str),timeframe,values.get("search").map(String::as_str),offset,limit) {
        Ok(payload) => respond(stream,crate::wire::encode(&payload).unwrap_or_default(),"200 OK"),
        Err(error) => api_error(stream,"400 Bad Request",error),
    }
    Ok(())
}

#[cfg(test)]
#[path = "entry_validation_tests.rs"]
mod tests;
