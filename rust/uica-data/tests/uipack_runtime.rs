use std::collections::BTreeMap;

use tempfile::tempdir;
use uica_data::{
    encode_uipack, load_manifest_runtime, read_uipack_header, record_view_to_instruction_record,
    DataPack, DataPackManifest, DataPackManifestArchEntry, InstructionRecord, MappedUiPackRuntime,
    PerfRecord, DATAPACK_MANIFEST_SCHEMA_VERSION, DATAPACK_SCHEMA_VERSION, UIPACK_VERSION,
};

fn sample_pack(arch: &str) -> DataPack {
    DataPack {
        schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
        all_ports: vec!["0".to_string(), "1".to_string()],
        alu_ports: vec!["0".to_string()],
        instructions: vec![
            record(
                arch,
                "ADD_GPRv_GPRv",
                "ADD",
                BTreeMap::from([("0156".to_string(), 1)]),
            ),
            record(
                arch,
                "IMUL_GPRv_GPRv",
                "IMUL",
                BTreeMap::from([("01".to_string(), 2)]),
            ),
        ],
    }
}

fn record(
    arch: &str,
    iform: &str,
    string: &str,
    ports: BTreeMap<String, i32>,
) -> InstructionRecord {
    InstructionRecord {
        arch: arch.to_string(),
        iform: iform.to_string(),
        string: string.to_string(),
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
            ports,
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
    }
}

fn write_manifest_pack_fixture(temp: &tempfile::TempDir, arch: &str) -> std::path::PathBuf {
    let generated_dir = temp.path().join("generated");
    let arch_dir = generated_dir.join("arch");
    std::fs::create_dir_all(&arch_dir).unwrap();

    let pack = sample_pack(arch);
    let bytes = encode_uipack(&pack, arch).unwrap();
    let header = read_uipack_header(&bytes).unwrap();
    let pack_path = arch_dir.join(format!("{arch}.uipack"));
    std::fs::write(&pack_path, bytes).unwrap();

    let manifest = DataPackManifest {
        schema_version: DATAPACK_MANIFEST_SCHEMA_VERSION.to_string(),
        uipack_version: UIPACK_VERSION,
        architectures: BTreeMap::from([(
            arch.to_string(),
            DataPackManifestArchEntry {
                path: format!("arch/{arch}.uipack"),
                size: header.file_len,
                checksum_kind: "fnv1a64".to_string(),
                checksum: format!("{:016x}", header.checksum),
                record_count: header.records_count,
            },
        )]),
    };

    let manifest_path = generated_dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    manifest_path
}

#[test]
fn byte_runtime_keeps_view_and_index_available() {
    let bytes = encode_uipack(&sample_pack("SKL"), "SKL").unwrap();
    let runtime = MappedUiPackRuntime::from_bytes(bytes).unwrap();

    {
        let view = runtime.view().unwrap();
        assert_eq!(view.arch(), "SKL");
        assert_eq!(view.record_count(), 2);
        assert_eq!(view.record(0).unwrap().iform(), "ADD_GPRv_GPRv");
    }

    assert_eq!(
        runtime.index().record_indices_for_mnemonic("add rax, rbx"),
        &[0]
    );
    assert_eq!(
        runtime.index().record_indices_for_iform("imul_gprv_gprv"),
        &[1]
    );

    let view = runtime.view().unwrap();
    let imul_index = runtime.index().record_indices_for_mnemonic("IMUL")[0];
    assert_eq!(view.record(imul_index).unwrap().string(), "IMUL");
}

#[test]
fn record_view_materializes_single_instruction_record() {
    let bytes = encode_uipack(&sample_pack("SKL"), "SKL").unwrap();
    let runtime = MappedUiPackRuntime::from_bytes(bytes).unwrap();
    let view = runtime.view().unwrap();
    let record = record_view_to_instruction_record(view.record(1).unwrap()).unwrap();
    let materialized_pack = view.to_data_pack().unwrap();

    assert_eq!(record, materialized_pack.instructions[1]);
    assert_eq!(record.all_ports, vec!["0".to_string(), "1".to_string()]);
    assert_eq!(record.alu_ports, vec!["0".to_string()]);
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn mmap_runtime_keeps_view_and_index_available() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("SKL.uipack");
    let bytes = encode_uipack(&sample_pack("SKL"), "SKL").unwrap();
    std::fs::write(&path, bytes).unwrap();

    let runtime = MappedUiPackRuntime::open(&path).unwrap();

    assert_eq!(runtime.view().unwrap().arch(), "SKL");
    assert_eq!(runtime.index().record_indices_for_mnemonic("IMUL"), &[1]);
    assert_eq!(
        runtime.view().unwrap().record(1).unwrap().iform(),
        "IMUL_GPRv_GPRv"
    );
}

#[test]
fn manifest_runtime_loads_validated_view_and_index() {
    let temp = tempdir().unwrap();
    let manifest_path = write_manifest_pack_fixture(&temp, "SKL");

    let runtime = load_manifest_runtime(&manifest_path, "skl").unwrap();

    assert_eq!(runtime.view().unwrap().arch(), "SKL");
    assert_eq!(runtime.view().unwrap().record_count(), 2);
    assert_eq!(runtime.index().record_indices_for_mnemonic("ADD"), &[0]);
    assert_eq!(
        runtime.index().record_indices_for_iform("IMUL_GPRv_GPRv"),
        &[1]
    );
}
