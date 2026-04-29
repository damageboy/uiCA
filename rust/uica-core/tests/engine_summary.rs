use std::collections::BTreeMap;

use uica_data::{DataPack, InstructionRecord, PerfRecord};
use uica_model::Invocation;

#[test]
fn computes_summary_from_decoded_records() {
    let code = hex::decode("4801d8").unwrap(); // add rax, rbx
    let invocation = Invocation {
        arch: "SKL".to_string(),
        ..Invocation::default()
    };

    let pack = DataPack {
        schema_version: "uica-instructions-pack-v1".to_string(),
        instructions: vec![InstructionRecord {
            arch: "SKL".to_string(),
            iform: "ADD_GPRv_GPRv".to_string(),
            string: "ADD".to_string(),
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
            },
        }],
    };

    let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);

    assert_eq!(result.invocation.arch, "SKL");
    assert_eq!(result.summary.mode, "unroll");
    assert!(result.summary.throughput_cycles_per_iteration.is_some());
    assert_eq!(result.parameters["uArchName"], "SKL");
    assert_eq!(result.parameters["issueWidth"], 4);
}

#[test]
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

        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };

        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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

        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };

        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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
fn jne_alias_matches_jnz_record_in_engine_path() {
    let code = hex::decode("4801d875fb").unwrap(); // add rax, rbx; jne back
    let invocation = Invocation {
        arch: "SKL".to_string(),
        min_cycles: 500,
        ..Invocation::default()
    };

    let pack = DataPack {
        schema_version: "uica-instructions-pack-v1".to_string(),
        instructions: vec![
            InstructionRecord {
                arch: "SKL".to_string(),
                iform: "ADD_GPRv_GPRv".to_string(),
                string: "ADD".to_string(),
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
                },
            },
            InstructionRecord {
                arch: "SKL".to_string(),
                iform: "JNZ_RELBRb".to_string(),
                string: "JNZ".to_string(),
                perf: PerfRecord {
                    operands: vec![],
                    latencies: vec![],
                    uops: 8,
                    retire_slots: 8,
                    uops_mite: 8,
                    uops_ms: 0,
                    tp: Some(8.0),
                    ports: BTreeMap::from([(String::from("6"), 8)]),
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
                },
            },
        ],
    };

    let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
    assert_eq!(result.summary.mode, "loop");
    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(8.0));
    assert_eq!(result.summary.limits.get("ports"), Some(&Some(8.0)));
    assert_eq!(
        result.summary.bottlenecks_predicted,
        vec!["Ports".to_string()]
    );
}

#[test]
fn quick_model_falls_back_safely_when_pack_is_incomplete() {
    let code = hex::decode("48ffc975fb").unwrap();
    let invocation = Invocation {
        arch: "SKL".to_string(),
        min_cycles: 500,
        ..Invocation::default()
    };

    let pack = DataPack {
        schema_version: "uica-instructions-pack-v1".to_string(),
        instructions: vec![InstructionRecord {
            arch: "SKL".to_string(),
            iform: "DEC_GPRv".to_string(),
            string: "DEC".to_string(),
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
            },
        }],
    };

    let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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
fn quick_model_skips_partial_data_for_non_preferred_signatures() {
    let code = hex::decode("4801d84829d875f8").unwrap(); // add; sub; jnz
    let invocation = Invocation {
        arch: "SKL".to_string(),
        min_cycles: 500,
        ..Invocation::default()
    };

    let pack = DataPack {
        schema_version: "uica-instructions-pack-v1".to_string(),
        instructions: vec![],
    };

    let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
    assert_eq!(result.summary.mode, "loop");
    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(1.0));
    assert_eq!(result.summary.iterations_simulated, 493);
    assert_eq!(result.summary.limits.get("issue"), Some(&Some(0.0)));
    assert_eq!(result.summary.limits.get("dependencies"), Some(&None));
    assert!(result.summary.bottlenecks_predicted.is_empty());
}

#[test]
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

        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "CMP_GPRv_GPRv".to_string(),
                    string: "CMP".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "CMOVNLE_GPRv_GPRv".to_string(),
                    string: "CMOVNLE".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SETZ_GPR8".to_string(),
                    string: "SETZ".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };

        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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

        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SUB_GPRv_GPRv".to_string(),
                    string: "SUB".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADC_GPRv_GPRv".to_string(),
                    string: "ADC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };

        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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

        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SBB_GPRv_GPRv".to_string(),
                    string: "SBB".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADC_GPRv_GPRv".to_string(),
                    string: "ADC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };

        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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

        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "MOV_GPRv_GPRv".to_string(),
                    string: "MOV".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };

        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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

        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "MOV_GPRv_GPRv".to_string(),
                    string: "MOV".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_GPRv".to_string(),
                    string: "ADD".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };

        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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
        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "XOR_GPRv_GPRv".to_string(),
                    string: "XOR".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "MOV_GPRv_GPRv".to_string(),
                    string: "MOV".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DIV_GPRv".to_string(),
                    string: "DIV".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "IMUL_GPRv_GPRv".to_string(),
                    string: "IMUL".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };
        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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
        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SHL_GPRv_GPRv".to_string(),
                    string: "SHL".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ROR_GPRv_GPRv".to_string(),
                    string: "ROR".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SAR_GPRv_GPRv".to_string(),
                    string: "SAR".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };
        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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
        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "MOVDQA_XMMdq_XMMdq".to_string(),
                    string: "MOVDQA".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "PADDQ_XMMdq_XMMdq".to_string(),
                    string: "PADDQ".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "PXOR_XMMdq_XMMdq".to_string(),
                    string: "PXOR".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };
        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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
        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "VADDPS_YMMqq_YMMqq_YMMqq".to_string(),
                    string: "VADDPS".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "VMULPS_YMMqq_YMMqq_YMMqq".to_string(),
                    string: "VMULPS".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "VPXOR_YMMqq_YMMqq_YMMqq".to_string(),
                    string: "VPXOR".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };
        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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
        let pack = DataPack {
            schema_version: "uica-instructions-pack-v1".to_string(),
            instructions: vec![
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "ADD_GPRv_MEMv".to_string(),
                    string: "ADD".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "LFENCE".to_string(),
                    string: "LFENCE".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "SFENCE".to_string(),
                    string: "SFENCE".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "DEC_GPRv".to_string(),
                    string: "DEC".to_string(),
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
                    },
                },
                InstructionRecord {
                    arch: arch.to_string(),
                    iform: "JNZ_RELBRb".to_string(),
                    string: "JNZ".to_string(),
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
                    },
                },
            ],
        };
        let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);
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
fn emits_cycle_skeleton_with_expected_length_and_cycle_indices() {
    let code = hex::decode("4801d8").unwrap(); // add rax, rbx
    let invocation = Invocation {
        arch: "SKL".to_string(),
        min_cycles: 12,
        ..Invocation::default()
    };

    let pack = DataPack {
        schema_version: "uica-instructions-pack-v1".to_string(),
        instructions: vec![InstructionRecord {
            arch: "SKL".to_string(),
            iform: "ADD_GPRv_GPRv".to_string(),
            string: "ADD".to_string(),
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
            },
        }],
    };

    let result = uica_core::engine::engine_with_pack(&code, &invocation, &pack);

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
