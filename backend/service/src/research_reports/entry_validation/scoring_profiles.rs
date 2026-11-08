use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MIN_PROFILE_SYMBOLS: usize = 5;
const MIN_PROFILE_TRADES: usize = 30;
const MIN_COHORT_COVERAGE_PCT: f64 = 80.0;
const TARGET_AFTER_FEE_WIN_RATE_PCT: f64 =
    quant_core::ENTRY_SCORE_TARGET_AFTER_FEE_WIN_RATE_PCT;

pub(crate) type ScoringProfileKey = (String, String, String, String);
pub(crate) type ActiveScoringProfiles = BTreeMap<ScoringProfileKey, ActiveScoringProfile>;

#[derive(Clone, Debug)]
pub(crate) struct ActiveScoringProfile {
    pub(crate) version: i64,
    pub(crate) weights: [u8; 4],
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Scope {
    score_model_id: String,
    asset_category_id: String,
    timeframe: String,
    source_family: String,
}

#[derive(Default)]
struct Cohort {
    report_count: usize,
    product_report_counts: BTreeMap<(String, String), usize>,
    candidates: BTreeMap<[u8; 4], CandidateEvidence>,
}

#[derive(Default)]
struct CandidateEvidence {
    symbol_count: usize,
    validation: MetricTotals,
    holdout: MetricTotals,
    baseline_validation: MetricTotals,
    baseline_holdout: MetricTotals,
    minimum_history_days: i64,
    minimum_holdout_days: i64,
    product_slices: BTreeMap<(String, String), CandidateSlice>,
}

#[derive(Default)]
struct CandidateSlice {
    symbol_count: usize,
    validation: MetricTotals,
    holdout: MetricTotals,
    baseline_validation: MetricTotals,
    baseline_holdout: MetricTotals,
    minimum_history_days: i64,
    minimum_holdout_days: i64,
}

#[derive(Default, Clone, Copy)]
struct MetricTotals {
    trades: usize,
    wins: usize,
    losses: usize,
    net_sum_pct: f64,
    positive_sum_pct: f64,
    negative_sum_pct: f64,
}

impl MetricTotals {
    fn add_json(&mut self, summary: &Value) {
        let trades = summary.get("trades").and_then(Value::as_u64).unwrap_or(0) as usize;
        if trades == 0 || summary.get("fees_applied").and_then(Value::as_bool) != Some(true) {
            return;
        }
        let wins = summary.get("wins").and_then(Value::as_u64).unwrap_or(0) as usize;
        self.trades = self.trades.saturating_add(trades);
        self.wins = self.wins.saturating_add(wins);
        let losses = summary.get("losses").and_then(Value::as_u64).unwrap_or_else(|| {
            trades.saturating_sub(
                summary.get("wins").and_then(Value::as_u64).unwrap_or(0) as usize,
            ) as u64
        }) as usize;
        self.losses = self.losses.saturating_add(losses);
        let mean = summary.get("mean_net_return_pct").and_then(Value::as_f64);
        if let Some(mean) = mean.filter(|value| value.is_finite()) {
            self.net_sum_pct += mean * trades as f64;
        }
        let average_win = summary.get("average_win_pct").and_then(Value::as_f64);
        if let Some(value) = average_win.filter(|value| value.is_finite()) {
            self.positive_sum_pct += value * wins as f64;
        }
        let average_loss = summary.get("average_loss_pct").and_then(Value::as_f64);
        if let Some(value) = average_loss.filter(|value| value.is_finite()) {
            self.negative_sum_pct += value * losses as f64;
        }
    }

    fn mean_net(self) -> Option<f64> {
        (self.trades > 0).then(|| self.net_sum_pct / self.trades as f64)
    }

    fn win_rate(self) -> Option<f64> {
        (self.trades > 0).then(|| self.wins as f64 / self.trades as f64 * 100.0)
    }

    fn profit_factor(self) -> Option<f64> {
        if self.negative_sum_pct > 0.0 {
            Some(self.positive_sum_pct / self.negative_sum_pct)
        } else if self.positive_sum_pct > 0.0 {
            Some(99.0)
        } else {
            None
        }
    }

