use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};

use serde_json::json;
use uica_data::{
    load_manifest_runtime, load_manifest_runtime_verified, record_view_to_instruction_record,
    DataPack, DataPackIndex, MappedUiPackRuntime, DATAPACK_MANIFEST_FILE_NAME,
};
use uica_decoder::decode_raw;
use uica_model::{Invocation, Summary, UicaResult};

use crate::analytical::{
    compute_final_prediction, compute_frontend_limits, compute_issue_limit,
    compute_maximum_latency_for_graph, compute_port_usage_limit, generate_latency_graph,
    AnalyticalInstruction, AnalyticalLatencyInstruction, AnalyticalMemOperand,
    InstructionPortUsage,
};
use crate::matcher::{match_instruction_record_iter, NormalizedInstrRef};
use crate::micro_arch::{get_micro_arch, MicroArchConfig};

#[derive(Clone, Debug, Default)]
pub struct EngineOutput {
    pub result: UicaResult,
    pub reports: Option<uica_model::ReportBundle>,
}

pub fn engine_output(
    code: &[u8],
    invocation: &Invocation,
    include_reports: bool,
    verify_uipack: bool,
) -> Result<EngineOutput, String> {
    let runtime = if verify_uipack {
        load_default_runtime_verified(&invocation.arch)?
    } else {
        let Some(runtime) = load_default_runtime(&invocation.arch, false) else {
            if include_reports {
                return Err(format!("uipack data not found for {}", invocation.arch));
            }
            return Ok(EngineOutput {
                result: fallback_result(code, invocation),
                reports: None,
            });
        };
        runtime
    };
    engine_output_with_uipack_runtime(code, invocation, &runtime, include_reports)
}

pub fn engine(code: &[u8], invocation: &Invocation) -> UicaResult {
    if let Some(runtime) = load_default_runtime(&invocation.arch, false) {
        return engine_output_with_uipack_runtime(code, invocation, &runtime, false)
            .map(|output| output.result)
            .unwrap_or_else(|_| fallback_result(code, invocation));
    }

    fallback_result(code, invocation)
}

