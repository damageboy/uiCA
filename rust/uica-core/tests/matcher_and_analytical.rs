use std::collections::BTreeMap;

use uica_core::{
    compute_issue_limit, compute_port_usage_limit, match_instruction, normalize_mnemonic,
    CandidateRecord, InstructionPortUsage, NormalizedInstr,
};

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
        iform_signature: String::new(),
        mnemonic: "add".to_string(),
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
        iform_signature: String::new(),
        mnemonic: "mov".to_string(),
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
        iform_signature: String::new(),
        mnemonic: "sub".to_string(),
    };
    let candidates = vec![CandidateRecord {
        iform: "ADD_GPRv_GPRv".to_string(),
        string: "ADD".to_string(),
    }];

    assert!(match_instruction(&instr, &candidates).is_none());
}

#[test]
fn matches_jne_to_jnz_candidate_alias() {
    let instr = NormalizedInstr {
        max_op_size_bytes: 0,
        iform_signature: String::new(),
        mnemonic: "jne rel8".to_string(),
    };
    let candidates = vec![CandidateRecord {
        iform: "JNZ_RELBRb".to_string(),
        string: "JNZ".to_string(),
    }];

    let matched = match_instruction(&instr, &candidates).expect("alias should match");

    assert_eq!(matched.string, "JNZ");
}

#[test]
fn normalizes_jne_to_jnz_alias() {
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
        iform_signature: String::new(),
        mnemonic: "cmovnle".to_string(),
    };
    let setcc = NormalizedInstr {
        max_op_size_bytes: 0,
        iform_signature: String::new(),
        mnemonic: "setz".to_string(),
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