    fn wilson_lower_pct(self) -> Option<f64> {
        if self.trades == 0 {
            return None;
        }
        let z = 1.959_963_984_540_054;
        let n = self.trades as f64;
        let p = self.wins as f64 / n;
        let denominator = 1.0 + z * z / n;
        let center = (p + z * z / (2.0 * n)) / denominator;
        let half_width = z * ((p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt()) / denominator;
        Some((center - half_width).max(0.0) * 100.0)
    }

    fn target_passes(self) -> bool {
        self.trades >= MIN_PROFILE_TRADES
            && self
                .win_rate()
                .is_some_and(|rate| rate >= TARGET_AFTER_FEE_WIN_RATE_PCT)
            && self
                .wilson_lower_pct()
                .is_some_and(|lower| lower >= TARGET_AFTER_FEE_WIN_RATE_PCT)
    }

    fn as_json(self) -> Value {
        json!({
            "trades": self.trades,
            "wins": self.wins,
            "losses": self.losses,
            "win_rate_after_fees_pct": self.win_rate(),
            "win_rate_after_fees_wilson_95_lower_pct": self.wilson_lower_pct(),
            "mean_net_return_pct": self.mean_net(),
            "profit_factor": self.profit_factor(),
        })
    }
}

fn value_text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn weights_from(value: &Value) -> Option<[u8; 4]> {
    let values = value.as_array()?;
    if values.len() != 4 {
        return None;
    }
    let read = |index: usize| {
        values
            .get(index)?
            .as_u64()
            .and_then(|weight| u8::try_from(weight).ok())
    };
    let weights = [read(0)?, read(1)?, read(2)?, read(3)?];
    (weights.iter().all(|weight| weight % 5 == 0)
        && weights.iter().map(|weight| u16::from(*weight)).sum::<u16>() == 40)
        .then_some(weights)
}

fn weight_distance_from_equal(weights: [u8; 4]) -> u16 {
    weights
        .iter()
        .map(|weight| u16::from((*weight).abs_diff(10)))
        .sum()
}

fn add_candidate_summary(
    evidence: &mut CandidateEvidence,
    exchange: &str,
    product: &str,
    candidate: &Value,
    baseline: &Value,
    history_days: i64,
    holdout_days: i64,
) {
    let validation = candidate.get("validation").unwrap_or(&Value::Null);
    let holdout = candidate.get("holdout").unwrap_or(&Value::Null);
    let baseline_validation = baseline.get("validation").unwrap_or(&Value::Null);
    let baseline_holdout = baseline.get("holdout").unwrap_or(&Value::Null);
    evidence.symbol_count = evidence.symbol_count.saturating_add(1);
    evidence.validation.add_json(validation);
    evidence.holdout.add_json(holdout);
    evidence.baseline_validation.add_json(baseline_validation);
    evidence.baseline_holdout.add_json(baseline_holdout);
    evidence.minimum_holdout_days = if evidence.minimum_holdout_days == 0 {
        holdout_days
    } else {
        evidence.minimum_holdout_days.min(holdout_days)
    };
    evidence.minimum_history_days = if evidence.minimum_history_days == 0 {
        history_days
    } else {
        evidence.minimum_history_days.min(history_days)
    };
    let slice = evidence
        .product_slices
        .entry((exchange.to_string(), product.to_string()))
        .or_default();
    slice.symbol_count = slice.symbol_count.saturating_add(1);
    slice.validation.add_json(validation);
    slice.holdout.add_json(holdout);
    slice.baseline_validation.add_json(baseline_validation);
    slice.baseline_holdout.add_json(baseline_holdout);
    slice.minimum_holdout_days = if slice.minimum_holdout_days == 0 {
        holdout_days
    } else {
        slice.minimum_holdout_days.min(holdout_days)
    };
    slice.minimum_history_days = if slice.minimum_history_days == 0 {
        history_days
    } else {
        slice.minimum_history_days.min(history_days)
    };
}

fn report_scope(report: &Value, model: &Value) -> Option<Scope> {
    let scope = Scope {
        score_model_id: value_text(model, "score_model_id").to_string(),
        asset_category_id: value_text(report.get("instrument")?, "asset_category_id").to_string(),
        timeframe: value_text(report, "timeframe").to_string(),
        source_family: value_text(report.get("source")?, "id").to_string(),
    };
    (!scope.score_model_id.is_empty()
        && !scope.asset_category_id.is_empty()
        && !scope.timeframe.is_empty()
        && !scope.source_family.is_empty())
        .then_some(scope)
}

fn collect_cohorts(
    connection: &Connection,
    cycle_started_at: i64,
) -> Result<BTreeMap<Scope, Cohort>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT report_json FROM app.entry_model_validation_reports
          WHERE protocol_id=?1 AND computed_at>=?2 ORDER BY instrument_id,timeframe",
    )?;
    let rows = statement.query_map(params![super::PROTOCOL_ID, cycle_started_at], |row| {
        row.get::<_, String>(0)
    })?;
    let mut cohorts = BTreeMap::<Scope, Cohort>::new();
    for encoded in rows {
        let encoded = encoded?;
        let Ok(report) = serde_json::from_str::<Value>(&encoded) else {
            continue;
        };
        let Some(models) = report.get("models").and_then(Value::as_array) else {
            continue;
        };
        let instrument = report.get("instrument").unwrap_or(&Value::Null);
        let exchange = value_text(instrument, "exchange_id").to_string();
        let product = value_text(instrument, "product_id").to_string();
        let history_days = report
            .pointer("/history/coverage_days")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        for model in models {
            let Some(scope) = report_scope(&report, model) else {
                continue;
            };
            let cohort = cohorts.entry(scope).or_default();
            cohort.report_count += 1;
            *cohort
                .product_report_counts
                .entry((exchange.clone(), product.clone()))
                .or_default() += 1;
            let learning = model.pointer("/backtest/weight_learning").unwrap_or(&Value::Null);
            let Some(weights) = weights_from(learning.get("selected_weights").unwrap_or(&Value::Null)) else {
                continue;
            };
            let Some(candidate) = learning.get("candidate") else {
                continue;
            };
            let baseline = learning.get("baseline").unwrap_or(&Value::Null);
            let evidence = cohort.candidates.entry(weights).or_default();
            let holdout_days = model
                .pointer("/backtest/holdout_window_days")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            add_candidate_summary(
                evidence,
                &exchange,
                &product,
                candidate,
                baseline,
                history_days,
                holdout_days,
            );
        }
    }
    Ok(cohorts)
}

