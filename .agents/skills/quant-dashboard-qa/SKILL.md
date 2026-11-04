---
name: quant-dashboard-qa
description: Run read-only browser smoke checks against the deployed Quant Dashboard, including app filters, signal availability, chart selection, API response integrity, and mobile layout. Use when verifying https://quant-dashboard.velezer.blitz.cloud/ or reporting a production dashboard defect.
---

# Quant Dashboard production QA

Verify the live dashboard through its normal browser UI. This skill is for
production smoke checks and defect reproduction, not implementation. Read the
repository `AGENTS.md`, `EXPERIENCE.md`, and the `ui-tester` skill first. Keep
the product read-only: do not place orders, grant browser-notification
permissions, change stored data, trigger market syncs, or start signal scans.

## Start with a clean session

1. Use `https://quant-dashboard.velezer.blitz.cloud/` in a fresh, isolated
   browser context. Do not reuse user cookies, saved browser profiles, or
   existing tabs. Prefer the browser automation available in the current
   environment. If using Playwright, launch the installed Chromium and use an
   incognito context.
2. Record the local time, initial HTTP status, final URL, viewport, and
   dashboard build/version shown in the header. Keep screenshots and logs
   local unless the user asks to share them. Never capture or report cookie,
   authorization, or request-header values.
3. Blitz may return a `503` wake-up page before the service starts. Wait for
   the page to redirect to the dashboard for up to 60 seconds. If it remains
   asleep, reload once and wait up to another 30 seconds. If it still does not
   load, report **blocked by hosting cold start** with the status, elapsed
   wait, final URL, and screenshot. Do not call this a dashboard UI defect or
   keep retrying indefinitely.
4. If browser navigation succeeds but direct Playwright API requests fail
   before receiving an HTTP response, check APIs through same-origin `fetch`
   in the loaded page. Some environments route browser traffic through a
   proxy that a standalone API client does not use. A transport failure with
   no HTTP status is an environment/network block, not proof that the API is
   broken.

## Read-only smoke checks

Run checks at desktop `1365 × 900` and mobile `390 × 844`. Wait for the UI to
settle after each navigation or selection; do not infer readiness from initial
HTML alone.

### App shell and routes

- Confirm the dashboard root renders its app header, build/version label,
  screener status, app selector, signal availability section, and chart area.
- Confirm the event-stream badge reaches its connected state, or record the
  visible reconnect/error state and related console or network evidence.
- Measure the initial time until the timeframe buttons become usable. The
  supported choices should appear while the signal snapshot is still loading;
  a slow signal query should not block selecting `15m`, `30m`, or another
  supported timeframe. Check that bootstrap starts only one initial
  `/api/signals` request; duplicate concurrent requests are a performance bug.
- Open `/scoring`, `/registry`, and `/entry-validation`; each page should render
  its own heading and a working link back to the screener. Do not invoke the
  registry or validation refresh/run controls.
- Capture console errors, uncaught page errors, failed request paths, and HTTP
  status/content type for failed requests. Ignore unrelated browser extension
  errors. Redact query values if they could contain user data.

### Screener filters and empty states

- Select Stockbit, Pintu, Pluang, and Tokocrypto one at a time. Confirm exactly
  one app button is pressed and that product/category choices match that app.
  Tokocrypto tokenized stocks must remain a separate category from crypto spot.
- Check **All** and each available product category, then check `15m`, `30m`,
  and another available timeframe. Confirm the selected labels and results
  update together. Search for a visible symbol when present; then search for a
  deliberately unmatched string and clear it.
- Treat zero rows as a valid outcome only when the current screener and Signal
  availability explain the result or show that checks are unknown/pending. A
  failed request, stale snapshot, filtered-out set, and completed scan with no
  qualifying setup are different states; record which one the UI and response
  show. Never require the app to fabricate a low-score signal.
- Do not compare notification history/unread count to current screener rows;
  saved notifications can outlive the current signal snapshot. Verify that any
  **View current signal** action opens a matching current row only if it is
  already available without triggering a scan.
- Changing app, product, timeframe, or search is a read-only UI action. Do not
  click **Refresh signals**, **Refresh market data**, **Retry listings**, or
  other controls that schedule scans or provider syncs during a routine
  production smoke check.

### API response integrity

- Observe the browser's own requests for `/api/status` and `/api/signals` while
  using the filters. If needed, issue same-origin `fetch` calls from the loaded
  page using only GET requests and the app/product/timeframe values shown in
  the UI.
- For every JSON API response, check HTTP success, a JSON content type, valid
  JSON, and the expected top-level shape before interpreting counts or rows.
  If a supposed API response starts with HTML (for example `<!doctype`), report
  the exact endpoint, status, content type, and a short redacted body prefix;
  classify it as an API/proxy/routing failure, not as an empty signal set.
- Do not treat a successful status endpoint as proof that signal, chart, or
  readiness endpoints succeeded. Record each relevant request independently.

### Signal selection and chart

- When a current signal row exists, select one. Confirm the selected app,
  product, ticker, timeframe, and direction match the row and chart context.
- Confirm a chart-data request completes and returns candles before judging the
  canvas. If candles are returned, check that the chart, OHLC readout, axes,
  and applicable NWE/DSS status render; a populated response with a blank chart
  is a rendering defect. Capture a screenshot at desktop and mobile sizes.
- Exercise chart timeframe selection, fit/reset, and one basic pan/zoom action
  only when the chart has data. Keep the signal's timeframe distinct from the
  chart's displayed timeframe.
- If no current row exists, do not fabricate one or trigger a scan. Verify the
  empty/readiness reason and record that chart selection could not be tested.

### Mobile and accessibility pass

- At `390 × 844`, verify the app and product controls remain reachable, the
  selected signal and chart labels are readable, buttons do not overlap, and
  the page has no accidental horizontal overflow outside intended scroll
  containers.
- On narrow screens, verify the initial DOM contains no more than 12 signal
  cards and that **Load more matching signals** reveals more rows without
  skipping loaded matches or fetching another page unnecessarily.
- Use keyboard Tab/Shift+Tab to reach the app selector, filters, signal row,
  chart controls, and page links. Confirm visible focus and accessible names;
  do not rely on color alone to distinguish stale, empty, selected, or error
  states.

## Report findings

For each check, label it **pass**, **fail**, **blocked**, or **not tested**.
Include the deployed build/version, time and timezone, viewport, app/product/
timeframe, the exact UI state, relevant HTTP status/content type, concise
reproduction steps, and local screenshot/log paths. Classify impact and separate
observed defects from hypotheses. A cold-start 503 or unavailable network is a
blocker; do not claim the dashboard itself passed or failed until its UI loads.
Do not create an issue, PR, deployment, or data mutation from this skill.
