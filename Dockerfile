# syntax=docker/dockerfile:1.7
FROM rust:1-bookworm AS build
WORKDIR /build
RUN --mount=type=secret,id=proxy_ca,required=false \
    if [ -f /run/secrets/proxy_ca ]; then \
        SSL_CERT_FILE=/run/secrets/proxy_ca rustup target add wasm32-unknown-unknown; \
    else \
        rustup target add wasm32-unknown-unknown; \
    fi
COPY . .
RUN --mount=type=secret,id=proxy_ca,required=false \
    if [ -f /run/secrets/proxy_ca ]; then \
        SSL_CERT_FILE=/run/secrets/proxy_ca CARGO_HTTP_CAINFO=/run/secrets/proxy_ca \
            cargo build --release --target wasm32-unknown-unknown -p quant_pine_runtime \
            && SSL_CERT_FILE=/run/secrets/proxy_ca CARGO_HTTP_CAINFO=/run/secrets/proxy_ca \
                cargo build --release -p rust_chart_api; \
    else \
        cargo build --release --target wasm32-unknown-unknown -p quant_pine_runtime \
            && cargo build --release -p rust_chart_api; \
    fi

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
    QUANT_SIGNAL_DB=/data/signals/signals.sqlite3
EXPOSE 8080
CMD ["/app/rust_chart_api"]
