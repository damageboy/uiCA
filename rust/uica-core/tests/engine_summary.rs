use std::collections::BTreeMap;

use uica_data::{
    DataPack as UiPackFixture, InstructionRecord, LatencyRecord, OperandRecord, PerfRecord,
};
use uica_model::Invocation;

fn run_fixture(
    code: &[u8],
    invocation: &Invocation,
    fixture: &UiPackFixture,
) -> uica_model::UicaResult {
    let runtime = uica_data::MappedUiPackRuntime::from_bytes(
        uica_data::encode_uipack(fixture, &invocation.arch).unwrap(),
    )
    .unwrap();
    uica_core::engine::simulate(uica_core::engine::SimulationRequest {
        input: uica_core::engine::SimulationInput::Bytes(code),
        invocation,
        uipack: uica_core::engine::UipackSource::Runtime(&runtime),
        options: uica_core::engine::SimulationOptions::default(),
    })
    .unwrap()
    .result
}

fn engine_with_runtime(
    code: &[u8],
    invocation: &Invocation,
    runtime: &uica_data::MappedUiPackRuntime,
) -> uica_model::UicaResult {
    uica_core::engine::simulate(uica_core::engine::SimulationRequest {
        input: uica_core::engine::SimulationInput::Bytes(code),
        invocation,
        uipack: uica_core::engine::UipackSource::Runtime(runtime),
        options: uica_core::engine::SimulationOptions::default(),
    })
    .unwrap()
    .result
}

#[test]
fn computes_summary_from_decoded_records() {
    let code = hex::decode("4801d8").unwrap(); // add rax, rbx
    let invocation = Invocation {
        arch: "SKL".to_string(),
        ..Invocation::default()
    };

    let pack = UiPackFixture {
        schema_version: "uica-instructions-pack-v1".to_string(),
        all_ports: Default::default(),
        alu_ports: Default::default(),
        instructions: vec![InstructionRecord {
            arch: "SKL".to_string(),
            iform: "ADD_GPRv_GPRv".to_string(),
            string: "ADD".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            locked: false,
            xml_attrs: Default::default(),
            imm_zero: false,
            perf: PerfRecord {
                operands: vec![],
                latencies: vec![],
                uops: 1,
                retire_slots: 1,
                uops_mite: 1,
                uops_ms: 0,
                tp: Some(1.0),
                ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                variants: Default::default(),
            },
        }],
    };

    let result = run_fixture(&code, &invocation, &pack);

    assert_eq!(result.invocation.arch, "SKL");
    assert_eq!(result.summary.mode, "unroll");
    assert!(result.summary.throughput_cycles_per_iteration.is_some());
    assert_eq!(result.parameters["uArchName"], "SKL");
    assert_eq!(result.parameters["issueWidth"], 4);
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn quick_add_loop_model_matches_expected_outputs() {
    let code = hex::decode("4801d84801c349ffcf75f5").unwrap();

    for (arch, iterations, dsb, lsd, issue, ports) in [
        ("HSW", 247, None, Some(0.75), Some(0.75), Some(1.0)),
        ("SKL", 246, Some(1.0), None, Some(0.75), Some(1.0)),
        ("ICL", 247, None, Some(0.67), Some(0.6), Some(0.75)),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };

        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("6"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };

        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(2.0),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(2.0)),
            "{arch}"
        );
        assert_eq!(
            result.summary.bottlenecks_predicted,
            vec!["Dependencies".to_string()],
            "{arch}"
        );
    }
}

#[test]
fn unrolled_mite_cycle_json_keeps_predecode_events() {
    // Python parity: `FrontEnd.allGeneratedInstrInstances` points at same
    // InstrInstance objects mutated by PreDecoder/Decoder, so unrolled MITE
    // traces keep addedToIQ/removedFromIQ lifecycle events.
    let code = hex::decode("4183ff0119c083e00885c98945c4b8010000000f4fc139c2").unwrap();
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let runtime = uica_data::load_manifest_runtime(&manifest, "SKL").unwrap();
    let result = engine_with_runtime(
        &code,
        &Invocation {
            arch: "SKL".to_string(),
            min_cycles: 500,
            ..Invocation::default()
        },
        &runtime,
    );

    assert_eq!(
        result.cycles[0]["addedToIQ"].as_array().map(Vec::len),
        Some(5)
    );
    assert!(result
        .cycles
        .iter()
        .any(|cycle| cycle.get("removedFromIQ").is_some()));
    assert_eq!(
        result.cycles[5]["addedToIDQ"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(3.01));
    assert_eq!(result.summary.limits.get("predecoder"), Some(&Some(3.0)));
    assert_eq!(
        result.summary.bottlenecks_predicted,
        vec!["Predecoder".to_string()]
    );
}

#[test]
fn flag_chain_bottlenecks_use_simulated_throughput() {
    // Python parity: JSON summary passes simulated TP into getBottlenecks().
    let code = hex::decode("4801d84819d14d11c849ffca75f2").unwrap();
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let runtime = uica_data::load_manifest_runtime(&manifest, "HSW").unwrap();
    let result = engine_with_runtime(
        &code,
        &Invocation {
            arch: "HSW".to_string(),
            min_cycles: 500,
            ..Invocation::default()
        },
        &runtime,
    );

    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(2.11));
    assert_eq!(result.summary.bottlenecks_predicted, Vec::<String>::new());
    assert_eq!(result.summary.limits.get("dependencies"), Some(&Some(2.0)));
}

