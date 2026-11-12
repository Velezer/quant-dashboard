# Build memory and container startup

`blitz-peak-memory` at the end of a build measures the builder, including Rust
compilation and image work. The container's stderr and exit/OOM status identify
a subsequent startup failure. The uploaded build for `f35a65c` completed with a
builder peak of 3,773,493,248 bytes using one Cargo job and thin LTO.
After switching the build stage to `rust:1-slim-bookworm`, Blitz reported a
builder peak of 2,260,475,904 bytes (about 35% lower than the earlier
3,497,357,312-byte build). That confirms the smaller build image reduced the
host-reported peak. A subsequent GitHub Actions comparison will check whether
the higher codegen-unit count below lowers compiler RSS as well.

Clean GitHub Actions builds compared the same source with the current profile
and the bounded candidate (LTO off, sixteen codegen units, one Cargo job):

| Artifact | Previous peak compiler RSS | New peak compiler RSS | Previous / new size |
| --- | ---: | ---: | ---: |
| Native service | 940,596 KiB | 588,372 KiB | 6,484,080 / 7,222,944 bytes |
| Pine WASM | 431,208 KiB | 316,712 KiB | 441,669 / 462,576 bytes |

The service compiler peak fell 37.4%; its artifact grew 11.4%. Service build
duration was 105.27 / 107.66 seconds. This is one clean comparison on GitHub's
Ubuntu runner, recorded in workflow run `43051643474`. Compiler RSS measures
the largest compiler process; it is distinct from Blitz's whole-builder peak.
The final hosted build must confirm its own result.

The runtime image defaults `PORT` to `8080`, matching `EXPOSE`; platforms can
override it. Without this default an empty-environment container binds to the
native command's loopback development address at port 7777.

The CI container check builds the actual Dockerfile, starts it with isolated
storage under a 32 MiB cgroup limit, verifies `/health`, and repeats startup on
the same storage. Background workers are disabled for this check. Passing it
does not establish memory use with production history or overlapping scans,
backfills, and backtests. On failure, preserve container logs and inspect
`State.ExitCode` and `State.OOMKilled` before attributing the failure to memory.

Default WAL compatibility is checked before attaching persistent stores. For
SQLite `IOERR_SHMSIZE` (4874), close the failed probe and recover committed WAL
through a fresh exclusive connection before selecting DELETE for that path.
Explicit journal settings remain authoritative. Never delete WAL files to
make startup succeed.

Candle writers attach the shared app database for instrument metadata. SQLite
`BEGIN IMMEDIATE` reserves writers for every attached database, so independent
ticker updates previously contended for the app database. Main-only candle
persistence uses a deferred transaction, resolves app metadata before starting
it, and qualifies mutations with `main.`. Its first write reserves the ticker
database while another ticker or app writer can continue independently.
Signal-snapshot transactions similarly reserve the signal store with a no-op
write immediately after `BEGIN DEFERRED`. Taking that reservation before reads
retains writer serialization without locking the attached app database or
attempting a later WAL snapshot upgrade.

Registry sync, foreign-flow imports, and scoring-profile promotion write only
the app store. Their transactions now begin deferred so their first app write
does not also reserve the attached signal store. Market-history
selection and Binance stream-history checks retry transient SQLite busy/locked
errors before marking data unavailable; provider candle writes retry only the
database-lock errors after rolling back the failed attempt.

## Decision trail

1. Direct effect: reduce compiler working memory, align the image listener with
   its exposed port, and recover unsupported WAL storage before attachment.
2. Immediate consequence: smaller LLVM modules trade artifact size for lower
   memory; startup performs a bounded compatibility probe per persistent path.
3. Repeated operation: serialize journal preparation per path and verify restart using the same
   stored data rather than repeatedly changing journal mode.
4. Interactions: Docker and CI share the release profile; recovery remains in
   the existing SQLite connection owner. Main-only provider transactions avoid
   taking an unrelated writer reservation on the attached app store.
5. Accumulation: measurements stay in CI artifacts and summaries; no new
   application cache or persisted runtime store is added.
6. Failure boundaries: explicit WAL errors and unrelated lock/I/O errors must
   propagate rather than silently changing journal policy. Only transactions
   verified to write one database change lock behavior; multi-store migrations
   keep their existing atomicity.
7. Recovery: logs and OOM status identify container failures; journal checks
   verify committed-WAL retention and supported-volume behavior.
8. Governance: no execution capability, new service, or storage engine is added.
9. Longer-term hypothesis: measured builds and container checks should catch
   deployment failures earlier; subsequent host measurements must confirm this.
10. Decision: retain changes with measured compiler improvement and require
    release, journal-recovery, and container checks in CI. A startup exit
    without runtime stderr remains unconfirmed rather than presumed to be OOM.
