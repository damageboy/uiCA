use std::collections::BTreeMap;

use uica_core::analytical::{compute_dsb_limit, AnalyticalInstruction};
use uica_core::{
    compute_issue_limit, compute_port_usage_limit, get_micro_arch, match_instruction,
    match_instruction_record, normalize_mnemonic, CandidateRecord, InstructionPortUsage,
    NormalizedInstr,
};
use uica_data::{InstructionRecord, PerfRecord};

#[test]
fn computes_port_usage_limit_from_combined_port_sets() {
    let instructions = vec![
        InstructionPortUsage {
            port_data: BTreeMap::from([(String::from("0"), 1)]),
            uops: 1,
        },
        InstructionPortUsage {
            port_data: BTreeMap::from([(String::from("1"), 1)]),
            uops: 1,
        },
        InstructionPortUsage {
            port_data: BTreeMap::from([(String::from("01"), 1)]),
            uops: 1,
        },
    ];

    assert_eq!(compute_port_usage_limit(&instructions), 1.5);
}

#[test]
fn dsb_limit_uses_python_six_uop_block_capacity_not_arch_width() {
    let arch = get_micro_arch("HSW").unwrap();
    let mut instructions = vec![AnalyticalInstruction::default(); 6];
    for instr in &mut instructions {
        instr.uops_mite = 1;
        instr.size = 3;
    }
    instructions[3].uops_mite = 2;
    instructions[5].macro_fused_with_prev = true;

    assert_eq!(compute_dsb_limit(&instructions, 0, &arch), Some(1.0));
}

#[test]
fn returns_zero_port_usage_limit_for_empty_input() {
    assert_eq!(compute_port_usage_limit(&[]), 0.0);
}

#[test]
fn computes_issue_limit_from_total_uops_and_issue_width() {
    assert_eq!(compute_issue_limit(10, 4), 2.5);
}

#[test]
fn matches_instruction_by_string_case_insensitively() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 0,
        immediate: None,
        iform_signature: String::new(),
        mnemonic: "add".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: Default::default(),
        agen: None,
    };
    let candidates = vec![CandidateRecord {
        iform: "ADD_GPRv_GPRv".to_string(),
        string: "ADD".to_string(),
    }];

    let matched = match_instruction(&instr, &candidates).expect("candidate should match");

    assert_eq!(matched.iform, "ADD_GPRv_GPRv");
}

#[test]
fn matches_instruction_by_iform_prefix_when_string_differs() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 0,
        immediate: None,
        iform_signature: String::new(),
        mnemonic: "mov".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: Default::default(),
        agen: None,
    };
    let candidates = vec![CandidateRecord {
        iform: "MOV_GPRv_GPRv".to_string(),
        string: "MOVE".to_string(),
    }];

    let matched = match_instruction(&instr, &candidates).expect("iform prefix should match");

    assert_eq!(matched.string, "MOVE");
}

#[test]
fn returns_none_when_no_candidate_matches() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 0,
        immediate: None,
        iform_signature: String::new(),
        mnemonic: "sub".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: Default::default(),
        agen: None,
    };
    let candidates = vec![CandidateRecord {
        iform: "ADD_GPRv_GPRv".to_string(),
        string: "ADD".to_string(),
    }];

    assert!(match_instruction(&instr, &candidates).is_none());
}

#[test]
fn matches_jcc_aliases_to_xed_candidate_names() {
    let je = NormalizedInstr {
        max_op_size_bytes: 0,
        immediate: None,
        iform_signature: String::new(),
        mnemonic: "je rel8".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: Default::default(),
        agen: None,
    };
    let jne = NormalizedInstr {
        max_op_size_bytes: 0,
        immediate: None,
        iform_signature: String::new(),
        mnemonic: "jne rel8".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: Default::default(),
        agen: None,
    };
    let candidates = vec![
        CandidateRecord {
            iform: "JZ_RELBRb".to_string(),
            string: "JZ".to_string(),
        },
        CandidateRecord {
            iform: "JNZ_RELBRb".to_string(),
            string: "JNZ".to_string(),
        },
    ];

    let matched_je = match_instruction(&je, &candidates).expect("je alias should match");
    let matched_jne = match_instruction(&jne, &candidates).expect("jne alias should match");

    assert_eq!(matched_je.string, "JZ");
    assert_eq!(matched_jne.string, "JNZ");
}

#[test]
fn normalizes_jcc_aliases_to_xed_names() {
    assert_eq!(normalize_mnemonic("je"), "JZ");
    assert_eq!(normalize_mnemonic("JE rel8"), "JZ");
    assert_eq!(normalize_mnemonic("jne"), "JNZ");
    assert_eq!(normalize_mnemonic("JNE rel8"), "JNZ");
}

