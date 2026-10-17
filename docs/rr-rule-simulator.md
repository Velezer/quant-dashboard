# Saved R:R setup simulator

## User outcome and UX contract

The user wants to save a rule with a 1.2% stop, 2.6% target, 50% win-rate
assumption, and a five-trade daily cap, then inspect both its fee-adjusted
per-trade result and expected account-equity growth over time. The simulator
belongs in the existing dashboard and stores only the R:R rule in the existing
app SQLite database. The equity curve is a derived projection; it creates no
account, position, paper trade, order, or performance evidence. The user
explicitly approved this narrow project-rule exception on 2026-10-01.

The percentages are interpreted as adverse/favorable price movement relative
to each trade's entry notional. The selected app and product supply the fee
policy. The default direction is long; futures users can choose short. Fees are
charged on entry and exit values. If a venue fee is unknown, net outputs remain
unavailable until the user enters an explicit manual fee assumption, which is
labelled as user supplied and is not saved with the rule. The fee panel links to
the selected product's official exchange fee rules; that reference does not
turn unknown or unsupported fees into a configured simulator assumption. The
win rate is a user assumption, not observed or validated evidence. Daily
expectancy assumes the same notional for every trade and does not compound. The
daily sum is expressed against one trade's starting notional, not account equity
or the sum of traded notional.
Stockbit's configured 0.15% buy and 0.25% sell assumptions use its published
stock transaction rates. The model excludes conditional stamp duty, the Live
Data Fee, slippage, and market depth; see [Stockbit's fee schedule](https://help.stockbit.com/id/article/transaksi-saham-berapa-biaya-trading-di-stockbit-sekuritas-1lbkyq9/).

The per-trade result cards show target and stop outcomes, using net percentages
when fees are available and visibly labeling gross-only values when fees are
missing. The account-growth chart plots expected balance by modeled day. Its
default horizon is 3,650 modeled days (ten years at 365 modeled days per year),
and its maximum is ten years. Long horizons show years on the horizontal axis
and retain modeled-day detail in the accessible description. When the balance
range spans at least 100×, the chart uses a labeled logarithmic USD axis so early
and late compounding remain visible; equal vertical distances then represent
equal proportional changes, not equal dollar changes. Shorter ranges stay
linear. The model assumes the daily trade cap is filled on every modeled day,
including weekends.
It compounds the expected account multiplier per trade, with each trade's
notional set to an editable percentage of the current balance. The projection
assumes the saved maximum number of trades occurs every trading day and that
outcomes are independent at the assumed win rate. Starting balance, position
size, and horizon are editable projection-only inputs, reset from typed
defaults when the dialog is reopened, and are not persisted. Starting balance
defaults to $100 USD, remains editable, and is shown in USD across the chart.
Currency conversion to the selected venue is not modeled; leverage, funding,
slippage, liquidation, stochastic sample path, and market history are also not
modeled.
An account balance cannot fall below zero in this arithmetic model. The chart is
an expected-value illustration, not a likely path, forecast, backtest, or
validated evidence. When fees are unknown, it is labeled gross-only.

The primary task is to see what the saved setup implies after fees. The main
decision is whether the fee-adjusted reward, risk, and break-even win rate fit
the user's assumptions. The simulator has accessible labels and a live result
announcement, reports loading/unsaved/saved/error states, and offers retry for
failed reads. Fee-unknown and unsupported-venue states explain how to recover.
Advanced market or strategy diagnostics do not appear because this calculator
uses no market history.

## Seven-order review

1. **Direct effect:** The user can save the four rule values and view a fee-aware
   expected equity curve across an editable number of trading days. Acceptance
   is a clearly labeled curve that moves when an editable input changes.
2. **Immediate consequence:** Account movement depends on position size relative
   to current balance, trade frequency, fees, and compounding. Those assumptions
   appear beside the curve, use the selected app/product fee policy, and do not
   silently treat unknown fees as zero.
3. **User response:** Users may change win rate, trade size, starting balance,
   and horizon. The UI identifies these as assumptions, explains that all days
   use the full trade cap, and does not present expected balance as a likely or
   validated outcome.
4. **Interactions:** Venue fees and compounding arithmetic stay in `quant/core`.
   The approved projection remains inside the existing simulator and API; it
   cannot affect plan readiness or create account, position, order, or evidence
   state.
5. **Accumulation:** One upserted saved-rule row replaces the previous values.
   Projection assumptions and points stay in memory for the current dialog;
   repeated use creates no account or trade ledger and browser storage contains
   no financial inputs.
6. **Boundary and failure:** Invalid percentages and trade counts are rejected.
   Missing saved settings show draft defaults as unsaved. Failed loads or saves
   preserve the draft and offer retry. Unknown venue fees suppress net metrics
   unless explicit manual fees are entered; a gross-only curve remains visibly
   labeled. Non-finite or out-of-range projection settings are rejected, output
   is bounded to 3,650 modeled days, and expected equity is floored at zero.
   Long runs return a clear numeric-range error if assumptions exceed finite
   arithmetic.
   Market-data freshness is inapplicable because this model uses no candles.
7. **Decision and learning:** A core calculation, existing app SQLite rule,
   existing JSON route, and existing dashboard dialog satisfy the request
   without adding persistence or execution state. Reconsider if projection
   assumptions or fee labels become unclear, or output diverges from
   probability-weighted per-trade compounding.

## Acceptance

- Defaults are 1.2% risk, 2.6% reward, 50% assumed win rate, and at most five
  trades per day.
- The user can edit and save the R:R rule in app SQLite, then change starting
  balance, trade size, and trading-day horizon for an ephemeral projection.
- The editable starting-balance input defaults to $100 USD and all plotted
  account balances are explicitly labeled in USD.
- The account-growth chart plots expected account balance per trading day,
  compounds the expected net per-trade equity multiplier, and assumes the daily
  trade cap is filled. It labels user assumptions and does not imply a sampled
  trade sequence or forecast.
- Net outcomes use selected venue fees for entry and exit values and expose the
  fee source and an official fee-rules link for the selected app product.
  Unknown fees never count as zero.
- The interface states that results are arithmetic scenarios, not a backtest,
  forecast, or order instruction.
- The growth chart has a labeled trading-day axis and account-value axis; its
  accessible description states the starting and ending expected balance,
  assumed win rate, trade size, and daily trade count. Changing rule, fee, or
  projection inputs recalculates it. Missing fees produce a clearly labeled
  gross-only curve, never a net claim.
- No paper position, order, or trade-history row is created.

## Ten-year horizon decision

The default and maximum horizon are 3,650 modeled days, using the simulator's
existing daily-cap assumption for every day of the year. This exposes a full
ten-year curve without changing API/storage architecture or adding persisted
account state. The endpoint returns 3,651 daily points, a bounded payload; the
chart labels long horizons in years, uses a labeled log scale for wide balance
ranges, and keeps modeled days in its accessible description. Reconsider if
normal requests exceed the 16 MB steady-state target or the chart no longer
responds promptly; numeric overflow remains an explicit calculation error
rather than a clipped financial value.
