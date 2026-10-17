use crate::runtime::{
    Candle, Diagnostic, IndicatorOutput, PlotOutput, Validation, MAX_CANDLES, MAX_SOURCE_BYTES,
};
use quant_core::{calculate_nadaraya_watson_envelope, NadarayaWatsonEnvelopeConfig};

mod dss_bressert;

pub(crate) use dss_bressert::{run_hpotter_dss, validate_hpotter_dss};

const LUXALGO_NWE_TITLE: &str = "Nadaraya-Watson Envelope [LuxAlgo]";
const LUXALGO_NWE_LICENSE: &str = "CC BY-NC-SA 4.0";
const LUXALGO_NWE_LICENSE_URL: &str = "https://creativecommons.org/licenses/by-nc-sa/4.0/";
const NWE_REPAINTING_NOTICE: &str = "This recognized Pine v5 source runs through the LuxAlgo NWE compatibility profile. It uses declared input defaults (Bandwidth, multiplier, close source, and repaint mode); generic Pine v5 execution is unsupported.";
const NWE_DRAWING_NOTICE: &str = "line.new and label.new output is rendered as bounded chart lines and triangle markers; the 1x1 Pine table and exact drawing-object styling are not reproduced.";
const NWE_RIGHTS_NOTICE: &str = "The source declares CC BY-NC-SA 4.0: use must be noncommercial, attribution and the license link must be retained, and adaptations must use the same license.";

#[derive(Clone, Copy)]
struct LuxAlgoNweProfile {
    bandwidth: f64,
    multiplier: f64,
    repaint: bool,
}

pub(crate) fn validate_luxalgo_nwe(source: &str) -> Option<Validation> {
    if !source.contains(LUXALGO_NWE_TITLE) {
        return None;
    }
    Some(match profile_from_source(source) {
        Ok(_) => Validation {
            valid: true,
            script_type: "indicator".to_string(),
            version: 5,
            name: LUXALGO_NWE_TITLE.to_string(),
            overlay: true,
            approximations: vec![
                NWE_REPAINTING_NOTICE.to_string(),
                NWE_DRAWING_NOTICE.to_string(),
                NWE_RIGHTS_NOTICE.to_string(),
            ],
            diagnostics: Vec::new(),
        },
        Err(message) => invalid_nwe(message),
    })
}

pub(crate) fn run_luxalgo_nwe(
    source: &str,
    candles: &[Candle],
) -> Option<Result<IndicatorOutput, Vec<Diagnostic>>> {
    if !source.contains(LUXALGO_NWE_TITLE) {
        return None;
    }
    Some(run_profile(source, candles))
}

fn profile_from_source(source: &str) -> Result<LuxAlgoNweProfile, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(format!(
            "Pine source exceeds the {MAX_SOURCE_BYTES} byte limit."
        ));
    }
    let normalized = normalize_markup(source);
    let compact = compact_executable(&normalized);
    let valid_version = normalized.lines().any(|line| {
        line.trim_start_matches('*')
            .trim()
            .starts_with("//@version=5")
    });
    if !valid_version {
        return Err("The LuxAlgo NWE compatibility profile requires the supplied Pine v5 version directive.".to_string());
    }
    if !normalized.contains(LUXALGO_NWE_LICENSE)
        || !normalized.contains("© LuxAlgo")
        || !normalized.contains(LUXALGO_NWE_LICENSE_URL)
    {
        return Err(
            "The LuxAlgo source must retain its CC BY-NC-SA 4.0 and © LuxAlgo notices.".to_string(),
        );
    }
    if compact.contains("strategy(") || compact.contains("request.") {
        return Err(
            "This indicator profile cannot execute strategies or external data requests."
                .to_string(),
        );
    }
    let required_fragments = [
        "gauss(x,h)=>math.exp(-(math.pow(x,2)/(h*h*2)))",
        "mae=ta.sma(math.abs(src-out),499)*mult",
        "math.min(499,n-1)",
        "barstate.islastandrepaint",
        "plot(repaint?na:out+mae,'Upper',upCss)",
        "plot(repaint?na:out-mae,'Lower',dnCss)",
        "plotshape(ta.crossunder(close,out-mae)?low:na",
        "plotshape(ta.crossover(close,out+mae)?high:na",
        "array.new_line(0)",
        "array.new_float(0)",
        "line.new(",
        "label.new(",
        "table.new(",
        "tb.cell(",
        "src=input(close,'Source')",
        "ifbarstate.isfirstandnotrepaint",
        "ifnotrepaint",
        "out+=src[i]*coefs.get(i)",
        "out/=den",
        "y2:=sum/sumw",
        "nwe.push(y2)",
        "sae:=sae/math.min(499,n-1)*mult",
        "ifsrc[i]>nwe.get(i)+saeandsrc[i+1]<nwe.get(i)+sae",
        "ifsrc[i]<nwe.get(i)-saeandsrc[i+1]>nwe.get(i)-sae",
    ];
    if !required_fragments
        .iter()
        .all(|fragment| compact.contains(fragment))
        || compact.matches("plot(").count() != 2
        || compact.matches("plotshape(").count() != 2
        || compact.matches("input.float(").count() != 2
        || compact.matches("indicator(").count() != 1
        || compact.matches("line.new(").count() != 3
        || compact.matches("label.new(").count() != 2
        || compact.matches("table.new(").count() != 1
        || compact.matches("tb.cell(").count() != 1
        || compact.matches("for").count() != 6
    {
        return Err("This source title matches LuxAlgo NWE, but its code differs from the supported reference profile. No fallback interpretation was applied.".to_string());
    }
    let float_defaults = input_float_defaults(&normalized)?;
    let repaint = if compact.contains("repaint=input(true,'RepaintingSmoothing'") {
        true
    } else if compact.contains("repaint=input(false,'RepaintingSmoothing'") {
        false
    } else {
        return Err("The Repainting Smoothing input default could not be read.".to_string());
    };
    let profile = LuxAlgoNweProfile {
        bandwidth: float_defaults[0],
        multiplier: float_defaults[1],
        repaint,
    };
    if !profile.bandwidth.is_finite()
        || profile.bandwidth <= 0.0
        || profile.bandwidth > 500.0
        || !profile.multiplier.is_finite()
        || profile.multiplier < 0.0
        || profile.multiplier > 100.0
    {
        return Err("The NWE input defaults exceed the bounded runtime ranges.".to_string());
    }
    Ok(profile)
}