fn candidate_decision(
    scope: &Scope,
    cohort: &Cohort,
    weights: [u8; 4],
    evidence: &CandidateEvidence,
    cycle_complete: bool,
    evaluated_at: i64,
) -> Value {
    let coverage_pct = if cohort.report_count == 0 {
        0.0
    } else {
        evidence.symbol_count as f64 / cohort.report_count as f64 * 100.0
    };
    let mut blockers = Vec::new();
    if !cycle_complete {
        blockers.push("The daily backtest cycle contains failures; its cohort is incomplete.");
    }
    if evidence.symbol_count < MIN_PROFILE_SYMBOLS {
        blockers.push("At least five independent registered instruments must share the candidate vector.");
    }
    if coverage_pct + f64::EPSILON < MIN_COHORT_COVERAGE_PCT {
        blockers.push("The candidate vector does not cover at least 80% of the refreshed cohort.");
    }
    if evidence.minimum_history_days < quant_core::MIN_EVIDENCE_SAMPLE_DAYS {
        blockers.push("Every included instrument needs at least 730 calendar days of stored history.");
    }
    if !evidence.validation.target_passes() {
        blockers.push("Validation does not meet the 70% after-fee win-rate target with a 95% Wilson lower bound.");
    }
    if !evidence.holdout.target_passes() {
        blockers.push("The untouched holdout does not meet the 70% after-fee win-rate target with a 95% Wilson lower bound.");
    }
    if evidence.validation.mean_net().zip(evidence.baseline_validation.mean_net())
        .is_none_or(|(candidate, baseline)| candidate <= baseline)
        || evidence.holdout.mean_net().zip(evidence.baseline_holdout.mean_net())
            .is_none_or(|(candidate, baseline)| candidate <= baseline)
    {
        blockers.push("The candidate must improve mean net return over equal weights in validation and holdout.");
    }
    if evidence.validation.profit_factor().is_none_or(|factor| factor <= 1.0)
        || evidence.holdout.profit_factor().is_none_or(|factor| factor <= 1.0)
    {
        blockers.push("Validation and holdout profit factors must both exceed 1 after fees.");
    }
    let product_slices = cohort
        .product_report_counts
        .iter()
        .map(|((exchange, product), expected)| {
            let mut slice_blockers = Vec::new();
            let slice = evidence.product_slices.get(&(exchange.clone(), product.clone()));
            let symbol_count = slice.map(|slice| slice.symbol_count).unwrap_or_default();
            let validation = slice.map(|slice| slice.validation).unwrap_or_default();
            let holdout = slice.map(|slice| slice.holdout).unwrap_or_default();
            let minimum_history_days = slice.map(|slice| slice.minimum_history_days).unwrap_or_default();
            let slice_coverage = if *expected == 0 { 0.0 } else { symbol_count as f64 / *expected as f64 * 100.0 };
            if slice_coverage + f64::EPSILON < MIN_COHORT_COVERAGE_PCT {
                slice_blockers.push("candidate coverage below 80%".to_string());
            }
            if !validation.target_passes() || !holdout.target_passes() {
                slice_blockers.push("validation or holdout misses the 70% after-fee target".to_string());
            }
            if minimum_history_days < quant_core::MIN_EVIDENCE_SAMPLE_DAYS {
                slice_blockers.push("history is shorter than 730 days".to_string());
            }
            if slice
                .is_none_or(|slice| {
                    validation
                        .mean_net()
                        .zip(slice.baseline_validation.mean_net())
                        .is_none_or(|(candidate, baseline)| candidate <= baseline)
                        || holdout
                            .mean_net()
                            .zip(slice.baseline_holdout.mean_net())
                            .is_none_or(|(candidate, baseline)| candidate <= baseline)
                })
            {
                slice_blockers.push("candidate net return does not beat equal weights".to_string());
            }
            if validation.profit_factor().is_none_or(|factor| factor <= 1.0)
                || holdout.profit_factor().is_none_or(|factor| factor <= 1.0)
            {
                slice_blockers.push("profit factor does not exceed 1".to_string());
            }
            json!({"exchange_id":exchange,"product_id":product,"symbols":symbol_count,
                "expected_reports":expected,"coverage_pct":slice_coverage,
                "validation":validation.as_json(),"holdout":holdout.as_json(),
                "blockers":slice_blockers})
        })
        .collect::<Vec<_>>();
    if product_slices.iter().any(|slice| {
        slice.get("blockers").and_then(Value::as_array).is_some_and(|items| !items.is_empty())
    }) {
        blockers.push("Every app/product slice must independently meet cohort coverage and the after-fee target.");
    }
    json!({
        "evaluated_at": evaluated_at,
        "decision": if blockers.is_empty() {"PROMOTED"} else {"BLOCKED"},
        "scope": {"score_model_id":scope.score_model_id,"asset_category_id":scope.asset_category_id,
            "timeframe":scope.timeframe,"source_family":scope.source_family},
        "target_after_fee_win_rate_pct": TARGET_AFTER_FEE_WIN_RATE_PCT,
        "weights": weights,
        "cohort": {"reports":cohort.report_count,"candidate_symbols":evidence.symbol_count,"coverage_pct":coverage_pct,
            "minimum_history_days":evidence.minimum_history_days,"minimum_holdout_days":evidence.minimum_holdout_days},
        "validation": evidence.validation.as_json(),
        "holdout": evidence.holdout.as_json(),
        "equal_weight_validation": evidence.baseline_validation.as_json(),
        "equal_weight_holdout": evidence.baseline_holdout.as_json(),
        "fees_applied": true,
        "return_basis": "after configured transaction fees; spread, slippage, market impact, funding and borrow are not included",
        "product_slices": product_slices,
        "blockers": blockers,
    })
}

