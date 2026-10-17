# Harness engineering

The dashboard treats product claims as executable contracts. A change is ready when the harness can prove the important boundaries through the same service surface that the UI uses.

Run `tooling/start.bat` first so the local Rust service is available, then run:

```text
tooling/harness.bat
```

The harness has three layers:

1. `tooling/check_source_line_limit.ps1` rejects source files over 1,000 nonblank, non-comment-only lines.
2. `cargo test --workspace` checks the Rust data, strategy, storage, and route behavior.
3. A fresh browser session checks the rendered dashboard interaction and evidence contracts.

The Rust and browser checks verify that:

- health advertises both the historical research engine and chart bundle;
- every trade plan identifies its app exchange, asset category, holding style,
  actual signal model, and evidence status;
- Stockbit remains bound to IDX, Pintu Futures remains bound to crypto futures, and Tokocrypto exposes separate crypto-spot and tokenized-stock categories;
- research reads SQLite history, never performs provider sync, and stays `RESEARCH_ONLY`;
- multibagger discovery scans beyond the registered seeds, ranks the stored IDX daily universe, and keeps `MONTHLY_RETURN_TARGET_MET` behind the 100% complete-calendar-month account-return gate;
- tokenized-stock rows keep their real underlying symbol;
- Tokocrypto crypto-spot research excludes tokenized-stock symbols and uses the stored USD pair;
- an incompatible app/category mapping returns a clear HTTP 400 instead of silently remapping the request;
- stored `15m` and `1d` chart series contain bars, are not marked stale, and are inside their freshness contract;
- the dashboard source keeps the exchange-first research controls, read-only labeling, tokenized-stock wording, and live status region.

The dashboard has no simulated-order or live-order flow. Keep thresholds tied to the product contract rather than to a single captured date so a future stale-data regression fails for the right reason.
