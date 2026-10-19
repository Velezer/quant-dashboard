# Exchange data providers

The dashboard is app-first at the classification layer. It chooses the app
exchange first, then the instrument category, and finally the data provider.
Tokocrypto's public adapter is approved for bounded selected-asset candle sync.
It decodes the official response in memory, validates completed OHLCV rows, and
persists only typed records in SQLite. Hourly and daily sync select active
registered tickers across exchanges. When the native-to-proxy policy allows
research sync, the bounded proxy adapter uses an ordered fallback chain and
stops at the first successful current source for each interval. Binance is the
first crypto reference. Coinbase Spot, Bybit Spot/Linear, OKX Spot, and Yahoo
Finance are fallbacks where each provider lists the mapped pair. Eligible
equities and tokenized-stock underlyings use Yahoo Finance. Each attempted or
successful provider retains its own series and diagnostics; the app exchange
and data provider stay distinct. Proxy bars remain labeled as research proxies.

## Feasibility decision

| App | Official access found | What can be used | Decision |
| --- | --- | --- | --- |
| Tokocrypto | Public REST and WebSocket documentation | Symbols, klines, trades, order book, live streams; private account/order data needs an API key | Native public candle adapter enabled |
| Pintu | First-party public Retail and Pro listing endpoints; separate partner market-data access | Daily Retail Spot, Pro Spot, and Pro Futures discovery; no Pintu-native candles yet | App listing adapter enabled; candle proxies remain research-only |
| Stockbit | No public complete Stockbit ticker feed or candle API was found; the official IDX listed-company directory is publicly readable | The approved IDX directory snapshot supplies the ticker universe for the Stockbit app choice; Yahoo `.JK` candles stay research-only | Registry provenance identifies IDX, and proxy prices do not establish Stockbit execution availability |
| Pluang | Official pages document live IDX price, bid/ask, volume, a 5-minute chart, and order book in the app, but no public developer API was found | Requires a partner/data agreement or an approved export | Scraping is not enabled; terms prohibit automated extraction without written permission |
| Research reference APIs | Binance, Coinbase, Bybit, and OKX publish public candle endpoints | Used only for external crypto reference series; ticker coverage varies by provider | These series do not confirm app availability, venue liquidity, or execution eligibility |

“No public API was found” is a feasibility result, not proof that a private
commercial integration cannot exist. The Tokocrypto adapter remains limited to
public symbol discovery and completed candles; account, order, and execution
endpoints are out of scope.

## Stockbit 15-minute free-source decision — 2026-10-02

**Outcome:** Keep Yahoo Finance as the best-effort, research-only `.JK` candle
fallback and fix its request window. Use a 30-day window for equities and a
7-day window for 24/7 crypto, retaining the 1,000-row parser cap. This does not
meet a strict live-price guarantee: Yahoo lists IDX data as 10-minute delayed
and provides no application API freshness promise. Binance is already the
first crypto research source after Tokocrypto's native adapter; its data
remains a Binance reference series, never Tokocrypto-native evidence.

The provider comparison found no free source that guarantees fresh 15-minute
candles for the complete registered IDX universe. Yahoo's published coverage
lists IDX (`.JK`) with a 10-minute delay. The runtime's 60-day Yahoo 15-minute
query can exceed the adapter's 1,000-row cap, which rejects the whole response.
The narrower window reduces response size while retaining recent history; the
parser still rejects any response above its cap. Yahoo remains an explicitly
labeled research proxy without a service-level promise.

Alternatives do not meet the current coverage and request-volume contract:

- The IDX Data Services portal offers subscribed real-time, delayed, and
  historical products. Its FAQ directs subscribers to a fee schedule, says
  direct feeds use a subscriber connection, and identifies free IDX website
  data as end-of-day. IDX's website terms prohibit scraping and crawling, so
  its web pages are not an adapter fallback.
- Twelve Data's Basic plan allows 8 credits per minute and 800 per day and
  lists real-time equities for US markets; wider market coverage is paid.
- Alpha Vantage's free tier allows 25 requests per day, and its documentation
  marks intraday series as premium. That quota is too small for a 15-minute
  refresh across registered Stockbit instruments.
- EODHD's Free tier does not include intraday stock prices.

Decision effects, scaled to the data and reliability impact:

1. **Direct effect:** Yahoo 15-minute stock requests use 30 days and crypto
   requests use 7 days to reduce response volume. Acceptance requires bounded
   completed candles and fresh `last_success` data; the parser rejects any
   response above its 1,000-row cap.
2. **Immediate consequence:** Recent-history coverage is shorter for these
   Yahoo series; signal generation still uses the same indicators, timeframes,
   and minimum-bar checks.
3. **Repeated operation:** Each scheduler sweep continues to request only the
   recent window and upsert those candles; provider throttling remains visible
   as an ingestion error.
4. **Interactions:** `exchange_id=stockbit`, `.JK` symbol mapping, and
   `data_provider=yfinance` stay distinct. Proxy rows cannot validate Stockbit
   execution or be mixed with venue-native data.
5. **Accumulation:** Existing SQLite bars remain canonical and are upserted by
   timestamp. This change adds no cache, store, or pruning behavior.
6. **Failure boundary:** Missing, malformed, oversized, or incomplete provider
   responses still fail closed; stale candles do not produce current signals.
7. **Recovery:** The ingestion error and last successful timestamp remain the
   recovery evidence. Repeated failure means keep the screen stale and seek a
   licensed feed; do not widen the response beyond the parser bound.
8. **Governance:** No new provider, API key, license, data redistribution right,
   or scraping path is assumed by this change.
9. **Longer-term hypothesis:** Yahoo may continue to serve useful research
   candles, but undocumented endpoint changes or rate limits may invalidate
   that assumption; repeated stale rows are the trigger to reconsider.
10. **Decision and learning:** The smallest reversible path is to bound the
    existing Yahoo requests. If a live run still returns over-bound or stale
    data, disable that source for current signals and pursue an approved feed.

