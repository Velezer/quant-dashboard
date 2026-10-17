use super::{score_four_indicators, StrategyBar};

fn bar(index: usize, open: f64, high: f64, low: f64, close: f64, volume: f64) -> StrategyBar {
    StrategyBar {
        time: index as i64,
        open,
        high,
        low,
        close,
        volume,
    }
}

#[test]
fn four_indicator_score_is_equal_weighted_and_uses_prior_relative_volume() {
    let mut bars = (0..20)
        .map(|index| bar(index, 99.0, 101.0, 98.0, 100.0, 100.0))
        .collect::<Vec<_>>();
    bars.push(bar(20, 100.0, 105.0, 99.0, 105.0, 200.0));

    let score = score_four_indicators(&bars, "LONG", "NWE_CROSS", 10.0, "DSS_CROSS", 10.0)
        .expect("valid latest bar should score");

    assert_eq!(score.components.len(), 4);
    assert_eq!(score.components[0].points, 10.0);
    assert_eq!(score.components[1].points, 10.0);
    assert_eq!(score.components[2].points, 10.0);
    assert_eq!(score.components[2].value, Some(2.0));
    assert_eq!(score.components[3].points, 10.0);
    assert_eq!(score.price_score, 20.0);
    assert_eq!(score.volume_score, 20.0);
    assert_eq!(score.total_score, 40.0);
}

#[test]
fn unavailable_or_opposed_volume_evidence_never_adds_points() {
    let mut bars = (0..20)
        .map(|index| bar(index, 99.0, 101.0, 98.0, 100.0, 100.0))
        .collect::<Vec<_>>();
    bars.push(bar(20, 105.0, 106.0, 100.0, 100.0, 200.0));
    let opposed = score_four_indicators(&bars, "LONG", "NWE_CROSS", 10.0, "DSS_CROSS", 10.0)
        .expect("valid history should score");
    assert_eq!(opposed.components[2].state, "OPPOSED");
    assert_eq!(opposed.components[2].points, 0.0);
    assert_eq!(opposed.components[3].state, "OPPOSED_PROXY");
    assert_eq!(opposed.components[3].points, 0.0);

    bars[20].volume = 0.0;
    let unavailable = score_four_indicators(&bars, "LONG", "NWE_CROSS", 10.0, "DSS_CROSS", 10.0)
        .expect("price setup remains scoreable");
    assert_eq!(unavailable.components[2].state, "UNAVAILABLE");
    assert_eq!(unavailable.components[3].state, "UNAVAILABLE");
    assert_eq!(unavailable.volume_score, 0.0);
    assert_eq!(unavailable.total_score, 20.0);
}
