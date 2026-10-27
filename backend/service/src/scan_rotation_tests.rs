use super::run::next_scan_worker;

fn rotation(mut remaining: Vec<usize>) -> Vec<usize> {
    let mut cursor = 0;
    let mut order = Vec::new();
    while let Some(worker) = next_scan_worker(&remaining, &mut cursor) {
        assert!(remaining[worker] > 0, "an exhausted worker was selected");
        remaining[worker] -= 1;
        order.push(worker);
    }
    assert!(remaining.iter().all(|count| *count == 0));
    order
}

#[test]
fn equal_queues_rotate_stockbit_then_each_crypto_app() {
    // Worker order: Stockbit, Pintu, Pluang, Tokocrypto.
    assert_eq!(rotation(vec![2, 2, 2, 2]), vec![0, 1, 2, 3, 0, 1, 2, 3]);
}

#[test]
fn uneven_queues_skip_exhausted_apps_without_losing_tickers() {
    assert_eq!(rotation(vec![2, 4, 0, 1]), vec![0, 1, 3, 0, 1, 1, 1]);
    assert_eq!(rotation(vec![0, 1, 2, 0]), vec![1, 2, 2]);
}

#[test]
fn empty_queues_end_and_cursor_wraps() {
    assert_eq!(next_scan_worker(&[], &mut 0), None);
    assert_eq!(next_scan_worker(&[0, 0, 0, 0], &mut 0), None);
    let mut cursor = 3;
    assert_eq!(next_scan_worker(&[1, 0, 0, 0], &mut cursor), Some(0));
}