fn record_evaluation(
    connection: &Connection,
    scope: &Scope,
    evaluation: &Value,
    promote: bool,
    weights: Option<[u8; 4]>,
    evaluated_at: i64,
) -> Result<(), rusqlite::Error> {
    let evaluation_json = serde_json::to_string(evaluation)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let save_latest = || {
        connection.execute(
        "INSERT INTO app.entry_model_scoring_evaluations
         (score_model_id,asset_category_id,timeframe,source_family,evaluated_at,evaluation_json)
         VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(score_model_id,asset_category_id,timeframe,source_family)
         DO UPDATE SET evaluated_at=excluded.evaluated_at,evaluation_json=excluded.evaluation_json",
            params![scope.score_model_id,scope.asset_category_id,scope.timeframe,scope.source_family,evaluated_at,evaluation_json],
        )
    };
    if !promote {
        save_latest()?;
        return Ok(());
    }
    let weights = weights.expect("promotion requires a complete vector");
    let weights_json = serde_json::to_string(&weights)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        let version = connection.query_row(
            "SELECT COALESCE(MAX(version),0)+1 FROM app.entry_model_scoring_profiles
              WHERE score_model_id=?1 AND asset_category_id=?2 AND timeframe=?3 AND source_family=?4",
            params![scope.score_model_id,scope.asset_category_id,scope.timeframe,scope.source_family],
            |row| row.get::<_, i64>(0),
        )?;
        connection.execute(
            "UPDATE app.entry_model_scoring_profiles SET status='superseded'
              WHERE score_model_id=?1 AND asset_category_id=?2 AND timeframe=?3
                AND source_family=?4 AND status='active'",
            params![scope.score_model_id,scope.asset_category_id,scope.timeframe,scope.source_family],
        )?;
        connection.execute(
            "INSERT INTO app.entry_model_scoring_profiles
             (score_model_id,asset_category_id,timeframe,source_family,version,status,weights_json,protocol_id,evaluation_json,created_at)
             VALUES(?1,?2,?3,?4,?5,'active',?6,?7,?8,?9)",
            params![scope.score_model_id,scope.asset_category_id,scope.timeframe,scope.source_family,version,weights_json,
                quant_core::ENTRY_SCORE_VALIDATION_PROTOCOL_ID,evaluation_json,evaluated_at],
        )?;
        save_latest()?;
        connection.execute_batch("COMMIT")
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK");
    }
    result
}

