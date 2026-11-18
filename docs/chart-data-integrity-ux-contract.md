# Chart data integrity UX contract

The primary task is to inspect candles and studies for the selected app,
registered product, and chart timeframe. A chart is ready only when each visible
series belongs to that exact context and studies are attached to the candles
currently displayed.

1. **Direct effect:** Switching or retrying a chart shows candles for the
   selected timeframe and hydrates studies for those candle objects. Acceptance
   is that visible bar times, OHLC values, and study values all match the active
   timeframe after selection and retry.
2. **Immediate consequence:** An empty timeframe stays empty until its own
   history loads; cached candles from another timeframe are never presented as
   a substitute. Loading and empty states identify the requested interval.
3. **Response:** Retry replaces stale data, then reports ready only after the
   current candles have their required studies. Pine `request.security()` can
   use a stored requested interval without requiring the user to visit it first.
4. **Interactions:** Preserve app, exact product, asset category, provider,
   source, and timeframe when loading candles, metadata, or indicators. An
   ambiguous source must remain unavailable instead of borrowing another
   product's series.
5. **Accumulation:** Keep history in the existing per-symbol SQLite stores and
   caches in memory. Do not persist chart state or introduce another data store.
6. **Failure boundaries:** Missing, stale, partial, aborted, or mismatched
   history remains distinguishable from a valid empty result. Studies cannot
   inherit a ready state from a different set of candle objects unless values
   are reapplied to those objects.
7. **Recovery:** The normal chart retry reloads the requested series and
   rehydrates its studies; a missing Pine interval reports which interval is
   unavailable and keeps the existing retry path.
8. **Governance:** Chart and Pine results remain read-only research. No
   execution, account, order, or paper-trading state is introduced.
9. **Trajectory:** Hypothesis: matching series identity across chart layers
   reduces false visual conclusions. Check support tickets and chart error
   telemetry before expanding cache lifetime or fetching extra intervals.
10. **Decision:** Preserve the selected context end-to-end, keep readiness tied
    to the displayed candle objects, and verify frame switching, retries,
    missing history, and the browser console in a fresh session. Reconsider if
    a chart shows any bar or study value from another timeframe or product.
