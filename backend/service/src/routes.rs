use super::*;

#[derive(Debug)]
pub(super) struct RequestQuery {
    pub(super) symbol: String,
    pub(super) timeframe: String,
    pub(super) limit: i64,
    pub(super) strategy: String,
    pub(super) venue: String,
    pub(super) initial_equity: f64,
    pub(super) entry_price: Option<f64>,
    pub(super) stop_loss: Option<f64>,
    pub(super) available_cash: Option<f64>,
    pub(super) atr: Option<f64>,
    pub(super) leverage: Option<f64>,
    pub(super) win_rate: Option<f64>,
    pub(super) payoff: Option<f64>,
    pub(super) sizing_model: Option<String>,
    pub(super) current_equity: Option<f64>,
    pub(super) peak_equity: Option<f64>,
    pub(super) asset_class: String,
    pub(super) direction: String,
    pub(super) dss: DssConfig,
}

fn respond(mut stream: TcpStream, body: String, status: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(format!("{head}{body}").as_bytes());
}

fn respond_html(mut stream: TcpStream, body: String, status: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(format!("{head}{body}").as_bytes());
}

fn api_error(stream: TcpStream, status: &str, message: impl std::fmt::Display) {
    let payload = serde_json::json!({"error": message.to_string(), "candles": []});
    respond(
        stream,
        super::wire::encode(&payload).unwrap_or_default(),
        status,
    );
}

fn chart_market_bars(candles: &[Candle]) -> Vec<serde_json::Value> {
    candles
        .iter()
        .map(|candle| {
            serde_json::json!({
                "time": candle.time,
                "open": candle.open,
                "high": candle.high,
                "low": candle.low,
                "close": candle.close,
                "volume": candle.volume,
            })
        })
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = |digit: u8| match digit {
                b'0'..=b'9' => Some(digit - b'0'),
                b'a'..=b'f' => Some(digit - b'a' + 10),
                b'A'..=b'F' => Some(digit - b'A' + 10),
                _ => None,
            };
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        decoded.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn query_values(path: &str) -> HashMap<String, String> {
    let mut values = HashMap::<String, String>::new();
    if let Some(raw_query) = path.split_once('?').map(|(_, query)| query) {
        for item in raw_query.split('&') {
            let mut parts = item.splitn(2, '=');
            let key = parts.next().unwrap_or("").trim();
            let value = percent_decode(parts.next().unwrap_or("").trim());
            if !key.is_empty() {
                values.insert(key.to_string(), value);
            }
        }
    }
    values
}

pub(super) fn canonical_asset_class(venue: &str, requested: &str) -> String {
    match venue.trim().to_lowercase().as_str() {
        "stockbit" | "stockbit_idx" | "idx" => "idx".to_string(),
        "tokocrypto"
        | "tokocrypto_spot"
        | "tokenized_stock"
        | "tokenized_stock_spot"
        | "tokocrypto_tokenized_stock" => "tokocrypto".to_string(),
        "pintu_spot" => "pintu_spot".to_string(),
        "pintu" | "pintu_futures" | "pintu_perpetual" => "crypto_futures".to_string(),
        _ => requested.to_lowercase(),
    }
}

pub(super) fn parse_query(path: &str) -> RequestQuery {
    let values = query_values(path);
    let parse_usize = |key: &str, fallback: usize| {
        values
            .get(key)
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(fallback)
    };
    let parse_f64 = |key: &str, fallback: f64| {
        values
            .get(key)
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .unwrap_or(fallback)
    };
    let parse_optional_f64 = |key: &str| {
        values
            .get(key)
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
    };
    let symbol = values
        .get("symbol")
        .cloned()
        .unwrap_or_default()
        .to_uppercase();
    let strategy = values
        .get("strategy")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            if symbol.ends_with(".JK") {
                "idx_stockbit_day_trade".to_string()
            } else {
                "pintu_futures_day_trade".to_string()
            }
        });
    let venue = values
        .get("exchange")
        .or_else(|| values.get("venue"))
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            if symbol.ends_with(".JK") {
                "idx".to_string()
            } else {
                "pintu_futures".to_string()
            }
        });
    let requested_asset_class = values
        .get("asset_class")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            if symbol.ends_with(".JK") {
                "idx".to_string()
            } else if venue.to_lowercase().contains("spot") {
                "crypto_spot".to_string()
            } else {
                "crypto_futures".to_string()
            }
        });
    let asset_class = canonical_asset_class(&venue, &requested_asset_class);
    let limit = values
        .get("limit")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(200)
        .clamp(1, MAX_CANDLES);
    RequestQuery {
        symbol,
        timeframe: values
            .get("timeframe")
            .filter(|value| !value.is_empty())
            .cloned()
            .unwrap_or_else(|| DEFAULT_TIMEFRAME.to_string()),
        limit,
        strategy,
        venue,
        initial_equity: parse_f64("initial_equity", 100_000.0),
        entry_price: parse_optional_f64("entry_price"),
        stop_loss: parse_optional_f64("stop_loss"),
        available_cash: parse_optional_f64("available_cash"),
        atr: parse_optional_f64("atr"),
        leverage: parse_optional_f64("leverage"),
        win_rate: parse_optional_f64("win_rate"),
        payoff: parse_optional_f64("payoff"),
        sizing_model: values.get("sizing_model").cloned(),
        current_equity: parse_optional_f64("current_equity"),
        peak_equity: parse_optional_f64("peak_equity"),
        asset_class,
        direction: values
            .get("direction")
            .filter(|value| !value.is_empty())
            .cloned()
            .unwrap_or_else(|| "LONG".to_string()),
        dss: DssConfig {
            pds: parse_usize("pds", 10),
            ema_len: parse_usize("ema_len", 9),
            trigger_len: parse_usize("trigger_len", 5),
            oversold: parse_f64("oversold", 22.0),
            overbought: parse_f64("overbought", 78.0),
        },
    }
}