**Sources:** [Yahoo Finance market coverage and delays](https://help.yahoo.com/kb/finance-for-web/article-exchanges-data-delays-sln2310.html),
[IDX Data Services](https://data.idx.co.id/),
[IDX Data Services FAQ](https://data.idx.co.id/faq.php?p=sFWjtRbH1z2wag4),
[IDX Terms of Use](https://idx.co.id/id/syarat-penggunaan/),
[Twelve Data pricing](https://twelvedata.com/pricing),
[Alpha Vantage usage limits](https://www.alphavantage.co/support/),
[Alpha Vantage API documentation](https://www.alphavantage.co/documentation/),
and [EODHD pricing](https://eodhd.com/pricing).

## Historical Windows market-data transport recovery — 2026-10-02 (superseded)

**Observed failure:** The app's Windows `curl.exe` request to Yahoo failed with
Schannel `CRYPT_E_NO_REVOCATION_CHECK`. The same bounded `.JK` 15-minute query
with `--ssl-revoke-best-effort` returned HTTP 200. Binance Spot's public
`data-api.binance.vision` request returned HTTP 200, while Binance USDⓈ-M
Futures' documented `fapi.binance.com` request reset the connection even with
that option. Spot candles are not substituted for the Pintu Futures product.

At that time, the research-candle transport added
`--ssl-revoke-best-effort` on Windows and fell back to PowerShell. The service
has since moved these requests to the Rust `ureq` HTTP client. That change
removes the runtime dependency on `curl` and PowerShell, keeps bounded request
timeouts and response sizes, and uses the platform certificate verifier with
strict TLS validation. It does not implement curl's best-effort behavior for
offline revocation checks. A TLS validation failure remains a failed request;
stored history remains visible and current signals stay withheld until fresh
data is available.

Decision effects for this transport adjustment:

1. **Direct effect:** The historical workaround could allow Windows
   research-candle requests when only a revocation lookup was unreachable.
   That behavior no longer describes the active transport.
2. **Immediate consequence:** Yahoo `.JK` could supply recent research
   candles under the historical workaround. All providers remain subject to
   bounded response parsing and freshness checks.
3. **Repeated operation:** The existing per-symbol SQLite upsert and scheduled
   refresh cadence are unchanged; provider errors remain recorded on failure.
4. **Interactions:** The historical option applied at the shared
   research-candle boundary on Windows only. The current Rust transport does
   not relabel proxy data or change venue and instrument identity.
5. **Accumulation:** No extra database, cache, retry loop, or response memory
   is added; the fixed response bound remains in effect.
6. **Failure boundary:** The active Rust transport fails closed on TLS errors,
   oversized responses, malformed candles, and connection failures.
7. **Recovery:** Failed ingestion leaves the UI stale and keeps current signals
   withheld. Once certificate validation can complete, subsequent refreshes
   resume normal ingestion.
8. **Governance:** The transport adds no credentials, scraping, new provider,
   or redistribution right.
9. **Longer-term hypothesis:** If Windows revocation endpoints are unreachable,
   requests may fail until network access or the certificate environment is
   repaired.
10. **Decision and learning:** The Rust client removes executable availability
    as a runtime dependency. Binance Futures resets remain separate network
    failures and are not hidden by a Spot-data fallback.

**Source:** [curl TLS revocation options](https://curl.se/docs/manpage.html#--ssl-revoke-best-effort).

## 15-minute screener refresh and scan path — 2026-10-02

**Observed failure:** A live run of the 15-minute scheduler processed 195
registered products per batch in 156–168 seconds with 16 workers. The active
universe had 2,911 products, so a 15-batch sweep would take about 39–42 minutes
before daily work. At the same time, the scan had not completed after 10 minutes
and process private memory reached 19,771,392 bytes, above the 16 MiB target.
The proxy adapter was also refreshing every eligible crypto source after a
successful Binance response. A live Yahoo `.JK` 30-day request returned 864
timestamps; its final timestamp was an unaligned in-progress bar, which caused
the parser to reject otherwise valid completed history.

The scanner now reports the current symbol and percentage, keeps historical
evidence scratch data scoped to one registered product, omits non-directional
watchlist rows from the screener path, and computes the six-frame DSS diagnostic
only for an actual directional signal. Stockbit and other products use the same
configured confluence strategy independently on each enabled frame; the extra
Stockbit swing and day-trade research-plan generation is not part of the signal
screener. Yahoo parsing discards a still-open timestamp before alignment
validation, while completed historical candles with invalid alignment still
fail closed. Crypto refresh tries Binance first and stops after the first
successful provider; fallbacks remain separately labeled and are tried only
when earlier sources fail.

Decision effects:

1. **Direct effect:** The 15-minute path avoids repeated research-provider
fetches after a preferred source succeeds and avoids building non-signal rows.
Acceptance is a completed full scan and market-data sweep within the 900-second
cadence, with process private memory at or below 16 MiB.
2. **Immediate consequence:** Secondary proxy series may become older while the
selected successful Binance series stays current. All six enabled timeframes
still run the same strategy rules and retain separate timeframe identity.
3. **Repeated operation:** On each refresh, the adapter checks Binance first;
an unavailable source falls through to the next eligible source. The scan
records its current instrument and progress for monitoring.
4. **Interactions:** Tokocrypto native history remains first. Pintu, Stockbit,
and other proxy data keep their app product, provider label, and research-only
classification. Signals still require fresh 15-minute price data.
5. **Accumulation:** Existing SQLite series and ingestion history are preserved.
The change adds no cache, file state, or second store; stale fallback series are
not deleted or relabeled.
6. **Failure boundary:** A connection error, oversized response, malformed
closed candle, stale 15-minute price, or incomplete timeframe continues to
withhold current signals. An unaligned open Yahoo bar is discarded because it
is not completed market history.
7. **Recovery:** `/api/status` exposes scan progress and current symbol;
`/api/signals` exposes stale/error status and the selected source. If the full
sweep still exceeds 900 seconds or private memory exceeds 16 MiB, the next
recovery step is to reduce scheduler work per target or stream candidates into
SQLite in bounded batches.
8. **Governance:** This follows the user's Binance-first and screener-only
instructions. It adds no provider entitlement, venue-native claim, account,
order, or trade-history behavior.
9. **Longer-term hypothesis:** First-success fallback and directional-only
processing should reduce request time and retained memory. A live run that
still misses its interval or memory bound disproves that expectation.
10. **Decision and learning:** This is the smallest reversible change that
removes confirmed redundant work while preserving source labels and scoring.
Keep it only if live duration, memory, freshness, and signal provenance meet
the stated acceptance checks; otherwise continue with bounded incremental
scanning.

**Live evidence and provider limits:** One Pintu Retail Spot registration
received 999 Binance Spot 15-minute candles in the initial run. This is a
Binance research reference, not Pintu-native evidence. The documented Binance
USDⓈ-M Futures endpoint reset its connection in this environment, so Spot data
is not substituted for futures candles. Yahoo lists IDX data as 10-minute
delayed; its feed is a research fallback and cannot establish an exact live
price guarantee.

**Sources:** [Binance Spot REST API](https://developers.binance.com/en/docs/products/spot/rest-api),
[Binance USDⓈ-M Futures market data](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-usd-s-m-futures/api/rest-api/market-data),
and [Yahoo Finance market coverage and delays](https://help.yahoo.com/kb/finance-for-web/article-exchanges-data-delays-sln2310.html).

## Plug-and-play provider contract

Use Ports and Adapters at the existing backend service boundary. The ordered
adapter registry and application-owned `CandleSyncAdapter` port live in
`backend/service/src/market_sync.rs`; Tokocrypto native and research-proxy
adapters implement that contract. An application-owned typed port describes
one consumed capability; a provider adapter translates that contract to the
upstream protocol and returns application-owned typed records. Keep HTTP
details, credentials, upstream schemas, and protocol-specific parsing inside
the adapter. Do not pass raw provider payloads to consumers or through
application APIs.

Keep capabilities narrow. Candle adapters implement bounded candle reads;
listing adapters implement approved listing discovery. Add separate ports for
other read-only market-data capabilities only when the product has an approved
use for them. An adapter is not required to implement unrelated capabilities.
The dashboard has no account, order-placement, or execution adapter.

Every adapter declares the app products, instrument categories, symbols, and
timeframes it supports. It validates provider identity and returned values at
the boundary, including completed-candle status, chronological order, OHLCV
consistency, and request limits. Returned records preserve `venue`, `provider`,
`instrument_type`, `source_timestamp`, `received_timestamp`, `timezone`,
`price_basis`, and `data_quality` where applicable. Missing, malformed,
unsupported, rate-limited, or stale data produces an explicit typed failure or
unavailable result; it must not be converted into fabricated or apparently
valid data.

Wire adapters at one explicit registry or selection point in the existing
composition path. Select by the active instrument registration, app product,
declared adapter capabilities, and the approved source policy. Keep app
`exchange_id` and product identity separate from upstream `data_provider`.
Provider discovery or candle data alone cannot register an app instrument or
create a plan. Avoid provider-specific branches throughout consumers.

Fallbacks must be explicitly allowed and ordered for the instrument category.
Try a research proxy after a venue-native source is unavailable or fails; stop
that native-to-proxy chain after native success. Within the research-proxy
adapter, try the preferred source first and fall through only when it fails;
keep every source in its own labeled series and record each attempt. Never
splice native and proxy candles into one series, present a proxy as venue-native
liquidity, or let a proxy satisfy venue execution evidence.
Provider calls and response sizes remain bounded, and failures must leave the
remaining registered instruments eligible for sync.

For each new adapter, add dedicated sibling `*_tests.rs` contract tests for
normalization, malformed and unsupported responses, bounds, provenance, and
fallback behavior. Keep tests outside production source files. Verify the
corresponding native, proxy, stale, empty, failed, and recovered chart states
in a fresh browser session before enabling a new candle source.

## Recommended rollout

1. Keep the Tokocrypto public candle adapter bounded and typed. Keep normal
   crypto spot and tokenized stocks as separate app products and categories;
   use symbol discovery to classify tokenized stocks and do not assume every
   tokenized symbol has the same endpoint or trading rules.
2. Keep the Pintu listing adapter bounded and fail closed on incomplete product
   responses. Pintu-native candles still require an approved market-data
   contract. Futures validation requires the venue's contract metadata, mark
   price, funding, fees, and liquidation rules, not only OHLC candles.
3. Keep Stockbit, Pluang, and Pintu without a native candle adapter until an
   approved candle API or export is provided. Stockbit's approved registry
   universe comes from the official IDX listed-company directory; it is not a
   Stockbit-issued availability feed. Pluang remains snapshot-only and is not
   scraped. Use supported public crypto reference markets or mapped Yahoo
   history for research, clearly labeled as proxies.
4. Keep Binance, Coinbase, Bybit, OKX, and Yahoo sync research-only, bounded,
   and explicitly labeled.
   Their coverage must not be treated as native venue liquidity or execution
   evidence.

## Current provider state

The Rust service syncs app-confirmed tickers and reports listing attempts,
stale status, 15-minute/daily candle coverage, screener status, and provider
errors in SQLite diagnostics. Tokocrypto native candles and proxy candles
retain distinct source labels. Stockbit, Pluang, and Pintu still have no native
candle provider adapter. When research collection is allowed, registered
crypto spot instruments use Binance first and fall through to Coinbase, Bybit,
OKX, or Yahoo only when needed; perpetual/futures instruments use Binance,
Bybit, and Yahoo as eligible references. Eligible equities and tokenized-stock
underlyings use Yahoo. Each successful source keeps its own series label.
These rows remain research evidence, not venue-native execution evidence.
`series.exchange_id` identifies the app exchange and `series.data_provider`
identifies the upstream candle provider.

The market-data worker runs independently of dashboard heartbeats. It refreshes
registered-product `15m` and `1h` candles in a rotating batch sized to cover the
current registry in twelve scheduler ticks, rounded up. Each bounded wave has
at most 512 products and at most 10 concurrent workers. With 2,911 active
products, a tick attempts 243 products. Batch duration and refreshed interval
count are logged. Each product keeps its own SQLite history file and
app-product cursor. Intraday work keeps its one-minute cadence even when the
daily queue is busy. A shared per-symbol gate serializes intraday and daily
writes to the same history file. The database gate admits ten regular market
connections and reserves one additional connection for the signal scan, so a
provider request cannot consume the scanner's only path to SQLite.

Signals are regenerated on a fixed 15-minute schedule, independent of
dashboard activity. The screener creates separate rows for `15m`, `1h`, `2h`,
`4h`, `1d`, and `1w` using the same scoring rules; each row retains its own
timeframe identity. A `15m` bar older than 30 minutes by its open timestamp is
stale; the scan withholds it until fresh history is available. Proxy-origin
rows retain their research label.

The service drains daily history on one bounded background worker, separate
from the intraday scheduler. It attempts up to 12 app-product/storage-symbol
pairs every five minutes, saves completed `1d` bars into the canonical SQLite
`bars` table, and records provider, source, row count, and result in
`ingestion_runs`. At 2,911 products this cadence covers the daily queue in
about 20 hours when providers respond; failed daily attempts remain visible and
are eligible again on the next UTC day. The queue resumes from SQLite after a
service restart. The worker shares the per-symbol gate with intraday sync and
never starts a second daily batch while its current batch is running.

Signal scans check freshness for the registered app product and strategy
timeframe using the existing candle freshness threshold. If stored candles are
stale, the scan withholds strategy and indicator signals and does not emit a
`WAIT`/`MONITOR_ONLY` plan row. Previously active plans that depend on that
timeframe are retired until fresh history is available. The chart continues to
show its candle freshness and recovery state. The existing market-data worker
gives one stale product per timeframe first place in its bounded batch; the
priority matches exchange, product, and storage symbol so products sharing a
symbol keep their own history. A completed refresh triggers a rescan when that
exact product/timeframe is fresh. A failed or still-stale refresh leaves the
chart freshness state visible and cannot create a signal from old candles.
Daily stale-priority retries are limited to one per hour, while the ordinary
daily backlog remains once per UTC day.

Signal-alert lifecycles are refreshed by each scheduled scan. The screener
snapshot is replaced after a successful scan, and rows expire after the
two-hour signal lifetime; expired alerts and alerts detected more than two
hours ago are removed from the app SQLite database. The screen carries ranking,
source, timeframe, and freshness state without a trade-plan lifecycle.

## Implemented venue-first candlestick flow

Tokocrypto's `type` field selects a market system and candle route; it does not
classify an asset as a tokenized stock. The registry identifies tokenized
stocks by matching an enabled USDT listing to the configured tokenized-stock
catalog. Other enabled IDR and USDT listings are crypto spot instruments. If a
ticker has multiple quote markets, the registry keeps one deterministic app
identity and prefers IDR for crypto spot.

**User outcome:** For a selected Stockbit, Tokocrypto, Pluang, or Pintu
instrument, keep the app and product identity fixed while displaying the
available completed candle series. Tokocrypto uses its native public adapter;
after native data is unavailable or fails, eligible crypto research collection
attempts Binance, Coinbase, Bybit, OKX, and Yahoo independently. Crypto
perpetual/futures collect Binance, Bybit, and Yahoo. Eligible stock mappings
and tokenized-stock underlyings use Yahoo. The Stockbit app choice uses the
approved IDX listed-company directory for its ticker universe, while the candle
series remains an independent research proxy. IDX provenance is stored
explicitly and does not establish Stockbit execution availability. Each proxy
series is labeled and remains ineligible as venue-native execution evidence.
Native Stockbit, Pluang, and Pintu feeds remain pending approved access
contracts.

Trade plans are created only for active tickers in the app instrument registry.
The plan's `Exchange` and strategy come from that registration; `Data_Source`,
`Data_Provider`, and `Data_Symbol` describe the independently selected candle
series. A provider response or previously stored price series cannot register
an app ticker or create a trade plan. An explicit manual app registration is
marked unverified until a listing source confirms it.

| Selected app | Native candle source | Current integration gate | Fallback / research sources |
| --- | --- | --- | --- |
| Tokocrypto | Public symbol-type-specific kline endpoint | Enabled for supported, discovered symbols; symbol type and quote pair are validated | After native failure: Binance Spot, Coinbase Spot, Bybit Spot, OKX Spot, and Yahoo independently for crypto; Yahoo underlying reference for tokenized stocks. Chart preference: Binance, Coinbase, Bybit, OKX, Yahoo. |
| Stockbit | No native candle feed configured | Approved IDX directory snapshot supplies the stock ticker universe; it is not a Stockbit-issued availability list | Yahoo `.JK` candles for stocks; research-only |
| Pluang | No native feed configured | Approved feed required for the exact product and price basis; scraping requires written consent | Binance, Coinbase, Bybit, OKX, and Yahoo for eligible crypto spot; Binance, Bybit, and Yahoo for perpetuals; Yahoo for eligible equity mappings |
| Pintu Retail Spot / Pro Spot / Pro Futures | No native candle feed configured | First-party app listings confirm product eligibility; venue-native execution validation still requires an approved candle and product contract | Spot: Binance, Coinbase, Bybit, and OKX. Futures: Binance USDⓈ-M and Bybit Linear. Yahoo is also attempted; none represents Pintu venue pricing |

The Pintu app registry refresh reads the bounded first-party
[`Retail market listing`](https://api.pintu.co.id/v2/trade/price-changes),
[`Pro Spot symbols`](https://api.pintu.pro/v1/public/get-symbols-reference), and
[`Pro Futures tickers`](https://api.pintu.pro/perps/v1/public/get-tickers). A
failed or incomplete response retains the last verified product listing and
records the failure in SQLite. `USDT-PX` symbols are duplicate contract variants
only when the matching `USDT-PERP` symbol exists; the perpetual identity is
used for plans. Retail Spot is confirmed by its own crypto market listing,
independently of Pro Spot. Pluang uses its dated saved app snapshot; no automated Pluang listing
collection runs. Pintu and Pluang market data uses clearly labeled Binance,
Coinbase, Bybit, OKX, or Yahoo research references as applicable.

Stockbit refresh reads the full server-rendered table from the public [IDX
Company Profiles directory](https://www.idx.co.id/en/listed-companies/company-profiles/).
It checks the embedded record total against the extracted unique tickers and
requires at least 500 validated symbols before reconciling SQLite. Failed,
truncated, or inconsistent responses retain the previous listing. The selected
app identity remains `stockbit`; registration provenance records IDX. This
registry source confirms IDX listing, not Stockbit product availability or
execution eligibility. Old sitemap registrations are reconfirmed by the IDX
snapshot before remaining active. Tokocrypto registration writes separate `tokocrypto_spot` and
`tokocrypto_tokenized_stock` products. Its native adapter uses the same
canonical storage-symbol key as the registry. Listing failures are visible in
the app status row and retry after 15 minutes.

On an existing database, a maintainer runs
`cargo run -p rust_chart_api -- --migrate-app-products` once before starting the
service. The transaction rekeys Pintu Futures and Pluang registrations and
related plan metadata while retaining canonical `bars` and their source labels.
`--refresh-pintu-listings` performs a bounded manual refresh, and the service
also refreshes Pintu once daily. On a fresh clone, run
`tooling/regather-market-data.bat` to explicitly refresh approved public
registries and drain bounded daily/hourly candle batches into the active SQLite
database. The command is safe to rerun, honors `QUANT_MARKET_DIR` and
`QUANT_APP_DB`, and does not fetch during startup. For an existing legacy
database, stop the service and run `--split-market-database` when app-owned
tables remain, then run `--shard-market-database` before starting the service.
Neither conversion runs during startup.
On other platforms, run:

```powershell
cargo run --release -p rust_chart_api -- --regather-market-data
```

For one active, verified Stockbit IDX listing, a maintainer may fetch only its
bounded 15-minute Yahoo research series and then run `--scan-once`:

```powershell
cargo run -p rust_chart_api -- --sync-research-15m=BBCA
cargo run -p rust_chart_api -- --scan-once
```

Replace `BBCA` with the registered ticker. This command refuses symbols that
are not active, verified Stockbit listings, writes the validated result to that
symbol's canonical SQLite database, and keeps the Yahoo research-proxy label.
It does not register a ticker or establish Stockbit execution support.

`--scan-once` rebuilds stored research plans from app-confirmed registrations
and qualifying SQLite candles.

### Source selection and recovery

1. The scheduler selects active registered instruments by app and storage
   symbol. The registry's `underlying_symbol` identifies the research mapping;
   IDX tickers map to `.JK`, while eligible crypto products map to a Binance
   USDT market, Coinbase USD spot pair, Bybit/OKX USDT spot pair, and a Yahoo
   USD symbol. Bybit Linear is available to eligible perpetual/futures
   products. A Pintu perpetual proxy is an
   international-exchange reference, not Pintu contract pricing.
2. Tokocrypto attempts its bounded native adapter first for 15-minute, hourly,
   and daily candles. Only when the native source is unavailable or fails does
   the research-proxy adapter run; that adapter attempts every eligible
   research source independently. Crypto spot research can retain Binance,
   Coinbase, Bybit, OKX, and Yahoo references; perpetual/futures research can
   retain Binance, Bybit, and Yahoo. Tokenized-stock references use Yahoo
   underlying prices, and eligible equities use Yahoo.
3. `bars`, `series`, and `ingestion_runs` carry source identity, app
   `exchange_id`, and upstream `data_provider`. Chart and
   evidence reads select one source for the selected app; they do not read
   another app's native or proxy rows. A fresher app proxy may be selected when
   the app's native series has passed its staleness threshold. When both
   research providers have the requested history, chart preference is Binance,
   Coinbase, Bybit, OKX, then Yahoo. Native and proxy candles are never spliced
   together.
4. Diagnostics include source label, latest completed candle time, freshness,
   coverage, and sync attempts. The chart keeps the selected app identity while
   it shows native, proxy, stale, empty, failed, or recovered history. Proxy
   candles remain research-only; unknown-source candles cannot pass the
   venue-history evidence gate.

### Storage and migration boundary

The migration changes the `bars` primary key to `(symbol, interval, source,
ts)` and the `series` key to `(symbol, interval, source)` in the active SQLite
database. Existing bars inherit their prior series source in a transaction;
known `yfinance` rows are labeled as fallback and row counts are checked before
the old tables are retired. Provider payloads are parsed in memory, while
typed bars and per-source ingestion attempts remain in SQLite. No second chart
cache or file-backed runtime state is introduced. APIs use normalized JSON
messages with the `application/json` content type.

### Rollout and acceptance

1. Source-partitioned storage, migration, source-aware chart reads, and
   source-aware evidence selection are implemented.
2. Tokocrypto native-to-proxy fallback remains ordered and stops after native
   success. After native failure, the research-proxy adapter collects every
   eligible Binance, Coinbase, Bybit, OKX, and Yahoo series independently;
   eligible equity mappings use Yahoo. Chart selection still uses one source
   at a time, with provenance visible.
3. Add each remaining native adapter only after its official or licensed feed
   contract is available, with product, symbol, timeframe, and permissions
   validated at the provider boundary.
4. Verify native, proxy, stale, empty, failed, and recovered charts in a fresh
   browser session before enabling any new native adapter. Proxy rows cannot
   validate venue execution evidence.

**Rollback signal:** A proxy overwrites a native candle, a chart combines
sources without disclosure, an app/product identity changes on fallback, or a
proxy contributes to venue-native evidence. Disable that adapter and retain
the last verified SQLite series until corrected.

### Public feasibility references

- [Tokocrypto API documentation](https://www.tokocrypto.com/apidocs/) defines
  separate kline endpoints by symbol type.
- [Pintu Institutional](https://pintu.co.id/institutional) describes API
  access through a separate application; published material does not establish
  a public futures-candle contract.
- [Stockbit datafeed information](https://help.stockbit.com/id/article/apa-itu-biaya-datafeed-data-fee-kc4dhl/)
  describes in-app live data, not a public candle API.
- [IDX Company Profiles](https://www.idx.co.id/en/listed-companies/company-profiles/)
  is the public listed-company directory used as the Stockbit-choice ticker
  universe; its data remains attributed to IDX.
- [Pluang IDX market-data FAQ](https://pluang.com/en/faq/indonesian-stocks/indonesian-stocks-information/apakah-pluang-menyediakan-data-harga-saham-secara-real-time)
  describes an in-app 5-minute chart, not a public developer candle API.
- [Stockbit Terms](https://stockbit.com/terms) restrict data mining, robots,
  spiders, and similar collection tools without prior written consent.
- [Pluang BSC Terms](https://pluang.com/tnc/bsc-terms) prohibit scraping,
  automated collection, and data extraction without written permission.
- [Pintu BTC market page](https://pintu.co.id/market/btc) attributes public
  price data to CoinGecko; it is not a Pintu Pro native candle API.
- [Binance Spot REST API](https://developers.binance.com/en/docs/products/spot/rest-api)
  documents the public market-data endpoints used for crypto references.
- [Binance USDⓈ-M Futures market data](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-usd-s-m-futures/api/rest-api/market-data)
  documents public futures klines. These represent Binance markets, not Pintu
  contracts.
- [Coinbase Exchange product candles](https://docs.cdp.coinbase.com/api-reference/exchange-api/rest-api/products/get-product-candles)
  documents public USD candle reads with a 300-candle request cap.
- [Bybit V5 klines](https://bybit-exchange.github.io/docs/v5/market/kline)
  documents public Spot and linear-contract candles with a 1,000-row request
  cap. The adapter tries Bybit's documented Indonesia endpoint first, then its
  documented global alternate and default endpoint when a host cannot be
  reached. These remain one Bybit research source.
- [OKX market candles](https://app.okx.com/docs-v5/en/#rest-api-market-data-get-candlesticks)
  documents public Spot candle reads with a 300-row request cap.

## Stockbit multibagger use case

Stockbit-choice tickers come from the approved IDX listed-company directory or
an explicitly unverified manual app registration. IDX listing identity does
not guarantee availability in Stockbit. Observing a `.JK` price series no longer
registers the ticker. The earlier history-derived registrations, including
`TRUK.JK`, are retired transactionally while their candles stay in SQLite.

## 15-minute cadence and provider-host follow-up — 2026-10-02

The four-connection market-history gate had limited a 40-thread scheduler to
four concurrent provider requests, while the scanner shared the same permits.
The initial live tuning capped the candle worker pool at eight, allowed eight
regular market database leases, and reserved one lease for signal scans. The existing
per-symbol write gate and SQLite-only storage remain authoritative.

The live 2,911-product sweep completed in 13m22s with that pool. A completed
signal scan took 5m08s; the next scheduled scan began on the 15-minute boundary
and completed in 3m29s. Private-memory samples peaked at 14.4 MiB, below the
16 MiB limit. The first audit found 2,594 products with a fresh `15m` series
and 317 with recent provider errors. Several Pintu Pro Futures examples had
Bybit's default host reset the connection while Yahoo had no mapped ticker.

Bybit's official integration guide lists `api.bybit.id` for Indonesia and
`api.bytick.com` as a mainnet alternate. Direct public `15m` linear klines
returned HTTP 200 from both hosts while `api.bybit.com` reset this connection.
The existing Bybit adapter now retries those documented hosts on transport or
response-shape failures, keeps the `bybit` provider label, and does not claim
Binance USDⓈ-M data when that endpoint fails. The new adapter tests pass, and
the live service stored Pintu Pro Futures `BTC-USD` `15m` and `1h` bars from
`pintu_futures_bybit_linear_proxy`.

Decision effects:

1. **Direct effect:** Eight bounded sync workers and one reserved scan lease
allow full refreshes and scans to proceed without competing for the same four
connection permits. Acceptance is a complete registry sweep under 900 seconds,
scan completion on its 15-minute schedule, provider-labelled new futures bars,
and private memory at or below 16 MiB.
2. **Immediate consequence:** More provider requests can be active at once; the
database gate still caps connections, SQLite remains the only persisted store,
and the scanner retains one reserved connection.
3. **Repeated operation:** The scheduler rotates across the registered
universe; Bybit retries only its documented hosts after request/response
failures, while valid Bybit candles stop the host chain.
4. **Interactions:** The per-symbol writer gate still serializes daily and
intraday writes. Scanner reads retain product and timeframe identity. Binance
remains first where configured, and every Bybit/Yahoo series remains a labeled
research proxy.
5. **Accumulation:** No new cache, database, or file state was introduced.
The periodic sweep rewrites existing SQLite bars and `series` metadata under
their existing source keys.
6. **Boundary conditions:** Unsupported symbols, host failures, invalid
responses, and stale `15m` bars remain errors; the screener withholds stale
signals. IDX Yahoo data is delayed and cannot promise an exact live price.
7. **Recovery:** `/api/status`, candle freshness, `series.last_success`, and
`ingestion_runs` reveal progress and failed sources. The fallback can be
reverted independently; a missed 900-second sweep or memory above 16 MiB
requires reducing load or changing the existing bounded schedule.
8. **Governance:** This changes transport selection inside the existing Bybit
adapter. It adds no credentials, provider entitlement, venue-native claim,
account state, order path, or execution evidence.
9. **Longer-term hypothesis:** Regional/alternate API hosts may reduce
geography-specific failures for other Bybit instruments; repeated audit results
across full sweeps will confirm or reject that hypothesis.
10. **Decision and learning:** The smallest reversible path is to keep the
eight-worker pool and documented Bybit host list only while full sweeps,
provider provenance, stale withholding, and memory continue to pass. Any
unsupported Pintu Futures symbol remains unavailable rather than inheriting
Spot or another product's candles.

IDX still offers free website End-of-Day data; system-to-system Real-Time or
Delayed products are licensed. Yahoo's published IDX delay is ten minutes, so
the Stockbit path is a 15-minute refresh of delayed research candles rather
than a promise of tick-current IDX prices. See the [IDX Data Services FAQ](https://data.idx.co.id/faq.php?p=sFWjtRbH1z2wag4),
[IDX Data Services](https://www.idx.id/en/products/idx-data-services/),
[Yahoo exchange delays](https://help.yahoo.com/kb/finance-for-web/article-exchanges-data-delays-sln2310.html),
and Bybit's [official integration guidance](https://bybit-exchange.github.io/docs/v5/guide).
If `TRUK.JK` is registered through the app again, its multibagger profile can
use the retained daily candles, with their provider and research-proxy labels.
An actionable plan still needs at least 10 matching trades, at least 730
calendar days, and an untouched holdout validating at least 100% fee-adjusted,
risk-sized account return in one complete calendar month. Annualized CAGR is
not the multibagger target.

## Per-series 15-minute refresh follow-up — 2026-10-02

The new Bybit-host build attempted all 2,911 active registered products in
14m25s from service start. A read-only audit at 10:04 UTC found 2,440 exact
`15m` series whose `last_success` was within 15 minutes and 471 older. For
example, Pintu Pro Futures `BTC-USD` was marked attempted at 10:02:09 UTC while
its Bybit series still had `last_success` at 09:48:03 UTC. The per-source cache
reused results for up to the full 900-second candle duration, so a product
could be revisited before that duration, skip the provider request, and then
wait another sweep before trying again.

The correction keeps candle parsing and provider interval semantics at 900
seconds, but limits successful `15m` proxy-result reuse to 600 seconds. This
gives the next approximately 14-minute registry pass room to request a current
completed candle. The existing one-hour cache remains 3,600 seconds. Failed
requests retain their existing five-minute cooldown, provenance labels, and
stored bars; stale source data still cannot become fresh merely from a cursor
attempt.

Decision effects:

1. **Direct effect:** A `15m` result older than ten minutes triggers a provider
   read; a live full pass must leave available exact-product series within the
   15-minute freshness window.
2. **Immediate consequence:** More crypto and stock research-provider requests
   may run in a sweep. The eight-worker limit, reserved scan connection, and
   per-symbol write gate still bound concurrency.
3. **Repeated operation:** Products remain selected by the existing scheduler
   cursor; each pass can fetch again at its next rotation instead of recording
   a cached success just before the 15-minute deadline.
4. **Interactions:** Candle parsing still rejects open bars, one-hour data keeps
   its own refresh interval, and provider failures remain separate by source.
5. **Accumulation:** No new storage or cache is added; fresh provider metadata
   and bars replace or extend their existing SQLite series.
6. **Boundary conditions:** Unsupported symbols, closed markets, delayed IDX
   feeds, and failed requests can still have old bars; these remain visibly
   stale and cannot produce a fresh price-dependent signal.
7. **Recovery:** Inspect `series.last_success`, `market_sync_cursor`, `/api/status`,
   and `/api/signals`; revert the 600-second policy if the live sweep exceeds
   900 seconds or private memory exceeds 16 MiB, then address provider capacity
   before claiming the cadence is met.
8. **Governance:** The change only tunes cache reuse inside the current provider
   adapter; it adds no feed entitlement, venue-native claim, trading action, or
   new persistence boundary.
9. **Longer-term hypothesis:** A 600-second reuse limit should preserve enough
   margin for provider delays while fitting within the observed sweep; another
   complete live pass will confirm or reject that expectation.
10. **Decision and learning:** This is the smallest reversible correction
    because it keeps the registry, adapter boundary, candle duration, and
    scheduler unchanged. Any fresh-series coverage gap after a full pass or
    sweep runtime above 900 seconds triggers a further capacity fix.

### Bounded provider capacity follow-up — 2026-10-02

After lowering the `15m` reuse limit, the network-enabled live service completed
its first 243-product batch in 88 seconds. At 10:48:24 UTC it had attempted 419
distinct products in 154 seconds, about 2.72 products per second; 2,911
products require about 3.24 per second to fit in 900 seconds. This observation
indicates the eight-worker pool may miss the data-age target once proxy series
are due for a provider read. The measured process used 13.76 MB of private
memory at that point, below the 16 MiB ceiling, leaving capacity to try two
additional bounded workers. The new pool and database gate will be validated
with a complete live sweep and memory samples before the cadence is considered
fixed.

Decision effects:

1. **Direct effect:** Raise the existing provider worker cap and regular
   database leases from eight to ten; keep the one scan-reserved lease.
2. **Immediate consequence:** Allow up to two more candle requests to proceed
   concurrently, increasing provider load while shortening each sweep.
3. **Repeated operation:** The same registry cursor and per-source refresh
   policy remain in place, so provider failures still retry on later sweeps.
4. **Interactions:** The per-symbol lock continues to serialize SQLite writes;
   the scan lease remains outside the regular-worker cap.
5. **Accumulation:** No new store or queue is introduced; only bounded worker
   threads and connections change.
6. **Boundary conditions:** Unsupported products can still fail and remain
   stale; more concurrency must not relabel or borrow another product's data.
7. **Recovery:** Revert both caps to eight if the full pass does not improve,
   private memory exceeds 16 MiB, or SQLite/provider errors increase.
8. **Governance:** This is an operator-capacity tune inside the existing
   scheduler and storage boundaries, with no new provider or execution path.
9. **Longer-term hypothesis:** A 25% increase in workers should raise request
   throughput enough for a 15-minute pass; only complete live-sweep timing and
   fresh-series coverage can validate that expectation.
10. **Decision and learning:** Increase the two existing bounds together,
    compare sweep duration, data freshness, scan completion, and memory, and
    reconsider if the measured evidence misses any limit.

### SQLite cache memory correction — 2026-10-02

The ten-worker live pass attempted all 2,911 products in 14m56s. At 11:10 UTC,
2,702 exact-product `15m` series had a successful source update in the prior 15
minutes; 209 had no fresh successful series, led by 113 Stockbit products and
65 Pintu retail products whose public sources returned no completed matching
candles. A separate candle-timestamp check found 849 refreshed Stockbit series
still had bars over 30 minutes old after the IDX session closed. Those rows
must stay stale and cannot produce current-price signals. During the concurrent
scan and candle pass, the process reached 17.08 MB private memory, above the
16 MB target. The database connection policy reserves up to 512 KiB of page
cache per attached schema on each connection; the existing per-worker main
cache remains separately capped at 64 KiB.

The correction halves the shared SQLite page-cache limit to 256 KiB per
attached schema while keeping ten bounded provider workers and the dedicated
scan lease. A complete live pass must confirm both the 900-second cadence and
private memory below 16 MB while a scan overlaps the data refresh.

1. **Direct effect:** Reduce SQLite page-cache allocation from 512 to 256 KiB
   per attached schema; acceptance is sub-900-second registry progress and
   under-16-MB private memory during concurrent scan and sync.
2. **Immediate consequence:** Connections have a smaller hot-page cache, while
   the ten-worker provider concurrency and reserved scan slot remain available.
3. **Repeated operation:** The limit applies consistently to the existing
   short-lived per-symbol connections and app-database attachments.
4. **Interactions:** Workers still use their narrower 64 KiB main-database
   cache, and the per-symbol writer gate and source identity are unchanged.
5. **Accumulation:** Lower cache residency may cause more SQLite reads; the live
   sweep runtime will show whether that cost affects the cadence.
6. **Boundary conditions:** SQLite remains the only store, and missing/stale
   provider data stays withheld even if memory and cadence pass.
7. **Recovery:** Restore 512 KiB only if reads measurably slow the sweep and
   another bounded configuration can satisfy both memory and cadence; monitor
   private bytes, batch durations, and `series.last_success`.
8. **Governance:** This changes a bounded runtime resource limit within the
   existing storage boundary and adds no state or provider entitlement.
9. **Longer-term hypothesis:** Halving attached-schema caches should recover
   enough memory for the ten-worker pool; concurrent live samples will confirm
   or reject it.
10. **Decision and learning:** Keep the smallest reversible cache reduction,
    then remeasure memory and elapsed time under the actual scan-plus-sync
    workload before accepting the capacity change.

### Stale daily-frame recovery follow-up — 2026-10-02

The full service test suite found that a stale `1d` series did not enter the
daily refresh priority queue when that same instrument lacked a fresh `15m`
price. The scan skipped all timeframe metadata before evaluating daily
freshness. It now checks and queues stale `1d` history before the current-price
gate; screener rows still require a fresh `15m` price and each timeframe's
own fresh completed candles.

1. **Direct effect:** Stale or missing daily history can be refreshed even
   while the instrument has no current `15m` price; the existing
   `stale_stockbit_daily_history_queues_without_emitting_a_screener_row` test
   is the acceptance check.
2. **Immediate consequence:** A Stockbit or crypto product can enter the
   existing bounded daily queue while its intraday refresh is pending.
3. **Repeated operation:** The queue retains the existing cooldown and
   per-product cursor, preventing a missing price from causing a retry loop.
4. **Interactions:** The scanner still emits no signal without fresh `15m`
   pricing, and daily history still uses the existing provider and SQLite
   boundaries.
5. **Accumulation:** Existing `series`, bars, and scheduler cursor remain the
   sole data sources; no duplicate storage is created.
6. **Boundary conditions:** Unavailable daily providers preserve old bars and
   visible source errors; missing or stale history remains unavailable to the
   screener.
7. **Recovery:** `/api/status`, the daily cursor, and `series.last_success`
   expose queue progress and provider outcomes; the code path is independently
   reversible.
8. **Governance:** This restores the expected recovery behavior without
   changing signals-only product behavior or adding daily feed entitlements.
9. **Longer-term hypothesis:** Independent timeframe refresh priorities should
   reduce recovery lag when one frame fails; other stale-frame cases should be
   audited if they recur.
10. **Decision and learning:** Keep the targeted metadata check before the
    price gate and verify the full suite; reconsider only if daily queue load
    or scan latency measurably rises.

### Screener price-bar age display — 2026-10-02

The live `INSP-USD` `15m` screener result carried `price_bar_time` in
nanoseconds, while the UI age formatter expected epoch seconds. The card
therefore displayed `Price 0s ago` for a bar about 29 minutes old. The card and
selected-signal summary now convert the stored bar time before formatting its
age; scan age continues to use the scan timestamp in seconds.

1. **Direct effect:** The UI shows the age of the actual price candle; a unit
   regression test requires the live-equivalent 29-minute value.
2. **Immediate consequence:** Users can distinguish an old completed candle
   from a newly generated scan without interpreting raw timestamps.
3. **Repeated operation:** Rerenders and timeframe-specific rows share the same
   conversion helper and preserve each row's independent bar time.
4. **Interactions:** Signal ordering, score, source provenance, stale policy,
   and two-hour signal expiry stay unchanged.
5. **Accumulation:** This changes only presentation; no database or API schema
   is added.
6. **Boundary conditions:** Missing, non-finite, or non-positive bar times show
   `age unavailable`; future times clamp to `0s ago`.
7. **Recovery:** The unit test and a fresh browser session check the label; the
   display helper is isolated and reversible.
8. **Governance:** No signal eligibility or product identity rule changes.
9. **Longer-term hypothesis:** Explicit unit conversion at the screener edge
   should prevent similar age-label errors in future consumers.
10. **Decision and learning:** Convert the existing nanosecond field in the
    existing UI and verify the rendered age against `price_bar_time` and scan
    time from the live API.

### Suppress signals when their price bars become stale — 2026-10-02

A live signal generated with a still-fresh completed bar remained in the saved
screener snapshot after that bar later crossed its freshness limit. The scan
status could remain ready for its full 15-minute interval while the displayed
price bar aged beyond the allowed threshold. The read path will apply the same
existing completed-bar freshness rule before returning rows and report how
many stored rows were withheld, so the UI can explain the empty/recovery state.

1. **Direct effect:** API consumers receive only screener rows whose price bar
   is still fresh at read time; acceptance is a dedicated test with one fresh
   and one stale row plus a visible withheld-row count.
2. **Immediate consequence:** A price that ages out between scheduled scans is
   removed from the live ranking without waiting for another scan.
3. **Repeated operation:** Each API read applies the same check; a later scan
   can make a row visible again only after it creates a fresh-price snapshot.
4. **Interactions:** The existing per-timeframe score, source labels, read-only
   screener behavior, and two-hour snapshot retention remain owned by their
   current modules.
5. **Accumulation:** Stale stored rows remain bounded by the existing two-hour
   cleanup and full-snapshot replacement; this adds no market-data cache.
6. **Boundary conditions:** Missing or invalid bar timestamps are withheld;
   provider failures do not borrow a different product's data, and an empty
   result reports that stale-price rows were withheld.
7. **Recovery:** The live refresh scheduler continues its provider attempts;
   the next successful fresh bar and scan can repopulate the ranking. The
   withheld count and API status provide observation.
8. **Governance:** This changes signal visibility at the existing screener API
   boundary only; it adds no provider, storage, execution, or architecture
   boundary.
9. **Longer-term hypothesis:** Applying freshness at read time should prevent
   delayed scans from making stale prices appear current; live API checks will
   show whether provider cadence still needs tuning.
10. **Decision and learning:** Reuse the existing completed-bar freshness
    helper at the screener read boundary. Reconsider if fresh rows are filtered
    unexpectedly or if the live provider sweep misses its 15-minute window.

Implementation reuses `completed_bar_is_fresh_at` in the existing screener
reader. The JSON summary reports `stale_price_signal_count`; the dashboard
explains when stale-price rows are hidden. Dedicated tests cover stale-price
filtering and physical deletion at the two-hour retention boundary. A live
read-only audit found 2,701 of 2,911 exact-product `15m` series with a successful
provider update inside 15 minutes after the scheduler attempted all 2,911 in
14m37s. The remaining 210 were Stockbit 113, Pintu 75, Pluang 19, and
Tokocrypto 3. These are provider/session coverage gaps, not scheduler skips;
signals continue to require a fresh completed price bar. Highest observed
service private-memory sample during overlapping work was 15.73 MiB, below the
16 MiB target. This result validates the bounded scheduler and resource limit,
but not universal provider success.

The rebuilt live API then withheld one stored signal whose Tokocrypto-native
`15m` price bar had aged to 33 minutes while the `1h` signal was only 3.7
minutes old. It returned zero visible rows, `stale_price_signal_count: 1`, and
the ready scan state. A fresh browser session displayed the stale-price
withheld message, the two-hour cleanup notice, and all six enabled frames.

### Free IDX intraday source check — 2026-10-02

The official [IDX Data Services Portal](https://data.idx.co.id/index.php)
lists real-time, delayed, end-of-day, and historical market-data products.
Its [FAQ](https://data.idx.co.id/faq.php?p=sFWjtRbH1z2wag4) identifies the
public IDX website alternative as end-of-day data and describes licensed feed
delivery or redistributors for connected data. IDX's
[terms of use](https://idx.co.id/id/syarat-penggunaan/) prohibit web scraping
and crawling. [Twelve Data's stock coverage and pricing page](https://twelvedata.com/stocks)
lists Indonesia as end-of-day delayed; its free Basic plan is limited to real-
time US stock data, with international exchange access on paid tiers. Neither
is a compliant free source for an IDX 15-minute screener price. [EODHD's
intraday API](https://eodhd.com/financial-apis/intraday-historical-data-api)
documents global intraday bars, but its [free plan](https://eodhd.com/pricing)
excludes that API. [Marketstack's free plan](https://marketstack.com/) is
EOD-only, with intraday access for US tickers on paid tiers. No provider was
added; Yahoo `.JK` remains the existing delayed research proxy and stale rows
remain withheld.

1. **Direct effect:** Keep the current authorized Yahoo `.JK` research source;
   acceptance is no new unauthenticated IDX intraday endpoint or web scrape.
2. **Immediate consequence:** IDX signals can be absent outside the trading
   session or when the delayed proxy has no fresh completed bar.
3. **Repeated operation:** The existing 15-minute scheduler will keep attempting
   registered series, while the screener continues to hide stale prices.
4. **Interactions:** The provider adapter and source-quality labels stay within
   their current boundaries; EOD data cannot substitute for a 15-minute bar.
5. **Accumulation:** No new data store, credential, refresh job, or provider
   dependency is introduced.
6. **Boundary conditions:** Closed markets, missing listings, and provider
   failures remain visible as missing/stale coverage rather than invented bars.
7. **Recovery:** Reassess if IDX offers an authorized free intraday interface
   or the user supplies a licensed source; retain the existing delayed proxy in
   the meantime.
8. **Governance:** This avoids unauthorized redistribution or scraping and
   does not add a paid account requirement.
9. **Longer-term hypothesis:** A licensed or officially documented IDX adapter
   could improve intraday coverage; current evidence does not establish any
   free source with the required interval.
10. **Decision and learning:** The smallest compliant path is to preserve the
    existing adapter and withhold stale prices. Revisit only with a source that
    explicitly supports authorized IDX intraday data.

### Prioritize intraday sync by recent volume — 2026-10-02

The intraday queue currently selects the least recently attempted registered
products, then dispatches them in that order. Keep that aging order as the
fairness boundary, but rank the selected due batch by the latest completed
15-minute bar's quote turnover divided by that instrument's prior 20-bar median.
This uses the canonical ticker SQLite history and compares dimensionless
relative activity instead of mixing share, token, IDR, and USD units.

1. **Direct effect:** Among equally prioritized due targets, instruments with a
   higher latest relative-volume reading start provider sync first; acceptance
   is a dedicated ordering test over canonical `bars` rows.
2. **Immediate consequence:** The current batch's most active instruments get
   fresher price data earlier in the one-minute scheduler sweep.
3. **Repeated operation:** The existing attempt-age order rotates the batch on
   later ticks, so low-volume registered products continue receiving attempts.
4. **Interactions:** Stale-signal and explicit chart-refresh targets retain
   precedence; provider adapters, source labels, and 15-minute scan cadence do
   not change.
5. **Accumulation:** Ranking reads only a bounded batch of existing candles,
   stores no duplicate market data, and introduces no additional database.
6. **Boundary conditions:** Missing history, stale or incomplete bars,
   non-finite values, and zero prior turnover receive neutral priority; ties
   preserve the queue's existing order.
7. **Recovery:** If the bounded history read fails, the target keeps its
   original queue position and still enters the normal provider attempt path.
8. **Governance:** The change only orders refresh work; it adds no execution,
   instrument-registration, provider, or storage ownership boundary.
9. **Longer-term hypothesis:** Relative-volume ordering may improve how quickly
   active instruments refresh during a provider backlog; live batch ordering
   and full-sweep duration will show whether it helps.
10. **Decision and learning:** Reuse the existing candidate batch and canonical
    15-minute candle reader. Reconsider if low-volume products miss their
    freshness window or the added reads push the full sweep past 15 minutes.

### Schedule signal scans against completed snapshots — 2026-10-02

The scan scheduler used a fixed 15-minute start-to-start interval. A live run
grew from 5m03s to 8m47s, so the next completed screener snapshot arrived
18m44s after the previous one and the read path correctly marked it stale
while the scan was still running. Keep the single scanner and move the next
start earlier using the most recent measured run time plus a two-minute
safety margin.

1. **Direct effect:** A scan starts before the 15-minute publication deadline;
   acceptance is the live interval between successive successful snapshots.
2. **Immediate consequence:** The idle wait after a completed scan shortens
   when scan runtime increases and remains non-overlapping.
3. **Repeated operation:** Each completed run updates the in-memory runtime
   estimate; the next scan then recalculates its start time.
4. **Interactions:** The existing scan-status API, SSE progress, stale-price
   withholding, and independent market-sync scheduler stay in their owners.
5. **Accumulation:** No timing cursor is persisted; a service restart uses the
   named conservative runtime estimate and learns from its first scan.
6. **Boundary conditions:** If a run takes longer than 15 minutes, the next
   scan starts immediately after it; one sequential scanner cannot publish a
   complete snapshot faster than one full scan takes.
7. **Recovery:** Compare scan start, `generated_at`, and the next scheduled
   time; if publication still exceeds 15 minutes, reduce scan work or parallelize
   within the existing memory and database-connection limits.
8. **Governance:** This is a timing change within the existing runtime loop; it
   adds no service, API, provider, persistence, or execution boundary.
9. **Longer-term hypothesis:** Duration-aware starts should prevent a slower
   cycle from consuming the following snapshot's full freshness window; live
   completion timestamps will test this.
10. **Decision and learning:** Use the last measured scan duration plus a
      bounded two-minute lead, never overlap scans, and reconsider if CPU/memory
      rises above the project targets or completed snapshots still miss 15m.

### Keep full-store diagnostics out of scan finalization — 2026-10-02

The rebuilt scan published at 13:45:55 UTC and scheduled the next start for
13:50:46. At 13:53:05, `/api/status` still showed that past start time and no
new scan, while the log had no scan-completed event. The scheduler was still
walking all 2,708 ticker databases in `load_all_sync_diagnostics` after it had
already published the snapshot and marked itself idle. The dashboard's
`scan_completed` handler does not consume that `sync` field.

1. **Direct effect:** Remove the all-ticker diagnostic walk from scan
   finalization; preserve the event shape with `sync: null`. Acceptance is a
   scheduled scan starting after the computed idle wait and a completed
   snapshot no more than 15 minutes after its predecessor.
2. **Immediate consequence:** The scan daemon returns to its scheduler after
   signal publication instead of opening every market database for an unused
   SSE field.
3. **Repeated operation:** Each 15-minute scan avoids repeating thousands of
   database opens. Provider and per-symbol diagnostics remain available at the
   existing diagnostic owners.
4. **Interactions:** `signal-refresh.js` only reads the next-scan epoch from
   `scan_completed`; it refreshes authoritative signals through `/api/signals`.
   Stale-price withholding and independent market synchronization are unchanged.
5. **Accumulation:** Repeated full-store reads no longer add I/O and connection
   churn after every snapshot. No new cache, storage, or worker is introduced.
6. **Boundary conditions:** An absent global diagnostic summary is represented
   as `null`; provider errors remain in their existing per-series SQLite
   records and market-sync logs rather than appearing as a false clean summary.
7. **Recovery:** Compare each scan's `last_scan_time`, `next_scan_time`,
   completion log, and `/api/signals` freshness. If scan cadence still misses,
   profile the scan itself before changing worker counts or concurrency.
8. **Governance:** This removes unused work from the existing scheduler and
   does not add a service, persistence path, or provider/venue capability.
9. **Longer-term hypothesis:** Removing post-publication database traversal
   should let the scheduler meet the measured adaptive start time; the next
   live completion will confirm or reject this.
10. **Decision and learning:** Keep only the smallest hot-path removal because
    the dashboard ignores this field. Restore a diagnostic consumer only if
    it is required and can run without delaying signal publication.

## Stockbit 4h Envelope history warm-up — 2026-10-06

**Observed gap on 2026-10-05:** The Stockbit 4h Envelope signal is registered and the scanner
can evaluate it from completed 4h candles, deriving that timeframe from the
existing hourly history. In the active SQLite snapshot, none of 962 active
Stockbit instruments had enough fresh 4h candles for the default Envelope's
500-bar endpoint plus the prior bar needed to test a crossing. The sampled
hourly histories were far shorter. Recent Yahoo `.JK` requests also failed
with curl exit code 7, so no current signal can be claimed from the existing
data.

**Decision:** When an active Stockbit regular IDX ticker lacks enough derived
4h history, make a bounded Yahoo 60-minute history request through the existing
research-proxy adapter. Decode at most 4,000 hourly rows, then retain only the
existing candle-history aggregator's source limit for the required 4h window
(currently 2,124 hourly rows). Persist through the existing
`stockbit_regular_yfinance_fallback` series in the symbol's SQLite database.
Keep the regular 1h request at five days. Once there is enough history, normal
hourly refreshes continue as before. A provider failure may retry after one
hour; a successful response that still lacks the required history may retry
after 30 days. Provider failures, too-short listings, stale candles, and a
missing recent upper-band cross continue to produce no signal.

Decision effects:

1. **Direct effect:** Stockbit's own `idx_stock` 4h screener can receive the
   historical candles it needs from the already-approved Yahoo proxy.
   Acceptance requires at least 501 completed 4h bars from the existing
   500-bar Envelope configuration, a fresh latest completed 4h candle, and an
   actual crossing above the upper envelope before a row is emitted.
2. **Immediate consequence:** The first warm-up can add at most 2,124 hourly
   bars for one ticker and never changes the five-day refresh used for ordinary
   hourly maintenance. The data stays explicitly research-proxy data.
3. **Repeated operation:** Enough stored history suppresses further warm-ups.
   A failed fetch cools down for one hour unless a later successful standard
   1h fetch proves Yahoo is reachable; that recovery permits one bounded
   warm-up retry. A valid but insufficient response cools down for 30 days so
   a young listing does not repeat a large request.
4. **Interactions:** The existing app-to-product identity, `.JK` mapping,
   Jakarta candle aggregation, source selection, 2-hour signal expiry, and
   Yahoo provider label remain authoritative. No 15-minute price is required
   for this one strategy.
5. **Accumulation:** The warm-up upserts into canonical per-symbol SQLite
   `bars`; it adds no parallel 4h cache, new database, or runtime file. At the
   observed 962 instruments, the hard ceiling is 2,043,288 added hourly rows
   if every ticker needs a full warm-up; actual row counts depend on Yahoo
   coverage and existing history.
6. **Failure boundary:** Yahoo timeouts, oversized responses, malformed or
   unordered rows, short listings, incomplete candles, and data older than the
   existing freshness limit remain unavailable. The implementation must not
   lower the Envelope warm-up or manufacture a recent bar.
7. **Recovery:** One warm-up request is serialized at a time to bound parsing
   memory. Existing `ingestion_runs` diagnostics record failure or insufficient
   history. A later successful standard 1h fetch clears the transport-failure
   cooldown for one warm-up retry; a new failure restores the cooldown. A live
   Yahoo request and a later Stockbit 4h scan are required to prove operational
   recovery.
8. **Governance:** The change uses only the already-approved Yahoo research
   fallback and remains informational. Proxy rows do not validate Stockbit
   execution or authorize order, account, or paper-trading state.
9. **Longer-term hypothesis:** A bounded two-year Yahoo response should supply
   enough source candles for many listed tickers, but source coverage and
   network access can change. Repeated failures, short history, or observed
   memory above the 16 MB target would disprove the operational assumption.
10. **Decision and learning:** Reuse the existing hourly series, aggregator,
    database, freshness checks, and indicator configuration. Reconsider the
    request window or data source if a live scan still lacks adequate fresh
    history; do not add another indicator or relax signal expiry to make the
    list look populated.

**Operational verification — 2026-10-06:** The existing Yahoo adapter returned
3,856 hourly bars for BBCA over its bounded two-year request and persisted the
2,124-row aggregation window to BBCA's SQLite history. The existing API then
returned 938 completed 4h bars, above the 501-bar signal requirement. A fresh
Stockbit/4h dashboard scan completed with zero active rows at 01:55 WIB. The
latest BBCA 4h close was 16:00 WIB on the prior session, about ten hours old,
and below its existing NWE upper band; the two-hour signal-age limit therefore
correctly withheld a current signal. Of 962 active Stockbit regular tickers,
759 had reached the 2,124-hourly-row source bound during this check; the
scanner still validates each ticker's completed 4h history, freshness, and
crossing independently. After a 30-second settle interval following the scan
and backfill, service private memory measured 15,917,056 bytes, below the
16 MiB steady-state target.

This improves input availability; it does not promise that a market is
currently crossing the upper band. Outside Stockbit's trading session or after
the signal-age limit, an empty list can still be the correct result.
