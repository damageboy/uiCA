use std::collections::BTreeMap;

use tempfile::tempdir;
use uica_data::{
    encode_uipack, load_manifest, load_manifest_pack, read_uipack_header,
    resolve_manifest_pack_path, DataPack, DataPackManifest, DataPackManifestArchEntry,
    InstructionRecord, PerfRecord, DATAPACK_MANIFEST_SCHEMA_VERSION, DATAPACK_SCHEMA_VERSION,
    UIPACK_VERSION,
};

fn sample_pack(arch: &str) -> DataPack {
    DataPack {
        schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
        instructions: vec![InstructionRecord {
            arch: arch.to_string(),
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
fn resolves_manifest_arch_pack_and_loads_binary_pack() {
    let temp = tempdir().unwrap();
    let manifest_path = write_manifest_pack_fixture(&temp, "SKL");

    let manifest = load_manifest(&manifest_path).unwrap();
    assert_eq!(manifest.schema_version, DATAPACK_MANIFEST_SCHEMA_VERSION);
    assert_eq!(manifest.uipack_version, UIPACK_VERSION);

    let pack_path = resolve_manifest_pack_path(&manifest_path, "skl").unwrap();
    assert!(pack_path.ends_with("arch/SKL.uipack"));

    let pack = load_manifest_pack(&manifest_path, "SKL").unwrap();
    assert_eq!(pack, sample_pack("SKL"));
}

#[test]
fn rejects_manifest_pack_header_mismatches() {
    let temp = tempdir().unwrap();
    let manifest_path = write_manifest_pack_fixture(&temp, "SKL");

    let mut manifest: DataPackManifest =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest.uipack_version = UIPACK_VERSION + 1;
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let err = load_manifest_pack(&manifest_path, "SKL")
        .unwrap_err()
        .to_string();
    assert!(err.contains("manifest uipack version mismatch"), "{err}");

    manifest.uipack_version = UIPACK_VERSION;
    manifest.architectures.get_mut("SKL").unwrap().size -= 1;
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let err = load_manifest_pack(&manifest_path, "SKL")
        .unwrap_err()
        .to_string();
    assert!(err.contains("manifest pack size mismatch"), "{err}");

    manifest.architectures.get_mut("SKL").unwrap().size += 1;
    manifest.architectures.get_mut("SKL").unwrap().checksum_kind = "sha256".to_string();
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let err = load_manifest_pack(&manifest_path, "SKL")
        .unwrap_err()
        .to_string();
    assert!(err.contains("manifest checksum kind mismatch"), "{err}");

    manifest.architectures.get_mut("SKL").unwrap().checksum_kind = "fnv1a64".to_string();
    manifest.architectures.get_mut("SKL").unwrap().record_count += 1;
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let err = load_manifest_pack(&manifest_path, "SKL")
        .unwrap_err()
        .to_string();
    assert!(err.contains("manifest record count mismatch"), "{err}");

    manifest.architectures.get_mut("SKL").unwrap().record_count -= 1;
    manifest.architectures.get_mut("SKL").unwrap().checksum = "0000000000000000".to_string();
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let err = load_manifest_pack(&manifest_path, "SKL")
        .unwrap_err()
        .to_string();
    assert!(err.contains("manifest checksum mismatch"), "{err}");
}
