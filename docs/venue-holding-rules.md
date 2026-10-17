# Venue and plan constraints

Updated 2026-09-25. These are inputs to plan screening and historical research. The dashboard does not manage simulated positions or submit orders.

- Stockbit Day Trade facility has a same-day sale deadline of 15:40 WIB. This is not a universal holding limit for cash-funded shares. Day-trade research profiles use the deadline as a strategy constraint.
- Stockbit plan candidates use a latest stored close of at least Rp60. This user-selected floor is above exchange rules and is not a profitability guarantee.
- Pintu perpetuals have no fixed expiry or four-hour cap. Funding at 07:00, 15:00, and 23:00 WIB is a payment event, not a mandatory exit. Funding and venue-native fills are not modeled by the local history store.
- Naive market timestamps are interpreted as WIB; timezone-aware timestamps are converted to WIB. Quote refreshes without timestamps use the current WIB clock.
- The scanner is periodic and uses stored market bars. It does not schedule or submit stop, target, cutoff, or trailing orders. Exchange holidays, halts, missing quotes, and execution liquidity still need handling before any external execution.
- Quote snapshots do not replay 24-hour highs/lows as post-entry candles. This avoids false historical fills, but quote-only checks can miss moves between updates.
- Day Trade's research goal is at least 50% win rate with a 1.20 profit-factor quality floor. It remains unvalidated until matching venue-native holdout evidence passes. Fee, sizing, funding, and leverage assumptions remain separate from displayed plan prices.
- Swing goal: at least 100% annualized CAGR on an untouched holdout.
- Multibagger goal: at least 100% fee-adjusted, risk-sized account return in one complete calendar month on an untouched holdout.

Sources:

- [Stockbit Day Trade terms](https://help.stockbit.com/id/article/day-trade-penting-apa-syarat-dan-ketentuan-menggunakan-fitur-day-trade-stockbit-ctzlzt/)
- [Pintu perpetual trading rules](https://www.pintu.co.id/en/pages/pintu-pro-futures-trading-rules)
- [Pintu funding schedule](https://pintu.co.id/en/faq/what-is-funding-rate)