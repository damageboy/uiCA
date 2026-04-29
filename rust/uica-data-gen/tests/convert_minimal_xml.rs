use std::path::Path;

use tempfile::tempdir;
use uica_data::{read_uipack_header, UIPACK_VERSION};
use uica_data_gen::{convert_xml_to_pack_dir, DataPackManifest, DATAPACK_MANIFEST_SCHEMA_VERSION};

#[test]
fn converts_minimal_xml_to_manifest_and_per_arch_uipacks() {
    let xml = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/minimal.xml");
    let temp = tempdir().unwrap();
    let out_dir = temp.path().join("generated");

    let manifest = convert_xml_to_pack_dir(Path::new(xml), &out_dir).unwrap();

    assert_eq!(manifest.schema_version, DATAPACK_MANIFEST_SCHEMA_VERSION);
    assert_eq!(manifest.uipack_version, UIPACK_VERSION);
    assert!(out_dir.join("manifest.json").is_file());
    assert!(out_dir.join("arch/SKL.uipack").is_file());
    assert!(out_dir.join("arch/HSW.uipack").is_file());
    assert!(!out_dir.join("instructions.json").exists());
    assert!(!out_dir.join("instructions_full.json").exists());

    let manifest_json = std::fs::read_to_string(out_dir.join("manifest.json")).unwrap();
    let manifest_from_disk: DataPackManifest = serde_json::from_str(&manifest_json).unwrap();
    assert_eq!(manifest_from_disk, manifest);

    let skl_entry = manifest_from_disk.architectures.get("SKL").unwrap();
    assert_eq!(skl_entry.path, "arch/SKL.uipack");
    assert_eq!(skl_entry.checksum_kind, "fnv1a64");
    assert_eq!(skl_entry.record_count, 1);

    let skl_path = out_dir.join(&skl_entry.path);
    let skl_bytes = std::fs::read(&skl_path).unwrap();
    let skl_header = read_uipack_header(&skl_bytes).unwrap();
    assert_eq!(skl_entry.size, skl_bytes.len() as u64);
    assert_eq!(skl_entry.size, skl_header.file_len);
    assert_eq!(skl_entry.checksum, format!("{:016x}", skl_header.checksum));

    let skl_pack = uica_data::load_uipack(&skl_path).unwrap();
    assert_eq!(skl_pack.instructions.len(), 1);
    assert_eq!(skl_pack.instructions[0].arch, "SKL");
    assert_eq!(skl_pack.instructions[0].iform, "ADD_GPRv_GPRv");
    assert_eq!(skl_pack.instructions[0].perf.uops, 1);
    assert_eq!(skl_pack.instructions[0].perf.retire_slots, 1);
    assert_eq!(skl_pack.instructions[0].perf.uops_mite, 1);
    assert_eq!(skl_pack.instructions[0].perf.uops_ms, 0);
    assert_eq!(skl_pack.instructions[0].perf.tp, Some(1.0));
    assert_eq!(skl_pack.instructions[0].perf.ports.get("0156"), Some(&1));
    assert!(!skl_pack.instructions[0].perf.can_be_used_by_lsd);
    assert!(
        skl_pack.instructions[0]
            .perf
            .cannot_be_in_dsb_due_to_jcc_erratum
    );
    assert!(skl_pack.instructions[0].perf.no_micro_fusion);
    assert!(skl_pack.instructions[0].perf.no_macro_fusion);
    assert_eq!(skl_pack.instructions[0].perf.operands.len(), 4);
    assert_eq!(skl_pack.instructions[0].perf.operands[0].name, "REG0");
    assert_eq!(skl_pack.instructions[0].perf.operands[2].name, "MEM0");
    assert_eq!(
        skl_pack.instructions[0].perf.operands[2]
            .mem_operand_role
            .as_deref(),
        Some("read")
    );
    assert_eq!(skl_pack.instructions[0].perf.operands[3].name, "AGEN0");
    assert!(skl_pack.instructions[0].perf.operands[3].is_agen);
    assert_eq!(
        skl_pack.instructions[0].perf.operands[3]
            .mem_operand_role
            .as_deref(),
        Some("agen")
    );
    assert_eq!(skl_pack.instructions[0].perf.latencies.len(), 1);
    assert_eq!(skl_pack.instructions[0].perf.latencies[0].start_op, "REG1");
    assert_eq!(skl_pack.instructions[0].perf.latencies[0].target_op, "REG0");
    assert_eq!(
        skl_pack.instructions[0].perf.latencies[0].cycles_same_reg,
        Some(0)
    );

    let hsw_entry = manifest_from_disk.architectures.get("HSW").unwrap();
    assert_eq!(hsw_entry.path, "arch/HSW.uipack");
    let hsw_pack = uica_data::load_uipack(out_dir.join(&hsw_entry.path)).unwrap();
    assert_eq!(hsw_pack.instructions.len(), 1);
    assert_eq!(hsw_pack.instructions[0].arch, "HSW");
    assert_eq!(hsw_pack.instructions[0].perf.uops, 2);
    assert_eq!(hsw_pack.instructions[0].perf.tp, Some(0.5));
    assert_eq!(hsw_pack.instructions[0].perf.ports.get("01"), Some(&2));
}

