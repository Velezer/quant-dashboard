FROM rust:1-bookworm AS build
WORKDIR /build
RUN rustup target add wasm32-unknown-unknown
COPY . .
RUN cargo build --release --target wasm32-unknown-unknown -p quant_pine_runtime \
    && cargo build --release -p rust_chart_api

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /build/target/release/rust_chart_api /app/rust_chart_api
COPY --from=build /build/target/wasm32-unknown-unknown/release/quant_pine_runtime.wasm /app/target/wasm32-unknown-unknown/release/quant_pine_runtime.wasm
COPY frontend/dashboard /app/frontend/dashboard
ENV QUANT_MARKET_DIR=/data/market/tickers \
    QUANT_APP_DB=/data/app/app.sqlite3 \
    QUANT_SIGNAL_DB=/data/signals/signals.sqlite3 \
    QUANT_SQLITE_JOURNAL_MODE=DELETE \
    MALLOC_ARENA_MAX=2 \
    MALLOC_MMAP_THRESHOLD_=131072 \
    MALLOC_TRIM_THRESHOLD_=131072
VOLUME ["/data"]
EXPOSE 8080
CMD ["/app/rust_chart_api"]
