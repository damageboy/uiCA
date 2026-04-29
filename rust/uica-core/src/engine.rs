use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use serde_json::json;
use uica_data::{
    load_manifest_pack, load_uipack, DataPack, DataPackIndex, DATAPACK_MANIFEST_FILE_NAME,
};
use uica_decoder::decode_raw;
use uica_model::{Invocation, Summary, UicaResult};

use crate::analytical::{
    compute_final_prediction, compute_frontend_limits, compute_issue_limit,
    compute_maximum_latency_for_graph, compute_port_usage_limit, generate_latency_graph,
    AnalyticalInstruction, AnalyticalLatencyInstruction, InstructionPortUsage,
};
use crate::matcher::{match_instruction_record, NormalizedInstr};
use crate::micro_arch::{get_micro_arch, MicroArchConfig};

pub fn engine(code: &[u8], invocation: &Invocation) -> UicaResult {
    if let Some(pack) = load_default_pack(&invocation.arch) {
        return engine_with_pack(code, invocation, &pack);
    }

    fallback_result(code, invocation)
}

pub fn engine_with_pack(code: &[u8], invocation: &Invocation, pack: &DataPack) -> UicaResult {
    let normalized_invocation = Invocation {
        arch: invocation.arch.to_ascii_uppercase(),
        ..invocation.clone()
    };
    let mut result = UicaResult {
        uica_commit: env::var("UICA_COMMIT").unwrap_or_else(|_| "unknown".to_string()),
        invocation: normalized_invocation.clone(),
        ..UicaResult::default()
    };

    let arch = match get_micro_arch(&normalized_invocation.arch) {
        Some(arch) => arch,
        None => return fallback_result(code, &normalized_invocation),
    };

    let decoded = match decode_raw(code) {
        Ok(decoded) => decoded,
        Err(_) => return fallback_result(code, &normalized_invocation),
    };

    let index = DataPackIndex::new(pack.clone());

    let mut total_uops = 0;
    let mut port_usage_inputs = Vec::new();
    let mut loop_facts = Vec::with_capacity(decoded.len());

    for decoded_instr in &decoded {
        let norm = NormalizedInstr {
            max_op_size_bytes: 0,
            mnemonic: decoded_instr.mnemonic.clone(),
            iform_signature: decoded_instr.iform_signature.clone(),
        };

        let mut fact = LoopInstrFacts {
            mnemonic: decoded_instr.mnemonic.to_ascii_lowercase(),
            size: decoded_instr.len,
            uops: 1,
            uops_mite: 1,
            uops_ms: 0,
            port_data: BTreeMap::new(),
            complex_decoder: false,
            n_available_simple_decoders: arch.n_decoders.saturating_sub(1),
            can_be_used_by_lsd: true,
            no_macro_fusion: false,
            input_operands: decoded_latency_inputs(decoded_instr),
            output_operands: decoded_latency_outputs(decoded_instr),
            latencies: BTreeMap::new(),
            may_be_eliminated: false,
            matched: false,
        };

        let candidates = index.candidates_for(&result.invocation.arch, &decoded_instr.mnemonic);
        if let Some(record) = match_instruction_record(&norm, candidates) {
            let uops = record.perf.uops.max(0);
            total_uops += uops;
            port_usage_inputs.push(InstructionPortUsage {
                port_data: record.perf.ports.clone(),
                uops,
            });
            fact.uops = uops;
            fact.uops_mite = if record.perf.uops_mite > 0 {
                record.perf.uops_mite as u32
            } else {
                record.perf.uops.max(0) as u32
            };
            fact.uops_ms = record.perf.uops_ms.max(0) as u32;
            fact.port_data = record.perf.ports.clone();
            fact.complex_decoder = record.perf.complex_decoder;
            fact.n_available_simple_decoders = record.perf.n_available_simple_decoders;
            if result.invocation.no_micro_fusion {
                let retire_slots = (record.perf.uops.max(0) as u32)
                    .max(fact.uops_mite + fact.uops_ms)
                    .max(1);
                fact.uops_mite = retire_slots.saturating_sub(fact.uops_ms);
                if fact.uops_mite > 4 {
                    fact.uops_ms += fact.uops_mite - 4;
                    fact.uops_mite = 4;
                }
                if fact.uops_mite > 1 {
                    fact.complex_decoder = true;
                    let ms_limit = if fact.uops_ms > 0 { 0 } else { 3 };
                    fact.n_available_simple_decoders = fact
                        .n_available_simple_decoders
                        .min(5u32.saturating_sub(fact.uops_mite))
                        .min(ms_limit);
                }
            }
            fact.can_be_used_by_lsd = record.perf.can_be_used_by_lsd
                && fact.uops_ms == 0
                && record.perf.implicit_rsp_change == 0;
            fact.no_macro_fusion = record.perf.no_macro_fusion || result.invocation.no_macro_fusion;
            fact.may_be_eliminated = record.perf.may_be_eliminated;
            let (input_map, output_map) = mapped_record_operands(record, decoded_instr);
            fact.input_operands = flatten_operand_map(&input_map);
            fact.output_operands = flatten_operand_map(&output_map);
            fact.latencies = map_record_latencies_to_decoded(record, decoded_instr);
            fact.matched = true;
        }

        loop_facts.push(fact);
    }

    let issue_limit = round2(compute_issue_limit(total_uops, arch.issue_width as i32));
    let ports_limit = round2(compute_port_usage_limit(&port_usage_inputs));

    let mnemonic_seq: Vec<&str> = decoded
        .iter()
        .map(|instr| instr.mnemonic.as_str())
        .collect();
    let signature = canonical_signature(&mnemonic_seq);
    let mode = if mnemonic_seq.iter().any(|m| m.starts_with('j')) {
        "loop"
    } else {
        "unroll"
    };

    result.summary.mode = mode.to_string();

    let mut limits = empty_limits();
    limits.insert("issue".to_string(), Some(issue_limit));
    limits.insert("ports".to_string(), Some(ports_limit));

    let mut throughput = issue_limit.max(ports_limit);
    if !throughput.is_finite() || throughput <= 0.0 {
        throughput = 1.0;
    }

    result.summary.cycles_simulated = result.invocation.min_cycles + 1;
    let mut iterations = estimate_iterations(
        result.summary.cycles_simulated,
        throughput,
        result.invocation.min_iterations,
    );
    let mut bottlenecks = predicted_bottlenecks(&limits, throughput);

    if mode == "loop" {
        let model_confident = is_high_confidence_loop_model(&loop_facts);

        if model_confident {
            if let Some(loop_model) = compute_loop_model(
                &arch,
                &loop_facts,
                &signature,
                result.summary.cycles_simulated,
                result.invocation.min_iterations,
            ) {
                throughput = loop_model.throughput;
                iterations = loop_model.iterations;
                limits = loop_model.limits;
                bottlenecks = loop_model.bottlenecks;
            }
        }
    }

    result.summary.throughput_cycles_per_iteration = Some(throughput);
    result.summary.limits = limits;
    result.summary.iterations_simulated = iterations;
    result.summary.bottlenecks_predicted = bottlenecks;

    // Run simulator to generate cycles JSON. Keep summary fields on the
    // analytical path: the simulator is authoritative for cycle event JSON
    // only, not for iterations_simulated / cycles_simulated. Summary stays
    // matched to the existing tests and Python oracle summary output.
    let mut lsd_active = false;
    let mut lsd_unroll_count = 1u32;
    if let Ok((frontend, final_clock)) =
        run_simulation_for_cycles(code, &normalized_invocation, pack)
    {
        lsd_active = frontend.uop_source.as_deref() == Some("LSD");
        lsd_unroll_count = frontend.lsd_unroll_count;
        result.cycles = build_cycles_json(&frontend, final_clock);
        result.instructions = build_instructions_json(&frontend);
    } else {
        result.cycles = Vec::new();
        result.instructions = build_instructions_json_from_decode(code);
    }

    let all_ports: Vec<&str> = crate::micro_arch::all_ports(arch.name).to_vec();
    result.parameters = json!({
        "uArchName": arch.name,
        "IQWidth": arch.iq_width,
        "IDQWidth": arch.idq_width,
        "issueWidth": arch.issue_width,
        "RBWidth": arch.rb_width,
        "RSWidth": arch.rs_width,
        "allPorts": all_ports,
        "nDecoders": arch.n_decoders,
        "DSBBlockSize": arch.dsb_block_size,
        "LSD": lsd_active,
        "LSDUnrollCount": lsd_unroll_count,
        "mode": result.summary.mode,
    });

    result
}

