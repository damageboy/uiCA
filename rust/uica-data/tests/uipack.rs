use std::collections::BTreeMap;

use tempfile::tempdir;
use uica_data::{
    encode_uipack, load_pack, load_pack_bytes, load_uipack, load_uipack_bytes,
    load_uipack_bytes_verified, load_uipack_verified, read_uipack_header,
    read_uipack_header_verified, DataPack, DataPackIndex, InstructionRecord, LatencyRecord,
    MappedUiPack, MappedUiPackRuntime, OperandRecord, PerfRecord, DATAPACK_SCHEMA_VERSION,
    UIPACK_MAGIC, UIPACK_VERSION,
};

const CHECKSUM_OFFSET: usize = 24;
const FNV1A64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100000001b3;

fn rewrite_checksum(bytes: &mut [u8]) {
    bytes[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 8].fill(0);
    let mut hash = FNV1A64_OFFSET_BASIS;
    for byte in bytes.iter() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV1A64_PRIME);
    }
    bytes[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 8].copy_from_slice(&hash.to_le_bytes());
}

fn sample_pack() -> DataPack {
    let mut add_ports = BTreeMap::new();
    add_ports.insert("0156".to_string(), 1);

    let mut mul_ports = BTreeMap::new();
    mul_ports.insert("01".to_string(), 2);
    mul_ports.insert("5".to_string(), 1);

    DataPack {
        schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
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
        instructions: vec![
            InstructionRecord {
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
                xml_attrs: BTreeMap::from([
                    ("eosz".to_string(), "3".to_string()),
                    ("rm".to_string(), "3".to_string()),
                ]),
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
                        OperandRecord {
                            name: "AGEN0".to_string(),
                            r#type: "mem".to_string(),
                            read: true,
                            write: false,
                            implicit: false,
                            flags: vec![],
                            flags_read: vec![],
                            flags_write: vec![],
                            mem_base: Some("RAX".to_string()),
                            mem_index: Some("RCX".to_string()),
                            mem_scale: Some(2),
                            mem_disp: Some(8),
                            is_agen: true,
                            mem_operand_role: Some("agen".to_string()),
                        },
                    ],
                    latencies: vec![LatencyRecord {
                        start_op: "REG1".to_string(),
                        target_op: "REG0".to_string(),
                        cycles: 1,
                        cycles_addr: None,
                        cycles_addr_index: None,
                        cycles_mem: None,
                        cycles_same_reg: Some(0),
                    }],
                    uops: 1,
                    retire_slots: 1,
                    uops_mite: 1,
                    uops_ms: 0,
                    tp: Some(1.0),
                    ports: add_ports,
                    div_cycles: 0,
                    may_be_eliminated: false,
                    complex_decoder: false,
                    n_available_simple_decoders: 0,
                    lcp_stall: false,
                    implicit_rsp_change: 0,
                    can_be_used_by_lsd: true,
                    cannot_be_in_dsb_due_to_jcc_erratum: true,
                    no_micro_fusion: true,
                    no_macro_fusion: true,
                    macro_fusible_with: vec!["JZ (Rel8)".to_string(), "JO (Rel8)".to_string()],
                    variants: Default::default(),
                },
            },
            InstructionRecord {
                arch: "SKL".to_string(),
                iform: "IMUL_GPRv_GPRv".to_string(),
                string: "IMUL".to_string(),
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
                locked: true,
                xml_attrs: Default::default(),
                imm_zero: false,
                perf: PerfRecord {
                    operands: vec![],
                    latencies: vec![],
                    uops: 2,
                    retire_slots: 1,
                    uops_mite: 2,
                    uops_ms: 0,
                    tp: None,
                    ports: mul_ports,
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
    }
}

#[test]
fn roundtrips_single_arch_uipack_and_keeps_index_compatibility() {
    let pack = sample_pack();
    let bytes = encode_uipack(&pack, "SKL").unwrap();
    let header = read_uipack_header(&bytes).unwrap();

    assert!(bytes.starts_with(&UIPACK_MAGIC));
    assert_eq!(header.version, UIPACK_VERSION);
    assert_eq!(header.records_count, 2);
    assert_eq!(header.ports_count, 3);

    let decoded = load_uipack_bytes(&bytes).unwrap();
    assert_eq!(decoded, pack);
    assert_eq!(decoded.all_ports, vec!["0", "1", "5", "6"]);
    assert_eq!(decoded.alu_ports, vec!["0", "1", "5", "6"]);
    assert_eq!(
        decoded.instructions[0].xml_attrs.get("eosz"),
        Some(&"3".to_string())
    );
    assert_eq!(
        decoded.instructions[0].xml_attrs.get("rm"),
        Some(&"3".to_string())
    );
    assert!(decoded.instructions[0].perf.can_be_used_by_lsd);
    assert!(
        decoded.instructions[0]
            .perf
            .cannot_be_in_dsb_due_to_jcc_erratum
    );
    assert!(decoded.instructions[0].perf.no_micro_fusion);
    assert!(decoded.instructions[0].perf.no_macro_fusion);
    assert_eq!(
        decoded.instructions[0].perf.macro_fusible_with,
        vec!["JZ (Rel8)".to_string(), "JO (Rel8)".to_string()]
    );
    let agen = &decoded.instructions[0].perf.operands[2];
    assert!(agen.is_agen);
    assert_eq!(agen.mem_operand_role.as_deref(), Some("agen"));
    assert_eq!(agen.mem_base.as_deref(), Some("RAX"));

    let decoded_via_auto = load_pack_bytes(&bytes).unwrap();
    let index = DataPackIndex::new(&decoded_via_auto);
    let add_candidates: Vec<_> = index.candidates_for("skl", "add rax, rbx").collect();
    let mul_candidates: Vec<_> = index.candidates_for("SKL", "IMUL").collect();

    assert_eq!(add_candidates.len(), 1);
    assert_eq!(add_candidates[0].iform, "ADD_GPRv_GPRv");
    assert_eq!(mul_candidates.len(), 1);
    assert!(decoded.instructions[1].locked);
    assert_eq!(mul_candidates[0].perf.tp, None);
}

#[test]
fn rejects_bad_magic_and_version_but_skips_checksum_by_default() {
    let bytes = encode_uipack(&sample_pack(), "SKL").unwrap();

    let mut bad_magic = bytes.clone();
    bad_magic[0] ^= 0xff;
    let err = load_uipack_bytes(&bad_magic).unwrap_err().to_string();
    assert!(err.contains("invalid uipack magic"), "{err}");

    let mut bad_version = bytes.clone();
    bad_version[8..10].copy_from_slice(&(UIPACK_VERSION + 1).to_le_bytes());
    let err = load_uipack_bytes(&bad_version).unwrap_err().to_string();
    assert!(err.contains("unsupported uipack version"), "{err}");

    let mut bad_checksum = bytes.clone();
    bad_checksum[24] ^= 1;
    assert!(load_uipack_bytes(&bad_checksum).is_ok());
    let err = load_uipack_bytes_verified(&bad_checksum)
        .unwrap_err()
        .to_string();
    assert!(err.contains("uipack checksum mismatch"), "{err}");
}

#[test]
fn verified_header_and_file_load_reject_bad_checksum() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("SKL.uipack");
    let mut bytes = encode_uipack(&sample_pack(), "SKL").unwrap();
    bytes[24] ^= 1;
    std::fs::write(&path, &bytes).unwrap();

    assert!(read_uipack_header(&bytes).is_ok());
    let err = read_uipack_header_verified(&bytes).unwrap_err().to_string();
    assert!(err.contains("uipack checksum mismatch"), "{err}");
    assert!(load_uipack(&path).is_ok());
    assert!(MappedUiPack::open(&path).is_ok());

    let err = load_uipack_verified(&path).unwrap_err().to_string();
    assert!(err.contains("uipack checksum mismatch"), "{err}");
    let err = match MappedUiPack::open_verified(&path) {
        Ok(_) => panic!("verified mapped open should reject bad checksum"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("uipack checksum mismatch"), "{err}");
    let err = match MappedUiPackRuntime::open_verified(&path) {
        Ok(_) => panic!("verified runtime open should reject bad checksum"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("uipack checksum mismatch"), "{err}");
}

#[test]
fn rejects_record_string_offset_overflow_or_oob() {
    let bytes = encode_uipack(&sample_pack(), "SKL").unwrap();
    let header = read_uipack_header(&bytes).unwrap();

    let mut malformed = bytes.clone();
    let records_base = usize::try_from(header.records_offset).unwrap();
    malformed[records_base..records_base + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    rewrite_checksum(&mut malformed);

    let err = load_uipack_bytes(&malformed).unwrap_err().to_string();
    assert!(err.contains("uipack string offset"), "{err}");
}

#[test]
fn preserves_schema_version_across_binary_roundtrip() {
    let mut pack = sample_pack();
    pack.schema_version = "custom-schema-version".to_string();

    let decoded = load_uipack_bytes(&encode_uipack(&pack, "SKL").unwrap()).unwrap();
    assert_eq!(decoded.schema_version, "custom-schema-version");
    assert_eq!(decoded.instructions, pack.instructions);
}

#[test]
fn load_pack_and_load_uipack_read_binary_files() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("SKL.uipack");
    let pack = sample_pack();
    std::fs::write(&path, encode_uipack(&pack, "SKL").unwrap()).unwrap();

    assert_eq!(load_uipack(&path).unwrap(), pack);
    assert_eq!(load_pack(&path).unwrap(), sample_pack());
}
