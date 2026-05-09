use serde_json::Value;

#[test]
fn run_reports_invalid_hex_as_json_error() {
    let pack = include_bytes!("../../uica-data/generated/arch/SKL.uipack");
    let output = uica_emscripten::run_request_json(r#"{"hex":"9z","arch":"SKL"}"#, pack);
    let value: Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["schema_version"], "uica-error-v1");
    assert_eq!(value["engine"], "rust-emscripten-xed");
    assert!(value["error"]
        .as_str()
        .unwrap()
        .contains("invalid hex digit 'z' at position 1"));
}

#[test]
fn run_rejects_uipack_arch_mismatch() {
    let pack = include_bytes!("../../uica-data/generated/arch/SKL.uipack");
    let output = uica_emscripten::run_request_json(r#"{"hex":"48 01 d8","arch":"HSW"}"#, pack);
    let value: Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["schema_version"], "uica-error-v1");
    assert_eq!(value["engine"], "rust-emscripten-xed");
    assert!(value["error"]
        .as_str()
        .unwrap()
        .contains("UIPack architecture SKL does not match requested architecture HSW"));
}

#[test]
fn run_decodes_hex_and_returns_uica_result() {
    let pack = include_bytes!("../../uica-data/generated/arch/SKL.uipack");
    let output = uica_emscripten::run_request_json(r#"{"hex":"48 01 d8","arch":"SKL"}"#, pack);
    let value: Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["schema_version"], "uica-web-result-v1");
    assert_eq!(value["engine"], "rust-emscripten-xed");
    assert_eq!(value["result"]["schema_version"], "uica-result-v1");
    assert_eq!(value["result"]["engine"], "rust");
    assert_eq!(value["result"]["invocation"]["arch"], "SKL");
    assert!(value["result"]["summary"]["throughput_cycles_per_iteration"].is_number());
}

#[test]
fn run_returns_web_envelope_with_trace_html() {
    let pack = include_bytes!("../../uica-data/generated/arch/SKL.uipack");
    let output = uica_emscripten::run_request_json(r#"{"hex":"48 01 d8","arch":"SKL"}"#, pack);
    let value: Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["schema_version"], "uica-web-result-v1");
    assert_eq!(value["engine"], "rust-emscripten-xed");
    assert_eq!(value["result"]["schema_version"], "uica-result-v1");
    assert_eq!(value["result"]["engine"], "rust");
    assert_eq!(value["result"]["invocation"]["arch"], "SKL");
    let trace_html = value["trace_html"].as_str().unwrap();
    assert!(trace_html.contains("Execution Trace"));
    assert!(trace_html.contains("var tableData = ["));
}
