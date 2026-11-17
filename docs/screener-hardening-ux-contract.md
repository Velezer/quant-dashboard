# Screener filtering and freshness contract

The primary task is to narrow the selected app, product, and timeframe to a
scoring factor, then decide whether a retained row still represents a current
market entry.

- All six score factors, including Markov regime and lag-one return
  autocorrelation, filter the full server-side result set before pagination.
  Stable factor IDs and their display labels identify the same component.
- An explicit no-entry state stays no-entry. Missing or expired freshness
  evidence never becomes a current entry. A closed-market reference may remain
  visible with its recorded score and an explicit no-entry label.
- Missing prices, counts, coverage state, and pagination metadata remain
  unavailable. A malformed response preserves the last good rows and shows a
  retryable error instead of an empty or healthy result.
- Loading, stale, empty, unavailable, and request-error states keep their
  existing screen-reader announcements and keyboard controls. No filter action
  changes model rules or signal data.

Acceptance checks cover each factor filter, label/ID matching, expired and
unknown freshness, no-entry preservation, null prices, incomplete coverage,
and malformed pagination. Verify the screener at desktop and 320px/390px widths;
the signal list remains the desktop scroll region and the phone page has no
horizontal overflow.