pub(crate) fn evaluate_daily_profiles(
    connection: &Connection,
    cycle_started_at: i64,
    evaluated_at: i64,
    failed_pairs: usize,
) -> Result<usize, rusqlite::Error> {
    let cohorts = collect_cohorts(connection, cycle_started_at)?;
    let cycle_complete = failed_pairs == 0;
    let mut evaluated = 0;
    for (scope, cohort) in cohorts {
        let candidates = cohort
            .candidates
            .iter()
            .map(|(weights, evidence)| {
                let decision = candidate_decision(
                    &scope,
                    &cohort,
                    *weights,
                    evidence,
                    cycle_complete,
                    evaluated_at,
                );
                (*weights, evidence, decision)
            })
            .collect::<Vec<_>>();
        let selected = candidates.iter().max_by(|left, right| {
            let left_promoted = left.2["decision"].as_str() == Some("PROMOTED");
            let right_promoted = right.2["decision"].as_str() == Some("PROMOTED");
            left_promoted
                .cmp(&right_promoted)
                .then_with(|| left.1.symbol_count.cmp(&right.1.symbol_count))
                .then_with(|| {
                    weight_distance_from_equal(right.0)
                        .cmp(&weight_distance_from_equal(left.0))
                })
                .then_with(|| left.0.cmp(&right.0))
        });
        let (weights, evidence, decision) = if let Some((weights, evidence, decision)) = selected {
            (Some(*weights), Some(*evidence), decision.clone())
        } else {
            let decision = json!({"evaluated_at":evaluated_at,"decision":"BLOCKED",
                "scope":{"score_model_id":scope.score_model_id,"asset_category_id":scope.asset_category_id,
                    "timeframe":scope.timeframe,"source_family":scope.source_family},
                "target_after_fee_win_rate_pct":TARGET_AFTER_FEE_WIN_RATE_PCT,
                "cohort":{"reports":cohort.report_count,"candidate_symbols":0,"coverage_pct":0.0},
                "blockers":["No report in this cohort produced a fee-aware candidate meeting the 70% training target."]});
            (None, None, decision)
        };
        let promote = decision.get("decision").and_then(Value::as_str) == Some("PROMOTED");
        let active_profile: Option<Vec<u8>> = connection
            .query_row(
                "SELECT weights_json,protocol_id FROM app.entry_model_scoring_profiles
                  WHERE score_model_id=?1 AND asset_category_id=?2 AND timeframe=?3
                    AND source_family=?4 AND status='active'",
                params![scope.score_model_id,scope.asset_category_id,scope.timeframe,scope.source_family],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .and_then(|(encoded, protocol)| {
                if protocol != quant_core::ENTRY_SCORE_VALIDATION_PROTOCOL_ID {
                    return None;
                }
                serde_json::from_str(&encoded).ok()
            });
        let same_active = weights.is_some_and(|candidate| {
            active_profile.as_deref() == Some(candidate.as_slice())
        });
        let promote = promote && !same_active;
        let mut decision = decision;
        if same_active && decision["decision"] == "PROMOTED" {
            decision["decision"] = json!("UNCHANGED");
        }
        record_evaluation(connection,&scope,&decision,promote,weights,evaluated_at)?;
        evaluated += 1;
    }
    Ok(evaluated)
}

pub(crate) fn active_profile_snapshot(
    connection: &Connection,
) -> Result<ActiveScoringProfiles, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT score_model_id,asset_category_id,timeframe,source_family,version,weights_json,protocol_id
           FROM app.entry_model_scoring_profiles WHERE status='active'",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut profiles = BTreeMap::new();
    for row in rows {
        let (model, category, timeframe, source, version, encoded, protocol_id) = row?;
        if protocol_id != quant_core::ENTRY_SCORE_VALIDATION_PROTOCOL_ID {
            continue;
        }
        let Some(weights) = serde_json::from_str::<Value>(&encoded)
            .ok()
            .and_then(|value| weights_from(&value))
        else {
            continue;
        };
        profiles.insert(
            (model, category, timeframe, source),
            ActiveScoringProfile { version, weights },
        );
    }
    Ok(profiles)
}

