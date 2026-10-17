use super::{
    generate_tokocrypto_confluence_signals, is_tokenized_stock_confluence_strategy,
    is_tokocrypto_spot_confluence_strategy, tokocrypto_confluence_indicator_ids, StrategySignal,
    TokocryptoConfluenceConfig, TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID,
    TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID, TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
};

#[test]
fn tokocrypto_profiles_use_versioned_two_of_four_research_models() {
    let config = TokocryptoConfluenceConfig::default();

    assert_eq!(config.minimum_agreeing_indicators, 2);
    assert_eq!(tokocrypto_confluence_indicator_ids().len(), 4);
    assert_eq!(
        TOKOCRYPTO_CONFLUENCE_SIGNAL_STRATEGY_ID,
        "tokocrypto_confluence_2_of_4_v1"
    );
    assert_ne!(
        TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID,
        TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID
    );
    assert!(is_tokocrypto_spot_confluence_strategy(
        TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID
    ));
    assert!(is_tokenized_stock_confluence_strategy(
        TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID
    ));
}

#[test]
fn tokocrypto_confluence_still_requires_dss_and_nwe_confirmations() {
    let bars = super::indicator_combination_tests::bars_with_bullish_breakout();
    let signals =
        generate_tokocrypto_confluence_signals(&bars, TokocryptoConfluenceConfig::default());

    assert_eq!(signals.last(), Some(&StrategySignal::default()));
}

#[test]
fn invalid_vote_threshold_disables_the_tokocrypto_signal() {
    let bars = super::indicator_combination_tests::bars_with_bullish_breakout();
    let mut config = TokocryptoConfluenceConfig::default();
    config.minimum_agreeing_indicators = 5;
    let signals = generate_tokocrypto_confluence_signals(&bars, config);

    assert_eq!(signals.last(), Some(&StrategySignal::default()));
}
