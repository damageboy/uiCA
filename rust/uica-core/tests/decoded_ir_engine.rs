use std::collections::BTreeMap;

use uica_data::{
    encode_uipack, DataPack as UiPackFixture, InstructionRecord, MappedUiPackRuntime, PerfRecord,
};
use uica_decode_ir::DecodedInstruction;
use uica_model::Invocation;

#[test]
fn analyzes_caller_supplied_decoded_ir_with_runtime() {
    let decoded = vec![DecodedInstruction {
        ip: 0,
        len: 3,
        mnemonic: "add".to_string(),
        disasm: "add rax, rbx".to_string(),
        bytes: vec![0x48, 0x01, 0xd8],
        pos_nominal_opcode: 1,
        input_regs: vec!["RAX".to_string(), "RBX".to_string()],
        output_regs: vec!["RAX".to_string()],
        reads_flags: false,
        writes_flags: true,
        has_memory_read: false,
        has_memory_write: false,
        mem_addrs: vec![],
        implicit_rsp_change: 0,
        immediate: None,
        immediate_width_bits: 0,
        has_66_prefix: false,
        iform: "ADD_GPRv_GPRv".to_string(),
        iform_signature: "ADD_GPRv_GPRv".to_string(),
        max_op_size_bytes: 8,
        uses_high8_reg: false,
        explicit_reg_operands: vec!["RAX".to_string(), "RBX".to_string()],
        agen: None,
        xml_attrs: BTreeMap::new(),
    }];

    let fixture = UiPackFixture {
        schema_version: "uica-instructions-pack-v1".to_string(),
        all_ports: vec![
            "0".to_string(),
            "1".to_string(),
            "5".to_string(),
            "6".to_string(),
        ],
        alu_ports: vec![
            "0".to_string(),
            "1".to_string(),
            "5".to_string(),
            "6".to_string(),
        ],
        instructions: vec![InstructionRecord {
            arch: "SKL".to_string(),
            iform: "ADD_GPRv_GPRv".to_string(),
            string: "ADD".to_string(),
            all_ports: vec![
                "0".to_string(),
                "1".to_string(),
                "5".to_string(),
                "6".to_string(),
            ],
            alu_ports: vec![
                "0".to_string(),
                "1".to_string(),
                "5".to_string(),
                "6".to_string(),
            ],
            locked: false,
            xml_attrs: BTreeMap::new(),
            imm_zero: false,
            perf: PerfRecord {
                operands: vec![],
                latencies: vec![],
                uops: 1,
                retire_slots: 1,
                uops_mite: 1,
                uops_ms: 0,
                tp: Some(1.0),
                ports: BTreeMap::from([("0156".to_string(), 1)]),
                div_cycles: 0,
                may_be_eliminated: false,
                complex_decoder: false,
                n_available_simple_decoders: 0,
                lcp_stall: false,
                implicit_rsp_change: 0,
                can_be_used_by_lsd: false,
                cannot_be_in_dsb_due_to_jcc_erratum: false,
                no_micro_fusion: false,
                no_macro_fusion: false,
                macro_fusible_with: vec![],
                variants: BTreeMap::new(),
            },
        }],
    };

    let runtime = MappedUiPackRuntime::from_bytes(encode_uipack(&fixture, "SKL").unwrap()).unwrap();
    let invocation = Invocation {
        arch: "SKL".to_string(),
        ..Invocation::default()
    };
    let result = uica_core::engine::simulate(uica_core::engine::SimulationRequest {
        input: uica_core::engine::SimulationInput::Decoded(&decoded),
        invocation: &invocation,
        uipack: uica_core::engine::UipackSource::Runtime(&runtime),
        options: uica_core::engine::SimulationOptions::default(),
    })
    .unwrap()
    .result;

    assert_eq!(result.invocation.arch, "SKL");
    assert_eq!(result.summary.mode, "unroll");
    assert!(result.summary.throughput_cycles_per_iteration.is_some());
    assert_eq!(result.parameters["uArchName"], "SKL");
}
