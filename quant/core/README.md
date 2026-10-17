# Quant Core

`quant_core` is the Rust calculation library used by `backend/service`. It owns
strategy signals, indicator calculations, risk rules, plan construction, and
historical evidence evaluation. The service owns HTTP, SQLite, and provider
boundaries; this crate does not persist data or make network requests.

## Modules

- `strategies`: registered Stockbit, Pintu, and intraday signal calculations.
- `indicators`: DSS Bressert and Nadaraya–Watson Envelope.
- `risk`: position sizing, venue policy, liquidity zones, and tail-risk scoring.
- `plans` and `day_trade`: plan construction and day-trade eligibility.
- `evidence`: chronological strategy evaluation and evidence gates.
- `markov`: regime classification and transition modeling.
- `venue_rules`: venue-specific instrument and price constraints.

The crate is a native Rust workspace library. Run workspace checks from the
repository root with `cargo check --workspace --locked`.
