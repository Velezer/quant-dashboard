use super::*;

fn bars(count: usize) -> Vec<StrategyBar> {
    (0..count).map(|index| {
        let open = 100.0 + (index as f64 / 8.0).sin() * 5.0;
        let close = 100.0 + ((index + 1) as f64 / 8.0).sin() * 5.0;
        StrategyBar { time: index as i64 * 4 * crate::SECONDS_PER_HOUR * crate::NANOS_PER_SECOND,
            open, close, high: open.max(close) + 0.5, low: open.min(close) - 0.5, volume: 1000.0 + index as f64 }
    }).collect()
}

#[test]
fn sampled_entry_scores_match_live_prefix_and_do_not_change_after_future_bars() {
    let history = bars(650);
    let collect = |slice: &[StrategyBar]| {
        let mut rows = Vec::new();
        visit_sampled_entry_scores(slice, EntryModelKind::Dss, |index| index >= 510 && index % 24 == 0,
            |index, score| rows.push((index,score)));
        rows
    };
    let original = collect(&history[..600]);
    assert!(!original.is_empty());
    let extended = collect(&history);
    for (index, score) in original {
        let signal = generate_signed_entry_signal(&history[..=index], EntryModelKind::Dss).unwrap();
        let live = score_entry_signal(&history[..=index], &signal, EntryModelKind::Dss).unwrap();
        assert!((live.total_score - score).abs() < 1e-9);
        assert!((extended.iter().find(|(at,_)| *at == index).unwrap().1 - score).abs() < 1e-9);
    }
}

#[test]
fn sampled_entry_scores_do_not_emit_before_model_warmup() {
    let mut count = 0;
    visit_sampled_entry_scores(&bars(5), EntryModelKind::Dss, |_| true, |_,_| count += 1);
    assert_eq!(count,0);
}