fn input_float_defaults(source: &str) -> Result<Vec<f64>, String> {
    let mut defaults = Vec::new();
    let mut remaining = source;
    while let Some(start) = remaining.find("input.float(") {
        let argument_start = start + "input.float(".len();
        let argument = remaining[argument_start..]
            .split_once(',')
            .map(|(argument, _)| argument)
            .ok_or_else(|| "An input.float() default is incomplete.".to_string())?;
        let value = argument
            .trim()
            .parse::<f64>()
            .map_err(|_| "An input.float() default is not a number.".to_string())?;
        defaults.push(value);
        remaining = &remaining[argument_start..];
    }
    if defaults.len() != 2 {
        return Err("The NWE profile expects Bandwidth and multiplier defaults.".to_string());
    }
    Ok(defaults)
}

fn run_profile(source: &str, candles: &[Candle]) -> Result<IndicatorOutput, Vec<Diagnostic>> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("Pine source exceeds the {MAX_SOURCE_BYTES} byte limit."),
        )]);
    }
    let profile =
        profile_from_source(source).map_err(|message| vec![Diagnostic::error(1, 1, message)])?;
    if candles.len() > MAX_CANDLES {
        return Err(vec![Diagnostic::error(
            1,
            1,
            format!("Indicator input exceeds the {MAX_CANDLES} candle limit."),
        )]);
    }
    if candles.iter().any(|candle| {
        ![
            candle.open,
            candle.high,
            candle.low,
            candle.close,
            candle.volume,
        ]
        .iter()
        .all(|value| value.is_finite())
            || candle.close <= 0.0
    }) {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "NWE needs finite candles with positive closes.",
        )]);
    }
    let closes = candles
        .iter()
        .map(|candle| candle.close)
        .collect::<Vec<_>>();
    let config = NadarayaWatsonEnvelopeConfig {
        bandwidth_bars: profile.bandwidth,
        envelope_multiplier: profile.multiplier,
        ..NadarayaWatsonEnvelopeConfig::default()
    };
    let result = calculate_nadaraya_watson_envelope(&closes, &config);
    let mode = if profile.repaint {
        result.repainting
    } else {
        result.endpoint
    };
    let line_kind = if profile.repaint { "dashed" } else { "line" };
    let mut diagnostics = vec![
        Diagnostic::warning(NWE_REPAINTING_NOTICE),
        Diagnostic::warning(NWE_DRAWING_NOTICE),
        Diagnostic::warning(NWE_RIGHTS_NOTICE),
    ];
    if candles.len() < 2 {
        diagnostics.push(Diagnostic::warning(
            "The NWE envelope needs at least two stored candles before it can draw values.",
        ));
    }
    let upper_cross_values =
        marker_values(&mode.crossed_above_upper, candles, profile.repaint, false);
    let lower_cross_values =
        marker_values(&mode.crossed_below_lower, candles, profile.repaint, true);
    Ok(IndicatorOutput {
        valid: true,
        name: LUXALGO_NWE_TITLE.to_string(),
        overlay: true,
        plots: vec![
            plot("Upper", "#14b8a6", line_kind, mode.upper),
            plot("Lower", "#ef4444", line_kind, mode.lower),
            plot("Crossover", "#ef4444", "shape_down", upper_cross_values),
            plot("Crossunder", "#14b8a6", "shape_up", lower_cross_values),
        ],
        diagnostics: std::mem::take(&mut diagnostics),
        requested_timeframes: Vec::new(),
    })
}

fn marker_values(flags: &[bool], candles: &[Candle], repaint: bool, lower: bool) -> Vec<f64> {
    flags
        .iter()
        .zip(candles)
        .map(|(crossed, candle)| {
            if !crossed {
                f64::NAN
            } else if repaint {
                candle.close
            } else if lower {
                candle.low
            } else {
                candle.high
            }
        })
        .collect()
}

fn plot(title: &str, color: &str, kind: &str, values: Vec<f64>) -> PlotOutput {
    PlotOutput {
        title: title.to_string(),
        overlay: true,
        color: color.to_string(),
        kind: kind.to_string(),
        values: values
            .into_iter()
            .map(|value| value.is_finite().then_some(value))
            .collect(),
    }
}

fn invalid_nwe(message: String) -> Validation {
    Validation {
        valid: false,
        script_type: "indicator".to_string(),
        version: 5,
        name: LUXALGO_NWE_TITLE.to_string(),
        overlay: true,
        approximations: Vec::new(),
        diagnostics: vec![Diagnostic::error(1, 1, message)],
    }
}

fn normalize_markup(source: &str) -> String {
    source
        .replace("**", "")
        .replace("&#x20;", " ")
        .replace("&#xA0;", " ")
        .replace("\\*", "*")
        .replace("\\<", "<")
        .replace("\\>", ">")
}

fn compact_executable(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .flat_map(str::chars)
        .filter(|character| !character.is_whitespace())
        .collect()
}