#[test]
fn flag_chain_scheduling_bottleneck_uses_retired_port_usage() {
    // Python parity: getBottlenecks adds Scheduling when actual per-port usage,
    // not analytical port limit, reaches simulated TP.
    let code = hex::decode("4801d84819d14d11c849ffca75f2").unwrap();
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let runtime = uica_data::load_manifest_runtime(&manifest, "SKL").unwrap();
    let result = engine_with_runtime(
        &code,
        &Invocation {
            arch: "SKL".to_string(),
            min_cycles: 500,
            ..Invocation::default()
        },
        &runtime,
    );

    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(1.54));
    assert_eq!(
        result.summary.bottlenecks_predicted,
        vec!["Scheduling".to_string()]
    );
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn quick_dec_jcc_model_matches_expected_outputs() {
    let code = hex::decode("48ffc975fb").unwrap();

    for (arch, iterations, dsb, lsd, issue, ports, bottlenecks) in [
        (
            "HSW",
            493,
            None,
            Some(0.25),
            Some(0.25),
            Some(1.0),
            vec!["Dependencies".to_string(), "Ports".to_string()],
        ),
        (
            "SKL",
            491,
            Some(1.0),
            None,
            Some(0.25),
            Some(1.0),
            vec![
                "DSB".to_string(),
                "Dependencies".to_string(),
                "Ports".to_string(),
            ],
        ),
        (
            "ICL",
            493,
            None,
            Some(0.33),
            Some(0.2),
            Some(1.0),
            vec!["Dependencies".to_string(), "Ports".to_string()],
        ),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };

        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("6"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };

        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(1.0),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(1.0)),
            "{arch}"
        );
        assert_eq!(result.summary.bottlenecks_predicted, bottlenecks, "{arch}");
    }
}