#[test]
fn normalizes_cmov_and_setcc_aliases() {
    assert_eq!(normalize_mnemonic("cmovnle"), "CMOVG");
    assert_eq!(normalize_mnemonic("CMOVNLE rax, rbx"), "CMOVG");
    assert_eq!(normalize_mnemonic("setz"), "SETE");
    assert_eq!(normalize_mnemonic("SETZ al"), "SETE");
}

#[test]
fn matches_cmov_and_setcc_aliases_to_candidates() {
    let cmov = NormalizedInstr {
        max_op_size_bytes: 0,
        immediate: None,
        iform_signature: String::new(),
        mnemonic: "cmovnle".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: Default::default(),
        agen: None,
    };
    let setcc = NormalizedInstr {
        max_op_size_bytes: 0,
        immediate: None,
        iform_signature: String::new(),
        mnemonic: "setz".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: Default::default(),
        agen: None,
    };

    let candidates = vec![
        CandidateRecord {
            iform: "CMOVG_GPRv_GPRv".to_string(),
            string: "CMOVG".to_string(),
        },
        CandidateRecord {
            iform: "SETE_GPR8".to_string(),
            string: "SETE".to_string(),
        },
    ];

    let matched_cmov = match_instruction(&cmov, &candidates).expect("cmov alias should match");
    let matched_setcc = match_instruction(&setcc, &candidates).expect("setcc alias should match");

    assert_eq!(matched_cmov.string, "CMOVG");
    assert_eq!(matched_setcc.string, "SETE");
}

#[test]
fn does_not_fallback_to_mnemonic_when_decoded_iform_signature_misses() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 8,
        immediate: None,
        iform_signature: "VGPR64q_VGPR64q_VGPR64q".to_string(),
        mnemonic: "mulx".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: vec!["RAX".to_string(), "RBX".to_string(), "RCX".to_string()],
        xml_attrs: Default::default(),
        agen: None,
    };
    let candidates = vec![record("MULX_GPR64q_GPR64q_GPR64q", "MULX (R64, R64, R64)")];

    assert!(match_instruction_record(&instr, &candidates).is_none());
}

#[test]
fn matches_nonzero_immediate_to_general_immediate_record() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 8,
        immediate: Some(3),
        iform_signature: "GPRv_IMMb".to_string(),
        mnemonic: "sar".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: Default::default(),
        agen: None,
    };
    let candidates = vec![
        record_with_immzero("SAR_GPRv_IMMb", "SAR (R64, 0)", true),
        record("SAR_GPRv_IMMb", "SAR (R64, I8)"),
    ];

    let matched = match_instruction_record(&instr, &candidates).expect("candidate should match");

    assert_eq!(matched.string, "SAR (R64, I8)");
}

#[test]
fn matches_zero_immediate_metadata_independent_of_display_position() {
    let zero = NormalizedInstr {
        max_op_size_bytes: 0,
        immediate: Some(0),
        iform_signature: "IMMb".to_string(),
        mnemonic: "push".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: Default::default(),
        agen: None,
    };
    let nonzero = NormalizedInstr {
        immediate: Some(7),
        ..zero.clone()
    };
    let candidates = vec![
        record_with_immzero("PUSH_IMMb", "PUSH (0)", true),
        record("PUSH_IMMb", "PUSH (I8)"),
    ];

    let matched_zero = match_instruction_record(&zero, &candidates).expect("zero candidate");
    let matched_nonzero =
        match_instruction_record(&nonzero, &candidates).expect("nonzero candidate");

    assert_eq!(matched_zero.string, "PUSH (0)");
    assert_eq!(matched_nonzero.string, "PUSH (I8)");
}

#[test]
fn matches_python_xml_attributes_before_string_fallbacks() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 8,
        immediate: None,
        iform_signature: "GPRv_MEMv".to_string(),
        mnemonic: "cmp".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: [("rm".to_string(), "7".to_string())].into_iter().collect(),
        agen: None,
    };
    let mut wrong = record("CMP_GPRv_MEMv", "CMP (R64, M64 wrong rm)");
    wrong.xml_attrs.insert("rm".to_string(), "3".to_string());
    let mut right = record("CMP_GPRv_MEMv", "CMP (R64, M64 right rm)");
    right.xml_attrs.insert("rm".to_string(), "67".to_string());

    let candidates = vec![wrong, right];
    let matched = match_instruction_record(&instr, &candidates).expect("xml attr match");

    assert_eq!(matched.string, "CMP (R64, M64 right rm)");
}

