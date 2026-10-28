//! Keep warmed Binance research-reference candle series current from public
//! kline WebSocket subscriptions. REST remains the bounded history bootstrap/fallback.
use super::{parsing::validate_bar, providers::yahoo_base_asset, shared, Bar};
use rusqlite::params;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::{connect, Message};

const BINANCE_STREAM_INTERVALS: [&str; 3] = ["15m", "1h", "1d"];
// Three intervals per pair keeps every subscription below Binance's 1,024
// stream limit while leaving room for a bounded connection wave.
const MAX_SYMBOLS_PER_CONNECTION: usize = 300;
const MAX_SYMBOLS_PER_SUBSCRIBE: usize = 20;
const MAX_ACTIVE_STREAM_CONNECTIONS: usize = 4;
const STREAM_WORKER_STACK_BYTES: usize = 512 * 1024;
const SUBSCRIBE_REQUEST_PAUSE_MILLISECONDS: u64 = 300;
const REGISTRY_REFRESH_SECONDS: u64 = 5 * 60;
const INITIAL_RECONNECT_BACKOFF_SECONDS: u64 = 1;
const MAX_RECONNECT_BACKOFF_SECONDS: u64 = 60;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum BinanceMarket {
    Spot,
    UsdmFutures,
}

impl BinanceMarket {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Spot => "wss://data-stream.binance.vision/ws",
            Self::UsdmFutures => "wss://fstream.binance.com/ws",
        }
    }

    fn source_suffix(self) -> &'static str {
        match self {
            Self::Spot => "binance_spot_proxy",
            Self::UsdmFutures => "binance_usdm_futures_proxy",
        }
    }
}

#[derive(Clone)]
struct StreamTarget {
    storage_symbol: String,
    category: String,
    source: String,
}

struct StreamBatch {
    market: BinanceMarket,
    symbols: BTreeMap<String, Vec<StreamTarget>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BinanceKlineMessage {
    Combined { data: BinanceKlineEvent },
    Raw(BinanceKlineEvent),
}

#[derive(Deserialize)]
struct BinanceKlineEvent {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "k")]
    kline: BinanceKline,
}

#[derive(Deserialize)]
struct BinanceKline {
    #[serde(rename = "t")]
    open_time_ms: u64,
    #[serde(rename = "T")]
    close_time_ms: u64,
    #[serde(rename = "i")]
    interval: String,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "x")]
    is_closed: bool,
}

pub(super) fn run_loop(market_directory: String) {
    let mut batch_cursor = 0usize;
    loop {
        let all_batches = match load_stream_batches(&market_directory) {
            Ok(batches) => batches,
            Err(error) => {
                eprintln!("[BinanceWS] Cannot load registered crypto streams: {error}");
                thread::sleep(Duration::from_secs(REGISTRY_REFRESH_SECONDS));
                continue;
            }
        };
        if all_batches.is_empty() {
            thread::sleep(Duration::from_secs(REGISTRY_REFRESH_SECONDS));
            continue;
        }
        let batches = take_batch_wave(all_batches, &mut batch_cursor);

        let refresh_deadline = Instant::now() + Duration::from_secs(REGISTRY_REFRESH_SECONDS);
        thread::scope(|scope| {
            for batch in batches {
                let market_directory = market_directory.clone();
                thread::Builder::new()
                    .name("binance-kline-stream".into())
                    .stack_size(STREAM_WORKER_STACK_BYTES)
                    .spawn_scoped(scope, move || {
                        run_stream_batch_until(&market_directory, batch, refresh_deadline)
                    })
                    .expect("spawn bounded Binance kline stream worker");
            }
        });
    }
}

fn take_batch_wave(all_batches: Vec<StreamBatch>, cursor: &mut usize) -> Vec<StreamBatch> {
    let batch_count = all_batches.len();
    if batch_count == 0 {
        return Vec::new();
    }
    let wave_size = batch_count.min(MAX_ACTIVE_STREAM_CONNECTIONS);
    let start = *cursor % batch_count;
    *cursor = start.saturating_add(wave_size) % batch_count;
    let mut batches = all_batches.into_iter().map(Some).collect::<Vec<_>>();
    (0..wave_size)
        .filter_map(|offset| batches[(start + offset) % batch_count].take())
        .collect()
}