fn serve_static(mut stream: TcpStream, file_path: &str) {
    if file_path.to_ascii_lowercase().ends_with(".json") {
        let body = b"Data files are not served; use the JSON API.";
        let head = format!(
            "HTTP/1.1 410 Gone\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body);
        return;
    }
    match fs::read(file_path) {
        Ok(bytes) => {
            let content_type = if file_path.ends_with(".html") {
                "text/html; charset=utf-8"
            } else if file_path.ends_with(".js") {
                "application/javascript"
            } else if file_path.ends_with(".css") {
                "text/css"
            } else if file_path.ends_with(".wasm") {
                "application/wasm"
            } else {
                "application/octet-stream"
            };
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
                bytes.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&bytes);
        }
        Err(_) => {
            let body = b"Not Found";
            let head = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
        }
    }
}

fn redirect(mut stream: TcpStream, location: &str) {
    let head = format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let _ = stream.write_all(head.as_bytes());
}

fn respond_status(mut stream: TcpStream, body: String, status: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(format!("{head}{body}").as_bytes());
}

fn read_initial_request(
    stream: &mut TcpStream,
    timeout: std::time::Duration,
) -> std::io::Result<Option<String>> {
    stream.set_read_timeout(Some(timeout))?;
    const MAX_REQUEST_BYTES: usize = 64 * 1024;
    const MAX_REQUEST_BODY_BYTES: usize = 56 * 1024;
    let mut request = Vec::with_capacity(8 * 1024);
    let mut chunk = [0u8; 8 * 1024];
    let mut expected_bytes = None;
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(bytes_read) => request.extend_from_slice(&chunk[..bytes_read]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break
            }
            Err(error) => return Err(error),
        }
        if request.len() > MAX_REQUEST_BYTES {
            write_request_too_large(stream);
            return Ok(None);
        }
        if expected_bytes.is_none() {
            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|position| position + 4)
                .or_else(|| {
                    request
                        .windows(2)
                        .position(|window| window == b"\n\n")
                        .map(|position| position + 2)
                });
            if let Some(header_end) = header_end {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.trim()
                            .eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                if content_length > MAX_REQUEST_BODY_BYTES {
                    write_request_too_large(stream);
                    return Ok(None);
                }
                expected_bytes = Some(header_end.saturating_add(content_length));
            }
        }
        if expected_bytes.is_some_and(|expected| request.len() >= expected) {
            break;
        }
        if expected_bytes.is_none() && request.len() >= MAX_REQUEST_BYTES {
            break;
        }
    }
    if request.is_empty() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&request).into_owned()))
}

