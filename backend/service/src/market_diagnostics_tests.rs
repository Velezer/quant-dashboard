#[test]
fn unfiltered_storage_summary_bounds_ticker_database_inspection() {
    let inventory = (0..125)
        .map(|index| {
            (
                format!("SYMBOL-{index:03}"),
                format!("file-{index:03}.sqlite3"),
            )
        })
        .collect::<Vec<_>>();

    let inspected = super::summarized_market_inventory(&inventory).collect::<Vec<_>>();

    assert_eq!(inspected.len(), super::STORAGE_SUMMARY_SYMBOL_LIMIT);
    assert_eq!(
        inspected.first().map(|row| row.0.as_str()),
        Some("SYMBOL-000")
    );
    assert_eq!(
        inspected.last().map(|row| row.0.as_str()),
        Some("SYMBOL-049")
    );
}