fn load_stream_batches(market_directory: &str) -> Result<Vec<StreamBatch>, String> {
    let connection = crate::open_app_db_for_market_directory(market_directory)
        .map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT storage_symbol,underlying_symbol,asset_category_id
               FROM app.instruments
              WHERE is_active=1 AND venue_verified=1 AND storage_symbol<>''
                AND listing_status IN ('venue_active','public_listed_snapshot')
                AND asset_category_id IN ('crypto_spot','crypto_perpetual','crypto_futures')
              ORDER BY asset_category_id,storage_symbol,underlying_symbol",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    drop(connection);

    let mut symbols = BTreeMap::<BinanceMarket, BTreeMap<String, Vec<StreamTarget>>>::new();
    let mut checked_targets = HashSet::new();
    for row in rows {
        let (storage_symbol, underlying_symbol, category) = row;
        let market = match category.as_str() {
            "crypto_spot" => BinanceMarket::Spot,
            "crypto_perpetual" | "crypto_futures" => BinanceMarket::UsdmFutures,
            _ => continue,
        };
        let Ok(base) = yahoo_base_asset(&underlying_symbol) else {
            continue;
        };
        let pair = format!("{base}USDT");
        let Some(source) = shared::canonical_source(&category, market.source_suffix()) else {
            continue;
        };
        if !checked_targets.insert((storage_symbol.clone(), category.clone(), source.clone())) {
            continue;
        }
        match has_streamable_history(market_directory, &storage_symbol, &source) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => {
                eprintln!(
                    "[BinanceWS] Cannot inspect {storage_symbol} {category} stream history: {error}"
                );
                continue;
            }
        }
        let targets = symbols.entry(market).or_default().entry(pair).or_default();
        targets.push(StreamTarget {
            storage_symbol,
            category,
            source,
        });
    }

    let mut batches = Vec::new();
    for (market, market_symbols) in symbols {
        let mut batch_symbols = BTreeMap::new();
        for (symbol, targets) in market_symbols {
            batch_symbols.insert(symbol, targets);
            if batch_symbols.len() == MAX_SYMBOLS_PER_CONNECTION {
                batches.push(StreamBatch {
                    market,
                    symbols: std::mem::take(&mut batch_symbols),
                });
            }
        }
        if !batch_symbols.is_empty() {
            batches.push(StreamBatch {
                market,
                symbols: batch_symbols,
            });
        }
    }
    Ok(batches)
}

