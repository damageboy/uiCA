use std::collections::BTreeMap;

use uica_data::{DataPack, InstructionRecord, PerfRecord};

#[test]
fn finds_candidates_by_arch_and_mnemonic() {
    let pack = DataPack {
        schema_version: "uica-instructions-pack-v1".to_string(),
        instructions: vec![
            InstructionRecord {
                arch: "SKL".to_string(),
                iform: "ADD_GPRv_GPRv".to_string(),
                string: "ADD".to_string(),
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
                    ports: BTreeMap::new(),
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
                iform: "ADD_GPRv_GPRv".to_string(),
                string: "ADD".to_string(),
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
                    ports: BTreeMap::new(),
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

    let idx = uica_data::DataPackIndex::new(pack);
    let candidates = idx.candidates_for("SKL", "ADD");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].arch, "SKL");
}

#[test]
fn returns_empty_for_missing_arch_or_mnemonic() {
    let pack = DataPack {
        schema_version: "uica-instructions-pack-v1".to_string(),
        instructions: vec![],
    };

    let idx = uica_data::DataPackIndex::new(pack);
    assert!(idx.candidates_for("SKL", "ADD").is_empty());
}

#[test]
fn resolves_mnemonic_aliases_in_index_lookup() {
    let pack = DataPack {
        schema_version: "uica-instructions-pack-v1".to_string(),
        instructions: vec![
            InstructionRecord {
                arch: "SKL".to_string(),
                iform: "JNZ_RELBRb".to_string(),
                string: "JNZ".to_string(),
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
                    ports: BTreeMap::new(),
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
                arch: "SKL".to_string(),
                iform: "CMOVG_GPRv_GPRv".to_string(),
                string: "CMOVG".to_string(),
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
                    ports: BTreeMap::new(),
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
                arch: "SKL".to_string(),
                iform: "SETE_GPR8".to_string(),
                string: "SETE".to_string(),
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
                    ports: BTreeMap::new(),
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

    let idx = uica_data::DataPackIndex::new(pack);
    assert_eq!(idx.candidates_for("SKL", "JNE").len(), 1);
    assert_eq!(idx.candidates_for("SKL", "cmovnle").len(), 1);
    assert_eq!(idx.candidates_for("SKL", "setz").len(), 1);
}

#[test]
fn indexes_noncanonical_string_under_iform_prefix() {
    let pack = DataPack {
        schema_version: "uica-instructions-pack-v1".to_string(),
        instructions: vec![InstructionRecord {
            arch: "SKL".to_string(),
            iform: "MOV_GPRv_GPRv".to_string(),
            string: "MOVE".to_string(),
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
                ports: BTreeMap::new(),
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

    let idx = uica_data::DataPackIndex::new(pack);
    let candidates = idx.candidates_for("SKL", "mov rax, rbx");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].string, "MOVE");
}