#[derive(Clone, Debug)]
struct LoopInstrFacts {
    mnemonic: String,
    size: u32,
    uops: i32,
    uops_mite: u32,
    uops_ms: u32,
    port_data: BTreeMap<String, i32>,
    complex_decoder: bool,
    n_available_simple_decoders: u32,
    can_be_used_by_lsd: bool,
    no_macro_fusion: bool,
    input_operands: Vec<String>,
    output_operands: Vec<String>,
    latencies: BTreeMap<(String, String), i32>,
    may_be_eliminated: bool,
    matched: bool,
}

#[derive(Clone, Debug)]
struct LoopModel {
    limits: BTreeMap<String, Option<f64>>,
    throughput: f64,
    iterations: u32,
    bottlenecks: Vec<String>,
}

fn empty_limits() -> BTreeMap<String, Option<f64>> {
    BTreeMap::from([
        ("predecoder".to_string(), None),
        ("decoder".to_string(), None),
        ("dsb".to_string(), None),
        ("lsd".to_string(), None),
        ("issue".to_string(), None),
        ("ports".to_string(), None),
        ("dependencies".to_string(), None),
    ])
}

fn is_high_confidence_loop_model(facts: &[LoopInstrFacts]) -> bool {
    facts.iter().all(|fact| fact.matched)
}

fn compute_loop_model(
    arch: &MicroArchConfig,
    facts: &[LoopInstrFacts],
    _signature: &str,
    cycles: u32,
    min_iterations: u32,
) -> Option<LoopModel> {
    if facts.is_empty() {
        return None;
    }

    let frontend_instrs = facts_to_analytical_instructions(facts);
    let fused_pairs = frontend_instrs
        .iter()
        .filter(|instr| instr.macro_fused_with_prev)
        .count();
    let fused_uops = fused_issue_uops(facts, fused_pairs);
    let issue_limit = round2(compute_issue_limit(fused_uops, arch.issue_width as i32));
    let ports_limit = round2(compute_port_usage_limit(&facts_to_port_usage_inputs(
        facts,
        &frontend_instrs,
    )));

    let frontend_limits = compute_frontend_limits(&frontend_instrs, arch, 0);

    let latency_instrs = facts_to_latency_instructions(facts);
    let latency_graph = generate_latency_graph(&latency_instrs);
    let max_latency = compute_maximum_latency_for_graph(&latency_graph).max_cycle_ratio;
    let dependencies = if max_latency > 0.0 {
        Some(round2(max_latency))
    } else {
        None
    };

    let mut limits = empty_limits();
    limits.insert("issue".to_string(), Some(issue_limit));
    limits.insert("ports".to_string(), Some(ports_limit));
    if let Some(decoder) = frontend_limits.decoder {
        limits.insert("decoder".to_string(), Some(round2(decoder)));
    }
    if let Some(dsb) = frontend_limits.dsb {
        limits.insert("dsb".to_string(), Some(round2(dsb)));
    }
    if let Some(lsd) = frontend_limits.lsd {
        limits.insert("lsd".to_string(), Some(round2(lsd)));
    }
    if let Some(dep) = dependencies {
        limits.insert("dependencies".to_string(), Some(dep));
    }

    let prediction = compute_final_prediction(&limits);
    let throughput = if prediction.throughput.is_finite() && prediction.throughput > 0.0 {
        round2(prediction.throughput)
    } else {
        1.0
    };
    let iterations = estimate_iterations(cycles, throughput, min_iterations);

    Some(LoopModel {
        limits,
        throughput,
        iterations,
        bottlenecks: prediction.bottlenecks,
    })
}