#[test]
fn matches_lea_agen_form_before_size_fallback() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 8,
        immediate: None,
        iform_signature: "GPRv_MEM".to_string(),
        mnemonic: "lea".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        xml_attrs: Default::default(),
        agen: Some("B_IS_D8".to_string()),
    };
    let candidates = vec![
        record("LEA_GPRv_AGEN", "LEA_B (R16)"),
        record("LEA_GPRv_AGEN", "LEA_B (R64)"),
        record("LEA_GPRv_AGEN", "LEA_B_IS_D8 (R64)"),
    ];

    let matched = match_instruction_record(&instr, &candidates).expect("candidate should match");

    assert_eq!(matched.string, "LEA_B_IS_D8 (R64)");
}

#[test]
fn matches_high8_record_even_when_larger_dest_sets_max_size() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 4,
        immediate: None,
        iform_signature: "GPRv_GPR8".to_string(),
        mnemonic: "movzx".to_string(),
        uses_high8_reg: true,
        explicit_reg_operands: vec!["ECX".to_string(), "AH".to_string()],
        xml_attrs: Default::default(),
        agen: None,
    };
    let candidates = vec![
        record("MOVZX_GPRv_GPR8", "MOVZX (R32, R8l)"),
        record("MOVZX_GPRv_GPR8", "MOVZX (R32, R8h)"),
    ];

    let matched = match_instruction_record(&instr, &candidates).expect("candidate should match");

    assert_eq!(matched.string, "MOVZX (R32, R8h)");
}

#[test]
fn prefers_unmasked_evex_record_when_k0_is_implicit() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 64,
        immediate: None,
        iform_signature: "ZMMu64_MASKmskw_MEMu64_AVX512".to_string(),
        mnemonic: "vmovdqu64".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: vec!["ZMM0".to_string()],
        xml_attrs: Default::default(),
        agen: None,
    };
    let candidates = vec![
        record(
            "VMOVDQU64_ZMMu64_MASKmskw_MEMu64_AVX512",
            "VMOVDQU64 (ZMM, K, M512)",
        ),
        record(
            "VMOVDQU64_ZMMu64_MASKmskw_MEMu64_AVX512",
            "VMOVDQU64 (ZMM, M512)",
        ),
    ];

    let matched = match_instruction_record(&instr, &candidates).expect("candidate should match");

    assert_eq!(matched.string, "VMOVDQU64 (ZMM, M512)");
}

#[test]
fn prefers_masked_evex_record_when_mask_is_explicit() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 64,
        immediate: None,
        iform_signature: "ZMMu64_MASKmskw_MEMu64_AVX512".to_string(),
        mnemonic: "vmovdqu64".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: vec!["ZMM0".to_string(), "K1".to_string()],
        xml_attrs: Default::default(),
        agen: None,
    };
    let candidates = vec![
        record(
            "VMOVDQU64_ZMMu64_MASKmskw_MEMu64_AVX512",
            "VMOVDQU64 (ZMM, K, M512)",
        ),
        record(
            "VMOVDQU64_ZMMu64_MASKmskw_MEMu64_AVX512",
            "VMOVDQU64 (ZMM, M512)",
        ),
    ];

    let matched = match_instruction_record(&instr, &candidates).expect("candidate should match");

    assert_eq!(matched.string, "VMOVDQU64 (ZMM, K, M512)");
}

#[test]
fn matches_low8_record_without_high8_substring() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 1,
        immediate: None,
        iform_signature: "GPR8_GPR8".to_string(),
        mnemonic: "mov".to_string(),
        uses_high8_reg: false,
        explicit_reg_operands: vec!["AL".to_string(), "BL".to_string()],
        xml_attrs: Default::default(),
        agen: None,
    };
    let candidates = vec![
        record("MOV_GPR8_GPR8_88", "MOV_88 (R8h, R8l)"),
        record("MOV_GPR8_GPR8_88", "MOV_88 (R8l, R8h)"),
        record("MOV_GPR8_GPR8_88", "MOV_88 (R8l, R8l)"),
    ];

    let matched = match_instruction_record(&instr, &candidates).expect("candidate should match");

    assert_eq!(matched.string, "MOV_88 (R8l, R8l)");
}

fn record(iform: &str, string: &str) -> InstructionRecord {
    record_with_immzero(iform, string, false)
}

fn record_with_immzero(iform: &str, string: &str, imm_zero: bool) -> InstructionRecord {
    InstructionRecord {
        arch: "HSW".to_string(),
        iform: iform.to_string(),
        string: string.to_string(),
        xml_attrs: Default::default(),
        imm_zero,
        perf: PerfRecord {
            uops: 1,
            retire_slots: 1,
            uops_mite: 1,
            uops_ms: 0,
            tp: None,
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
            operands: vec![],
            latencies: vec![],
            variants: Default::default(),
            macro_fusible_with: vec![],
        },
    }
}
