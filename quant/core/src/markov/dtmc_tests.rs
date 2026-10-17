use super::*;

#[test]
fn transition_matrix_and_stationary_distribution_are_normalized() {
    let seq = vec![0, 0, 0, 3, 3, 1, 1, 2, 0, 0];
    let model = DtmcModel::from_sequences(&[seq], 6);
    for row in &model.transition_matrix {
        assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-6);
    }
    assert!((model.stationary_distribution.iter().sum::<f64>() - 1.0).abs() < 1e-6);
}

#[test]
fn sparse_rows_report_support_and_use_symmetric_prior() {
    let model = DtmcModel::from_sequences(&[vec![0, 1]], 2);
    assert_eq!(model.transition_row_support, vec![1, 0]);
    assert_eq!(model.transition_matrix[0], vec![0.25, 0.75]);
    assert_eq!(model.transition_matrix[1], vec![0.5, 0.5]);
    assert_eq!(model.predict_next_state(1), (1, 0.0));
}