fn engine_with_pack_internal(
    code: &[u8],
    invocation: &Invocation,
    pack: &DataPack,
    include_reports: bool,
) -> Result<EngineOutput, String> {
    let normalized_invocation = Invocation {
        arch: invocation.arch.to_ascii_uppercase(),
        ..invocation.clone()
    };
    let mut result = UicaResult {
        uica_commit: env::var("UICA_COMMIT").unwrap_or_else(|_| "unknown".to_string()),
        invocation: normalized_invocation.clone(),
        ..UicaResult::default()
    };

    let mut reports = None;
    let arch = match get_micro_arch(&normalized_invocation.arch) {
        Some(arch) => arch,
        None => {
            return Err(format!(
                "microarchitecture model not supported for {}; uipack data exists but no Rust pipeline configuration is available",
                normalized_invocation.arch
            ))
        }
    };

    let decoded = match decode_raw(code) {
        Ok(decoded) => decoded,
        Err(_) => {
            return Ok(EngineOutput {
                result: fallback_result(code, &normalized_invocation),
                reports: None,
            })
        }
    };

    let index = DataPackIndex::new(pack);

    let mut total_retire_slots = 0;
    let mut loop_facts = Vec::with_capacity(decoded.len());

    for decoded_instr in &decoded {
        let norm = NormalizedInstrRef {
            // Python parity: `getInstructions()` matches uops.info XML attrs,
            // including operand width. Use decoder width here like FrontEnd
            // metadata matching, rather than falling back to width-agnostic
            // records that can carry different port data (e.g. R16 vs R64 MOV).
            max_op_size_bytes: decoded_instr.max_op_size_bytes,
            immediate: decoded_instr.immediate,
            mnemonic: &decoded_instr.mnemonic,
            decoded_iform: &decoded_instr.iform,
            iform_signature: &decoded_instr.iform_signature,
            uses_high8_reg: decoded_instr.uses_high8_reg,
            explicit_reg_operands: &decoded_instr.explicit_reg_operands,
            xml_attrs: &decoded_instr.xml_attrs,
            agen: decoded_instr.agen.as_deref(),
        };

        let mut fact = LoopInstrFacts {
            mnemonic: decoded_instr.mnemonic.to_ascii_lowercase(),
            size: decoded_instr.len,
            uops: 1,
            uops_mite: 1,
            uops_ms: 0,
            retire_slots: 1,
            port_data: BTreeMap::new(),
            complex_decoder: false,
            n_available_simple_decoders: arch.n_decoders.saturating_sub(1),
            can_be_used_by_lsd: true,
            no_macro_fusion: false,
            lcp_stall: decoded_instr.has_66_prefix && decoded_instr.immediate_width_bits == 16,
            instr_str: String::new(),
            macro_fusible_with: Vec::new(),
            input_mem_operands: decoded_instr
                .mem_addrs
                .iter()
                .filter(|_| decoded_instr.has_memory_read)
                .map(|mem| AnalyticalMemOperand {
                    base: mem.base.as_deref().map(crate::x64::get_canonical_reg),
                    index: mem.index.as_deref().map(crate::x64::get_canonical_reg),
                    disp: mem.disp,
                })
                .collect(),
            mem_addr_operands: decoded_instr
                .mem_addrs
                .iter()
                .flat_map(|mem| [mem.base.as_ref(), mem.index.as_ref()])
                .flatten()
                .map(|reg| crate::x64::get_canonical_reg(reg))
                .collect(),
            // Python parity: absent `archData.instrData[iform]` becomes
            // UnknownInstr with empty input/output operand lists. Matched
            // instructions fill these from record/XED operands below.
            input_operands: Vec::new(),
            abstract_value_input_operands: Vec::new(),
            output_operands: Vec::new(),
            latencies: BTreeMap::new(),
            mem_addr_latency_pairs: std::collections::BTreeSet::new(),
            implicit_rsp_change: decoded_instr.implicit_rsp_change,
            non_implicit_input_operands: std::collections::BTreeSet::new(),
            may_be_eliminated: false,
            eliminated_move_input: None,
            eliminated_move_output_is_32_bit: decoded_instr
                .output_regs
                .first()
                .is_some_and(|reg| crate::x64::get_reg_size(reg) == 32),
            immediate: decoded_instr.immediate,
            // UnknownInstr still participates in frontend/issue loop modeling
            // with default uopsMITE=1 and retireSlots=1.
            matched: true,
        };

        let candidates = index.candidates_for(&result.invocation.arch, &decoded_instr.mnemonic);
        if let Some(record) = match_instruction_record_iter(norm, candidates) {
            let uses_sr_fallback_for_analytics =
                crate::sim::uop_expand::record_movzx_special_case_with_input_regs(
                    record,
                    &decoded_instr.input_regs,
                    &arch,
                );
            // Python parity: `getInstructions()` overlays `_SR` fields for
            // same-register forms, then `_I` fields for indexed memory forms.
            let uses_same_reg = crate::sim::uop_expand::explicit_regs_use_same_reg(
                &decoded_instr.explicit_reg_operands,
            );
            let uses_indexed_addr = decoded_instr
                .mem_addrs
                .iter()
                .any(|addr| addr.index.is_some());
            let perf = crate::sim::uop_expand::perf_for_python_getinstructions(
                record,
                uses_same_reg,
                uses_indexed_addr,
                &decoded_instr.input_regs,
                &arch,
            );
            let uops = if uses_sr_fallback_for_analytics && perf.uops == 0 {
                1
            } else {
                perf.uops.max(0)
            };
            let mut retire_slots = perf.retire_slots.max(1);
            fact.instr_str = record.string.clone();
            fact.uops = uops;
            fact.uops_mite = crate::sim::uop_expand::perf_uops_mite(&perf);
            fact.uops_ms = perf.uops_ms.max(0) as u32;
            fact.retire_slots = retire_slots;
            fact.port_data = perf.ports.clone();
            if uses_sr_fallback_for_analytics && fact.port_data.is_empty() {
                fact.port_data.insert(pack.alu_ports.join(""), 1);
            }
            if is_decoded_zero_idiom(decoded_instr) {
                // Python parity: `instructions.py` drops zero-idiom input
                // operands and leaves no analytical port contribution, while
                // simulator still emits zero-port bookkeeping uops.
                fact.port_data.clear();
            }
            let (mut complex_decoder, mut n_available_simple_decoders) =
                crate::sim::uop_expand::python_decoder_shape_from_record(
                    record,
                    &perf,
                    arch.n_decoders,
                );
            let opcode_hex = decoded_instr
                .bytes
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>();
            crate::sim::uop_expand::apply_python_pop5c_decoder_shape(
                record,
                &opcode_hex,
                &arch,
                &mut complex_decoder,
                &mut n_available_simple_decoders,
            );
            fact.complex_decoder = complex_decoder;
            fact.n_available_simple_decoders = n_available_simple_decoders;
            if result.invocation.no_micro_fusion {
                retire_slots = (perf.uops.max(0) as u32)
                    .max(fact.uops_mite + fact.uops_ms)
                    .max(1) as i32;
                fact.retire_slots = retire_slots;
                fact.uops_mite = (retire_slots as u32).saturating_sub(fact.uops_ms);
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
            fact.can_be_used_by_lsd = fact.uops_ms == 0
                && decoded_instr.implicit_rsp_change == 0
                && !decoded_instr
                    .output_regs
                    .iter()
                    .any(|reg| crate::x64::is_high8_reg(reg));
            fact.no_macro_fusion = perf.no_macro_fusion || result.invocation.no_macro_fusion;
            fact.macro_fusible_with = if fact.no_macro_fusion {
                Vec::new()
            } else {
                record.perf.macro_fusible_with.clone()
            };
            fact.lcp_stall |= perf.lcp_stall;
            fact.may_be_eliminated =
                crate::sim::uop_expand::python_may_be_eliminated_for_getinstructions(
                    record,
                    uses_same_reg,
                    uses_indexed_addr,
                    &decoded_instr.input_regs,
                    &arch,
                );
            let (input_map, output_map) = mapped_record_operands(record, decoded_instr);
            fact.eliminated_move_input = input_map
                .values()
                .flatten()
                .next()
                .map(|operand| operand.value.clone());
            fact.input_operands = flatten_input_operand_map(&input_map);
            fact.abstract_value_input_operands = abstract_value_input_operands(&input_map);
            fact.non_implicit_input_operands = non_implicit_input_operands(&input_map);
            fact.output_operands = flatten_operand_map(&output_map);
            fact.latencies = map_record_latencies_to_decoded(
                record,
                decoded_instr,
                arch.name,
                uses_sr_fallback_for_analytics || uses_same_reg,
            );
            fact.mem_addr_latency_pairs = map_record_mem_addr_latency_pairs(
                record,
                decoded_instr,
                arch.name,
                uses_sr_fallback_for_analytics || uses_same_reg,
            );
            fact.matched = true;
        }

        total_retire_slots += fact.retire_slots;
        loop_facts.push(fact);
    }

    apply_python_high8_latency_adjustments(&mut loop_facts, &decoded, &arch);

    let issue_limit = round2(compute_issue_limit(
        total_retire_slots,
        arch.issue_width as i32,
    ));
    let frontend_instrs_for_limits = facts_to_analytical_instructions(&loop_facts);
    let ports_limit = round2(compute_port_usage_limit(&facts_to_port_usage_inputs(
        &loop_facts,
        &frontend_instrs_for_limits,
        arch.name,
    )));

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
                &result.invocation.init_policy,
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

    // Run simulator to generate cycles JSON and final simulated counts.
    let mut lsd_active = false;
    let mut lsd_unroll_count = 1u32;
    if let Ok((frontend, uops_for_round, final_clock)) =
        run_simulation_for_cycles(code, &normalized_invocation, pack, &index)
    {
        lsd_active = frontend.uop_source.as_deref() == Some("LSD");
        lsd_unroll_count = frontend.lsd_unroll_count;
        result.summary.iterations_simulated = uops_for_round.len() as u32;
        result.summary.cycles_simulated = final_clock + 1;
        if let Some(simulated_throughput) = compute_simulated_throughput(&frontend, &uops_for_round)
        {
            // Python parity: JSON `TP` is derived from `uopsForRound` retirement
            // samples for both loop and unroll mode, then passed into
            // `getBottlenecks`.
            result.summary.throughput_cycles_per_iteration = Some(simulated_throughput);
        }
        refresh_summary_limits_from_python_bottlenecks(
            &mut result.summary,
            &decoded,
            &loop_facts,
            &arch,
            &frontend,
            &uops_for_round,
        );
        align_frontend_limits_with_simulated_sources(&mut result.summary, &frontend);
        if let Some(throughput) = result.summary.throughput_cycles_per_iteration {
            result.summary.bottlenecks_predicted =
                predicted_bottlenecks(&result.summary.limits, throughput);
        }
        append_python_runtime_bottlenecks(&mut result.summary, &frontend, &uops_for_round);
        result.cycles = build_cycles_json(&frontend, final_clock);
        result.instructions = build_instructions_json(&frontend);
        if include_reports {
            let last_relevant_round = uops_for_round.len().saturating_sub(2) as u32;
            let trace =
                crate::report::build_trace_report(&frontend, last_relevant_round, final_clock);
            let graph = crate::report::build_graph_report(
                &frontend,
                &normalized_invocation.arch,
                final_clock,
            );
            reports = Some(uica_model::ReportBundle { trace, graph });
        }
    } else {
        result.cycles = Vec::new();
        result.instructions = build_instructions_json_from_decode(code);
    }

    let all_ports: Vec<String> = pack.all_ports.clone();
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

    Ok(EngineOutput { result, reports })
}

pub fn engine_with_pack(code: &[u8], invocation: &Invocation, pack: &DataPack) -> UicaResult {
    engine_with_pack_internal(code, invocation, pack, false)
        .map(|output| output.result)
        .unwrap_or_else(|_| fallback_result(code, invocation))
}

pub fn engine_output_with_pack(
    code: &[u8],
    invocation: &Invocation,
    pack: &DataPack,
    include_reports: bool,
) -> Result<EngineOutput, String> {
    engine_with_pack_internal(code, invocation, pack, include_reports)
}

pub fn engine_output_with_uipack_runtime(
    code: &[u8],
    invocation: &Invocation,
    runtime: &MappedUiPackRuntime,
    include_reports: bool,
) -> Result<EngineOutput, String> {
    let pack = materialize_runtime_pack_for_code(code, runtime)?;
    engine_with_pack_internal(code, invocation, &pack, include_reports)
}

fn materialize_runtime_pack_for_code(
    code: &[u8],
    runtime: &MappedUiPackRuntime,
) -> Result<DataPack, String> {
    let view = runtime.view().map_err(|err| err.to_string())?;
    let mut selected = BTreeSet::new();
    let mut instructions = Vec::new();

    if let Ok(decoded) = decode_raw(code) {
        for decoded_instr in &decoded {
            for &record_index in runtime
                .index()
                .record_indices_for_mnemonic(&decoded_instr.mnemonic)
            {
                if selected.insert(record_index) {
                    instructions.push(
                        record_view_to_instruction_record(
                            view.record(record_index).map_err(|err| err.to_string())?,
                        )
                        .map_err(|err| err.to_string())?,
                    );
                }
            }
        }
    }

    Ok(DataPack {
        schema_version: view.schema_version().to_string(),
        all_ports: view.all_ports(),
        alu_ports: view.alu_ports(),
        instructions,
    })
}

#[derive(Clone, Debug)]
struct LoopInstrFacts {
    mnemonic: String,
    size: u32,
    uops: i32,
    uops_mite: u32,
    uops_ms: u32,
    retire_slots: i32,
    port_data: BTreeMap<String, i32>,
    complex_decoder: bool,
    n_available_simple_decoders: u32,
    can_be_used_by_lsd: bool,
    no_macro_fusion: bool,
    lcp_stall: bool,
    instr_str: String,
    macro_fusible_with: Vec<String>,
    input_operands: Vec<String>,
    abstract_value_input_operands: Vec<String>,
    output_operands: Vec<String>,
    input_mem_operands: Vec<AnalyticalMemOperand>,
    mem_addr_operands: Vec<String>,
    mem_addr_latency_pairs: std::collections::BTreeSet<(String, String)>,
    latencies: BTreeMap<(String, String), i32>,
    implicit_rsp_change: i32,
    non_implicit_input_operands: std::collections::BTreeSet<String>,
    may_be_eliminated: bool,
    eliminated_move_input: Option<String>,
    eliminated_move_output_is_32_bit: bool,
    immediate: Option<i64>,
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

fn align_frontend_limits_with_simulated_sources(
    summary: &mut Summary,
    frontend: &crate::sim::FrontEnd,
) {
    let has_mite = frontend
        .all_generated_instr_instances
        .iter()
        .any(|instr| instr.source == Some(crate::sim::UopSource::Mite));
    let has_dsb = frontend
        .all_generated_instr_instances
        .iter()
        .any(|instr| instr.source == Some(crate::sim::UopSource::Dsb));
    let has_lsd = frontend
        .all_generated_instr_instances
        .iter()
        .any(|instr| instr.source == Some(crate::sim::UopSource::Lsd));

    if !has_mite {
        summary.limits.insert("predecoder".to_string(), None);
        summary.limits.insert("decoder".to_string(), None);
    }
    if !has_dsb {
        summary.limits.insert("dsb".to_string(), None);
    }
    if !has_lsd {
        summary.limits.insert("lsd".to_string(), None);
    }

    if let Some(throughput) = summary.throughput_cycles_per_iteration {
        summary.bottlenecks_predicted = predicted_bottlenecks(&summary.limits, throughput);
    }
}

fn refresh_summary_limits_from_python_bottlenecks(
    summary: &mut Summary,
    decoded: &[uica_decoder::DecodedInstruction],
    facts: &[LoopInstrFacts],
    arch: &MicroArchConfig,
    frontend: &crate::sim::FrontEnd,
    uops_for_round: &[UopsForRound],
) {
    // Python parity: `getBottlenecks` recomputes frontend/issue/port limits
    // from simulated instruction sources for both loop and unroll mode.
    let has_mite = frontend
        .all_generated_instr_instances
        .iter()
        .any(|instr| instr.source == Some(crate::sim::UopSource::Mite));
    let has_dsb = frontend
        .all_generated_instr_instances
        .iter()
        .any(|instr| instr.source == Some(crate::sim::UopSource::Dsb));
    let has_lsd = frontend
        .all_generated_instr_instances
        .iter()
        .any(|instr| instr.source == Some(crate::sim::UopSource::Lsd));

    let frontend_instrs = facts_to_analytical_instructions(facts);
    let frontend_limits =
        compute_frontend_limits(&frontend_instrs, arch, frontend.alignment_offset);
    let mut limits = empty_limits();

    if has_mite {
        if !has_dsb && !has_lsd {
            if let Some(predecoder) =
                compute_predecode_limit(decoded, facts, !frontend.unroll, frontend.alignment_offset)
            {
                limits.insert("predecoder".to_string(), Some(round2(predecoder)));
            }
        }
        if let Some(decoder) = frontend_limits.decoder {
            limits.insert("decoder".to_string(), Some(round2(decoder)));
        }
    }
    if has_dsb {
        if let Some(dsb) = frontend_limits.dsb {
            limits.insert("dsb".to_string(), Some(round2(dsb)));
        }
    }
    if has_lsd {
        if let Some(lsd) = frontend_limits.lsd {
            limits.insert("lsd".to_string(), Some(round2(lsd)));
        }
    }

    limits.insert(
        "issue".to_string(),
        Some(round2(compute_issue_limit(
            issue_retire_slots(facts, &frontend_instrs),
            arch.issue_width as i32,
        ))),
    );
    limits.insert(
        "ports".to_string(),
        Some(round2(compute_python_runtime_port_usage_limit(
            facts,
            &frontend_instrs,
            arch.name,
            frontend,
            uops_for_round,
        ))),
    );

    let latency_instrs = facts_to_latency_instructions(facts);
    let latency_graph = generate_latency_graph(
        &latency_instrs,
        arch.fast_pointer_chasing,
        &frontend.init_policy,
    );
    let max_latency_result = compute_maximum_latency_for_graph(&latency_graph);
    limits.insert(
        "dependencies".to_string(),
        Some(round2(max_latency_result.max_cycle_ratio)),
    );

    summary.limits = limits;
}

fn compute_python_runtime_port_usage_limit(
    facts: &[LoopInstrFacts],
    frontend_instrs: &[AnalyticalInstruction],
    arch_name: &str,
    frontend: &crate::sim::FrontEnd,
    uops_for_round: &[UopsForRound],
) -> f64 {
    // Python parity: `facile.computePortUsageLimit()` replaces every
    // mayBeEliminated instruction's static port-data uop count with average
    // non-eliminated unfused uops over relevant-round simulated
    // `InstrInstance.uops`.
    let mut port_usage: Vec<(BTreeSet<char>, f64)> = Vec::new();
    let relevant_window = python_relevant_round_window(frontend, uops_for_round);

    for (instr_id, (fact, frontend_instr)) in facts.iter().zip(frontend_instrs).enumerate() {
        if frontend_instr.macro_fused_with_prev {
            continue;
        }

        let mut port_data = fact.port_data.clone();
        if frontend_instr.macro_fused_with_next {
            let fused_ports = if arch_name == "ICL" { "06" } else { "6" };
            if let Some((old_ports, uops)) = port_data
                .iter()
                .find(|(ports, _)| fused_ports.chars().all(|p| ports.contains(p)))
                .map(|(ports, uops)| (ports.clone(), *uops))
            {
                port_data.remove(&old_ports);
                port_data.insert(fused_ports.to_string(), uops);
            }
        }

        let eliminated_avg = if fact.may_be_eliminated {
            let instr_instances: Vec<_> = frontend
                .all_generated_instr_instances
                .iter()
                .filter(|instr_i| {
                    instr_i.instr_id as usize == instr_id
                        && relevant_window.is_none_or(|(first_round, last_round)| {
                            instr_i.rnd >= first_round && instr_i.rnd <= last_round
                        })
                })
                .collect();
            if instr_instances.is_empty() {
                continue;
            }
            Some(
                instr_instances
                    .iter()
                    .map(|instr_i| {
                        instr_i
                            .laminated_uops
                            .iter()
                            .filter_map(|lam_idx| frontend.uop_storage.get_laminated_uop(*lam_idx))
                            .flat_map(|lam| lam.fused_uop_idxs.iter())
                            .filter_map(|fused_idx| frontend.uop_storage.get_fused_uop(*fused_idx))
                            .flat_map(|fused| fused.unfused_uop_idxs.iter())
                            .filter_map(|uop_idx| frontend.uop_storage.get_uop(*uop_idx))
                            .filter(|uop| !uop.eliminated)
                            .count() as f64
                    })
                    .sum::<f64>()
                    / instr_instances.len() as f64,
            )
        } else {
            None
        };

        for (ports, static_uops) in port_data {
            let port_set: BTreeSet<char> = ports.chars().filter(|ch| !ch.is_whitespace()).collect();
            if port_set.is_empty() {
                continue;
            }
            let n_uops = eliminated_avg.unwrap_or(static_uops as f64);
            if let Some((_, total)) = port_usage.iter_mut().find(|(set, _)| *set == port_set) {
                *total += n_uops;
            } else {
                port_usage.push((port_set, n_uops));
            }
        }
    }

    let mut limit: f64 = 0.0;
    for (left_set, _) in &port_usage {
        for (right_set, _) in &port_usage {
            let candidate: BTreeSet<char> = left_set.union(right_set).copied().collect();
            if candidate.is_empty() {
                continue;
            }
            let total_uops: f64 = port_usage
                .iter()
                .filter(|(set, _)| set.is_subset(&candidate))
                .map(|(_, n_uops)| *n_uops)
                .sum();
            limit = limit.max(total_uops / candidate.len() as f64);
        }
    }
    limit
}

fn apply_python_high8_latency_adjustments(
    facts: &mut [LoopInstrFacts],
    decoded: &[uica_decoder::DecodedInstruction],
    arch: &MicroArchConfig,
) {
    if !arch.high8_renamed_separately {
        return;
    }

    // Python parity: `uiCA.py adjustLatenciesAndAddMergeUops()` first walks
    // the whole instruction list with `processInstrRegOutputs()`, then walks
    // it again, adding one cycle to clean high-8 input latencies before
    // updating clean/dirty state for that instruction.
    let mut high8_reg_clean = BTreeMap::from([
        ("RAX".to_string(), true),
        ("RBX".to_string(), true),
        ("RCX".to_string(), true),
        ("RDX".to_string(), true),
    ]);

    for (fact, instr) in facts.iter().zip(decoded) {
        process_python_high8_clean_outputs(fact, instr, &mut high8_reg_clean);
    }

    for (fact, instr) in facts.iter_mut().zip(decoded) {
        let penalized_regs: BTreeSet<String> = instr
            .input_regs
            .iter()
            .filter(|reg| crate::x64::is_high8_reg(reg))
            .map(|reg| crate::x64::get_canonical_reg(reg))
            .filter(|canonical| fact.input_operands.contains(canonical))
            .filter(|canonical| high8_reg_clean.get(canonical).copied().unwrap_or(true))
            .collect();
        if !penalized_regs.is_empty() {
            for ((input, _output), latency) in fact.latencies.iter_mut() {
                if penalized_regs.contains(input) {
                    *latency += 1;
                }
            }
        }
        process_python_high8_clean_outputs(fact, instr, &mut high8_reg_clean);
    }
}

fn process_python_high8_clean_outputs(
    fact: &LoopInstrFacts,
    instr: &uica_decoder::DecodedInstruction,
    high8_reg_clean: &mut BTreeMap<String, bool>,
) {
    for reg in &instr.input_regs {
        let canonical = crate::x64::get_canonical_reg(reg);
        if fact.input_operands.contains(&canonical)
            && matches!(canonical.as_str(), "RAX" | "RBX" | "RCX" | "RDX")
            && crate::x64::get_reg_size(reg) > 8
        {
            high8_reg_clean.insert(canonical, true);
        }
    }
    for mem_addr in &instr.mem_addrs {
        for reg in [&mem_addr.base, &mem_addr.index].into_iter().flatten() {
            let canonical = crate::x64::get_canonical_reg(reg);
            if fact.mem_addr_operands.contains(&canonical)
                && matches!(canonical.as_str(), "RAX" | "RBX" | "RCX" | "RDX")
                && crate::x64::get_reg_size(reg) > 8
            {
                high8_reg_clean.insert(canonical, true);
            }
        }
    }
    for reg in &instr.output_regs {
        let canonical = crate::x64::get_canonical_reg(reg);
        if !fact.output_operands.contains(&canonical) {
            continue;
        }
        if matches!(canonical.as_str(), "RAX" | "RBX" | "RCX" | "RDX")
            && crate::x64::get_reg_size(reg) > 8
        {
            high8_reg_clean.insert(canonical, true);
        } else if crate::x64::is_high8_reg(reg) {
            high8_reg_clean.insert(canonical, false);
        }
    }
}

fn compute_predecode_limit(
    decoded: &[uica_decoder::DecodedInstruction],
    facts: &[LoopInstrFacts],
    loop_mode: bool,
    alignment_offset: u32,
) -> Option<f64> {
    if decoded.is_empty() {
        return None;
    }

    let code_length: u32 = decoded.iter().map(|instr| instr.len).sum();
    if code_length == 0 {
        return None;
    }
    let unroll = if loop_mode {
        1
    } else {
        16 / gcd_u32(code_length, 16)
    };
    let n_b16_blocks = (unroll * code_length).div_ceil(16) as usize;
    let mut last_byte_in_block = vec![0u32; n_b16_blocks];
    let mut opcode_crosses_block = vec![0u32; n_b16_blocks];
    let mut lcp_in_block = vec![0u32; n_b16_blocks];

    let alignment = (alignment_offset % 16) as i64;
    let mut cur_addr = if alignment == 0 { 0 } else { -16 + alignment };
    let stop_addr = (unroll * code_length) as i64;
    for (idx, instr) in decoded.iter().cycle().enumerate() {
        if cur_addr >= stop_addr {
            break;
        }
        let instr_len = instr.len as i64;
        let next_addr = cur_addr + instr_len;
        let end_block = (next_addr - 1).div_euclid(16);
        let nominal_opcode_block = (cur_addr + instr.pos_nominal_opcode as i64).div_euclid(16);
        cur_addr = next_addr;

        if (0..n_b16_blocks as i64).contains(&end_block) {
            last_byte_in_block[end_block as usize] += 1;
        }
        if (0..n_b16_blocks as i64).contains(&nominal_opcode_block) {
            let block = nominal_opcode_block as usize;
            if nominal_opcode_block != end_block {
                opcode_crosses_block[block] += 1;
            }
            if facts
                .get(idx % decoded.len())
                .is_some_and(|fact| fact.lcp_stall)
            {
                lcp_in_block[block] += 1;
            }
        }
    }

    let mut cycles = 0u32;
    for block in 0..n_b16_blocks {
        cycles += (last_byte_in_block[block] + opcode_crosses_block[block]).div_ceil(5);
        let prev = if block == 0 {
            n_b16_blocks - 1
        } else {
            block - 1
        };
        let prev_cycles = (last_byte_in_block[prev] + opcode_crosses_block[prev]).div_ceil(5);
        cycles += (3 * lcp_in_block[block]).saturating_sub(prev_cycles.saturating_sub(1));
    }
    Some(cycles as f64 / unroll as f64)
}

fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn compute_loop_model(
    arch: &MicroArchConfig,
    facts: &[LoopInstrFacts],
    _signature: &str,
    cycles: u32,
    min_iterations: u32,
    init_policy: &str,
) -> Option<LoopModel> {
    if facts.is_empty() {
        return None;
    }

    let frontend_instrs = facts_to_analytical_instructions(facts);
    let issue_retire_slots = issue_retire_slots(facts, &frontend_instrs);
    let issue_limit = round2(compute_issue_limit(
        issue_retire_slots,
        arch.issue_width as i32,
    ));
    let ports_limit = round2(compute_port_usage_limit(&facts_to_port_usage_inputs(
        facts,
        &frontend_instrs,
        arch.name,
    )));

    let frontend_limits = compute_frontend_limits(&frontend_instrs, arch, 0);

    let latency_instrs = facts_to_latency_instructions(facts);
    let latency_graph =
        generate_latency_graph(&latency_instrs, arch.fast_pointer_chasing, init_policy);
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LatencyInputKind {
    Normal,
    MemData,
    MemAddrBase,
    MemAddrIndex,
    AgenBase,
    AgenIndex,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct MappedInputOperand {
    value: String,
    kind: LatencyInputKind,
    is_implicit_stack_operand: bool,
}

fn mapped_mem_key(mem_addr: &uica_decoder::DecodedMemAddr) -> String {
    format!(
        "MEM:{}:{}:{}:{}",
        mem_addr
            .base
            .as_deref()
            .map(crate::x64::get_canonical_reg)
            .unwrap_or_default(),
        mem_addr
            .index
            .as_deref()
            .map(crate::x64::get_canonical_reg)
            .unwrap_or_default(),
        mem_addr.scale,
        mem_addr.disp
    )
}

fn push_input(
    input_map: &mut BTreeMap<String, Vec<MappedInputOperand>>,
    name: &str,
    value: String,
    kind: LatencyInputKind,
    is_implicit_stack_operand: bool,
) {
    input_map
        .entry(name.to_string())
        .or_default()
        .push(MappedInputOperand {
            value,
            kind,
            is_implicit_stack_operand,
        });
}

fn mapped_record_operands(
    record: &uica_data::InstructionRecord,
    decoded: &uica_decoder::DecodedInstruction,
) -> (
    BTreeMap<String, Vec<MappedInputOperand>>,
    BTreeMap<String, Vec<String>>,
) {
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
    let explicit_regs: Vec<String> = decoded
        .explicit_reg_operands
        .iter()
        .map(|reg| crate::x64::get_canonical_reg(reg))
        .collect();
    let mut read_reg_idx = 0usize;
    let mut write_reg_idx = 0usize;
    let mut explicit_reg_idx = 0usize;
    let mut mem_operand_idx = 0usize;
    let mut input_map: BTreeMap<String, Vec<MappedInputOperand>> = BTreeMap::new();
    let mut output_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for operand in &record.perf.operands {
        match operand.r#type.as_str() {
            "reg" => {
                let explicit_reg = if operand.implicit {
                    None
                } else {
                    let reg = explicit_regs.get(explicit_reg_idx).cloned();
                    explicit_reg_idx += 1;
                    reg
                };
                let read_by_latency = record
                    .perf
                    .latencies
                    .iter()
                    .any(|latency| latency.start_op == operand.name);
                if operand.read || read_by_latency {
                    // Python parity: `instructions.py` maps register operands
                    // by XML/XED operand name. XED's aggregate `input_regs` is
                    // unique and can collapse same-register operands (e.g.
                    // CVTSD2SS XMM2,XMM2); use explicit operand order for
                    // non-implicit operands so latency rows starting at REG1
                    // still become `instrInputRegOperands`.
                    if operand.read {
                        if let Some(reg) = explicit_reg
                            .as_ref()
                            .or_else(|| input_regs.get(read_reg_idx))
                        {
                            push_input(
                                &mut input_map,
                                &operand.name,
                                reg.clone(),
                                LatencyInputKind::Normal,
                                false,
                            );
                        }
                        read_reg_idx += 1;
                    } else if operand.write {
                        if let Some(reg) = explicit_reg
                            .as_ref()
                            .or_else(|| output_regs.get(write_reg_idx))
                        {
                            push_input(
                                &mut input_map,
                                &operand.name,
                                reg.clone(),
                                LatencyInputKind::Normal,
                                false,
                            );
                        }
                        if input_regs
                            .get(read_reg_idx)
                            .is_some_and(|reg| output_regs.get(write_reg_idx) == Some(reg))
                        {
                            read_reg_idx += 1;
                        }
                    } else if let Some(reg) = input_regs.get(read_reg_idx) {
                        push_input(
                            &mut input_map,
                            &operand.name,
                            reg.clone(),
                            LatencyInputKind::Normal,
                            false,
                        );
                        read_reg_idx += 1;
                    }
                }
                if operand.write {
                    if let Some(reg) = explicit_reg
                        .as_ref()
                        .or_else(|| output_regs.get(write_reg_idx))
                    {
                        output_map
                            .entry(operand.name.clone())
                            .or_default()
                            .push(reg.clone());
                    }
                    write_reg_idx += 1;
                }
            }
            "flags" => {
                let read_flags = operand.flags_read.clone();
                let write_flags = operand.flags_write.clone();
                // Python parity: map only `flagsR` to inputFlagOperands and
                // only `flagsW` to outputFlagOperands. Do not turn read-only
                // flags into writes or write-only flags into reads.
                if !read_flags.is_empty() {
                    for flag in read_flags.into_iter().filter(|f| !f.is_empty()) {
                        push_input(
                            &mut input_map,
                            &operand.name,
                            flag,
                            LatencyInputKind::Normal,
                            false,
                        );
                    }
                }
                if !write_flags.is_empty() {
                    output_map
                        .entry(operand.name.clone())
                        .or_default()
                        .extend(write_flags.into_iter().filter(|f| !f.is_empty()));
                }
            }
            "mem" | "agen" => {
                let role = operand.mem_operand_role.as_deref();
                let mem_addr = decoded.mem_addrs.get(mem_operand_idx);
                if operand.r#type == "mem"
                    && (operand.read || matches!(role, Some("read" | "read_write")))
                {
                    let mem = mem_addr
                        .map(mapped_mem_key)
                        .unwrap_or_else(|| mem_key(decoded));
                    push_input(
                        &mut input_map,
                        &operand.name,
                        mem,
                        LatencyInputKind::MemData,
                        false,
                    );
                }
                if operand.r#type == "mem"
                    && (operand.write || matches!(role, Some("write" | "read_write")))
                {
                    let mem = mem_addr
                        .map(mapped_mem_key)
                        .unwrap_or_else(|| mem_key(decoded));
                    output_map
                        .entry(operand.name.clone())
                        .or_default()
                        .push(mem);
                }
                if operand.r#type == "mem"
                    || operand.r#type == "agen"
                    || operand.is_agen
                    || matches!(role, Some("agen" | "address"))
                {
                    // Python parity: `instructions.py` adds base/index registers
                    // for each memory operand to `Instr.memAddrOperands` (or
                    // input regs for AGEN), and `facile.generateLatencyGraph()`
                    // uses those address operands as dependency-graph inputs.
                    if let Some(mem_addr) = mem_addr {
                        let base_kind = if operand.r#type == "agen" || operand.is_agen {
                            LatencyInputKind::AgenBase
                        } else {
                            LatencyInputKind::MemAddrBase
                        };
                        let index_kind = if operand.r#type == "agen" || operand.is_agen {
                            LatencyInputKind::AgenIndex
                        } else {
                            LatencyInputKind::MemAddrIndex
                        };
                        if let Some(base) = &mem_addr.base {
                            push_input(
                                &mut input_map,
                                &operand.name,
                                crate::x64::get_canonical_reg(base),
                                base_kind,
                                mem_addr.is_implicit_stack_operand,
                            );
                        }
                        if let Some(index) = &mem_addr.index {
                            push_input(
                                &mut input_map,
                                &operand.name,
                                crate::x64::get_canonical_reg(index),
                                index_kind,
                                mem_addr.is_implicit_stack_operand,
                            );
                        }
                    }
                }
                mem_operand_idx += 1;
            }
            _ => {}
        }
    }

    for values in input_map.values_mut() {
        values.sort();
        values.dedup();
    }
    for values in output_map.values_mut() {
        values.sort();
        values.dedup();
    }
    (input_map, output_map)
}

fn flatten_input_operand_map(map: &BTreeMap<String, Vec<MappedInputOperand>>) -> Vec<String> {
    let mut values: Vec<String> = map
        .values()
        .flatten()
        .map(|operand| operand.value.clone())
        .collect();
    values.sort();
    values.dedup();
    values
}

fn abstract_value_input_operands(map: &BTreeMap<String, Vec<MappedInputOperand>>) -> Vec<String> {
    let mut values: Vec<String> = map
        .values()
        .flatten()
        .filter(|operand| {
            matches!(
                operand.kind,
                LatencyInputKind::Normal | LatencyInputKind::AgenBase | LatencyInputKind::AgenIndex
            )
        })
        .map(|operand| operand.value.clone())
        .collect();
    values.sort();
    values.dedup();
    values
}

fn non_implicit_input_operands(
    map: &BTreeMap<String, Vec<MappedInputOperand>>,
) -> std::collections::BTreeSet<String> {
    map.values()
        .flatten()
        .filter(|operand| {
            !operand.is_implicit_stack_operand && operand.kind != LatencyInputKind::MemData
        })
        .map(|operand| operand.value.clone())
        .collect()
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
    arch_name: &str,
    use_same_reg_latencies: bool,
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
                let key = (input.value.clone(), output.clone());
                let cycles = record_latency_cycles_for_decoded_input(
                    record,
                    latency,
                    arch_name,
                    &input.kind,
                    use_same_reg_latencies,
                );
                latencies
                    .entry(key)
                    .and_modify(|old: &mut i32| *old = (*old).max(cycles))
                    .or_insert(cycles);
            }
        }
    }
    latencies
}