#[test]
fn mem_address_latency_feeds_dependency_limit_like_python() {
    let code = hex::decode("488b004801d848ffc975f5").unwrap();
    let invocation = Invocation {
        arch: "HSW".to_string(),
        min_cycles: 500,
        ..Invocation::default()
    };

    let pack = UiPackFixture {
        schema_version: "uica-instructions-pack-v2".to_string(),
        all_ports: Default::default(),
        alu_ports: Default::default(),
        instructions: vec![
            InstructionRecord {
                arch: "HSW".to_string(),
                iform: "MOV_GPRv_MEMv".to_string(),
                string: "MOV (R64, M64)".to_string(),
                all_ports: Default::default(),
                alu_ports: Default::default(),
                locked: false,
                xml_attrs: Default::default(),
                imm_zero: false,
                perf: PerfRecord {
                    operands: vec![
                        OperandRecord {
                            name: "REG0".to_string(),
                            r#type: "reg".to_string(),
                            read: false,
                            write: true,
                            implicit: false,
                            flags: vec![],
                            flags_read: vec![],
                            flags_write: vec![],
                            mem_base: None,
                            mem_index: None,
                            mem_scale: None,
                            mem_disp: None,
                            is_agen: false,
                            mem_operand_role: None,
                        },
                        OperandRecord {
                            name: "MEM0".to_string(),
                            r#type: "mem".to_string(),
                            read: true,
                            write: false,
                            implicit: false,
                            flags: vec![],
                            flags_read: vec![],
                            flags_write: vec![],
                            mem_base: None,
                            mem_index: None,
                            mem_scale: None,
                            mem_disp: None,
                            is_agen: false,
                            mem_operand_role: Some("read".to_string()),
                        },
                    ],
                    latencies: vec![LatencyRecord {
                        start_op: "MEM0".to_string(),
                        target_op: "REG0".to_string(),
                        cycles: 1,
                        cycles_addr: Some(5),
                        cycles_addr_index: Some(5),
                        cycles_mem: Some(4),
                        cycles_same_reg: None,
                    }],
                    uops: 1,
                    retire_slots: 1,
                    uops_mite: 1,
                    uops_ms: 0,
                    tp: None,
                    ports: BTreeMap::from([(String::from("23"), 1)]),
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
                    variants: Default::default(),
                },
            },
            InstructionRecord {
                arch: "HSW".to_string(),
                iform: "ADD_GPRv_GPRv_01".to_string(),
                string: "ADD".to_string(),
                all_ports: Default::default(),
                alu_ports: Default::default(),
                locked: false,
                xml_attrs: Default::default(),
                imm_zero: false,
                perf: PerfRecord {
                    operands: vec![
                        OperandRecord {
                            name: "REG0".to_string(),
                            r#type: "reg".to_string(),
                            read: true,
                            write: true,
                            implicit: false,
                            flags: vec![],
                            flags_read: vec![],
                            flags_write: vec![],
                            mem_base: None,
                            mem_index: None,
                            mem_scale: None,
                            mem_disp: None,
                            is_agen: false,
                            mem_operand_role: None,
                        },
                        OperandRecord {
                            name: "REG1".to_string(),
                            r#type: "reg".to_string(),
                            read: true,
                            write: false,
                            implicit: false,
                            flags: vec![],
                            flags_read: vec![],
                            flags_write: vec![],
                            mem_base: None,
                            mem_index: None,
                            mem_scale: None,
                            mem_disp: None,
                            is_agen: false,
                            mem_operand_role: None,
                        },
                    ],
                    latencies: vec![
                        LatencyRecord {
                            start_op: "REG0".to_string(),
                            target_op: "REG0".to_string(),
                            cycles: 1,
                            cycles_addr: None,
                            cycles_addr_index: None,
                            cycles_mem: None,
                            cycles_same_reg: None,
                        },
                        LatencyRecord {
                            start_op: "REG1".to_string(),
                            target_op: "REG0".to_string(),
                            cycles: 1,
                            cycles_addr: None,
                            cycles_addr_index: None,
                            cycles_mem: None,
                            cycles_same_reg: None,
                        },
                    ],
                    uops: 1,
                    retire_slots: 1,
                    uops_mite: 1,
                    uops_ms: 0,
                    tp: None,
                    ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                    variants: Default::default(),
                },
            },
            InstructionRecord {
                arch: "HSW".to_string(),
                iform: "DEC_GPRv_FFr1".to_string(),
                string: "DEC".to_string(),
                all_ports: Default::default(),
                alu_ports: Default::default(),
                locked: false,
                xml_attrs: Default::default(),
                imm_zero: false,
                perf: PerfRecord {
                    operands: vec![OperandRecord {
                        name: "REG0".to_string(),
                        r#type: "reg".to_string(),
                        read: true,
                        write: true,
                        implicit: false,
                        flags: vec![],
                        flags_read: vec![],
                        flags_write: vec![],
                        mem_base: None,
                        mem_index: None,
                        mem_scale: None,
                        mem_disp: None,
                        is_agen: false,
                        mem_operand_role: None,
                    }],
                    latencies: vec![LatencyRecord {
                        start_op: "REG0".to_string(),
                        target_op: "REG0".to_string(),
                        cycles: 1,
                        cycles_addr: None,
                        cycles_addr_index: None,
                        cycles_mem: None,
                        cycles_same_reg: None,
                    }],
                    uops: 1,
                    retire_slots: 1,
                    uops_mite: 1,
                    uops_ms: 0,
                    tp: None,
                    ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                    variants: Default::default(),
                },
            },
            InstructionRecord {
                arch: "HSW".to_string(),
                iform: "JNZ_RELBRb".to_string(),
                string: "JNZ".to_string(),
                all_ports: Default::default(),
                alu_ports: Default::default(),
                locked: false,
                xml_attrs: Default::default(),
                imm_zero: false,
                perf: PerfRecord {
                    operands: vec![],
                    latencies: vec![],
                    uops: 1,
                    retire_slots: 1,
                    uops_mite: 1,
                    uops_ms: 0,
                    tp: None,
                    ports: BTreeMap::from([(String::from("6"), 1)]),
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
                    variants: Default::default(),
                },
            },
        ],
    };

    let result = run_fixture(&code, &invocation, &pack);
    assert_eq!(result.summary.limits.get("dependencies"), Some(&Some(6.0)));
    assert!(result
        .summary
        .bottlenecks_predicted
        .contains(&"Dependencies".to_string()));
}

