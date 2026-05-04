use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::sync::{Mutex, OnceLock};

use tempfile::tempdir;
use uica_core::{match_instruction_record_iter, NormalizedInstr};
use uica_data::{
    encode_uipack, load_manifest_pack, read_uipack_header, DataPack, DataPackIndex,
    DataPackManifest, DataPackManifestArchEntry, InstructionRecord, LatencyRecord, OperandRecord,
    PerfRecord, DATAPACK_MANIFEST_SCHEMA_VERSION, DATAPACK_SCHEMA_VERSION, UIPACK_VERSION,
};
use uica_model::Invocation;

fn assert_laminated_uops_populated_once(frontend: &uica_core::sim::FrontEnd) {
    for inst in &frontend.all_generated_instr_instances {
        if inst.macro_fused_with_prev_instr {
            continue;
        }

        assert!(
            !inst.laminated_uops.is_empty(),
            "instr instance {} ({}) missing laminated_uops",
            inst.idx,
            inst.mnemonic
        );

        let mut per_instr_seen = BTreeSet::new();
        for &lam_idx in &inst.laminated_uops {
            assert!(
                per_instr_seen.insert(lam_idx),
                "instr instance {} ({}) repeats lam id {lam_idx}",
                inst.idx,
                inst.mnemonic
            );
            let lam = frontend
                .uop_storage
                .get_laminated_uop(lam_idx)
                .unwrap_or_else(|| panic!("missing laminated uop storage entry {lam_idx}"));
            assert_eq!(
                lam.instr_instance_idx, inst.idx,
                "lam id {lam_idx} points at wrong instr instance"
            );
        }

        let storage_lams: BTreeSet<u64> = frontend
            .uop_storage
            .laminated_uops
            .values()
            .filter(|lam| lam.instr_instance_idx == inst.idx)
            .map(|lam| lam.idx)
            .collect();
        assert_eq!(
            storage_lams, per_instr_seen,
            "instr instance {} ({}) has late or duplicate storage expansion",
            inst.idx, inst.mnemonic
        );
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvVarGuard {
    key: &'static str,
    old_value: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &std::path::Path) -> Self {
        let old_value = env::var_os(key);
        env::set_var(key, value);
        Self { key, old_value }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.old_value {
            Some(value) => env::set_var(self.key, value),
            None => env::remove_var(self.key),
        }
    }
}

fn sample_pack(
    arch: &str,
    mnemonic: &str,
    string: &str,
    uops: i32,
    ports: &[(&str, i32)],
) -> DataPack {
    let iform = match mnemonic {
        "ADD" => "ADD_GPRv_GPRv_01".to_string(),
        _ => format!("{mnemonic}_GPRv_GPRv"),
    };

    DataPack {
        schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
        all_ports: Default::default(),
        alu_ports: Default::default(),
        instructions: vec![InstructionRecord {
            arch: arch.to_string(),
            iform,
            string: string.to_string(),
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
                latencies: vec![LatencyRecord {
                    start_op: "REG1".to_string(),
                    target_op: "REG0".to_string(),
                    cycles: 1,
                    cycles_addr: None,
                    cycles_addr_index: None,
                    cycles_mem: None,
                    cycles_same_reg: None,
                }],
                uops,
                retire_slots: 1,
                uops_mite: uops,
                uops_ms: 0,
                tp: Some(1.0),
                ports: ports
                    .iter()
                    .map(|(name, count)| (name.to_string(), *count))
                    .collect(),
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

fn write_manifest_fixture(
    temp: &tempfile::TempDir,
    packs: &[(&str, DataPack)],
) -> std::path::PathBuf {
    let generated_dir = temp.path().join("generated");
    let arch_dir = generated_dir.join("arch");
    std::fs::create_dir_all(&arch_dir).unwrap();

    let mut architectures = BTreeMap::new();
    for (arch, pack) in packs {
        let bytes = encode_uipack(pack, arch).unwrap();
        let header = read_uipack_header(&bytes).unwrap();
        let relative_path = format!("arch/{arch}.uipack");
        std::fs::write(generated_dir.join(&relative_path), bytes).unwrap();
        architectures.insert(
            arch.to_string(),
            DataPackManifestArchEntry {
                path: relative_path,
                size: header.file_len,
                checksum_kind: "fnv1a64".to_string(),
                checksum: format!("{:016x}", header.checksum),
                record_count: header.records_count,
            },
        );
    }

    let manifest = DataPackManifest {
        schema_version: DATAPACK_MANIFEST_SCHEMA_VERSION.to_string(),
        uipack_version: UIPACK_VERSION,
        architectures,
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
fn dsb_multi_slot_instruction_expands_real_laminated_uops_once() {
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let pack = load_manifest_pack(&manifest_path, "SKL").unwrap();
    let index = DataPackIndex::new(&pack);
    let arch = uica_core::get_micro_arch("SKL").unwrap();

    // shl rax, cl; dec r15; jnz -8
    // SHL-by-CL is a real multi-uop DSB instruction on SKL. DEC/JNZ keeps loop
    // mode and macro-fuses, so branch instance may legitimately have no lam IDs.
    let decoded =
        uica_decoder::decode_raw(&[0x48, 0xd3, 0xe0, 0x49, 0xff, 0xcf, 0x75, 0xf8]).unwrap();
    let base_instances = uica_core::sim::types::build_instruction_instances(&decoded, 0);

    let mut frontend = uica_core::sim::FrontEnd::new(arch, false, base_instances, 0, &pack, &index);

    assert_eq!(frontend.uop_source.as_deref(), Some("DSB"));
    assert_laminated_uops_populated_once(&frontend);

    let multi_slot = frontend
        .all_generated_instr_instances
        .iter()
        .find(|inst| inst.mnemonic.as_ref() == "shl" && inst.laminated_uops.len() > 1)
        .expect("SHL-by-CL should occupy multiple DSB slots");
    let multi_slot_idx = multi_slot.idx;
    let multi_slot_lams = multi_slot.laminated_uops.clone();

    for clock in 0..3 {
        frontend.cycle(clock);
    }

    assert_laminated_uops_populated_once(&frontend);
    let after = frontend
        .all_generated_instr_instances
        .iter()
        .find(|inst| inst.idx == multi_slot_idx)
        .unwrap();
    assert_eq!(after.laminated_uops, multi_slot_lams);
    assert!(multi_slot_lams.iter().all(|lam_idx| {
        frontend
            .uop_storage
            .get_laminated_uop(*lam_idx)
            .is_some_and(|lam| lam.added_to_idq.is_some())
    }));
}

#[test]
fn stack_pointer_changes_disable_lsd_like_python_getinstructions() {
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let pack = load_manifest_pack(&manifest_path, "HSW").unwrap();
    let index = DataPackIndex::new(&pack);
    let arch = uica_core::get_micro_arch("HSW").unwrap();

    // push rax; pop rbx; add rbx, rcx; dec rdx; jnz back
    // Python `getInstructions()` derives `implicitRSPChange` from XED STACKPUSH/
    // STACKPOP operands, so PUSH/POP cannot use LSD even if pack rows lack
    // XML-derived implicit_rsp_change metadata.
    let decoded =
        uica_decoder::decode_raw(&[0x50, 0x5b, 0x48, 0x01, 0xcb, 0x48, 0xff, 0xca, 0x75, 0xf6])
            .unwrap();
    let base_instances = uica_core::sim::types::build_instruction_instances(&decoded, 0);

    assert_eq!(base_instances[0].implicit_rsp_change, -8);
    assert_eq!(base_instances[1].implicit_rsp_change, 8);
    assert_eq!(base_instances[0].mem_addrs[0].disp, 0);
    assert!(base_instances[0].mem_addrs[0].is_implicit_stack_operand);

    let ret_imm = uica_core::sim::types::build_instruction_instances(
        &uica_decoder::decode_raw(&[0xc2, 0x10, 0x00]).unwrap(),
        0,
    );
    assert_eq!(ret_imm[0].implicit_rsp_change, 8);

    let enter = uica_core::sim::types::build_instruction_instances(
        &uica_decoder::decode_raw(&[0xc8, 0x10, 0x00, 0x00]).unwrap(),
        0,
    );
    assert_eq!(enter[0].implicit_rsp_change, -8);
    assert_eq!(enter[0].input_regs.as_ref(), ["RBP".to_string()].as_slice());
    assert_eq!(
        enter[0].output_regs.as_ref(),
        ["RBP".to_string()].as_slice()
    );

    let mut storage = uica_core::sim::uop_storage::UopStorage::new();
    let mut uop_idx = 0;
    let mut fused_idx = 0;
    let mut lam_idx = 0;
    uica_core::sim::uop_expand::expand_instr_instance_to_lam_uops_with_storage(
        &enter[0],
        &mut uop_idx,
        &mut fused_idx,
        &mut lam_idx,
        &mut storage,
        "HSW",
        &pack,
        None,
    )
    .unwrap();
    assert!(storage.uops.values().any(|uop| {
        uop.prop
            .input_operands
            .contains(&uica_core::sim::types::OperandKey::Reg("RBP".to_string()))
            || uop
                .prop
                .output_operands
                .contains(&uica_core::sim::types::OperandKey::Reg("RBP".to_string()))
    }));

    let push_rsp = uica_core::sim::types::build_instruction_instances(
        &uica_decoder::decode_raw(&[0x54]).unwrap(),
        0,
    );
    assert_eq!(
        push_rsp[0].input_regs.as_ref(),
        ["RSP".to_string()].as_slice()
    );
    assert!(push_rsp[0].output_regs.is_empty());

    let pop_rsp = uica_core::sim::types::build_instruction_instances(
        &uica_decoder::decode_raw(&[0x5c]).unwrap(),
        0,
    );
    assert!(pop_rsp[0].input_regs.is_empty());
    assert_eq!(
        pop_rsp[0].output_regs.as_ref(),
        ["RSP".to_string()].as_slice()
    );

    let pushf = uica_core::sim::types::build_instruction_instances(
        &uica_decoder::decode_raw(&[0x9c]).unwrap(),
        0,
    );
    assert_eq!(pushf[0].implicit_rsp_change, -8);
    assert!(pushf[0].mem_addrs[0].is_implicit_stack_operand);

    for instrs in [
        push_rsp,
        ret_imm,
        uica_core::sim::types::build_instruction_instances(
            &uica_decoder::decode_raw(&[0xff, 0xd0]).unwrap(),
            0,
        ),
    ] {
        let mut storage = uica_core::sim::uop_storage::UopStorage::new();
        let mut uop_idx = 0;
        let mut fused_idx = 0;
        let mut lam_idx = 0;
        uica_core::sim::uop_expand::expand_instr_instance_to_lam_uops_with_storage(
            &instrs[0],
            &mut uop_idx,
            &mut fused_idx,
            &mut lam_idx,
            &mut storage,
            "HSW",
            &pack,
            None,
        )
        .unwrap();
        assert!(storage.uops.values().all(|uop| {
            uop.prop.input_operands.iter().all(|op| {
                !matches!(op, uica_core::sim::types::OperandKey::Reg(reg) if reg.starts_with("REG"))
            }) && uop.prop.output_operands.iter().all(|op| {
                !matches!(op, uica_core::sim::types::OperandKey::Reg(reg) if reg.starts_with("REG"))
            })
        }));
    }

    let frontend = uica_core::sim::FrontEnd::new(arch, false, base_instances, 0, &pack, &index);

    assert_eq!(frontend.uop_source.as_deref(), Some("DSB"));
    assert!(frontend
        .all_generated_instr_instances
        .iter()
        .take(2)
        .all(|inst| !inst.can_be_used_by_lsd));
}

#[test]
fn binary_manifest_pack_supports_index_matcher_and_perf_lookup() {
    let temp = tempdir().unwrap();
    let manifest_path = write_manifest_fixture(
        &temp,
        &[(
            "SKL",
            sample_pack("SKL", "MOV", "MOVE", 2, &[("23", 2), ("5", 1)]),
        )],
    );

    let pack = load_manifest_pack(&manifest_path, "SKL").unwrap();
    let index = DataPackIndex::new(&pack);
    let candidates = index.candidates_for("skl", "move rax, rbx");
    let matched = match_instruction_record_iter(
        NormalizedInstr {
            max_op_size_bytes: 0,
            immediate: None,
            iform_signature: String::new(),
            mnemonic: "mov".to_string(),
            decoded_iform: String::new(),
            uses_high8_reg: false,
            explicit_reg_operands: Vec::new(),
            xml_attrs: Default::default(),
            agen: None,
        }
        .as_ref(),
        candidates,
    )
    .expect("iform fallback should match binary pack record");

    assert_eq!(matched.iform, "MOV_GPRv_GPRv");
    assert_eq!(matched.perf.uops, 2);
    assert_eq!(matched.perf.ports.get("23"), Some(&2));
    assert_eq!(matched.perf.ports.get("5"), Some(&1));
}

#[test]
fn engine_prefers_manifest_selected_arch_pack_from_env_dir() {
    let _lock = env_lock().lock().unwrap();
    let original = env::var_os("UICA_RUST_DATAPACK");
    let temp = tempdir().unwrap();
    let generated_dir = temp.path().join("generated");
    let _manifest_path = write_manifest_fixture(
        &temp,
        &[
            ("SKL", sample_pack("SKL", "ADD", "ADD", 1, &[("0", 2)])),
            ("HSW", sample_pack("HSW", "SUB", "SUB", 1, &[("1", 1)])),
        ],
    );

    {
        let _env = EnvVarGuard::set("UICA_RUST_DATAPACK", &generated_dir);
        let result = uica_core::engine::engine(
            &[0x48, 0x01, 0xd8],
            &Invocation {
                arch: "SKL".to_string(),
                ..Invocation::default()
            },
        );

        assert_eq!(result.invocation.arch, "SKL");
        assert_eq!(result.summary.throughput_cycles_per_iteration, Some(2.0));
        assert_eq!(result.summary.limits.get("ports"), Some(&Some(2.0)));
        assert_eq!(result.parameters["uArchName"], "SKL");
    }

    assert_eq!(env::var_os("UICA_RUST_DATAPACK"), original);
}

#[test]
fn engine_loads_manifest_file_from_env_path() {
    let _lock = env_lock().lock().unwrap();
    let original = env::var_os("UICA_RUST_DATAPACK");
    let temp = tempdir().unwrap();
    let manifest_path = write_manifest_fixture(
        &temp,
        &[
            ("SKL", sample_pack("SKL", "ADD", "ADD", 1, &[("0", 2)])),
            ("HSW", sample_pack("HSW", "SUB", "SUB", 1, &[("1", 1)])),
        ],
    );

    {
        let _env = EnvVarGuard::set("UICA_RUST_DATAPACK", &manifest_path);
        let result = uica_core::engine::engine(
            &[0x48, 0x01, 0xd8],
            &Invocation {
                arch: "SKL".to_string(),
                ..Invocation::default()
            },
        );

        assert_eq!(result.invocation.arch, "SKL");
        assert_eq!(result.summary.throughput_cycles_per_iteration, Some(2.0));
        assert_eq!(result.summary.limits.get("ports"), Some(&Some(2.0)));
        assert_eq!(result.parameters["uArchName"], "SKL");
    }

    assert_eq!(env::var_os("UICA_RUST_DATAPACK"), original);
}

#[test]
fn engine_trace_uses_manifest_uipack_from_env_path() {
    let _lock = env_lock().lock().unwrap();
    let original = env::var_os("UICA_RUST_DATAPACK");
    let temp = tempdir().unwrap();
    let manifest_path = write_manifest_fixture(
        &temp,
        &[("SKL", sample_pack("SKL", "ADD", "ADD", 1, &[("0", 1)]))],
    );

    {
        let _env = EnvVarGuard::set("UICA_RUST_DATAPACK", &manifest_path);
        let trace = uica_core::engine::engine_trace(
            &[0x48, 0x01, 0xd8],
            &Invocation {
                arch: "SKL".to_string(),
                min_cycles: 8,
                min_iterations: 1,
                ..Invocation::default()
            },
        )
        .unwrap();
        let trace_path = temp.path().join("events.trace");
        trace.finish_to_path(&trace_path).unwrap();
        assert!(trace_path.is_file());
    }

    assert_eq!(env::var_os("UICA_RUST_DATAPACK"), original);
}

#[test]
fn event_trace_supports_vcmpeqps_with_decoded_operands() {
    let _lock = env_lock().lock().unwrap();
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let _env = EnvVarGuard::set("UICA_RUST_DATAPACK", &manifest_path);

    // vcmpps ymm0, ymm1, ymm2, 0; vblendvps ymm3, ymm4, ymm5, ymm0; dec rcx; jnz -16
    let result = uica_core::engine::engine(
        &[
            0xc5, 0xf4, 0xc2, 0xc2, 0x00, 0xc4, 0xe3, 0x5d, 0x4a, 0xdd, 0x00, 0x48, 0xff, 0xc9,
            0x75, 0xf0,
        ],
        &Invocation {
            arch: "HSW".to_string(),
            min_cycles: 8,
            min_iterations: 1,
            ..Invocation::default()
        },
    );

    assert!(
        !result.cycles.is_empty(),
        "decoded SIMD compare/blend loop should run simulator, not analytical fallback"
    );
}

#[test]
fn event_trace_emits_executed_events_for_zero_port_uops() {
    let _lock = env_lock().lock().unwrap();
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let temp = tempdir().unwrap();
    let trace_path = temp.path().join("events.trace");

    let _env = EnvVarGuard::set("UICA_RUST_DATAPACK", &manifest_path);
    // xor rax, rax; dec rcx; jnz -6
    let trace = uica_core::engine::engine_trace(
        &[0x48, 0x31, 0xc0, 0x48, 0xff, 0xc9, 0x75, 0xf6],
        &Invocation {
            arch: "HSW".to_string(),
            ..Invocation::default()
        },
    )
    .unwrap();
    trace.finish_to_path(&trace_path).unwrap();

    let trace_text = std::fs::read_to_string(trace_path).unwrap();
    assert!(
        trace_text.lines().any(|line| line.contains(" E instr=0 ")),
        "zero-port XOR uop should still emit Python E events:\n{trace_text}"
    );
}

#[test]
fn engine_loads_single_uipack_file_from_env_path() {
    let _lock = env_lock().lock().unwrap();
    let original = env::var_os("UICA_RUST_DATAPACK");
    let temp = tempdir().unwrap();
    let pack_path = temp.path().join("SKL.uipack");
    let pack = sample_pack("SKL", "ADD", "ADD", 1, &[("0", 2)]);
    std::fs::write(&pack_path, encode_uipack(&pack, "SKL").unwrap()).unwrap();

    {
        let _env = EnvVarGuard::set("UICA_RUST_DATAPACK", &pack_path);
        let result = uica_core::engine::engine(
            &[0x48, 0x01, 0xd8],
            &Invocation {
                arch: "SKL".to_string(),
                ..Invocation::default()
            },
        );

        assert_eq!(result.invocation.arch, "SKL");
        assert_eq!(result.summary.throughput_cycles_per_iteration, Some(2.0));
        assert_eq!(result.summary.limits.get("ports"), Some(&Some(2.0)));
        assert_eq!(result.parameters["uArchName"], "SKL");
    }

    assert_eq!(env::var_os("UICA_RUST_DATAPACK"), original);
}