fn map_record_mem_addr_latency_pairs(
    record: &uica_data::InstructionRecord,
    decoded: &uica_decoder::DecodedInstruction,
    _arch_name: &str,
    _use_same_reg_latencies: bool,
) -> std::collections::BTreeSet<(String, String)> {
    let (input_map, output_map) = mapped_record_operands(record, decoded);
    let mut pairs = std::collections::BTreeSet::new();
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
            if !matches!(
                input.kind,
                LatencyInputKind::MemAddrBase | LatencyInputKind::MemAddrIndex
            ) {
                continue;
            }
            for output in outputs {
                pairs.insert((input.value.clone(), output.clone()));
            }
        }
    }
    pairs
}

fn record_latency_cycles_for_decoded_input(
    record: &uica_data::InstructionRecord,
    latency: &uica_data::LatencyRecord,
    arch_name: &str,
    input_kind: &LatencyInputKind,
    use_same_reg_latencies: bool,
) -> i32 {
    if use_same_reg_latencies {
        if let Some(sr) = latency.cycles_same_reg {
            return sr;
        }
    }

    match input_kind {
        LatencyInputKind::MemAddrIndex => latency.cycles_addr_index.unwrap_or(1),
        LatencyInputKind::MemAddrBase => latency.cycles_addr.unwrap_or(1),
        LatencyInputKind::MemData => latency.cycles_mem.unwrap_or(latency.cycles),
        LatencyInputKind::AgenIndex => crate::sim::uop_expand::record_latency_cycles_for_start(
            record,
            latency,
            arch_name,
            "__AGEN_ADDRI",
        ),
        LatencyInputKind::AgenBase => crate::sim::uop_expand::record_latency_cycles_for_start(
            record,
            latency,
            arch_name,
            "__AGEN_ADDR",
        ),
        LatencyInputKind::Normal => latency.cycles,
    }
}