fn mem_key(decoded: &uica_decoder::DecodedInstruction) -> String {
    if let Some(mem) = decoded.mem_addrs.first() {
        format!(
            "MEM:{}:{}:{}:{}",
            mem.base
                .as_deref()
                .map(crate::x64::get_canonical_reg)
                .unwrap_or_default(),
            mem.index
                .as_deref()
                .map(crate::x64::get_canonical_reg)
                .unwrap_or_default(),
            mem.scale,
            mem.disp
        )
    } else {
        "MEM".to_string()
    }
}

fn decoded_latency_inputs(decoded: &uica_decoder::DecodedInstruction) -> Vec<String> {
    let mut inputs: Vec<String> = decoded
        .input_regs
        .iter()
        .map(|reg| crate::x64::get_canonical_reg(reg))
        .collect();
    if decoded.reads_flags {
        inputs.extend(["C".to_string(), "SPAZO".to_string()]);
    }
    if decoded.has_memory_read {
        inputs.push(mem_key(decoded));
    }
    for mem in &decoded.mem_addrs {
        if let Some(base) = &mem.base {
            inputs.push(crate::x64::get_canonical_reg(base));
        }
        if let Some(index) = &mem.index {
            inputs.push(crate::x64::get_canonical_reg(index));
        }
    }
    inputs.sort();
    inputs.dedup();
    inputs
}

fn decoded_latency_outputs(decoded: &uica_decoder::DecodedInstruction) -> Vec<String> {
    let mut outputs: Vec<String> = decoded
        .output_regs
        .iter()
        .map(|reg| crate::x64::get_canonical_reg(reg))
        .collect();
    if decoded.writes_flags {
        outputs.extend(["C".to_string(), "SPAZO".to_string()]);
    }
    if decoded.has_memory_write {
        outputs.push(mem_key(decoded));
    }
    outputs.sort();
    outputs.dedup();
    outputs
}

