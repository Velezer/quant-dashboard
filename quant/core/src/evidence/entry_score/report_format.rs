use super::{TradeSummary, Value};

pub(super) fn summary_json(summary: TradeSummary, fees_known: bool) -> Value {
    let mut value = summary.to_json(fees_known);
    if value["profit_factor_unbounded"] == true {
        value["profit_factor"] = Value::Null;
    }
    value
}

pub(super) fn score_bands_json(bands: &[TradeSummary], width: f64, fees_known: bool) -> Value {
    let last_band = bands.len().saturating_sub(1);
    Value::Array(
        bands
            .iter()
            .enumerate()
            .map(|(index, summary)| {
                let mut value = summary_json(*summary, fees_known);
                value["label"] = Value::from(format!(
                    "{}–{}{}",
                    index as f64 * width,
                    (index + 1) as f64 * width,
                    if index == last_band {
                        " inclusive"
                    } else {
                        " (upper exclusive)"
                    }
                ));
                value["min_score"] = Value::from(index as f64 * width);
                value["max_score"] = Value::from((index + 1) as f64 * width);
                value["max_inclusive"] = Value::from(index == last_band);
                value
            })
            .collect(),
    )
}