pub(crate) fn scoring_profile_status(connection: &Connection) -> Result<Value, rusqlite::Error> {
    let profiles = {
        let mut statement = connection.prepare(
            "SELECT score_model_id,asset_category_id,timeframe,source_family,version,weights_json,created_at,protocol_id
               FROM app.entry_model_scoring_profiles WHERE status='active'
              ORDER BY score_model_id,asset_category_id,timeframe,source_family",
        )?;
        let rows = statement.query_map([], |row| {
            let weights_json: String = row.get(5)?;
            let protocol_id: String = row.get(7)?;
            Ok(json!({"score_model_id":row.get::<_,String>(0)?,
                "asset_category_id":row.get::<_,String>(1)?,"timeframe":row.get::<_,String>(2)?,
                "source_family":row.get::<_,String>(3)?,"version":row.get::<_,i64>(4)?,
                "weights":serde_json::from_str::<Value>(&weights_json).unwrap_or(Value::Null),
                "created_at":row.get::<_,i64>(6)?,"status":"active",
                "protocol_current":protocol_id == quant_core::ENTRY_SCORE_VALIDATION_PROTOCOL_ID}))
        })?;
        let profiles = rows.collect::<Result<Vec<_>, _>>()?;
        profiles
    };
    let evaluations = {
        let mut statement = connection.prepare(
            "SELECT evaluation_json FROM app.entry_model_scoring_evaluations ORDER BY evaluated_at DESC LIMIT 40",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let evaluations = rows
            .filter_map(Result::ok)
            .filter_map(|encoded| serde_json::from_str::<Value>(&encoded).ok())
            .collect::<Vec<_>>();
        evaluations
    };
    Ok(json!({"target_after_fee_win_rate_pct":TARGET_AFTER_FEE_WIN_RATE_PCT,
        "active_profiles":profiles,"latest_evaluations":evaluations,
        "default_weights":[10,10,10,10],"default_status":"equal-weight fallback until promotion gates pass"}))
}