#[test]
fn returns_error_for_malformed_xml() {
    let temp = tempdir().unwrap();
    let xml = temp.path().join("broken.xml");
    let out_dir = temp.path().join("generated");
    std::fs::write(&xml, "<root><instruction>").unwrap();

    let result = convert_xml_to_pack_dir(&xml, &out_dir);

    assert!(result.is_err());
}

#[test]
fn accepts_safe_arch_name_with_plus() {
    let temp = tempdir().unwrap();
    let xml = temp.path().join("instructions.xml");
    let out_dir = temp.path().join("generated");
    std::fs::write(
        &xml,
        r#"<root>
  <instruction iform="ADD_GPRv_GPRv" string="ADD" category="BINARY">
    <architecture name="ZEN+">
      <measurement uops="1" TP_unrolled="1.0" ports="1*p0156" />
    </architecture>
  </instruction>
</root>
"#,
    )
    .unwrap();

    let manifest = convert_xml_to_pack_dir(&xml, &out_dir).unwrap();

    let zen_entry = manifest.architectures.get("ZEN+").unwrap();
    assert_eq!(zen_entry.path, "arch/ZEN+.uipack");
    assert!(out_dir.join(&zen_entry.path).is_file());
}

#[test]
fn rejects_unsafe_arch_names() {
    for arch_name in ["../SKL", "", "SKL/HSW", "SKL\\HSW", "SKL\nHSW"] {
        let temp = tempdir().unwrap();
        let xml = temp.path().join("instructions.xml");
        let out_dir = temp.path().join("generated");
        std::fs::write(
            &xml,
            format!(
                "<root>\n  <instruction iform=\"ADD_GPRv_GPRv\" string=\"ADD\" category=\"BINARY\">\n    <architecture name=\"{arch_name}\">\n      <measurement uops=\"1\" TP_unrolled=\"1.0\" ports=\"1*p0156\" />\n    </architecture>\n  </instruction>\n</root>\n"
            ),
        )
        .unwrap();

        let err = convert_xml_to_pack_dir(&xml, &out_dir)
            .unwrap_err()
            .to_string();

        assert!(
            err.contains("unsafe architecture name"),
            "{arch_name}: {err}"
        );
    }
}

#[test]
fn returns_error_for_invalid_tp_value() {
    let temp = tempdir().unwrap();
    let xml = temp.path().join("instructions.xml");
    let out_dir = temp.path().join("generated");
    std::fs::write(
        &xml,
        r#"<root>
  <instruction iform="ADD_GPRv_GPRv" string="ADD" category="BINARY">
    <architecture name="SKL">
      <measurement uops="1" TP_unrolled="bad" ports="1*p0156" />
    </architecture>
  </instruction>
</root>
"#,
    )
    .unwrap();

    let err = convert_xml_to_pack_dir(&xml, &out_dir)
        .unwrap_err()
        .to_string();

    assert!(err.contains("invalid float literal"), "{err}");
}