fn write_request_too_large(stream: &mut TcpStream) {
    let body = r#"{"error":"Request exceeds the bounded 56 KB body limit."}"#;
    let response = format!("HTTP/1.1 413 Payload Too Large\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
    let _ = stream.write_all(response.as_bytes());
}

pub(super) fn reject_service_unavailable(mut stream: TcpStream) {
    if !matches!(
        read_initial_request(&mut stream, std::time::Duration::from_millis(250)),
        Ok(Some(_))
    ) {
        return;
    }
    let body = r#"{"error":"Server is handling its bounded request capacity. Retry shortly."}"#;
    let response = format!(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json; charset=utf-8\r\nRetry-After: 1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    );
    let _ = stream.write_all(response.as_bytes());
}

mod calculations;
mod charts;
mod control;
mod market;
mod pine;
mod rule_simulator;

struct RequestContext<'a> {
    stream: TcpStream,
    db: &'a str,
    state: &'a Arc<AppState>,
    remote_is_loopback: bool,
    request_text: &'a str,
    method: &'a str,
    path: &'a str,
    route: &'a str,
}

pub(super) fn handle(mut stream: TcpStream, db: &str, state: &Arc<AppState>) {
    let remote_is_loopback = stream
        .peer_addr()
        .map(|address| address.ip().is_loopback())
        .unwrap_or(false);
    // A browser may open a speculative TCP connection before sending a request.
    // Since ordinary requests are handled serially to keep memory bounded, an
    // idle connection must not block every subsequent API request indefinitely.
    let request_text =
        match read_initial_request(&mut stream, std::time::Duration::from_millis(250)) {
            Ok(Some(request)) => request,
            Ok(None) | Err(_) => return,
        };
    let line = request_text.lines().next().unwrap_or("");
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("");
    let route = path.split('?').next().unwrap_or("");

    // Handle CORS preflight
    if method == "OPTIONS" {
        let head = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(head.as_bytes());
        return;
    }

    let request = RequestContext {
        stream,
        db,
        state,
        remote_is_loopback,
        request_text: &request_text,
        method,
        path,
        route,
    };
    let request = match rule_simulator::handle(request) {
        Ok(()) => return,
        Err(request) => request,
    };
    let request = match control::handle(request) {
        Ok(()) => return,
        Err(request) => request,
    };
    let request = match pine::handle(request) {
        Ok(()) => return,
        Err(request) => request,
    };
    let request = match market::handle(request) {
        Ok(()) => return,
        Err(request) => request,
    };
    let request = match charts::handle(request) {
        Ok(()) => return,
        Err(request) => request,
    };
    let _ = calculations::handle(request);
}

/// Extract POST body from raw HTTP request text (everything after the blank line \r\n\r\n).
fn read_post_body(request_text: &str) -> String {
    if let Some(pos) = request_text.find("\r\n\r\n") {
        request_text[pos + 4..].trim_end_matches('\0').to_string()
    } else if let Some(pos) = request_text.find("\n\n") {
        request_text[pos + 2..].trim_end_matches('\0').to_string()
    } else {
        String::new()
    }
}

fn request_header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request
        .lines()
        .take_while(|line| !line.is_empty())
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then_some(value.trim())
        })
}

pub(super) fn same_origin_request(request: &str) -> bool {
    if request_header(request, "sec-fetch-site") == Some("cross-site") {
        return false;
    }
    let Some(origin) = request_header(request, "origin") else {
        return false;
    };
    let Some(host) = request_header(request, "host") else {
        return false;
    };
    let authority = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"));
    authority.is_some_and(|value| value.trim_end_matches('/').eq_ignore_ascii_case(host))
}

#[cfg(test)]
#[path = "routes_tests.rs"]
mod tests;
