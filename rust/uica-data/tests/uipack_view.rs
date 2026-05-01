use std::collections::BTreeMap;

use tempfile::tempdir;
use uica_data::{
    encode_uipack, read_uipack_header, DataPack, InstructionRecord, MappedUiPack, PerfRecord,
    UiPackView, UiPackViewIndex, DATAPACK_SCHEMA_VERSION,
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
        instructions: vec![
            InstructionRecord {
                arch: "SKL".to_string(),
                iform: "ADD_GPRv_GPRv".to_string(),
                string: "ADD".to_string(),
                imm_zero: false,
                perf: PerfRecord {
                    operands: vec![],
                    latencies: vec![],
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
                    can_be_used_by_lsd: false,
                    cannot_be_in_dsb_due_to_jcc_erratum: false,
                    no_micro_fusion: false,
                    no_macro_fusion: false,
                    variants: Default::default(),
                },
            },
            InstructionRecord {
                arch: "SKL".to_string(),
                iform: "IMUL_GPRv_GPRv".to_string(),
                string: "IMUL".to_string(),
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
                    variants: Default::default(),
                },
            },
        ],
    }
}

#[test]
fn creates_view_and_reads_records_without_materializing_pack() {
    let bytes = encode_uipack(&sample_pack(), "SKL").unwrap();
    let header = read_uipack_header(&bytes).unwrap();
    let view = UiPackView::from_bytes(&bytes).unwrap();

    assert_eq!(view.header(), header);
    assert_eq!(view.arch(), "SKL");
    assert_eq!(view.schema_version(), DATAPACK_SCHEMA_VERSION);
    assert_eq!(view.record_count(), 2);
    assert_eq!(view.resolve_string(header.arch_offset).unwrap(), "SKL");

    let add = view.record(0).unwrap();
    assert_eq!(add.index(), 0);
    assert_eq!(add.iform(), "ADD_GPRv_GPRv");
    assert_eq!(add.string(), "ADD");
    assert_eq!(add.perf().uops(), 1);
    assert_eq!(add.perf().tp(), Some(1.0));

    let mul = view.record(1).unwrap();
    let mul_ports = mul.ports().unwrap();
    assert_eq!(mul.index(), 1);
    assert_eq!(mul.iform(), "IMUL_GPRv_GPRv");
    assert_eq!(mul.string(), "IMUL");
    assert_eq!(mul.perf().uops(), 2);
    assert_eq!(mul.perf().retire_slots(), 1);
    assert_eq!(mul.perf().uops_mite(), 2);
    assert_eq!(mul.perf().uops_ms(), 0);
    assert_eq!(mul.perf().tp(), None);
    assert_eq!(mul_ports.len(), 2);
    assert_eq!(mul_ports[0].key(), "01");
    assert_eq!(mul_ports[0].count(), 2);
    assert_eq!(mul_ports[1].key(), "5");
    assert_eq!(mul_ports[1].count(), 1);
}

#[test]
fn lookup_index_finds_records_by_mnemonic_and_iform() {
    let bytes = encode_uipack(&sample_pack(), "SKL").unwrap();
    let view = UiPackView::from_bytes(&bytes).unwrap();
    let index = UiPackViewIndex::new(&view).unwrap();

    let add = index.record_indices_for_mnemonic("add rax, rbx");
    let mul = index.record_indices_for_mnemonic("IMUL");
    let iform = index.record_indices_for_iform("imul_gprv_gprv");

    assert_eq!(add, &[0]);
    assert_eq!(mul, &[1]);
    assert_eq!(iform, &[1]);
    assert!(index.record_indices_for_mnemonic("SUB").is_empty());
    assert!(index.record_indices_for_iform("missing").is_empty());
    assert_eq!(view.record(add[0]).unwrap().iform(), "ADD_GPRv_GPRv");
    assert_eq!(view.record(iform[0]).unwrap().string(), "IMUL");
}

#[test]
fn rejects_malformed_bytes_during_view_construction() {
    let bytes = encode_uipack(&sample_pack(), "SKL").unwrap();
    let header = read_uipack_header(&bytes).unwrap();
    let mut malformed = bytes.clone();

    malformed[44..48].copy_from_slice(&(header.records_offset + 4).to_le_bytes());
    rewrite_checksum(&mut malformed);

    let err = UiPackView::from_bytes(&malformed).unwrap_err().to_string();
    assert!(err.contains("records section misaligned"), "{err}");
}

#[test]
fn rejects_out_of_bounds_record_index() {
    let bytes = encode_uipack(&sample_pack(), "SKL").unwrap();
    let view = UiPackView::from_bytes(&bytes).unwrap();

    let err = view.record(view.record_count()).unwrap_err().to_string();
    assert!(err.contains("record index"), "{err}");
}

#[test]
fn byte_backed_container_exposes_uipack_view() {
    let bytes = encode_uipack(&sample_pack(), "SKL").unwrap();
    let mapped = MappedUiPack::from_bytes(bytes.clone());
    let view = mapped.view().unwrap();

    assert_eq!(mapped.bytes(), bytes.as_slice());
    assert_eq!(view.arch(), "SKL");
    assert_eq!(view.record(1).unwrap().iform(), "IMUL_GPRv_GPRv");
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn mmap_open_exposes_uipack_view() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("SKL.uipack");
    let bytes = encode_uipack(&sample_pack(), "SKL").unwrap();
    std::fs::write(&path, &bytes).unwrap();

    let mapped = MappedUiPack::open(&path).unwrap();
    let view = mapped.view().unwrap();

    assert_eq!(mapped.bytes(), bytes.as_slice());
    assert_eq!(view.arch(), "SKL");
    assert_eq!(view.record_count(), 2);
}
