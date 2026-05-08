use std::collections::BTreeMap;

use serde_json::Value;
use uica_decode_ir::DecodedInstruction;
use uica_wasm::{analyze_decoded_json, analyze_hex};

#[test]
fn analyze_hex_reports_xed_required_after_hex_validation() {
    let err = analyze_hex("90 90", "skl").expect_err("raw bytes should need XED wasm build");
    assert_eq!(
        err,
        "raw x86 byte analysis requires an XED-enabled wasm build; use analyze_decoded_json"
    );
}

#[test]
fn analyze_hex_rejects_invalid_hex() {
    let err = analyze_hex("9z", "SKL").expect_err("invalid hex should fail");
    assert_eq!(err, "invalid hex digit 'z' at position 1");
}

#[test]
fn analyze_decoded_json_returns_rust_result_json() {
    let decoded = vec![DecodedInstruction {
        ip: 0,
        len: 1,
        mnemonic: "nop".to_string(),
        disasm: "nop".to_string(),
        bytes: vec![0x90],
        pos_nominal_opcode: 0,
        input_regs: vec![],
        output_regs: vec![],
        reads_flags: false,
        writes_flags: false,
        has_memory_read: false,
        has_memory_write: false,
        mem_addrs: vec![],
        implicit_rsp_change: 0,
        immediate: None,
        immediate_width_bits: 0,
        has_66_prefix: false,
        iform: "NOP".to_string(),
        iform_signature: "NOP".to_string(),
        max_op_size_bytes: 0,
        uses_high8_reg: false,
        explicit_reg_operands: vec![],
        agen: None,
        xml_attrs: BTreeMap::new(),
    }];
    let decoded_json = serde_json::to_string(&decoded).expect("decoded IR should serialize");

    let output = analyze_decoded_json(&decoded_json, "skl").expect("decoded IR should analyze");
    let value: Value = serde_json::from_str(&output).expect("result should be json");

    assert_eq!(value["schema_version"], "uica-result-v1");
    assert_eq!(value["engine"], "rust");
    assert_eq!(value["invocation"]["arch"], "SKL");
    assert!(value["summary"]["throughput_cycles_per_iteration"].is_number());
}
