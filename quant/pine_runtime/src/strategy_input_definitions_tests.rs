use crate::strategy_input_definitions;

#[test]
fn exposes_supported_literal_strategy_inputs_with_declared_bounds() {
    let source = "//@version=6\nstrategy(\"Inputs\")\nint length = input.int(14, \"Length\", minval=2, maxval=80)\nfloat factor = input.float(1.5, \"Factor\", minval=0.5, maxval=3.0)\nbool enabled = input.bool(true, \"Enabled\")\nstring mode = input.string(\"fast\", \"Mode\")";
    let inputs =
        strategy_input_definitions(source).expect("valid strategy inputs should be described");
    assert_eq!(inputs.len(), 4);
    assert_eq!(inputs[0].title, "Length");
    assert_eq!(inputs[0].input_type, "int");
    assert_eq!(inputs[0].default_value, serde_json::json!(14.0));
    assert_eq!(inputs[0].min_value, Some(2.0));
    assert_eq!(inputs[0].max_value, Some(80.0));
    assert_eq!(inputs[1].default_value, serde_json::json!(1.5));
    assert_eq!(inputs[2].default_value, serde_json::json!(true));
    assert_eq!(inputs[3].default_value, serde_json::json!("fast"));
}

#[test]
fn rejects_indicator_source_when_strategy_inputs_are_requested() {
    let result = strategy_input_definitions("//@version=6\nindicator(\"Not a strategy\")");
    assert!(
        result.is_err(),
        "indicator source must not provide strategy controls"
    );
}