fn facts_to_port_usage_inputs(
    facts: &[LoopInstrFacts],
    frontend_instrs: &[AnalyticalInstruction],
    arch_name: &str,
) -> Vec<InstructionPortUsage> {
    facts
        .iter()
        .zip(frontend_instrs)
        .filter(|(_, instr)| !instr.macro_fused_with_prev)
        .map(|(fact, instr)| {
            let mut port_data = fact.port_data.clone();
            if instr.macro_fused_with_next {
                let fused_ports = if arch_name == "ICL" { "06" } else { "6" };
                if let Some((old_ports, uops)) = port_data
                    .iter()
                    .find(|(ports, _)| fused_ports.chars().all(|p| ports.contains(p)))
                    .map(|(ports, uops)| (ports.clone(), *uops))
                {
                    port_data.remove(&old_ports);
                    port_data.insert(fused_ports.to_string(), uops);
                }
            }
            InstructionPortUsage {
                port_data,
                uops: fact.uops.max(0),
            }
        })
        .collect()
}

fn facts_to_latency_instructions(facts: &[LoopInstrFacts]) -> Vec<AnalyticalLatencyInstruction> {
    facts
        .iter()
        .filter(|fact| !is_conditional_jump(&canonical_loop_mnemonic(&fact.mnemonic)))
        .map(|fact| AnalyticalLatencyInstruction {
            instr_str: fact.instr_str.clone(),
            uops: fact.uops,
            input_operands: fact.input_operands.clone(),
            abstract_value_input_operands: fact.abstract_value_input_operands.clone(),
            output_operands: fact.output_operands.clone(),
            input_mem_operands: fact.input_mem_operands.clone(),
            mem_addr_operands: fact.mem_addr_operands.clone(),
            mem_addr_latency_pairs: fact.mem_addr_latency_pairs.clone(),
            latencies: fact.latencies.clone(),
            implicit_rsp_change: fact.implicit_rsp_change,
            non_implicit_input_operands: fact.non_implicit_input_operands.clone(),
            may_be_eliminated: fact.may_be_eliminated,
            eliminated_move_input: fact.eliminated_move_input.clone(),
            eliminated_move_output_is_32_bit: fact.eliminated_move_output_is_32_bit,
            immediate: fact.immediate,
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
                macro_fusible_with_next: !fact.macro_fusible_with.is_empty(),
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
        if out[idx].is_branch
            && !facts[idx].no_macro_fusion
            && !facts[idx - 1].no_macro_fusion
            && facts[idx - 1]
                .macro_fusible_with
                .iter()
                .any(|instr_str| instr_str == &facts[idx].instr_str)
        {
            out[idx].macro_fused_with_prev = true;
            out[idx - 1].macro_fused_with_next = true;
        }
    }

    out
}

fn issue_retire_slots(facts: &[LoopInstrFacts], instrs: &[AnalyticalInstruction]) -> i32 {
    facts
        .iter()
        .zip(instrs.iter())
        .filter(|(_, instr)| !instr.macro_fused_with_prev)
        .map(|(fact, _)| fact.retire_slots.max(0))
        .sum()
}

fn canonical_loop_mnemonic(mnemonic: &str) -> String {
    let lower = mnemonic.to_ascii_lowercase();
    match lower.as_str() {
        "je" => "jz".to_string(),
        "jne" => "jnz".to_string(),
        _ => lower,
    }
}

fn is_conditional_jump(mnemonic: &str) -> bool {
    mnemonic.starts_with('j') && mnemonic != "jmp"
}

fn is_decoded_zero_idiom(decoded: &uica_decoder::DecodedInstruction) -> bool {
    matches!(
        decoded.mnemonic.to_ascii_lowercase().as_str(),
        "xor" | "sub" | "pxor" | "vxorps" | "vxorpd" | "vpxor"
    ) && !decoded.has_memory_read
        && !decoded.has_memory_write
        && decoded.input_regs.is_empty()
}

fn round2(v: f64) -> f64 {
    // Python parity: uiCA.py reports summary limits and simulated TP via
    // `round(value, 2)`, which uses ties-to-even rather than Rust's
    // half-away-from-zero `f64::round`. BHive raw blocks can hit exact .005
    // ties (e.g. predecoder 1.125 -> 1.12), so mirror Python rounding here.
    let scaled = v * 100.0;
    let lower = scaled.floor();
    let frac = scaled - lower;
    let rounded = if (frac - 0.5).abs() <= f64::EPSILON {
        if (lower as i64).rem_euclid(2) == 0 {
            lower
        } else {
            lower + 1.0
        }
    } else {
        scaled.round()
    };
    rounded / 100.0
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
            "je" => "jz".to_string(),
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

fn load_default_runtime(arch: &str, verify_uipack: bool) -> Option<MappedUiPackRuntime> {
    if let Ok(path) = env::var("UICA_RUST_DATAPACK") {
        if let Some(runtime) = load_runtime_source(Path::new(&path), arch, verify_uipack) {
            return Some(runtime);
        }
    }

    default_generated_dirs()
        .into_iter()
        .find_map(|generated_dir| load_runtime_source(&generated_dir, arch, verify_uipack))
}

fn load_default_runtime_verified(arch: &str) -> Result<MappedUiPackRuntime, String> {
    if let Ok(path) = env::var("UICA_RUST_DATAPACK") {
        return load_runtime_source_verified(Path::new(&path), arch);
    }

    for generated_dir in default_generated_dirs() {
        if let Ok(runtime) = load_runtime_source_verified(&generated_dir, arch) {
            return Ok(runtime);
        }
    }

    Err(format!("uipack data not found for {arch}"))
}

fn default_generated_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("rust/uica-data/generated"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../uica-data/generated"),
    ]
}

fn load_runtime_source(
    path: &Path,
    arch: &str,
    verify_uipack: bool,
) -> Option<MappedUiPackRuntime> {
    if let Some(manifest_path) = runtime_manifest_path(path) {
        if manifest_path.exists() {
            let loaded = if verify_uipack {
                load_manifest_runtime_verified(&manifest_path, arch)
            } else {
                load_manifest_runtime(&manifest_path, arch)
            };
            if let Ok(runtime) = loaded {
                return Some(runtime);
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
        return if verify_uipack {
            MappedUiPackRuntime::open_verified(path).ok()
        } else {
            MappedUiPackRuntime::open(path).ok()
        };
    }

    None
}

fn load_runtime_source_verified(path: &Path, arch: &str) -> Result<MappedUiPackRuntime, String> {
    if let Some(manifest_path) = runtime_manifest_path(path) {
        if manifest_path.exists() {
            return load_manifest_runtime_verified(&manifest_path, arch)
                .map_err(|err| err.to_string());
        }
    }

    if path.is_file()
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("uipack"))
    {
        return MappedUiPackRuntime::open_verified(path).map_err(|err| err.to_string());
    }

    Err(format!("uipack data not found for {arch}"))
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
    verify_uipack: bool,
) -> Result<crate::sim::TraceWriter, String> {
    let runtime = if verify_uipack {
        load_default_runtime_verified(&invocation.arch)?
    } else {
        load_default_runtime(&invocation.arch, false)
            .ok_or_else(|| format!("uipack data not found for {}", invocation.arch))?
    };
    let pack = materialize_runtime_pack_for_code(code, &runtime)?;
    let index = DataPackIndex::new(&pack);
    let (frontend, _, max_cycle) = run_simulation_for_cycles(code, invocation, &pack, &index)?;

    // Emit trace events from simulator state, mirroring Python's generateEventTrace.
    emit_trace_from_frontend(&frontend, max_cycle)
}

fn python_lam_uop_order(instr_i: &crate::sim::types::InstrInstance) -> Vec<u64> {
    // Python parity: `generateJSONOutput()` and `generateEventTrace()` enumerate
    // `instrI.regMergeUops + instrI.stackSyncUops + instrI.uops`. Merge/sync
    // uops are injected later than regular uops, so lam_idx creation order is
    // not Python's lamUopID order.
    instr_i
        .reg_merge_uops
        .iter()
        .chain(instr_i.stack_sync_uops.iter())
        .chain(instr_i.laminated_uops.iter())
        .copied()
        .collect()
}

fn emit_trace_from_frontend(
    frontend: &crate::sim::FrontEnd,
    max_cycle: u32,
) -> Result<crate::sim::TraceWriter, String> {
    use crate::sim::trace::{EventKind, TraceEvent, TraceWriter};

    let mut lam_meta: std::collections::HashMap<u64, (u32, u32, i64)> =
        std::collections::HashMap::new();
    for instr_i in &frontend.all_generated_instr_instances {
        for (lam_i, lam_idx) in python_lam_uop_order(instr_i).into_iter().enumerate() {
            lam_meta.insert(lam_idx, (instr_i.instr_id, instr_i.rnd, lam_i as i64));
        }
    }

    // Stable traversal only; lam IDs come from Python-shaped lam_meta above.
    let mut lam_idxs_sorted: Vec<u64> = lam_meta.keys().copied().collect();
    lam_idxs_sorted.sort_unstable();

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
        let Some(&(instr_id, rnd, lam_pos)) = lam_meta.get(&li) else {
            continue;
        };
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
        let Some(&(instr_id, rnd, lam_pos)) = lam_meta.get(&li) else {
            continue;
        };

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

        let lam_uop_indices = python_lam_uop_order(instr_i);

        for (lam_uop_i, &lam_idx) in lam_uop_indices.iter().enumerate() {
            let lam_uop = frontend.uop_storage.get_laminated_uop(lam_idx).unwrap();

            let mut base_uop_dict = json!({
                "rnd": rnd,
                "instrID": instr_id,
                "lamUopID": lam_uop_i,
            });
            if instr_i.reg_merge_uops.contains(&lam_idx) {
                base_uop_dict
                    .as_object_mut()
                    .unwrap()
                    .insert("regMergeUop".to_string(), json!(true));
            }
            if instr_i.stack_sync_uops.contains(&lam_idx) {
                base_uop_dict
                    .as_object_mut()
                    .unwrap()
                    .insert("stackSyncUop".to_string(), json!(true));
            }

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

#[derive(Default)]
struct UopsForRound {
    // Python: `uopsForRound[rnd][instr].append(fusedUop)`.
    instrs: BTreeMap<u32, Vec<u64>>,
}

/// Run the simulator and return the frontend, Python-style uopsForRound, and
/// final clock for cycles JSON extraction.
fn run_simulation_for_cycles<'a>(
    code: &[u8],
    invocation: &Invocation,
    pack: &'a DataPack,
    pack_index: &'a DataPackIndex<'a>,
) -> Result<(crate::sim::FrontEnd<'a>, Vec<UopsForRound>, u32), String> {
    use crate::sim::types::build_instruction_instances;
    use crate::sim::FrontEnd;

    let arch = get_micro_arch(&invocation.arch)
        .ok_or_else(|| format!("unknown architecture: {}", invocation.arch))?;

    let decoded = decode_raw(code).map_err(|e| format!("decode error: {e}"))?;

    let base_instances = build_instruction_instances(&decoded, invocation.alignment_offset);

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
        pack_index,
        invocation.init_policy.clone(),
        invocation.simple_front_end,
        invocation.no_micro_fusion,
        invocation.no_macro_fusion,
    );

    // Mirror Python runSimulation loop:
    //   while True: frontEnd.cycle(clock); handle retirement;
    //   if rnd >= minIterations and clock > minCycles: break; clock += 1;
    let mut clock = 0u32;
    let mut rnd = 0u32;
    let mut uops_for_round: Vec<UopsForRound> = Vec::new();
    loop {
        frontend.cycle(clock);
        if let Some(last_retired_round) = drain_retire_queue(&mut frontend, &mut uops_for_round) {
            rnd = last_retired_round;
        }
        if rnd >= min_iterations && clock > min_cycles {
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
    Ok((frontend, uops_for_round, max_cycle))
}

fn drain_retire_queue(
    frontend: &mut crate::sim::FrontEnd,
    uops_for_round: &mut Vec<UopsForRound>,
) -> Option<u32> {
    let mut last_retired_round = None;
    while let Some(fused_idx) = frontend.reorder_buffer.retire_queue.pop_front() {
        let Some((instr_id, rnd)) = retired_fused_instr_and_round(frontend, fused_idx) else {
            continue;
        };
        while rnd as usize >= uops_for_round.len() {
            uops_for_round.push(UopsForRound::default());
        }
        uops_for_round[rnd as usize]
            .instrs
            .entry(instr_id)
            .or_default()
            .push(fused_idx);
        last_retired_round = Some(rnd);
    }
    last_retired_round
}

fn retired_fused_instr_and_round(
    frontend: &crate::sim::FrontEnd,
    fused_idx: u64,
) -> Option<(u32, u32)> {
    let fused = frontend.uop_storage.get_fused_uop(fused_idx)?;
    let first_unfused_idx = *fused.unfused_uop_idxs.first()?;
    let uop = frontend.uop_storage.get_uop(first_unfused_idx)?;
    let instr_i = frontend
        .all_generated_instr_instances
        .iter()
        .find(|instr_i| instr_i.idx == uop.instr_instance_idx)?;
    Some((instr_i.instr_id, instr_i.rnd))
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
            map.insert("asm".to_string(), json!(inst.disasm.as_ref()));
            map.insert("opcode".to_string(), json!(inst.opcode_hex.as_ref()));
            map.insert("url".to_string(), json!(instr_url(inst.disasm.as_ref())));
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

fn compute_simulated_throughput(
    frontend: &crate::sim::FrontEnd,
    uops_for_round: &[UopsForRound],
) -> Option<f64> {
    let (first_relevant_round, last_relevant_round) =
        python_relevant_round_window(frontend, uops_for_round)?;
    let last_applicable_instr_id = python_last_applicable_instr_id(frontend)?;

    let (first_retired, _) = last_retired_fused_for_round(
        frontend,
        uops_for_round,
        last_applicable_instr_id,
        first_relevant_round,
    )?;
    if first_relevant_round == last_relevant_round {
        return Some(first_retired as f64);
    }

    let (last_retired, _) = last_retired_fused_for_round(
        frontend,
        uops_for_round,
        last_applicable_instr_id,
        last_relevant_round,
    )?;
    Some(round2(
        (last_retired.saturating_sub(first_retired) as f64)
            / (last_relevant_round - first_relevant_round) as f64,
    ))
}

fn python_relevant_round_window(
    frontend: &crate::sim::FrontEnd,
    uops_for_round: &[UopsForRound],
) -> Option<(u32, u32)> {
    let last_applicable_instr_id = python_last_applicable_instr_id(frontend)?;
    let mut last_relevant_round = (uops_for_round.len() as u32).saturating_sub(2);
    let first_relevant_round = last_relevant_round.min(uops_for_round.len() as u32 / 2);

    // Python parity: `uopsForRound[firstRelevantRound][lastApplicableInstr][-1]`
    // and same for candidate final rounds.
    if last_relevant_round.saturating_sub(first_relevant_round) > 10 {
        let first_retire_idx = last_retired_fused_for_round(
            frontend,
            uops_for_round,
            last_applicable_instr_id,
            first_relevant_round,
        )?
        .1;
        for rnd in (last_relevant_round.saturating_sub(4)..=last_relevant_round).rev() {
            let Some((_, retire_idx)) = last_retired_fused_for_round(
                frontend,
                uops_for_round,
                last_applicable_instr_id,
                rnd,
            ) else {
                continue;
            };
            if retire_idx == first_retire_idx {
                last_relevant_round = rnd;
                break;
            }
        }
    }

    Some((first_relevant_round, last_relevant_round))
}

fn append_python_runtime_bottlenecks(
    summary: &mut Summary,
    frontend: &crate::sim::FrontEnd,
    uops_for_round: &[UopsForRound],
) -> Option<()> {
    let throughput = summary.throughput_cycles_per_iteration?;
    let (first_round, last_round) = python_relevant_round_window(frontend, uops_for_round)?;
    let n_rounds = (last_round - first_round + 1) as f64;

    // Python parity: `relevantInstrInstancesForInstr` contains instruction
    // instances whose `instrI.rnd` falls in the relevant `uopsForRound` window.
    let relevant_instr_instances: Vec<&crate::sim::types::InstrInstance> = frontend
        .all_generated_instr_instances
        .iter()
        .filter(|instr_i| instr_i.rnd >= first_round && instr_i.rnd <= last_round)
        .collect();

    if summary
        .limits
        .get("ports")
        .and_then(|limit| *limit)
        .is_some_and(|limit| limit < 0.98 * throughput)
    {
        let mut port_usage: BTreeMap<String, u32> = BTreeMap::new();
        for instr_i in &relevant_instr_instances {
            for &lam_idx in instr_i
                .reg_merge_uops
                .iter()
                .chain(instr_i.stack_sync_uops.iter())
                .chain(instr_i.laminated_uops.iter())
            {
                let Some(lam) = frontend.uop_storage.get_laminated_uop(lam_idx) else {
                    continue;
                };
                for &fused_idx in &lam.fused_uop_idxs {
                    let Some(fused) = frontend.uop_storage.get_fused_uop(fused_idx) else {
                        continue;
                    };
                    for &uop_idx in &fused.unfused_uop_idxs {
                        let Some(uop) = frontend.uop_storage.get_uop(uop_idx) else {
                            continue;
                        };
                        if let Some(port) = &uop.actual_port {
                            *port_usage.entry(port.clone()).or_default() += 1;
                        }
                    }
                }
            }
        }
        if port_usage
            .values()
            .max()
            .is_some_and(|max_port_usage| (*max_port_usage as f64) / n_rounds >= 0.98 * throughput)
            && !summary
                .bottlenecks_predicted
                .iter()
                .any(|bottleneck| bottleneck == "Scheduling")
        {
            summary.bottlenecks_predicted.push("Scheduling".to_string());
        }
    }

    let div_usage: u32 = relevant_instr_instances
        .iter()
        .flat_map(|instr_i| {
            instr_i
                .laminated_uops
                .iter()
                .chain(instr_i.reg_merge_uops.iter())
                .chain(instr_i.stack_sync_uops.iter())
        })
        .filter_map(|lam_idx| frontend.uop_storage.get_laminated_uop(*lam_idx))
        .flat_map(|lam| lam.fused_uop_idxs.iter())
        .filter_map(|fused_idx| frontend.uop_storage.get_fused_uop(*fused_idx))
        .flat_map(|fused| fused.unfused_uop_idxs.iter())
        .filter_map(|uop_idx| frontend.uop_storage.get_uop(*uop_idx))
        .map(|uop| uop.prop.div_cycles)
        .sum();
    if (div_usage as f64) / n_rounds >= 0.99 * throughput
        && !summary
            .bottlenecks_predicted
            .iter()
            .any(|bottleneck| bottleneck == "Divider")
    {
        summary.bottlenecks_predicted.push("Divider".to_string());
    }

    summary.bottlenecks_predicted.sort();
    Some(())
}

fn python_last_applicable_instr_id(frontend: &crate::sim::FrontEnd) -> Option<u32> {
    let max_instr_id = frontend
        .all_generated_instr_instances
        .iter()
        .filter(|inst| inst.rnd == 0)
        .map(|inst| inst.instr_id)
        .max()?;

    let mut first_round_instrs: Vec<&crate::sim::types::InstrInstance> = frontend
        .all_generated_instr_instances
        .iter()
        .filter(|inst| inst.rnd == 0)
        .collect();
    first_round_instrs.sort_by_key(|inst| inst.instr_id);

    for inst in first_round_instrs {
        // Python `Instr.isLastDecodedInstr()` is true for next-to-last when it
        // macro-fuses with last branch, otherwise for last instruction.
        let is_next_to_last_instr = inst.instr_id + 1 == max_instr_id;
        let is_last_instr = inst.instr_id == max_instr_id;
        if (is_next_to_last_instr && inst.macro_fused_with_next_instr) || is_last_instr {
            return Some(inst.instr_id);
        }
    }
    None
}

fn last_retired_fused_for_round(
    frontend: &crate::sim::FrontEnd,
    uops_for_round: &[UopsForRound],
    instr_id: u32,
    rnd: u32,
) -> Option<(u32, u32)> {
    let fused_idx = *uops_for_round
        .get(rnd as usize)?
        .instrs
        .get(&instr_id)?
        .last()?;
    let fused = frontend.uop_storage.get_fused_uop(fused_idx)?;
    Some((fused.retired?, fused.retire_idx?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use uica_data::{
        encode_uipack, load_manifest_pack, DataPack, InstructionRecord, PerfRecord,
        DATAPACK_SCHEMA_VERSION,
    };

    fn sample_pack(
        arch: &str,
        iform: &str,
        string: &str,
        uops: i32,
        ports: &[(&str, i32)],
    ) -> DataPack {
        DataPack {
            schema_version: DATAPACK_SCHEMA_VERSION.to_string(),
            all_ports: Default::default(),
            alu_ports: Default::default(),
            instructions: vec![InstructionRecord {
                arch: arch.to_string(),
                iform: iform.to_string(),
                string: string.to_string(),
                all_ports: Default::default(),
                alu_ports: Default::default(),
                locked: false,
                xml_attrs: Default::default(),
                imm_zero: false,
                perf: PerfRecord {
                    uops,
                    retire_slots: uops.max(1),
                    uops_mite: uops.max(1),
                    uops_ms: 0,
                    tp: Some(1.0),
                    ports: ports
                        .iter()
                        .map(|(port, count)| ((*port).to_string(), *count))
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
                    operands: vec![],
                    latencies: vec![],
                    variants: Default::default(),
                },
            }],
        }
    }

    fn test_pack_with_add_loop() -> uica_data::DataPack {
        sample_pack("SKL", "ADD", "ADD", 1, &[("0", 1)])
    }

    #[test]
    fn engine_output_can_include_report_bundle() {
        let pack = test_pack_with_add_loop();
        let output = super::engine_output_with_pack(
            &[0x48, 0x01, 0xd8],
            &Invocation {
                arch: "SKL".to_string(),
                min_cycles: 8,
                min_iterations: 1,
                ..Invocation::default()
            },
            &pack,
            true,
        )
        .expect("engine output should run");

        assert_eq!(output.result.invocation.arch, "SKL");
        assert!(output.reports.is_some());
        let reports = output.reports.unwrap();
        assert!(!reports.trace.table_data.is_empty());
        assert!(reports
            .graph
            .series
            .iter()
            .any(|series| series.name == "IQ"));
    }

    #[test]
    fn round2_mirrors_python_ties_to_even() {
        assert_eq!(round2(1.125), 1.12);
        assert_eq!(round2(1.135), 1.14);
    }

    #[test]
    fn cmov_setcc_dependency_limit_uses_latency_start_operands() {
        let code = hex::decode("4839d8480f4fca0f94c04d01c849ffca75ee").unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../uica-data/generated/manifest.json");
        for arch in ["HSW", "ICL"] {
            let invocation = Invocation {
                arch: arch.to_string(),
                min_cycles: 500,
                ..Invocation::default()
            };
            let pack = load_manifest_pack(&manifest, arch).unwrap();
            let result = engine_with_pack(&code, &invocation, &pack);

            assert_eq!(
                result.summary.limits.get("dependencies"),
                Some(&Some(2.0)),
                "{arch}"
            );
        }
    }

    #[test]
    fn ivb_same_reg_scalar_conversion_dependency_limit_mirrors_python() {
        let code = hex::decode(
            "f20f5ad2660f7ed089c2f30f5ad283e03f83e2c0f20f58d10f28c2f20f59d2\
             f20f59e281c2c01f0000f20f59dac1e211660f6ecaf20f59e2f20f59c3f20f58c4\
             f20f5904c6f20f5804c6f20f5ac0f30f59c1",
        )
        .unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../uica-data/generated/manifest.json");
        let pack = load_manifest_pack(&manifest, "IVB").unwrap();
        let result = engine_with_pack(
            &code,
            &Invocation {
                arch: "IVB".to_string(),
                min_cycles: 500,
                ..Invocation::default()
            },
            &pack,
        );

        assert_eq!(result.summary.limits.get("dependencies"), Some(&Some(14.0)));
    }

    #[test]
    fn ivb_high8_clean_penalty_dependency_limit_mirrors_python() {
        let code = hex::decode("0fb757120fb7461280e67f80e47fc1ea02c1e80239c2").unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../uica-data/generated/manifest.json");
        let pack = load_manifest_pack(&manifest, "IVB").unwrap();
        let result = engine_with_pack(
            &code,
            &Invocation {
                arch: "IVB".to_string(),
                min_cycles: 500,
                ..Invocation::default()
            },
            &pack,
        );

        assert_eq!(result.summary.limits.get("dependencies"), Some(&Some(3.0)));
    }

    #[test]
    fn uipack_runtime_experimental_path_matches_owned_pack() {
        let code = vec![0x48, 0x01, 0xd8];
        let invocation = Invocation {
            arch: "SKL".to_string(),
            min_cycles: 8,
            min_iterations: 1,
            ..Invocation::default()
        };
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../uica-data/generated/manifest.json");
        let pack = load_manifest_pack(&manifest, "SKL").unwrap();
        let runtime = uica_data::load_manifest_runtime(&manifest, "SKL").unwrap();

        let owned = engine_output_with_pack(&code, &invocation, &pack, false)
            .unwrap()
            .result;
        let mapped = engine_output_with_uipack_runtime(&code, &invocation, &runtime, false)
            .unwrap()
            .result;

        assert_eq!(
            mapped.summary.throughput_cycles_per_iteration,
            owned.summary.throughput_cycles_per_iteration
        );
        assert_eq!(mapped.summary.limits, owned.summary.limits);
    }

    #[test]
    fn runtime_pack_source_ignores_legacy_instructions_json() {
        let temp = tempdir().unwrap();
        let legacy_path = temp.path().join("instructions.json");
        std::fs::write(
            &legacy_path,
            encode_uipack(&test_pack_with_add_loop(), "SKL").unwrap(),
        )
        .unwrap();

        assert!(load_runtime_source(temp.path(), "SKL", false).is_none());
    }
}
