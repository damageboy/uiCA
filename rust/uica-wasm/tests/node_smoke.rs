use serde_json::Value;
use uica_wasm::analyze_hex;

#[test]
fn analyze_hex_returns_rust_result_json() {
    let output = analyze_hex("90 90", "skl").expect("hex should analyze");
    let value: Value = serde_json::from_str(&output).expect("result should be json");

    assert_eq!(value["schema_version"], "uica-result-v1");
    assert_eq!(value["engine"], "rust");
    assert_eq!(value["invocation"]["arch"], "SKL");
    assert_eq!(value["summary"]["throughput_cycles_per_iteration"], 1.0);
}

#[test]
fn analyze_hex_rejects_invalid_hex() {
    let err = analyze_hex("9z", "SKL").expect_err("invalid hex should fail");
    assert_eq!(err, "invalid hex digit 'z' at position 1");
}
