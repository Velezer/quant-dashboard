use super::{chart_market_bars, read_initial_request};
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    time::{Duration, Instant},
};

#[test]
fn idle_connection_does_not_hold_serial_request_loop() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();

    let started = Instant::now();
    assert!(read_initial_request(&mut server, Duration::from_millis(50))
        .unwrap()
        .is_none());
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(client);
}

#[test]
fn initial_request_bytes_are_preserved() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    client
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();

    let request = read_initial_request(&mut server, Duration::from_secs(1))
        .unwrap()
        .unwrap();
    assert!(request.starts_with("GET /health HTTP/1.1\r\n"));
}

#[test]
fn chart_market_bars_contain_only_ohlcv_data() {
    let mut candle = crate::new_candle(1_700_000_000_000_000_000, 10.0, 12.0, 9.0, 11.0, 42.0);
    candle.dss = Some(55.0);
    candle.trigger = Some(50.0);
    candle.gradient = Some(5.0);
    candle.accel = Some(1.0);
    candle.indicators = Some(crate::ChartIndicatorValues::default());

    let bars = chart_market_bars(&[candle]);
    let bar = &bars[0];
    assert_eq!(bar["time"], 1_700_000_000_000_000_000_i64);
    assert_eq!(bar["open"], 10.0);
    assert_eq!(bar["high"], 12.0);
    assert_eq!(bar["low"], 9.0);
    assert_eq!(bar["close"], 11.0);
    assert_eq!(bar["volume"], 42.0);
    for key in ["dss", "trigger", "gradient", "accel", "indicators"] {
        assert!(
            bar.get(key).is_none(),
            "unexpected {key} in chart market bar"
        );
    }
}
