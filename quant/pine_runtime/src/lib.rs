mod ast;
mod compatibility;
mod lexer;
mod parser;
mod runtime;
mod strategy;

pub use runtime::{
    capabilities, run_indicator, run_indicator_with_inputs, run_indicator_with_timeframes,
    run_strategy, run_strategy_with_context, run_strategy_with_history_limit,
    run_strategy_with_history_limit_and_magnifier,
    run_strategy_with_history_limit_and_magnifier_and_tick_size,
    run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start,
    run_strategy_with_history_limit_and_magnifier_and_tick_size_and_evaluation_start_and_progress,
    run_strategy_with_inputs, strategy_input_definitions, strategy_uses_bar_magnifier, Candle,
    Diagnostic, IndicatorOutput, PlotOutput, RuntimeCapabilities, StrategyInputDefinition,
    StrategyOutput, StrategyTrade, Validation, MAX_CANDLES, MAX_EVALUATION_STEPS,
    MAX_HISTORY_OFFSET, MAX_PLOTS, MAX_SOURCE_BYTES, MAX_STRATEGY_CANDLES,
    MAX_STRATEGY_EVALUATION_STEPS, MAX_STRATEGY_MAGNIFIER_CANDLES,
    MAX_STRATEGY_RETAINED_SERIES_VALUES, MAX_STRATEGY_RETAINED_STRING_BYTES,
    MAX_STRATEGY_SECURITY_CACHE_ENTRIES,
};
pub use strategy::{StrategyCommission, StrategyCommissionType};

pub fn validate_source(source: &str) -> Validation {
    if let Some(validation) = compatibility::validate_luxalgo_nwe(source) {
        return validation;
    }
    if let Some(validation) = compatibility::validate_hpotter_dss(source) {
        return validation;
    }
    parser::parse(source)
        .map(|program| program.validation)
        .unwrap_or_else(|diagnostics| Validation {
            valid: false,
            script_type: "unknown".to_string(),
            version: 6,
            name: String::new(),
            overlay: false,
            approximations: Vec::new(),
            diagnostics,
        })
}

#[cfg(target_arch = "wasm32")]
mod wasm_abi {
    use super::{run_indicator_with_inputs, run_strategy_with_context, Candle};
    use serde::Deserialize;
    use serde_json::json;
    use std::slice;

    #[derive(Deserialize)]
    struct Request {
        source: String,
        candles: Vec<Candle>,
        #[serde(default = "default_timeframe")]
        timeframe: String,
        #[serde(default)]
        timeframes: std::collections::HashMap<String, Vec<Candle>>,
        #[serde(default)]
        input_sources: std::collections::HashMap<String, Vec<Option<f64>>>,
        #[serde(default)]
        mode: String,
        #[serde(default)]
        inputs: std::collections::HashMap<String, serde_json::Value>,
    }

    fn default_timeframe() -> String {
        "1d".to_string()
    }

    #[no_mangle]
    pub extern "C" fn alloc(length: usize) -> *mut u8 {
        Box::into_raw(vec![0u8; length].into_boxed_slice()) as *mut u8
    }

    #[no_mangle]
    pub unsafe extern "C" fn dealloc(pointer: *mut u8, length: usize) {
        if !pointer.is_null() {
            let slice = std::ptr::slice_from_raw_parts_mut(pointer, length);
            drop(Box::from_raw(slice));
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn run(pointer: *const u8, length: usize) -> u64 {
        let input = if pointer.is_null() || length == 0 {
            &[][..]
        } else {
            slice::from_raw_parts(pointer, length)
        };
        let result = std::str::from_utf8(input)
            .map_err(|error| error.to_string())
            .and_then(|text| serde_json::from_str::<Request>(text).map_err(|error| error.to_string()))
            .map(|request| {
                let result = if request.mode == "strategy" {
                    run_strategy_with_context(&request.source, &request.candles, &request.timeframe, &request.timeframes, 0.0, 0.0, &request.inputs)
                        .map(|output| serde_json::to_vec(&output).unwrap_or_default())
                } else {
                    run_indicator_with_inputs(&request.source, &request.candles, &request.timeframe, &request.timeframes, &request.input_sources)
                        .map(|output| serde_json::to_vec(&output).unwrap_or_default())
                };
                result.unwrap_or_else(|diagnostics| serde_json::to_vec(&json!({"valid": false, "diagnostics": diagnostics})).unwrap_or_default())
            })
            .unwrap_or_else(|error| serde_json::to_vec(&json!({"valid": false, "diagnostics": [{"severity":"error", "message":error}]})).unwrap_or_default());
        let result = result.into_boxed_slice();
        let output_length = result.len();
        let output_pointer = Box::into_raw(result) as *mut u8;
        ((output_length as u64) << 32) | output_pointer as u32 as u64
    }

    #[no_mangle]
    pub unsafe extern "C" fn free_result(pointer: *mut u8, length: usize) {
        dealloc(pointer, length);
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "input_sources_tests.rs"]
mod input_sources_tests;

#[cfg(test)]
mod compatibility_tests;

#[cfg(test)]
mod strategy_memory_tests;
#[cfg(test)]
#[path = "strategy_tests.rs"]
mod strategy_tests;

#[cfg(test)]
#[path = "strategy_trailing_order_tests.rs"]
mod strategy_trailing_order_tests;

#[cfg(test)]
#[path = "strategy_exit_level_tests.rs"]
mod strategy_exit_level_tests;

#[cfg(test)]
#[path = "strategy_calc_on_order_fills_tests.rs"]
mod strategy_calc_on_order_fills_tests;

#[cfg(test)]
#[path = "strategy_input_definitions_tests.rs"]
mod strategy_input_definitions_tests;

#[cfg(test)]
#[path = "indicator_parity_tests.rs"]
mod indicator_parity_tests;

#[cfg(test)]
#[path = "strategy_evaluation_start_tests.rs"]
mod strategy_evaluation_start_tests;

#[cfg(test)]
#[path = "strategy_commission_tests.rs"]
mod strategy_commission_tests;
