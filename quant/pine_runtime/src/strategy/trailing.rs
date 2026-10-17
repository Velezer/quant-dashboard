#[derive(Clone, Copy, Default)]
pub(super) struct TrailingState {
    pub(super) best_price: Option<f64>,
    pub(super) stop_price: Option<f64>,
}

pub(super) fn fill_event(
    direction: i8,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    activation_price: f64,
    offset: f64,
    start_distance: f64,
    through_distance: f64,
    mut state: TrailingState,
) -> (Option<(f64, f64)>, TrailingState) {
    let full_path = super::inferred_ohlc_path(open, high, low, close);
    let mut full_points = vec![(full_path[0], 0.0)];
    for segment in full_path.windows(2) {
        let distance =
            full_points.last().expect("path starts at open").1 + (segment[1] - segment[0]).abs();
        full_points.push((segment[1], distance));
    }
    let mut path = Vec::new();
    for (index, segment) in full_points.windows(2).enumerate() {
        if start_distance <= segment[1].1 {
            let length = segment[1].1 - segment[0].1;
            let fraction = if length == 0.0 {
                0.0
            } else {
                ((start_distance - segment[0].1) / length).clamp(0.0, 1.0)
            };
            let start_price = segment[0].0 + (segment[1].0 - segment[0].0) * fraction;
            path.push((start_price, start_distance));
            path.extend(full_points.iter().skip(index + 1).copied());
            break;
        }
    }
    if path.is_empty() {
        path.push((close, start_distance));
    }
    let start_price = path[0].0;
    if let Some(stop) = state.stop_price {
        let breached_at_start = if direction > 0 {
            start_price <= stop
        } else {
            start_price >= stop
        };
        if breached_at_start && start_distance <= through_distance {
            return (Some((start_distance, start_price)), state);
        }
        update_best(direction, start_price, offset, &mut state);
    } else if activation_reached(direction, start_price, activation_price) {
        activate(direction, start_price, offset, &mut state);
    }

    for segment in path.windows(2) {
        let from = segment[0].0;
        let to = segment[1].0;
        let segment_length = (to - from).abs();
        let segment_start = segment[0].1;
        let allowed_length = (through_distance - segment_start).clamp(0.0, segment_length);
        let endpoint = if segment_length == 0.0 {
            to
        } else {
            from + (to - from) * (allowed_length / segment_length)
        };

        if state.stop_price.is_none() {
            if activation_crossed(direction, from, endpoint, activation_price) {
                activate(direction, activation_price, offset, &mut state);
                update_best(direction, endpoint, offset, &mut state);
            }
        } else {
            let stop = state.stop_price.expect("active trail has a stop");
            let favorable = if direction > 0 {
                endpoint > from
            } else {
                endpoint < from
            };
            if favorable {
                update_best(direction, endpoint, offset, &mut state);
            } else if allowed_length > 0.0 && stop_crossed(direction, from, endpoint, stop) {
                let distance = segment_start + (stop - from).abs();
                if distance <= through_distance {
                    return (Some((distance, stop)), state);
                }
            }
        }

        if allowed_length < segment_length {
            break;
        }
    }
    (None, state)
}

fn activation_reached(direction: i8, price: f64, activation: f64) -> bool {
    if direction > 0 {
        price >= activation
    } else {
        price <= activation
    }
}

fn activation_crossed(direction: i8, from: f64, to: f64, activation: f64) -> bool {
    if direction > 0 {
        from < activation && to >= activation
    } else {
        from > activation && to <= activation
    }
}

fn stop_crossed(direction: i8, from: f64, to: f64, stop: f64) -> bool {
    if direction > 0 {
        from >= stop && to <= stop
    } else {
        from <= stop && to >= stop
    }
}

fn activate(direction: i8, price: f64, offset: f64, state: &mut TrailingState) {
    state.best_price = Some(price);
    state.stop_price = Some(price - f64::from(direction) * offset);
}

fn update_best(direction: i8, price: f64, offset: f64, state: &mut TrailingState) {
    let improves = state.best_price.is_none_or(|best| {
        if direction > 0 {
            price > best
        } else {
            price < best
        }
    });
    if improves {
        state.best_price = Some(price);
        state.stop_price = Some(price - f64::from(direction) * offset);
    }
}
