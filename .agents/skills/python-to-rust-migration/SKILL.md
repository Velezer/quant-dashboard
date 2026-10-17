---
name: python-to-rust-migration
description: Migrate Python quantitative or data-processing behavior into existing Rust boundaries with parity, bounded memory, explicit cutover, and measured performance. Use for migration work, not routine Rust edits.
---

# Python to Rust migration

Read [AGENTS.md](../../../AGENTS.md) and [project boundaries](references/project-boundaries.md). This repository is already a Rust workspace. Do not recreate a retired Python service or add crates/services without architecture approval. Use this skill when an actual Python workload or offline oracle is part of the requested migration.

## Establish the permitted target

Inventory the real source, callers, input/output semantics, persistence, and existing Rust owner. Preserve permitted timestamps, ordering, rounding, missing/NaN policy, error semantics, provenance, and explainability. Do not preserve prohibited paper APIs or duplicate caches for compatibility. Describe required retirement/conversion before implementation.

Record material ten-order effects, acceptance, and recovery before changing behavior, architecture, persisted data, or financial meaning. For new service/crate/storage boundaries, prepare the proposal and wait for explicit approval; continue independent parity discovery.

## Port bounded slices

1. Establish small deterministic fixtures and dedicated tests. Offline/synthetic test fixtures are not runtime application state. Compare typed values with justified numeric tolerances and event ordering exactly.
2. Port reads/validation, indicators, strategies, and evidence into existing owners in dependency order. Use typed application ports at integration boundaries and isolate provider protocols in adapters.
3. Preserve completed-bar timing, warmup, source/strategy identity, and research cost assumptions. Historical fill models are research evidence, not account ledgers.
4. Bound histories, concurrency, queues, and payloads. Keep canonical per-symbol SQLite bars, the single app database, and JSON structured payloads.
5. Update consumers through existing composition. Verify parity/APIs before cutover. Python may remain an offline oracle, never a hidden production fallback after cutover.
6. Retire obsolete production paths after validation. Data conversions are explicit maintainer-run, transactional/idempotent, recorded/validated in SQLite, and never implicit at startup. Discard rejected noncanonical datasets according to the contract.

## Validate and recover

Use dedicated test files and the source-size guard. Run relevant parity, financial-cost, missing/stale/partial input, and endpoint checks. Before cutover, run the project harness plus applicable formatting, lint, and release-build checks. Inspect current toolchain/commands rather than adding validation dependencies unnecessarily.

For changed interfaces, apply the UI/UX contract and verify in a fresh browser session. Report workload/build profile, cold/warm latency, steady-state/peak RSS, and storage growth when performance is part of the outcome. Compare equivalent workloads against 16 MB steady state; idle chart-only measurements are not full-service evidence.

Recover with the last verified compatible Rust build and authorized schema/checkpoint information. Prepare explicit data recovery for irreversible conversion; never restore a prohibited runtime or hidden fallback. Capture a mismatch fixture and dedicated regression check before retrying. Report passed gates and unverified cutover limits.