fn mapped_record_operands(
    record: &uica_data::InstructionRecord,
    decoded: &uica_decoder::DecodedInstruction,
) -> (BTreeMap<String, Vec<String>>, BTreeMap<String, Vec<String>>) {
    let input_regs: Vec<String> = decoded
        .input_regs
        .iter()
        .map(|reg| crate::x64::get_canonical_reg(reg))
        .collect();
    let output_regs: Vec<String> = decoded
        .output_regs
        .iter()
        .map(|reg| crate::x64::get_canonical_reg(reg))
        .collect();
    let mem = mem_key(decoded);
    let mut read_reg_idx = 0usize;
    let mut write_reg_idx = 0usize;
    let mut input_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut output_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for operand in &record.perf.operands {
        match operand.r#type.as_str() {
            "reg" => {
                if operand.read {
                    if let Some(reg) = input_regs.get(read_reg_idx) {
                        input_map
                            .entry(operand.name.clone())
                            .or_default()
                            .push(reg.clone());
                    }
                    read_reg_idx += 1;
                }
                if operand.write {
                    if let Some(reg) = output_regs.get(write_reg_idx) {
                        output_map
                            .entry(operand.name.clone())
                            .or_default()
                            .push(reg.clone());
                    }
                    write_reg_idx += 1;
                }
            }
            "flags" => {
                let read_flags = if operand.flags_read.is_empty() {
                    operand.flags.clone()
                } else {
                    operand.flags_read.clone()
                };
                let write_flags = if operand.flags_write.is_empty() {
                    operand.flags.clone()
                } else {
                    operand.flags_write.clone()
                };
                if operand.read || !read_flags.is_empty() {
                    input_map
                        .entry(operand.name.clone())
                        .or_default()
                        .extend(read_flags.into_iter().filter(|f| !f.is_empty()));
                }
                if operand.write || !write_flags.is_empty() {
                    output_map
                        .entry(operand.name.clone())
                        .or_default()
                        .extend(write_flags.into_iter().filter(|f| !f.is_empty()));
                }
            }
            "mem" => {
                let role = operand.mem_operand_role.as_deref();
                if operand.read || matches!(role, Some("read" | "read_write")) {
                    input_map
                        .entry(operand.name.clone())
                        .or_default()
                        .push(mem.clone());
                }
                if operand.write || matches!(role, Some("write" | "read_write")) {
                    output_map
                        .entry(operand.name.clone())
                        .or_default()
                        .push(mem.clone());
                }
                if operand.is_agen || matches!(role, Some("agen" | "address")) {
                    for mem_addr in &decoded.mem_addrs {
                        if let Some(base) = &mem_addr.base {
                            input_map
                                .entry(operand.name.clone())
                                .or_default()
                                .push(crate::x64::get_canonical_reg(base));
                        }
                        if let Some(index) = &mem_addr.index {
                            input_map
                                .entry(operand.name.clone())
                                .or_default()
                                .push(crate::x64::get_canonical_reg(index));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    for values in input_map.values_mut().chain(output_map.values_mut()) {
        values.sort();
        values.dedup();
    }
    (input_map, output_map)
}

fn flatten_operand_map(map: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut values: Vec<String> = map.values().flatten().cloned().collect();
    values.sort();
    values.dedup();
    values
}

fn map_record_latencies_to_decoded(
    record: &uica_data::InstructionRecord,
    decoded: &uica_decoder::DecodedInstruction,
) -> BTreeMap<(String, String), i32> {
    let (input_map, output_map) = mapped_record_operands(record, decoded);
    let mut latencies = BTreeMap::new();
    for latency in &record.perf.latencies {
        if latency.cycles < 0 {
            continue;
        }
        let Some(inputs) = input_map.get(&latency.start_op) else {
            continue;
        };
        let Some(outputs) = output_map.get(&latency.target_op) else {
            continue;
        };
        for input in inputs {
            for output in outputs {
                latencies.insert((input.clone(), output.clone()), latency.cycles);
            }
        }
    }
    latencies
}

fn facts_to_port_usage_inputs(
    facts: &[LoopInstrFacts],
    frontend_instrs: &[AnalyticalInstruction],
) -> Vec<InstructionPortUsage> {
    facts
        .iter()
        .zip(frontend_instrs)
        .filter(|(_, instr)| !instr.macro_fused_with_prev)
        .map(|(fact, _)| InstructionPortUsage {
            port_data: fact.port_data.clone(),
            uops: fact.uops.max(0),
        })
        .collect()
}

fn facts_to_latency_instructions(facts: &[LoopInstrFacts]) -> Vec<AnalyticalLatencyInstruction> {
    facts
        .iter()
        .filter(|fact| !is_conditional_jump(&canonical_loop_mnemonic(&fact.mnemonic)))
        .map(|fact| AnalyticalLatencyInstruction {
            input_operands: fact.input_operands.clone(),
            output_operands: fact.output_operands.clone(),
            latencies: fact.latencies.clone(),
            may_be_eliminated: fact.may_be_eliminated,
        })
        .collect()
}

fn facts_to_analytical_instructions(facts: &[LoopInstrFacts]) -> Vec<AnalyticalInstruction> {
    let mut out: Vec<AnalyticalInstruction> = facts
        .iter()
        .map(|fact| {
            let mnemonic = canonical_loop_mnemonic(&fact.mnemonic);
            AnalyticalInstruction {
                size: fact.size,
                macro_fused_with_prev: false,
                macro_fused_with_next: false,
                macro_fusible_with_next: is_macro_fusible_mnemonic(&mnemonic)
                    && !fact.no_macro_fusion,
                is_branch: is_conditional_jump(&mnemonic),
                complex_decoder: fact.complex_decoder,
                n_available_simple_decoders: fact.n_available_simple_decoders,
                uops_mite: fact.uops_mite,
                uops_ms: fact.uops_ms,
                can_be_used_by_lsd: fact.can_be_used_by_lsd,
            }
        })
        .collect();

    for idx in 1..out.len() {
        if out[idx].is_branch && !facts[idx].no_macro_fusion && out[idx - 1].macro_fusible_with_next
        {
            out[idx].macro_fused_with_prev = true;
            out[idx - 1].macro_fused_with_next = true;
        }
    }

    out
}

fn is_macro_fusible_mnemonic(mnemonic: &str) -> bool {
    matches!(mnemonic, "dec" | "cmp" | "sub" | "test" | "inc" | "and")
}

fn fused_issue_uops(facts: &[LoopInstrFacts], fused_pairs: usize) -> i32 {
    let total_uops: i32 = facts.iter().map(|fact| fact.uops.max(0)).sum();
    (total_uops - i32::try_from(fused_pairs).unwrap_or(0)).max(0)
}

fn canonical_loop_mnemonic(mnemonic: &str) -> String {
    let lower = mnemonic.to_ascii_lowercase();
    if lower == "jne" {
        "jnz".to_string()
    } else {
        lower
    }
}

fn is_conditional_jump(mnemonic: &str) -> bool {
    mnemonic.starts_with('j') && mnemonic != "jmp"
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn estimate_iterations(cycles: u32, throughput: f64, min_iterations: u32) -> u32 {
    if throughput <= 0.0 {
        return min_iterations;
    }

    let warmup = 8.0;
    let est = (((cycles as f64) - warmup) / throughput).floor().max(0.0) as u32;
    est.max(min_iterations)
}

fn predicted_bottlenecks(
    limits: &std::collections::BTreeMap<String, Option<f64>>,
    throughput: f64,
) -> Vec<String> {
    let mut out = Vec::new();
    for (name, maybe_limit) in limits {
        if let Some(limit) = maybe_limit {
            if *limit >= 0.98 * throughput {
                match name.as_str() {
                    "predecoder" => out.push("Predecoder".to_string()),
                    "decoder" => out.push("Decoder".to_string()),
                    "dsb" => out.push("DSB".to_string()),
                    "lsd" => out.push("LSD".to_string()),
                    "issue" => out.push("Issue".to_string()),
                    "ports" => out.push("Ports".to_string()),
                    "dependencies" => out.push("Dependencies".to_string()),
                    _ => {}
                }
            }
        }
    }
    out.sort();
    out
}

fn canonical_signature(mnemonics: &[&str]) -> String {
    mnemonics
        .iter()
        .map(|mnemonic| match mnemonic.to_ascii_lowercase().as_str() {
            "jne" => "jnz".to_string(),
            other => other.to_string(),
        })
        .collect::<Vec<String>>()
        .join(",")
}

fn fallback_result(code: &[u8], invocation: &Invocation) -> UicaResult {
    let normalized_invocation = Invocation {
        arch: invocation.arch.to_ascii_uppercase(),
        ..invocation.clone()
    };
    let mode = if decode_raw(code)
        .unwrap_or_default()
        .iter()
        .any(|instr| instr.mnemonic.starts_with('j'))
    {
        "loop".to_string()
    } else {
        "unroll".to_string()
    };
    let _decoded = decode_raw(code).unwrap_or_default();
    let summary = Summary {
        throughput_cycles_per_iteration: Some(1.0),
        iterations_simulated: normalized_invocation.min_iterations,
        cycles_simulated: normalized_invocation.min_cycles,
        mode,
        ..Summary::default()
    };

    // Fallback: no cycles detail
    let cycles = Vec::new();

    UicaResult {
        uica_commit: env::var("UICA_COMMIT").unwrap_or_else(|_| "unknown".to_string()),
        invocation: normalized_invocation,
        summary,
        cycles,
        ..UicaResult::default()
    }
}

fn load_default_pack(arch: &str) -> Option<DataPack> {
    if let Ok(path) = env::var("UICA_RUST_DATAPACK") {
        if let Some(pack) = load_runtime_pack_source(Path::new(&path), arch) {
            return Some(pack);
        }
    }

    let generated_dir = PathBuf::from("rust/uica-data/generated");
    if let Some(pack) = load_runtime_pack_source(&generated_dir, arch) {
        return Some(pack);
    }

    None
}

fn load_runtime_pack_source(path: &Path, arch: &str) -> Option<DataPack> {
    if let Some(manifest_path) = runtime_manifest_path(path) {
        if manifest_path.exists() {
            if let Ok(pack) = load_manifest_pack(&manifest_path, arch) {
                return Some(pack);
            }
        }
    }

    if path.is_dir() {
        return None;
    }

    if path.is_file()
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("uipack"))
    {
        return load_uipack(path).ok();
    }

    None
}

fn runtime_manifest_path(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        return Some(path.join(DATAPACK_MANIFEST_FILE_NAME));
    }

    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == DATAPACK_MANIFEST_FILE_NAME)
    {
        return Some(path.to_path_buf());
    }

    None
}

/// Trace-mode engine: runs FrontEnd simulation and emits Q events.
pub fn engine_trace(
    code: &[u8],
    invocation: &Invocation,
) -> Result<crate::sim::TraceWriter, String> {
    let pack = load_default_pack(&invocation.arch)
        .ok_or_else(|| format!("uipack data not found for {}", invocation.arch))?;
    let (frontend, max_cycle) = run_simulation_for_cycles(code, invocation, &pack)?;

    // Emit trace events from simulator state, mirroring Python's generateEventTrace.
    emit_trace_from_frontend(&frontend, max_cycle)
}

fn emit_trace_from_frontend(
    frontend: &crate::sim::FrontEnd,
    max_cycle: u32,
) -> Result<crate::sim::TraceWriter, String> {
    use crate::sim::trace::{EventKind, TraceEvent, TraceWriter};

    // Map instr_instance_idx → (instr_id, rnd, lam_position)
    // We need to emit per-lam events with the right lamUopID index.
    // Build instr_id and rnd from all_generated_instr_instances.
    let inst_meta: std::collections::HashMap<u64, (u32, u32)> = frontend
        .all_generated_instr_instances
        .iter()
        .map(|i| (i.idx, (i.instr_id, i.rnd)))
        .collect();

    // Collect laminated uop indices sorted by lam_idx for stable ordering.
    let mut lam_idxs_sorted: Vec<u64> = frontend
        .uop_storage
        .laminated_uops
        .keys()
        .copied()
        .collect();
    lam_idxs_sorted.sort_unstable();

    // Track (instr_instance_idx) → Vec<lam_idx> sorted, for lamUopID assignment.
    let mut lams_by_inst: std::collections::BTreeMap<u64, Vec<u64>> =
        std::collections::BTreeMap::new();
    for &li in &lam_idxs_sorted {
        if let Some(lam) = frontend.uop_storage.get_laminated_uop(li) {
            lams_by_inst
                .entry(lam.instr_instance_idx)
                .or_default()
                .push(li);
        }
    }

    let mut trace = TraceWriter::new();

    // Q events: addedToIDQ
    for &li in &lam_idxs_sorted {
        let lam = match frontend.uop_storage.get_laminated_uop(li) {
            Some(l) => l,
            None => continue,
        };
        let Some(added_to_idq) = lam.added_to_idq else {
            continue;
        };
        if added_to_idq > max_cycle {
            continue;
        }
        let Some(&(instr_id, rnd)) = inst_meta.get(&lam.instr_instance_idx) else {
            continue;
        };
        let lam_pos = lams_by_inst
            .get(&lam.instr_instance_idx)
            .and_then(|v| v.iter().position(|&x| x == li))
            .unwrap_or(0) as i64;
        trace.push(TraceEvent {
            cycle: added_to_idq,
            kind: EventKind::AddedToIdq,
            instr_id: instr_id as i64,
            rnd: rnd as i64,
            lam: lam_pos,
            fused: -1,
            uop: -1,
            port: None,
            source: lam.uop_source.map(|s| s.as_str().to_string()),
        });
    }

    // I / R / r / D / E events: walk fused → unfused uops
    for &li in &lam_idxs_sorted {
        let Some(lam) = frontend.uop_storage.get_laminated_uop(li) else {
            continue;
        };
        let Some(&(instr_id, rnd)) = inst_meta.get(&lam.instr_instance_idx) else {
            continue;
        };
        let lam_pos = lams_by_inst
            .get(&lam.instr_instance_idx)
            .and_then(|v| v.iter().position(|&x| x == li))
            .unwrap_or(0) as i64;

        for (fi, &fused_idx) in lam.fused_uop_idxs.iter().enumerate() {
            let Some(fused) = frontend.uop_storage.get_fused_uop(fused_idx) else {
                continue;
            };

            // I event
            if let Some(issued) = fused.issued {
                if issued <= max_cycle {
                    trace.push(TraceEvent {
                        cycle: issued,
                        kind: EventKind::Issued,
                        instr_id: instr_id as i64,
                        rnd: rnd as i64,
                        lam: lam_pos,
                        fused: fi as i64,
                        uop: -1,
                        port: None,
                        source: None,
                    });
                }
            }
            // R event
            if let Some(retired) = fused.retired {
                if retired <= max_cycle {
                    trace.push(TraceEvent {
                        cycle: retired,
                        kind: EventKind::Retired,
                        instr_id: instr_id as i64,
                        rnd: rnd as i64,
                        lam: lam_pos,
                        fused: fi as i64,
                        uop: -1,
                        port: None,
                        source: None,
                    });
                }
            }

            for (ui, &unfused_idx) in fused.unfused_uop_idxs.iter().enumerate() {
                let Some(uop) = frontend.uop_storage.get_uop(unfused_idx) else {
                    continue;
                };
                if uop.prop.possible_ports.is_empty() {
                    continue;
                } // no sched events for zero-port

                // r event
                if let Some(ready) = uop.ready_for_dispatch {
                    if ready <= max_cycle {
                        trace.push(TraceEvent {
                            cycle: ready,
                            kind: EventKind::ReadyForDispatch,
                            instr_id: instr_id as i64,
                            rnd: rnd as i64,
                            lam: lam_pos,
                            fused: fi as i64,
                            uop: ui as i64,
                            port: None,
                            source: None,
                        });
                    }
                }
                // D event
                if let Some(dispatched) = uop.dispatched {
                    if dispatched <= max_cycle {
                        trace.push(TraceEvent {
                            cycle: dispatched,
                            kind: EventKind::Dispatched,
                            instr_id: instr_id as i64,
                            rnd: rnd as i64,
                            lam: lam_pos,
                            fused: fi as i64,
                            uop: ui as i64,
                            port: uop.actual_port.as_ref().map(|p| format!("Port{p}")),
                            source: None,
                        });
                    }
                }
                // E event
                if let Some(executed) = uop.executed {
                    if executed <= max_cycle {
                        trace.push(TraceEvent {
                            cycle: executed,
                            kind: EventKind::Executed,
                            instr_id: instr_id as i64,
                            rnd: rnd as i64,
                            lam: lam_pos,
                            fused: fi as i64,
                            uop: ui as i64,
                            port: None,
                            source: None,
                        });
                    }
                }
            }
        }
    }

    Ok(trace)
}

/// Build cycles JSON from simulator state, matching Python's generateJSONOutput.
/// Each cycle dict contains optional keys for events that happened in that cycle.
fn build_cycles_json(frontend: &crate::sim::FrontEnd, max_cycle: u32) -> Vec<serde_json::Value> {
    use serde_json::json;
    use std::collections::HashMap;

    // Initialize cycles array
    let mut cycles: Vec<serde_json::Map<String, serde_json::Value>> = (0..=max_cycle)
        .map(|c| {
            let mut map = serde_json::Map::new();
            map.insert("cycle".to_string(), json!(c));
            map
        })
        .collect();

    // Track unfused uop dicts for dependency linking
    let mut unfused_uop_to_dict: HashMap<u64, serde_json::Value> = HashMap::new();

    // Walk all generated instruction instances
    for instr_i in &frontend.all_generated_instr_instances {
        let instr_id = instr_i.instr_id;
        let rnd = instr_i.rnd;

        // addedToIQ (predecoded)
        if let Some(predecoded) = instr_i.predecoded {
            if predecoded <= max_cycle {
                cycles[predecoded as usize]
                    .entry("addedToIQ")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .unwrap()
                    .push(json!({
                        "rnd": rnd,
                        "instr": instr_id,
                    }));
            }
        }

        // removedFromIQ
        if let Some(removed_from_iq) = instr_i.removed_from_iq {
            if removed_from_iq <= max_cycle {
                cycles[removed_from_iq as usize]
                    .entry("removedFromIQ")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .unwrap()
                    .push(json!({
                        "rnd": rnd,
                        "instr": instr_id,
                    }));
            }
        }

        // Walk laminated uops for this instruction (regMerge + stackSync + regular)
        // Python: for lamUopI, lamUop in enumerate(instrI.regMergeUops + instrI.stackSyncUops + instrI.uops)
        //
        // uop_storage.laminated_uops is a HashMap; sort by lam_idx so the
        // lamUopID emitted here matches the creation order (position within
        // the instruction's uop list).
        let mut lam_uop_indices: Vec<u64> = frontend
            .uop_storage
            .laminated_uops
            .keys()
            .filter(|&&lam_idx| {
                if let Some(lam_uop) = frontend.uop_storage.get_laminated_uop(lam_idx) {
                    lam_uop.instr_instance_idx == instr_i.idx
                } else {
                    false
                }
            })
            .copied()
            .collect();
        lam_uop_indices.sort_unstable();

        for (lam_uop_i, &lam_idx) in lam_uop_indices.iter().enumerate() {
            let lam_uop = frontend.uop_storage.get_laminated_uop(lam_idx).unwrap();

            let base_uop_dict = json!({
                "rnd": rnd,
                "instrID": instr_id,
                "lamUopID": lam_uop_i,
            });

            // addedToIDQ
            if let Some(added_to_idq) = lam_uop.added_to_idq {
                if added_to_idq <= max_cycle {
                    let mut lam_uop_dict = base_uop_dict.as_object().unwrap().clone();
                    lam_uop_dict.insert(
                        "source".to_string(),
                        json!(lam_uop.uop_source.map(|s| s.as_str()).unwrap_or("MITE")),
                    );
                    cycles[added_to_idq as usize]
                        .entry("addedToIDQ")
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .unwrap()
                        .push(json!(lam_uop_dict));
                }
            }

            // Walk fused uops
            for (f_uop_i, &f_idx) in lam_uop.fused_uop_idxs.iter().enumerate() {
                let fused_uop = frontend.uop_storage.get_fused_uop(f_idx).unwrap();

                let mut f_uop_dict = base_uop_dict.as_object().unwrap().clone();
                f_uop_dict.insert("fUopID".to_string(), json!(f_uop_i));

                // issued -> addedToRB and removedFromIDQ
                if let Some(issued) = fused_uop.issued {
                    if issued <= max_cycle {
                        // removedFromIDQ (only for first fused uop, fUopID==0, and if lam was added to IDQ)
                        if f_uop_i == 0 && lam_uop.added_to_idq.is_some() {
                            cycles[issued as usize]
                                .entry("removedFromIDQ")
                                .or_insert_with(|| json!([]))
                                .as_array_mut()
                                .unwrap()
                                .push(json!(f_uop_dict.clone()));
                        }

                        // addedToRB
                        cycles[issued as usize]
                            .entry("addedToRB")
                            .or_insert_with(|| json!([]))
                            .as_array_mut()
                            .unwrap()
                            .push(json!(f_uop_dict.clone()));
                    }
                }

                // retired -> removedFromRB
                if let Some(retired) = fused_uop.retired {
                    if retired <= max_cycle {
                        cycles[retired as usize]
                            .entry("removedFromRB")
                            .or_insert_with(|| json!([]))
                            .as_array_mut()
                            .unwrap()
                            .push(json!(f_uop_dict.clone()));
                    }
                }

                // Walk unfused uops
                for (uop_i, &uop_idx) in fused_uop.unfused_uop_idxs.iter().enumerate() {
                    let uop = frontend.uop_storage.get_uop(uop_idx).unwrap();

                    let mut unfused_uop_dict = f_uop_dict.clone();
                    unfused_uop_dict.insert("uopID".to_string(), json!(uop_i));

                    // Register in lookup table
                    unfused_uop_to_dict.insert(uop_idx, json!(unfused_uop_dict.clone()));

                    // addedToRS: only for uops that go through the scheduler
                    // (have ports) and whose execution happens after issue.
                    // Python: `if fusedUop.issued != uop.executed`
                    if let Some(issued) = fused_uop.issued {
                        let executed = uop.executed;
                        let goes_through_scheduler = !uop.prop.possible_ports.is_empty();
                        if goes_through_scheduler && issued <= max_cycle && Some(issued) != executed
                        {
                            let mut rs_dict = unfused_uop_dict.clone();

                            // Match Python: one dependsOn entry per renamed
                            // input operand. Python's Renamer uses separate
                            // RenamedOperand objects per (producer_uop,
                            // operand_role), so duplicates appear when the
                            // same producer writes multiple input roles.
                            let mut depends_on = Vec::new();
                            for ren_op in &uop.renamed_input_operands {
                                let ren_op_ref = ren_op.borrow();
                                if let Some(producer_uop_idx) = ren_op_ref.uop_idx {
                                    if let Some(producer_dict) =
                                        unfused_uop_to_dict.get(&producer_uop_idx)
                                    {
                                        depends_on.push(producer_dict.clone());
                                    }
                                }
                            }
                            rs_dict.insert("dependsOn".to_string(), json!(depends_on));

                            cycles[issued as usize]
                                .entry("addedToRS")
                                .or_insert_with(|| json!([]))
                                .as_array_mut()
                                .unwrap()
                                .push(json!(rs_dict));
                        }
                    }

                    // readyForDispatch / dispatched / executed are only
                    // emitted for scheduler-bound uops (have possible ports).
                    // Port-less uops (move-eliminated, NOP) run at issue time
                    // and produce no scheduling events in Python's output.
                    if !uop.prop.possible_ports.is_empty() {
                        if let Some(ready) = uop.ready_for_dispatch {
                            if ready <= max_cycle {
                                cycles[ready as usize]
                                    .entry("readyForDispatch")
                                    .or_insert_with(|| json!([]))
                                    .as_array_mut()
                                    .unwrap()
                                    .push(json!(unfused_uop_dict.clone()));
                            }
                        }
                        if let Some(dispatched) = uop.dispatched {
                            if dispatched <= max_cycle {
                                let port_label =
                                    format!("Port{}", uop.actual_port.as_deref().unwrap_or(""));
                                cycles[dispatched as usize]
                                    .entry("dispatched")
                                    .or_insert_with(|| json!({}))
                                    .as_object_mut()
                                    .unwrap()
                                    .insert(port_label, json!(unfused_uop_dict.clone()));
                            }
                        }
                        if let Some(executed) = uop.executed {
                            if executed <= max_cycle {
                                cycles[executed as usize]
                                    .entry("executed")
                                    .or_insert_with(|| json!([]))
                                    .as_array_mut()
                                    .unwrap()
                                    .push(json!(unfused_uop_dict.clone()));
                            }
                        }
                    } // end !possible_ports.is_empty() scheduler events

                    // executed is always emitted even for port-less uops
                    // (Python emits E for move-eliminated instructions).
                    if uop.prop.possible_ports.is_empty() {
                        if let Some(executed) = uop.executed {
                            if executed <= max_cycle {
                                cycles[executed as usize]
                                    .entry("executed")
                                    .or_insert_with(|| json!([]))
                                    .as_array_mut()
                                    .unwrap()
                                    .push(json!(unfused_uop_dict.clone()));
                            }
                        }
                    }
                }
            }
        }
    }

    // Convert to Vec<Value>
    cycles.into_iter().map(|c| json!(c)).collect()
}

/// Run the simulator and return the frontend + final clock for cycles JSON extraction.
fn run_simulation_for_cycles(
    code: &[u8],
    invocation: &Invocation,
    pack: &DataPack,
) -> Result<(crate::sim::FrontEnd, u32), String> {
    use crate::sim::types::build_instruction_instances;
    use crate::sim::FrontEnd;

    let arch = get_micro_arch(&invocation.arch)
        .ok_or_else(|| format!("unknown architecture: {}", invocation.arch))?;

    let decoded = decode_raw(code).map_err(|e| format!("decode error: {e}"))?;

    let base_instances = build_instruction_instances(&decoded, invocation.alignment_offset);

    // Bail out early if the simulator cannot model every instruction in the
    // stream yet. Keeps engine.rs on the analytical summary path for those
    // cases without running a doomed simulation to the safety cap.
    // Build DataPackIndex once for all checks (not per-instruction).
    let _check_index = uica_data::DataPackIndex::new(pack.clone());
    for inst in &base_instances {
        if inst.macro_fused_with_prev_instr {
            continue;
        }
        let has_memory = inst.has_memory_read || inst.has_memory_write;
        if !crate::sim::uop_expand::is_mnemonic_supported(
            &inst.mnemonic,
            inst.macro_fused_with_next_instr,
            inst.macro_fused_with_prev_instr,
            has_memory,
            &invocation.arch,
            pack,
        ) {
            return Err(format!(
                "unsupported mnemonic for simulator: {}",
                inst.mnemonic
            ));
        }
    }

    let min_cycles = invocation.min_cycles;
    let min_iterations = invocation.min_iterations;
    // Python: unroll = (not instructions[-1].isBranchInstr)
    let unroll = base_instances
        .last()
        .map(|inst| !inst.is_branch_instr)
        .unwrap_or(true);

    let mut frontend = FrontEnd::new_with_init_policy(
        arch,
        unroll,
        base_instances.clone(),
        invocation.alignment_offset,
        pack,
        invocation.init_policy.clone(),
        invocation.simple_front_end,
        invocation.no_micro_fusion,
        invocation.no_macro_fusion,
    );

    // Mirror Python runSimulation loop:
    //   while True: frontEnd.cycle(clock); handle retirement;
    //   if rnd >= minIterations and clock > minCycles: break; clock += 1;
    let mut clock = 0u32;
    loop {
        frontend.cycle(clock);
        let retired_rounds = count_retired_rounds(&frontend);
        if retired_rounds >= min_iterations && clock > min_cycles {
            break;
        }
        clock += 1;
        // Hard safety cap to avoid pathological infinite loops.
        if clock > min_cycles.saturating_add(10_000) {
            break;
        }
    }

    // Match Python's generateJSONOutput(clock-1) max-cycle contract: the
    // last cycle whose events we record is the one that triggered the
    // break, so the array length is clock (not clock + 1).
    let max_cycle = clock.saturating_sub(1);
    Ok((frontend, max_cycle))
}

fn canonicalize_instr_string(asm: &str) -> String {
    // Python: re.sub('[(){}, ]+', '_', s).strip('_')
    let mut out = String::with_capacity(asm.len());
    let mut last_underscore = true;
    for ch in asm.chars() {
        if matches!(ch, '(' | ')' | '{' | '}' | ',' | ' ') {
            if !last_underscore {
                out.push('_');
                last_underscore = true;
            }
        } else {
            out.push(ch);
            last_underscore = false;
        }
    }
    out.trim_matches('_').to_string()
}

fn instr_url(asm: &str) -> String {
    format!(
        "https://www.uops.info/html-instr/{}.html",
        canonicalize_instr_string(asm)
    )
}

fn build_instructions_json(frontend: &crate::sim::FrontEnd) -> Vec<serde_json::Value> {
    use serde_json::json;
    let mut by_id: std::collections::BTreeMap<u32, serde_json::Map<String, serde_json::Value>> =
        std::collections::BTreeMap::new();
    for inst in &frontend.all_generated_instr_instances {
        let entry = by_id.entry(inst.instr_id).or_insert_with(|| {
            let mut map = serde_json::Map::new();
            map.insert("instrID".to_string(), json!(inst.instr_id));
            map.insert("asm".to_string(), json!(inst.disasm.clone()));
            map.insert("opcode".to_string(), json!(inst.opcode_hex.clone()));
            map.insert("url".to_string(), json!(instr_url(&inst.disasm)));
            map
        });
        if inst.macro_fused_with_next_instr && !entry.contains_key("macroFusedWithNextInstr") {
            entry.insert("macroFusedWithNextInstr".to_string(), json!(true));
        }
        if !entry.contains_key("source") {
            if let Some(src) = inst.source {
                entry.insert("source".to_string(), json!(src.as_str()));
            }
        }
    }
    by_id.into_values().map(serde_json::Value::Object).collect()
}

fn build_instructions_json_from_decode(code: &[u8]) -> Vec<serde_json::Value> {
    use serde_json::json;
    let mut out = Vec::new();
    let Ok(decoded) = decode_raw(code) else {
        return out;
    };
    for (idx, dec) in decoded.iter().enumerate() {
        let mut map = serde_json::Map::new();
        map.insert("instrID".to_string(), json!(idx));
        map.insert("asm".to_string(), json!(dec.disasm.clone()));
        map.insert(
            "opcode".to_string(),
            json!(dec
                .bytes
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>()),
        );
        map.insert("url".to_string(), json!(instr_url(&dec.disasm)));
        out.push(serde_json::Value::Object(map));
    }
    out
}

fn count_retired_rounds(frontend: &crate::sim::FrontEnd) -> u32 {
    // InstrInstance.laminated_uops is not populated by uop_expand yet, so we
    // resolve (fused -> unfused -> instance) through the storage and the
    // instance vector. Mirrors Python's `rnd = uop.instrI.rnd` chain when
    // walking the retire queue.
    use std::collections::HashMap;
    let instance_rnd_by_idx: HashMap<u64, u32> = frontend
        .all_generated_instr_instances
        .iter()
        .map(|i| (i.idx, i.rnd))
        .collect();

    let mut max_rnd = 0u32;
    let mut any_retired = false;
    for lam in frontend.uop_storage.laminated_uops.values() {
        for &fused_idx in &lam.fused_uop_idxs {
            let Some(fused) = frontend.uop_storage.get_fused_uop(fused_idx) else {
                continue;
            };
            if fused.retired.is_none() {
                continue;
            }
            for &unfused_idx in &fused.unfused_uop_idxs {
                if let Some(uop) = frontend.uop_storage.get_uop(unfused_idx) {
                    if let Some(&rnd) = instance_rnd_by_idx.get(&uop.instr_instance_idx) {
                        any_retired = true;
                        if rnd > max_rnd {
                            max_rnd = rnd;
                        }
                    }
                }
            }
        }
    }
    if any_retired {
        max_rnd + 1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::tempdir;
    use uica_data::{
        encode_uipack, DataPack, InstructionRecord, PerfRecord, DATAPACK_SCHEMA_VERSION,
    };

    fn sample_pack() -> DataPack {
        DataPack {
            schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
            instructions: vec![InstructionRecord {
                arch: "SKL".to_string(),
                iform: "ADD_GPRv_GPRv".to_string(),
                string: "ADD".to_string(),
                perf: PerfRecord {
                    uops: 1,
                    retire_slots: 1,
                    uops_mite: 1,
                    uops_ms: 0,
                    tp: Some(1.0),
                    ports: BTreeMap::from([("0".to_string(), 1)]),
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
                },
            }],
        }
    }

    #[test]
    fn runtime_pack_source_ignores_legacy_instructions_json() {
        let temp = tempdir().unwrap();
        let legacy_path = temp.path().join("instructions.json");
        std::fs::write(&legacy_path, encode_uipack(&sample_pack(), "SKL").unwrap()).unwrap();

        assert!(load_runtime_pack_source(temp.path(), "SKL").is_none());
    }
}