fn has_streamable_history(
    market_directory: &str,
    storage_symbol: &str,
    source: &str,
) -> Result<bool, String> {
    let Some(connection) = crate::open_market_history_for_symbol(
        market_directory,
        storage_symbol,
        false,
    )
    .map_err(|error| error.to_string())?
    else {
        return Ok(false);
    };
    connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM series s
                  WHERE s.symbol=?1 AND s.interval='15m' AND s.source=?2
                    AND s.exchange_id=?3 AND s.product_id=''
                    AND s.data_provider=?4
                    AND EXISTS(SELECT 1 FROM bars b WHERE b.symbol=s.symbol
                               AND b.interval=s.interval AND b.source=s.source)
             )",
            params![storage_symbol, source, shared::SOURCE_EXCHANGE_ID, "binance"],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn run_stream_batch_until(market_directory: &str, batch: StreamBatch, deadline: Instant) {
    let mut backoff_seconds = INITIAL_RECONNECT_BACKOFF_SECONDS;
    while Instant::now() < deadline {
        match consume_stream_batch(market_directory, &batch, deadline) {
            Ok(()) => backoff_seconds = INITIAL_RECONNECT_BACKOFF_SECONDS,
            Err(error) => {
                eprintln!(
                    "[BinanceWS] {:?} {}-symbol stream disconnected: {error}",
                    batch.market,
                    batch.symbols.len()
                );
                thread::sleep(
                    Duration::from_secs(backoff_seconds)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
                backoff_seconds = backoff_seconds
                    .saturating_mul(2)
                    .min(MAX_RECONNECT_BACKOFF_SECONDS);
            }
        }
    }
}

fn consume_stream_batch(
    market_directory: &str,
    batch: &StreamBatch,
    deadline: Instant,
) -> Result<(), String> {
    let (mut socket, _) = connect(batch.market.endpoint()).map_err(|error| error.to_string())?;
    let symbol_keys = batch.symbols.keys().collect::<Vec<_>>();
    for (request_index, symbols) in symbol_keys.chunks(MAX_SYMBOLS_PER_SUBSCRIBE).enumerate() {
        let streams = symbols
            .iter()
            .flat_map(|symbol| {
                BINANCE_STREAM_INTERVALS.iter().map(move |interval| {
                    format!("{}@kline_{interval}", symbol.to_ascii_lowercase())
                })
            })
            .collect::<Vec<_>>();
        let subscription = serde_json::json!({
            "method":"SUBSCRIBE",
            "params":streams,
            "id":request_index.saturating_add(1)
        });
        socket
            .send(Message::Text(subscription.to_string().into()))
            .map_err(|error| error.to_string())?;
        if request_index + 1 < symbol_keys.len().div_ceil(MAX_SYMBOLS_PER_SUBSCRIBE) {
            thread::sleep(Duration::from_millis(SUBSCRIBE_REQUEST_PAUSE_MILLISECONDS));
        }
    }
    eprintln!(
        "[BinanceWS] Connected {:?} kline stream for {} registered pairs.",
        batch.market,
        batch.symbols.len()
    );

    while Instant::now() < deadline {
        let message = socket.read().map_err(|error| error.to_string())?;
        match message {
            Message::Text(payload) => {
                if let Some(error) = subscription_error(&payload) {
                    eprintln!("[BinanceWS] {:?} subscription rejected: {error}", batch.market);
                    continue;
                }
                let Some((symbol, interval, bar)) = parse_completed_kline(&payload) else {
                    continue;
                };
                let Some(targets) = batch.symbols.get(&symbol) else {
                    continue;
                };
                for target in targets {
                    if let Err(error) = persist_stream_candle(
                        market_directory,
                        target,
                        &symbol,
                        &interval,
                        &bar,
                    ) {
                        eprintln!(
                            "[BinanceWS] Failed to save {symbol} {interval} ({:?}): {error}",
                            batch.market
                        );
                    }
                }
            }
            Message::Ping(_) => socket.flush().map_err(|error| error.to_string())?,
            Message::Close(frame) => {
                return Err(format!("server closed stream: {frame:?}"));
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_completed_kline(payload: &str) -> Option<(String, String, Bar)> {
    let message: BinanceKlineMessage = serde_json::from_str(payload).ok()?;
    let event = match message {
        BinanceKlineMessage::Combined { data } => data,
        BinanceKlineMessage::Raw(event) => event,
    };
    if event.event_type != "kline" || !event.kline.is_closed {
        return None;
    }
    let duration_seconds = match event.kline.interval.as_str() {
        "15m" => 15 * quant_core::SECONDS_PER_MINUTE,
        "1h" => quant_core::SECONDS_PER_HOUR,
        "1d" => crate::SECONDS_PER_DAY,
        _ => return None,
    };
    let duration_ms = u64::try_from(duration_seconds)
        .ok()?
        .saturating_mul(quant_core::MILLISECONDS_PER_SECOND as u64);
    if duration_ms == 0
        || event.kline.open_time_ms % duration_ms != 0
        || event.kline.close_time_ms.saturating_add(1)
            != event.kline.open_time_ms.saturating_add(duration_ms)
    {
        return None;
    }
    let open = event.kline.open.parse::<f64>().ok()?;
    let high = event.kline.high.parse::<f64>().ok()?;
    let low = event.kline.low.parse::<f64>().ok()?;
    let close = event.kline.close.parse::<f64>().ok()?;
    let volume = event.kline.volume.parse::<f64>().ok()?;
    validate_bar(open, high, low, close, volume).ok()?;
    let timestamp_seconds = i64::try_from(
        event.kline.open_time_ms / quant_core::MILLISECONDS_PER_SECOND as u64,
    )
    .ok()?;
    Some((
        event.symbol.to_ascii_uppercase(),
        event.kline.interval,
        Bar {
            ts: timestamp_seconds,
            open,
            high,
            low,
            close,
            volume,
        },
    ))
}

fn subscription_error(payload: &str) -> Option<String> {
    let message: serde_json::Value = serde_json::from_str(payload).ok()?;
    let code = message.get("code").and_then(serde_json::Value::as_i64)?;
    let text = message
        .get("msg")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("provider rejected the subscription");
    Some(format!("Binance error {code}: {text}"))
}

fn persist_stream_candle(
    market_directory: &str,
    target: &StreamTarget,
    pair: &str,
    interval: &str,
    bar: &Bar,
) -> Result<(), String> {
    let Some(connection) = crate::open_market_history_for_symbol(
        market_directory,
        &target.storage_symbol,
        false,
    )
    .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let series_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM series
                           WHERE symbol=?1 AND interval=?2 AND source=?3
                             AND exchange_id=?4 AND data_provider=?5)",
            params![
                target.storage_symbol,
                interval,
                target.source,
                shared::SOURCE_EXCHANGE_ID,
                "binance"
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !series_exists {
        return Ok(());
    }
    super::persist(
        &connection,
        &target.storage_symbol,
        &target.category,
        shared::SOURCE_EXCHANGE_ID,
        "binance",
        &target.source,
        pair,
        interval,
        "binance_usdt_reference",
        std::slice::from_ref(bar),
    )
}