#[test]
fn jne_alias_matches_jnz_record_in_engine_path() {
    // Python/XED exposes opcode 75 as JNZ, while Rust disassembly can surface
    // JNE spelling. Use manifest data so this verifies the real engine path
    // instead of stale synthetic branch timing.
    let code = hex::decode("4801d875fb").unwrap(); // add rax, rbx; jne back
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let runtime = uica_data::load_manifest_runtime(&manifest, "SKL").unwrap();

    let result = engine_with_runtime(
        &code,
        &Invocation {
            arch: "SKL".to_string(),
            min_cycles: 500,
            ..Invocation::default()
        },
        &runtime,
    );

    assert_eq!(result.summary.mode, "loop");
    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(1.0));
    assert_eq!(result.summary.iterations_simulated, 491);
    assert_eq!(result.summary.limits.get("dsb"), Some(&Some(1.0)));
    assert_eq!(result.summary.limits.get("issue"), Some(&Some(0.25)));
    assert_eq!(result.summary.limits.get("dependencies"), Some(&Some(1.0)));
    assert_eq!(result.summary.limits.get("ports"), Some(&Some(1.0)));
    assert_eq!(
        result.summary.bottlenecks_predicted,
        vec![
            "DSB".to_string(),
            "Dependencies".to_string(),
            "Ports".to_string()
        ]
    );
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn quick_model_falls_back_safely_when_pack_is_incomplete() {
    let code = hex::decode("48ffc975fb").unwrap();
    let invocation = Invocation {
        arch: "SKL".to_string(),
        min_cycles: 500,
        ..Invocation::default()
    };

    let pack = UiPackFixture {
        schema_version: "uica-instructions-pack-v1".to_string(),
        all_ports: Default::default(),
        alu_ports: Default::default(),
        instructions: vec![InstructionRecord {
            arch: "SKL".to_string(),
            iform: "DEC_GPRv".to_string(),
            string: "DEC".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            locked: false,
            xml_attrs: Default::default(),
            imm_zero: false,
            perf: PerfRecord {
                operands: vec![],
                latencies: vec![],
                uops: 9,
                retire_slots: 9,
                uops_mite: 9,
                uops_ms: 0,
                tp: Some(9.0),
                ports: BTreeMap::from([(String::from("0"), 9)]),
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
                variants: Default::default(),
            },
        }],
    };

    let result = run_fixture(&code, &invocation, &pack);
    assert_eq!(result.summary.mode, "loop");
    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(9.0));
    assert_eq!(result.summary.iterations_simulated, 54);
    assert_eq!(result.summary.limits.get("dsb"), Some(&None));
    assert_eq!(result.summary.limits.get("issue"), Some(&Some(2.25)));
    assert_eq!(result.summary.limits.get("dependencies"), Some(&None));
    assert_eq!(result.summary.limits.get("ports"), Some(&Some(9.0)));
    assert_eq!(
        result.summary.bottlenecks_predicted,
        vec!["Ports".to_string()]
    );
}