#[test]
fn accumulates_duplicate_port_keys() {
    let temp = tempdir().unwrap();
    let xml = temp.path().join("instructions.xml");
    let out_dir = temp.path().join("generated");
    std::fs::write(
        &xml,
        r#"<root>
  <instruction iform="ADD_GPRv_GPRv" string="ADD" category="BINARY">
    <architecture name="SKL">
      <measurement uops="1" TP_unrolled="1.0" ports="1*p01+2*p01+3*p23" />
    </architecture>
  </instruction>
</root>
"#,
    )
    .unwrap();

    let manifest = convert_xml_to_pack_dir(&xml, &out_dir).unwrap();
    let pack = uica_data::load_uipack(out_dir.join(&manifest.architectures["SKL"].path)).unwrap();

    assert_eq!(pack.instructions[0].perf.ports.get("01"), Some(&3));
    assert_eq!(pack.instructions[0].perf.ports.get("23"), Some(&3));
}

#[test]
fn writes_empty_manifest_when_xml_has_no_matching_measurement_or_arch() {
    let temp = tempdir().unwrap();
    let xml = temp.path().join("instructions.xml");
    let out_dir = temp.path().join("generated");
    std::fs::write(
        &xml,
        r#"<root>
  <instruction iform="ADD_GPRv_GPRv" string="ADD" category="BINARY">
    <architecture>
      <measurement uops="1" ports="1*p0156" />
    </architecture>
    <architecture name="SKL" />
  </instruction>
</root>
"#,
    )
    .unwrap();

    let manifest = convert_xml_to_pack_dir(&xml, &out_dir).unwrap();

    assert_eq!(manifest.schema_version, DATAPACK_MANIFEST_SCHEMA_VERSION);
    assert!(manifest.architectures.is_empty());
    assert!(out_dir.join("manifest.json").is_file());
    assert!(out_dir.join("arch").is_dir());
    assert!(std::fs::read_dir(out_dir.join("arch"))
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn parses_flag_groups_and_instruction_metadata() {
    let temp = tempdir().unwrap();
    let xml = temp.path().join("instructions.xml");
    let out_dir = temp.path().join("generated");
    std::fs::write(
        &xml,
        r#"<root>
  <instruction iform="ADC_GPRv_GPRv" string="ADC" category="BINARY" mayBeEliminated="true" complexDecoder="1" nAvailableSimpleDecoders="2" lcpStall="true" implicitRSPChange="-8">
    <operand idx="1" name="REG0" type="reg" r="1" w="1" />
    <operand idx="2" name="REG1" type="reg" r="1" />
    <operand idx="3" name="REG2" type="flags" r="1" w="1" implicit="1" flag_CF="r/w" flag_ZF="w" flag_OF="w" />
    <architecture name="SKL">
      <measurement uops="1" uops_retire_slots="1" uops_MITE="1" uops_MS="0" div_cycles="7" TP_unrolled="1.0" ports="1*p06" />
    </architecture>
  </instruction>
</root>
"#,
    )
    .unwrap();

    let manifest = convert_xml_to_pack_dir(&xml, &out_dir).unwrap();
    let pack = uica_data::load_uipack(out_dir.join(&manifest.architectures["SKL"].path)).unwrap();
    let perf = &pack.instructions[0].perf;

    assert_eq!(perf.div_cycles, 7);
    assert!(perf.may_be_eliminated);
    assert!(perf.complex_decoder);
    assert_eq!(perf.n_available_simple_decoders, 2);
    assert!(perf.lcp_stall);
    assert_eq!(perf.implicit_rsp_change, -8);
    assert!(!perf.can_be_used_by_lsd);
    assert!(!perf.cannot_be_in_dsb_due_to_jcc_erratum);
    assert!(!perf.no_micro_fusion);
    assert!(!perf.no_macro_fusion);
    assert_eq!(perf.operands[2].r#type, "flags");
    assert_eq!(
        perf.operands[2].flags,
        vec!["C".to_string(), "SPAZO".to_string()]
    );
    assert_eq!(perf.operands[2].flags_read, vec!["C".to_string()]);
    assert_eq!(
        perf.operands[2].flags_write,
        vec!["C".to_string(), "SPAZO".to_string()]
    );
}
