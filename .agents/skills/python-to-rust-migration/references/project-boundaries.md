# Quant migration ownership boundaries

Paths below are relative to the repository root. Read [AGENTS.md](../../../../AGENTS.md), [the UI/product contract](../../../../docs/uiux-driven-development.md), and the applicable [provider contract](../../../../docs/exchange-data-providers.md). [ARCHITECTURE.md](../../../../ARCHITECTURE.md) provides an overview; verify details against current source because historical route descriptions can lag product changes.

## Existing owners

| Area | Owner | Constraint |
| --- | --- | --- |
| Dashboard | `frontend/dashboard/templates/dashboard.html`, `scripts/modules/`, `styles/` | Static shell/backend JSON; informational selection and chart recovery. |
| HTTP/composition | `backend/service/src/main.rs`, `routes.rs`, `routes/` | Existing routing/composition; inspect routes before claiming compatibility. |
| Signal flow | `frontend/dashboard/scripts/modules/signal-screener.js`, `signal-refresh.js`, backend signal modules | Timeframe identity, freshness, ranking explanations, async selection. |
| Registry/catalog | `backend/service/src/instrument_registry.rs`, `instrument_registry/`, `master_data.rs` | Registered instruments own app/product identity, never candle discovery. |
| Provider transport | `backend/service/src/market_sync.rs`, existing adapters | Application-owned typed capabilities; upstream protocols stay in adapters. |
| Market history | `backend/service/src/candle_history.rs`, `candles.rs`, `database_connections.rs` | Per-exact-symbol SQLite bars, bounded chronological reads. |
| Quant | `quant/core/src/strategies/`, `indicators/`, `risk/`, `evidence.rs`, `venue_rules.rs` | One API per rule; preserve strategy versions/evidence identity. |
| Pine | `quant/pine_runtime/`, existing browser worker | Existing calculation/transport boundary; no authoritative persisted browser state. |
| Tooling | `tooling/start.bat`, `harness.bat`, `check_source_line_limit.ps1` | Inspect commands; run applicable checks. |

Workspace members live in `Cargo.toml`. No active Python service, paper ledger, or chart sidecar is assumed. Discover the actual source workload before proposing migration.

## Storage and retirement

- `QUANT_APP_DB` owns app catalogs, registrations, retained history/evidence, scheduler cursors, and metadata.
- `QUANT_MARKET_DIR` contains per-symbol SQLite `bars`, `series`, and `ingestion_runs`; exact symbol bytes determine reversible filenames.
- `QUANT_MARKET_DB` is an offline source only. Explicit split/shard tools require a stopped service and validate destinations/checkpoints before source retirement. Never read this source at runtime.
- Retained plan history does not authorize an active plan lifecycle or removed plan API/UI.
- JSON is the structured application format. No raw provider payload persistence, duplicate chart cache, or file-backed runtime state. Browser storage contains only disposable presentation preferences.

## Cutover evidence

Compare normalization, warmup, completed-bar timing, deterministic outputs, source labels, JSON/errors, and bounded reads. Include research costs and ambiguous-bar treatment. Verify stale, partial, interrupted, and unavailable states through current consumers. Preserve the read-only dashboard and narrow R:R illustration.

Measure the same workload before/after with build profile, steady-state/peak RSS, and latency. Historical Python/sidecar figures do not establish current performance. Recover through compatible verified builds and explicit SQLite checkpoints, never hidden fallbacks or revived legacy datasets.