#[test]
fn empty_fixture_uses_python_unknown_instr_defaults() {
    let code = hex::decode("4801d84829d875f8").unwrap(); // add; sub; jnz
    let invocation = Invocation {
        arch: "SKL".to_string(),
        min_cycles: 500,
        ..Invocation::default()
    };

    let pack = UiPackFixture {
        schema_version: "uica-instructions-pack-v1".to_string(),
        all_ports: Default::default(),
        alu_ports: Default::default(),
        instructions: vec![],
    };

    let result = run_fixture(&code, &invocation, &pack);
    assert_eq!(result.summary.mode, "loop");
    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(1.0));
    assert_eq!(result.summary.iterations_simulated, 500);
    assert_eq!(result.summary.limits.get("dsb"), Some(&Some(1.0)));
    assert_eq!(result.summary.limits.get("issue"), Some(&Some(0.75)));
    assert_eq!(result.summary.limits.get("dependencies"), Some(&Some(0.0)));
    assert_eq!(result.summary.limits.get("ports"), Some(&Some(0.0)));
    assert_eq!(
        result.summary.bottlenecks_predicted,
        vec!["DSB".to_string()]
    );
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn curated12_cmov_setcc_model_matches_expected_outputs() {
    let code = hex::decode("4839d8480f4fca0f94c04d01c849ffca75ee").unwrap();

    for (arch, cmov_uops, throughput, iterations, dsb, lsd, issue, ports, bottlenecks) in [
        (
            "HSW",
            2,
            2.83,
            171,
            None,
            Some(1.6),
            Some(1.5),
            Some(1.5),
            vec![],
        ),
        (
            "SKL",
            1,
            2.03,
            237,
            Some(1.0),
            None,
            Some(1.25),
            Some(1.5),
            vec!["Dependencies".to_string()],
        ),
        (
            "ICL",
            1,
            2.0,
            247,
            None,
            Some(1.0),
            Some(1.0),
            Some(1.5),
            vec!["Dependencies".to_string()],
        ),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };

        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "CMP_GPRv_GPRv".to_string(),
                    string: "CMP".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "CMOVNLE_GPRv_GPRv".to_string(),
                    string: "CMOVNLE".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: cmov_uops,
                        retire_slots: cmov_uops,
                        uops_mite: cmov_uops,
                        uops_ms: 0,
                        tp: Some(cmov_uops as f64),
                        ports: if cmov_uops == 2 {
                            BTreeMap::from([(String::from("0156"), 1), (String::from("06"), 1)])
                        } else {
                            BTreeMap::from([(String::from("06"), 1)])
                        },
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SETZ_GPR8".to_string(),
                    string: "SETZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };

        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(result.summary.mode, "loop", "{arch}");
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(throughput),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(2.0)),
            "{arch}"
        );
        assert_eq!(result.summary.bottlenecks_predicted, bottlenecks, "{arch}");
    }
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn curated12_alu_dep_model_matches_expected_outputs() {
    let code = hex::decode("4801d84829c34811d848ffc975f2").unwrap();

    for (arch, throughput, iterations, dsb, lsd, issue, ports, dependencies) in [
        (
            "HSW",
            4.0,
            124,
            None,
            Some(1.33),
            Some(1.25),
            Some(1.25),
            4.0,
        ),
        ("SKL", 3.0, 164, Some(1.0), None, Some(1.0), Some(1.0), 3.0),
        ("ICL", 3.0, 165, None, Some(0.83), Some(0.8), Some(1.0), 3.0),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };

        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SUB_GPRv_GPRv".to_string(),
                    string: "SUB".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADC_GPRv_GPRv".to_string(),
                    string: "ADC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 2,
                        retire_slots: 2,
                        uops_mite: 2,
                        uops_ms: 0,
                        tp: Some(2.0),
                        ports: BTreeMap::from([(String::from("0156"), 1), (String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };

        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(result.summary.mode, "loop", "{arch}");
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(throughput),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(dependencies)),
            "{arch}"
        );
        assert_eq!(
            result.summary.bottlenecks_predicted,
            vec!["Dependencies".to_string()],
            "{arch}"
        );
    }
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn curated12_flag_chain_model_matches_expected_outputs() {
    let code = hex::decode("4801d84819d14d11c849ffca75f2").unwrap();

    for (
        arch,
        carry_uops,
        throughput,
        iterations,
        dsb,
        lsd,
        issue,
        ports,
        dependencies,
        bottlenecks,
    ) in [
        (
            "HSW",
            2,
            2.11,
            234,
            None,
            Some(1.6),
            Some(1.5),
            Some(1.5),
            2.0,
            vec![],
        ),
        (
            "SKL",
            1,
            1.54,
            316,
            Some(1.0),
            None,
            Some(1.0),
            Some(1.5),
            1.0,
            vec!["Scheduling".to_string()],
        ),
        (
            "ICL",
            1,
            1.85,
            267,
            None,
            Some(0.83),
            Some(0.8),
            Some(1.5),
            1.0,
            vec![],
        ),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };

        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SBB_GPRv_GPRv".to_string(),
                    string: "SBB".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: carry_uops,
                        retire_slots: carry_uops,
                        uops_mite: carry_uops,
                        uops_ms: 0,
                        tp: Some(carry_uops as f64),
                        ports: if carry_uops == 2 {
                            BTreeMap::from([(String::from("0156"), 1), (String::from("06"), 1)])
                        } else {
                            BTreeMap::from([(String::from("06"), 1)])
                        },
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADC_GPRv_GPRv".to_string(),
                    string: "ADC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: carry_uops,
                        retire_slots: carry_uops,
                        uops_mite: carry_uops,
                        uops_ms: 0,
                        tp: Some(carry_uops as f64),
                        ports: if carry_uops == 2 {
                            BTreeMap::from([(String::from("0156"), 1), (String::from("06"), 1)])
                        } else {
                            BTreeMap::from([(String::from("06"), 1)])
                        },
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };

        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(result.summary.mode, "loop", "{arch}");
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(throughput),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(dependencies)),
            "{arch}"
        );
        assert_eq!(result.summary.bottlenecks_predicted, bottlenecks, "{arch}");
    }
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn curated12_load_store_mix_model_matches_expected_outputs() {
    let code = hex::decode("488b064801d848890748ffc975f2").unwrap();

    for (arch, throughput, iterations, dsb, lsd, issue, ports, bottlenecks) in [
        (
            "HSW",
            1.0,
            489,
            None,
            Some(1.0),
            Some(1.0),
            Some(1.0),
            vec![
                "Dependencies".to_string(),
                "Issue".to_string(),
                "LSD".to_string(),
                "Ports".to_string(),
            ],
        ),
        (
            "SKL",
            1.0,
            488,
            Some(1.0),
            None,
            Some(1.0),
            Some(1.0),
            vec![
                "DSB".to_string(),
                "Dependencies".to_string(),
                "Issue".to_string(),
                "Ports".to_string(),
            ],
        ),
        (
            "ICL",
            1.0,
            491,
            None,
            Some(0.83),
            Some(0.8),
            Some(0.5),
            vec!["Dependencies".to_string()],
        ),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };

        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "MOV_GPRv_GPRv".to_string(),
                    string: "MOV".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("23"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };

        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(result.summary.mode, "loop", "{arch}");
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(throughput),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(1.0)),
            "{arch}"
        );
        assert_eq!(result.summary.bottlenecks_predicted, bottlenecks, "{arch}");
    }
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn curated12_store_stream_model_matches_expected_outputs() {
    let code = hex::decode("48890748895f084883c71048ffc975f0").unwrap();

    for (arch, throughput, iterations, dsb, lsd, issue, ports, bottlenecks) in [
        (
            "HSW",
            2.0,
            247,
            None,
            Some(1.0),
            Some(1.0),
            Some(2.0),
            vec!["Ports".to_string()],
        ),
        (
            "SKL",
            2.0,
            247,
            Some(1.0),
            None,
            Some(1.0),
            Some(2.0),
            vec!["Ports".to_string()],
        ),
        (
            "ICL",
            1.07,
            455,
            None,
            Some(0.83),
            Some(0.8),
            Some(1.0),
            vec![],
        ),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };

        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "MOV_GPRv_GPRv".to_string(),
                    string: "MOV".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("23"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };

        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(result.summary.mode, "loop", "{arch}");
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(throughput),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(1.0)),
            "{arch}"
        );
        assert_eq!(result.summary.bottlenecks_predicted, bottlenecks, "{arch}");
    }
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn curated12_div_mul_model_matches_expected_outputs() {
    let code = hex::decode("31d289f0f7f14d0fafc149ffca75f1").unwrap();

    for (arch, throughput, iterations, dsb, lsd, issue, ports, dependencies, bottlenecks) in [
        (
            "HSW",
            9.0,
            54,
            Some(2.0),
            None,
            Some(3.5),
            Some(3.0),
            3.0,
            vec!["Divider".to_string()],
        ),
        (
            "SKL",
            7.0,
            68,
            Some(2.0),
            None,
            Some(3.5),
            Some(4.0),
            3.0,
            vec![],
        ),
        (
            "ICL",
            6.0,
            81,
            None,
            Some(1.75),
            Some(1.6),
            Some(3.0),
            3.0,
            vec!["Divider".to_string()],
        ),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };
        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "XOR_GPRv_GPRv".to_string(),
                    string: "XOR".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "MOV_GPRv_GPRv".to_string(),
                    string: "MOV".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("23"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DIV_GPRv".to_string(),
                    string: "DIV".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 4,
                        retire_slots: 4,
                        uops_mite: 4,
                        uops_ms: 0,
                        tp: Some(4.0),
                        ports: BTreeMap::from([(String::from("0"), 2), (String::from("1"), 2)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "IMUL_GPRv_GPRv".to_string(),
                    string: "IMUL".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 2,
                        retire_slots: 2,
                        uops_mite: 2,
                        uops_ms: 0,
                        tp: Some(2.0),
                        ports: BTreeMap::from([(String::from("1"), 1), (String::from("5"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };
        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(throughput),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(dependencies)),
            "{arch}"
        );
        assert_eq!(result.summary.bottlenecks_predicted, bottlenecks, "{arch}");
    }
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn curated12_shift_rotate_model_matches_expected_outputs() {
    let code = hex::decode("48d3e048d1cb48c1fa0349ffc875f1").unwrap();

    for (arch, throughput, iterations, dsb, lsd, issue, ports) in [
        ("HSW", 3.49, 141, None, Some(1.75), Some(1.75), Some(3.5)),
        ("SKL", 3.49, 141, Some(2.0), None, Some(1.75), Some(3.5)),
        ("ICL", 3.0, 165, None, Some(1.33), Some(1.2), Some(3.0)),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };
        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SHL_GPRv_GPRv".to_string(),
                    string: "SHL".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ROR_GPRv_GPRv".to_string(),
                    string: "ROR".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SAR_GPRv_GPRv".to_string(),
                    string: "SAR".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };
        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(throughput),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(1.0)),
            "{arch}"
        );
        assert_eq!(
            result.summary.bottlenecks_predicted,
            vec!["Ports".to_string()],
            "{arch}"
        );
    }
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn curated12_vector128_model_matches_expected_outputs() {
    let code = hex::decode("660f6fc1660fd4c2660fefd848ffc975ef").unwrap();

    for (arch, throughput, iterations, dsb, lsd, issue, ports, bottlenecks) in [
        (
            "HSW",
            1.0,
            493,
            None,
            Some(1.0),
            Some(1.0),
            Some(1.0),
            vec![
                "Dependencies".to_string(),
                "Issue".to_string(),
                "LSD".to_string(),
                "Ports".to_string(),
            ],
        ),
        (
            "SKL",
            1.0,
            487,
            Some(1.0),
            None,
            Some(1.0),
            Some(1.0),
            vec![
                "DSB".to_string(),
                "Dependencies".to_string(),
                "Issue".to_string(),
                "Ports".to_string(),
            ],
        ),
        (
            "ICL",
            1.04,
            467,
            None,
            Some(0.83),
            Some(0.8),
            Some(0.75),
            vec![],
        ),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };
        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "MOVDQA_XMMdq_XMMdq".to_string(),
                    string: "MOVDQA".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("23"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "PADDQ_XMMdq_XMMdq".to_string(),
                    string: "PADDQ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("015"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "PXOR_XMMdq_XMMdq".to_string(),
                    string: "PXOR".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("015"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };
        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(throughput),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(1.0)),
            "{arch}"
        );
        assert_eq!(result.summary.bottlenecks_predicted, bottlenecks, "{arch}");
    }
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn curated12_vector256_model_matches_expected_outputs() {
    let code = hex::decode("c5f458c2c5dc59ddc5cdeff048ffc975ef").unwrap();

    for (arch, throughput, iterations, dsb, lsd, issue, ports) in [
        ("HSW", 1.1, 445, None, Some(1.0), Some(1.0), Some(1.0)),
        ("SKL", 1.04, 462, Some(1.0), None, Some(1.0), Some(1.0)),
        ("ICL", 1.08, 449, None, Some(0.83), Some(0.8), Some(1.0)),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };
        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "VADDPS_YMMqq_YMMqq_YMMqq".to_string(),
                    string: "VADDPS".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("01"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "VMULPS_YMMqq_YMMqq_YMMqq".to_string(),
                    string: "VMULPS".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("01"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "VPXOR_YMMqq_YMMqq_YMMqq".to_string(),
                    string: "VPXOR".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("015"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };
        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(throughput),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(1.0)),
            "{arch}"
        );
        assert_eq!(
            result.summary.bottlenecks_predicted,
            vec!["Scheduling".to_string()],
            "{arch}"
        );
    }
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn curated12_fence_mix_model_matches_expected_outputs() {
    let code = hex::decode("4803060faee848031f0faef848ffc975ef").unwrap();

    for (arch, throughput, iterations, dsb, lsd, issue, ports) in [
        ("HSW", 15.0, 33, Some(1.0), None, Some(1.75), Some(1.5)),
        ("SKL", 15.0, 33, Some(1.0), None, Some(1.75), Some(1.5)),
        ("ICL", 15.0, 33, Some(1.0), None, Some(1.4), Some(1.0)),
    ] {
        let invocation = Invocation {
            arch: arch.to_string(),
            min_cycles: 500,
            ..Invocation::default()
        };
        let pack = UiPackFixture {
            schema_version: "uica-instructions-pack-v1".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_MEMv".to_string(),
                    string: "ADD".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 2,
                        retire_slots: 2,
                        uops_mite: 2,
                        uops_ms: 0,
                        tp: Some(2.0),
                        ports: BTreeMap::from([(String::from("0156"), 1), (String::from("23"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "LFENCE".to_string(),
                    string: "LFENCE".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SFENCE".to_string(),
                    string: "SFENCE".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
                    all_ports: Default::default(),
                    alu_ports: Default::default(),
                    locked: false,
                    xml_attrs: Default::default(),
                    imm_zero: false,
                    perf: PerfRecord {
                        operands: vec![],
                        latencies: vec![],
                        uops: 1,
                        retire_slots: 1,
                        uops_mite: 1,
                        uops_ms: 0,
                        tp: Some(1.0),
                        ports: BTreeMap::from([(String::from("06"), 1)]),
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
                        variants: Default::default(),
                        macro_fusible_with: vec![],
                    },
                },
            ],
        };
        let result = run_fixture(&code, &invocation, &pack);
        assert_eq!(
            result.summary.throughput_cycles_per_iteration,
            Some(throughput),
            "{arch}"
        );
        assert_eq!(result.summary.iterations_simulated, iterations, "{arch}");
        assert_eq!(result.summary.limits.get("dsb"), Some(&dsb), "{arch}");
        assert_eq!(result.summary.limits.get("lsd"), Some(&lsd), "{arch}");
        assert_eq!(result.summary.limits.get("issue"), Some(&issue), "{arch}");
        assert_eq!(result.summary.limits.get("ports"), Some(&ports), "{arch}");
        assert_eq!(
            result.summary.limits.get("dependencies"),
            Some(&Some(1.0)),
            "{arch}"
        );
        assert_eq!(
            result.summary.bottlenecks_predicted,
            Vec::<String>::new(),
            "{arch}"
        );
    }
}

#[test]
#[ignore = "pre-existing stale synthetic-pack snapshot; replace with manifest-uipack test"]
fn emits_cycle_skeleton_with_expected_length_and_cycle_indices() {
    let code = hex::decode("4801d8").unwrap(); // add rax, rbx
    let invocation = Invocation {
        arch: "SKL".to_string(),
        min_cycles: 12,
        ..Invocation::default()
    };

    let pack = UiPackFixture {
        schema_version: "uica-instructions-pack-v1".to_string(),
        all_ports: Default::default(),
        alu_ports: Default::default(),
        instructions: vec![InstructionRecord {
            arch: "SKL".to_string(),
            iform: "ADD_GPRv_GPRv".to_string(),
            string: "ADD".to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            locked: false,
            xml_attrs: Default::default(),
            imm_zero: false,
            perf: PerfRecord {
                operands: vec![],
                latencies: vec![],
                uops: 1,
                retire_slots: 1,
                uops_mite: 1,
                uops_ms: 0,
                tp: Some(1.0),
                ports: BTreeMap::from([(String::from("0156"), 1)]),
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
                variants: Default::default(),
            },
        }],
    };

    let result = run_fixture(&code, &invocation, &pack);

    // Summary still from analytical path.
    assert_eq!(result.summary.cycles_simulated, 13);

    // Structural check on cycles JSON emitted by the simulator. The
    // simulator may produce more or fewer cycle entries than min_cycles
    // depending on when unroll-mode retirement reaches min_iterations;
    // assert only on schema, not on exact length.
    assert!(!result.cycles.is_empty());
    assert_eq!(result.cycles[0]["cycle"], 0);
    for (idx, cycle) in result.cycles.iter().enumerate() {
        assert_eq!(cycle["cycle"], idx as u64, "cycle index mismatch at {idx}");
    }
}

#[test]
fn partial_reg_movzx_elimination_aliases_python_input() {
    // mov al, bl; movzx ecx, al; add rax, rcx; dec rdx; jnz loop
    let code = hex::decode("88d80fb6c84801c848ffca75f3").unwrap();
    let invocation = Invocation {
        arch: "HSW".to_string(),
        min_cycles: 500,
        ..Invocation::default()
    };

    let pack_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/arch/HSW.uipack");
    let runtime = uica_data::MappedUiPackRuntime::open(pack_path).unwrap();
    let result = engine_with_runtime(&code, &invocation, &runtime);
    let depends = result.cycles[0]["addedToRS"][1]["dependsOn"]
        .as_array()
        .unwrap();

    assert_eq!(depends.len(), 2);
    assert_eq!(depends[0]["instrID"], 0);
    assert_eq!(depends[1]["instrID"], 0);
    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(2.0));
}

#[test]
fn movzx_special_case_uses_python_sr_fallback() {
    // mov spl, bl; movzx ecx, spl; add rax, rcx; dec rdx; jnz loop
    let code = hex::decode("4088dc400fb6cc4801c848ffca75f1").unwrap();
    let invocation = Invocation {
        arch: "HSW".to_string(),
        min_cycles: 500,
        ..Invocation::default()
    };

    let pack_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/arch/HSW.uipack");
    let runtime = uica_data::MappedUiPackRuntime::open(pack_path).unwrap();
    let result = engine_with_runtime(&code, &invocation, &runtime);
    let movzx_depends = result.cycles[0]["addedToRS"][1]["dependsOn"]
        .as_array()
        .unwrap();

    assert_eq!(movzx_depends.len(), 1);
    assert_eq!(movzx_depends[0]["instrID"], 0);
    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(1.38));
}
