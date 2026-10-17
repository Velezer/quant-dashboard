export const NWE_MAX_OUTSIDE_DISTANCE_ID = 'builtin:nwe-max-outside-distance';
export const NWE_UPPER_SOURCE_TITLE = 'NWE Upper';
export const NWE_LOWER_SOURCE_TITLE = 'NWE Lower';

export const NWE_MAX_OUTSIDE_DISTANCE_SOURCE = `//@version=6
// Add a Nadaraya-Watson Envelope to the chart, then select its Upper and Lower
// plots in these two source inputs. The app binds its active NWE bands directly.
indicator("NWE Max Outside Distance (%)", overlay=false)
float nweUpper = input.source(close, "${NWE_UPPER_SOURCE_TITLE}")
float nweLower = input.source(close, "${NWE_LOWER_SOURCE_TITLE}")
var bool trackingUpper = false
var float activeUpperPct = 0.0
var float longestUpperPct = na
bool wasTrackingUpper = trackingUpper
bool upperTouched = not na(nweUpper) and low <= nweUpper and high >= nweUpper
float upperDistancePct = not na(nweUpper) and nweUpper > 0 and high > nweUpper ? (high - nweUpper) / nweUpper * 100.0 : 0.0
float upperCandidatePct = wasTrackingUpper ? math.max(activeUpperPct, upperDistancePct) : upperDistancePct
bool upperEventActive = upperTouched or (wasTrackingUpper and close > nweUpper)
activeUpperPct := upperEventActive ? upperCandidatePct : 0.0
longestUpperPct := (upperTouched or wasTrackingUpper) ? math.max(nz(longestUpperPct, 0.0), upperCandidatePct) : longestUpperPct
trackingUpper := upperEventActive
var bool trackingLower = false
var float activeLowerPct = 0.0
var float longestLowerPct = na
bool wasTrackingLower = trackingLower
bool lowerTouched = not na(nweLower) and low <= nweLower and high >= nweLower
float lowerDistancePct = not na(nweLower) and nweLower > 0 and low < nweLower ? (nweLower - low) / nweLower * 100.0 : 0.0
float lowerCandidatePct = wasTrackingLower ? math.max(activeLowerPct, lowerDistancePct) : lowerDistancePct
bool lowerEventActive = lowerTouched or (wasTrackingLower and close < nweLower)
activeLowerPct := lowerEventActive ? lowerCandidatePct : 0.0
longestLowerPct := (lowerTouched or wasTrackingLower) ? math.max(nz(longestLowerPct, 0.0), lowerCandidatePct) : longestLowerPct
trackingLower := lowerEventActive
plot(longestUpperPct, title="Max above upper band (%)", color=color.teal)
plot(longestLowerPct, title="Max below lower band (%)", color=color.red)
`;

export const NWE_ENDPOINT_ENVELOPE_SOURCE = `//@version=6
// Endpoint adaptation of Nadaraya-Watson Envelope [LuxAlgo].
// © LuxAlgo · CC BY-NC-SA 4.0 · https://creativecommons.org/licenses/by-nc-sa/4.0/
// This source is noncommercial; keep the attribution and license for adaptations.
indicator("Nadaraya-Watson Envelope · Endpoint", overlay=true, max_bars_back=1000)
int windowBars = input.int(500, "Window (bars)", minval=2, maxval=500)
float bandwidthBars = input.float(8.0, "Bandwidth (bars)", minval=0.1, maxval=500.0)
float envelopeMultiplier = input.float(3.0, "Envelope multiplier", minval=0.0, maxval=100.0)
float weightedClose = 0.0
float weightTotal = 0.0
for age = 0 to windowBars - 1
    float ageBars = age
    float weight = math.exp(-(ageBars * ageBars) / (bandwidthBars * bandwidthBars * 2.0))
    weightedClose += close[age] * weight
    weightTotal += weight
float midline = weightTotal > 0.0 ? weightedClose / weightTotal : na
float meanAbsoluteError = ta.sma(math.abs(close - midline), windowBars - 1)
float upper = midline + meanAbsoluteError * envelopeMultiplier
float lower = midline - meanAbsoluteError * envelopeMultiplier
plot(upper, title="Upper", color=color.teal)
plot(lower, title="Lower", color=color.red)
`;

export const BUILTIN_PINE_INDICATORS = Object.freeze([
    {
        id: NWE_MAX_OUTSIDE_DISTANCE_ID,
        name: 'NWE Max Outside Distance (%)',
        author: 'Quant Research',
        rights_basis: 'original',
        source_reference: 'Project implementation; depends on Nadaraya-Watson upper and lower plot sources.',
        source: NWE_MAX_OUTSIDE_DISTANCE_SOURCE,
        runnable: true,
    },
    {
        id: 'builtin:nwe-endpoint-envelope',
        name: 'Nadaraya-Watson Envelope · Endpoint',
        author: 'LuxAlgo adaptation',
        rights_basis: 'CC BY-NC-SA 4.0',
        source_reference: 'https://creativecommons.org/licenses/by-nc-sa/4.0/',
        source: NWE_ENDPOINT_ENVELOPE_SOURCE,
        runnable: false,
    },
]);
