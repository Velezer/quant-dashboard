use super::{next_scan_epoch_after_completion, AppState, PineBacktestJobs};

#[test]
fn pine_backtest_jobs_allow_one_active_job_and_expose_monotonic_progress() {
    let jobs = PineBacktestJobs::default();
    let job = jobs.start().expect("the first bounded job should start");
    assert!(
        jobs.start().is_err(),
        "a second job must not overlap the first"
    );
    job.set_running();
    assert!(job.set_progress(5, 10, "evaluating", 0, 8_000));
    assert!(job.set_progress(3, 10, "evaluating", 0, 8_000));
    let response = job.response();
    assert_eq!(response["state"], "running");
    assert_eq!(response["progress_pct"], 40.0);
    assert_eq!(response["phase"], "evaluating");
}

#[test]
fn pine_backtest_job_cancellation_is_cooperative_and_terminal_results_are_bounded() {
    let jobs = PineBacktestJobs::default();
    let job = jobs.start().expect("the job should start");
    job.set_running();
    assert_eq!(job.request_cancel(), "cancelling");
    assert!(!job.set_progress(1, 10, "evaluating", 0, 8_000));
    assert!(job
        .complete(serde_json::json!({"research_only": true}))
        .is_err());
    assert_eq!(job.response()["state"], "cancelled");
    assert!(job.response()["result"].is_null());
    let completed = jobs
        .start()
        .expect("terminal jobs should release the single worker slot");
    assert!(completed
        .complete(serde_json::json!({"research_only": true}))
        .is_ok());
    assert_eq!(completed.response()["state"], "completed");
}

#[test]
fn scan_progress_reports_current_symbol_and_completion() {
    let state = AppState::new("unused.sqlite3".into(), 900);

    state.update_scan_progress("BTC-USD", 2, 5);

    assert_eq!(
        state
            .current_scan_symbol
            .lock()
            .expect("scan symbol lock should be available")
            .as_str(),
        "BTC-USD"
    );
    assert_eq!(
        *state
            .scan_progress_pct
            .lock()
            .expect("progress lock should be available"),
        40.0
    );

    state.complete_scan_progress();

    assert_eq!(
        state
            .current_scan_symbol
            .lock()
            .expect("scan symbol lock should be available")
            .as_str(),
        ""
    );
    assert_eq!(
        *state
            .scan_progress_pct
            .lock()
            .expect("progress lock should be available"),
        100.0
    );
}

#[test]
fn concurrent_scan_progress_never_moves_backwards() {
    let state = AppState::new("unused.sqlite3".into(), 900);

    state.update_scan_progress("TOKO-1", 12, 100);
    state.update_scan_progress("STOCK-1", 11, 100);

    assert_eq!(
        *state
            .scan_progress_pct
            .lock()
            .expect("progress lock should be available"),
        12.0
    );
    assert_eq!(
        state
            .current_scan_symbol
            .lock()
            .expect("scan symbol lock should be available")
            .as_str(),
        "TOKO-1"
    );
}

#[test]
fn failed_scan_progress_resets_to_stale_idle_state() {
    let state = AppState::new("unused.sqlite3".into(), 900);
    state.update_scan_progress("AAPL", 3, 4);

    state.fail_scan_progress();

    assert_eq!(
        state
            .current_scan_symbol
            .lock()
            .expect("scan symbol lock should be available")
            .as_str(),
        ""
    );
    assert_eq!(
        *state
            .scan_progress_pct
            .lock()
            .expect("progress lock should be available"),
        0.0
    );
}

#[test]
fn successful_scan_keeps_the_fixed_start_to_start_interval() {
    let scan_started_epoch = 10_000;
    let completed_epoch = scan_started_epoch + 8 * 60 + 47;
    let interval_secs = 15 * 60;
    let next_scan_epoch =
        next_scan_epoch_after_completion(scan_started_epoch, completed_epoch, interval_secs, true);

    assert_eq!(next_scan_epoch, scan_started_epoch + interval_secs);
}

#[test]
fn overdue_scan_target_runs_only_after_the_current_scan_completes() {
    let scan_started_epoch = 10_000;
    let completed_epoch = scan_started_epoch + 16 * 60;
    let next_scan_epoch =
        next_scan_epoch_after_completion(scan_started_epoch, completed_epoch, 15 * 60, true);

    assert_eq!(next_scan_epoch, scan_started_epoch + 15 * 60);
    assert!(next_scan_epoch < completed_epoch);
}

#[test]
fn failed_scan_retries_after_one_minute() {
    let scan_started_epoch = 10_000;
    let completed_epoch = scan_started_epoch + 500;
    let next_scan_epoch =
        next_scan_epoch_after_completion(scan_started_epoch, completed_epoch, 900, false);

    assert_eq!(next_scan_epoch, completed_epoch + 60);
}
