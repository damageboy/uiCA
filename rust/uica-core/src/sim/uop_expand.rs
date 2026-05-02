//! 1:1 port of Python's `computeUopProperties` from `uiCA.py` / `facile.py`.
//!
//! Given a DataPack record (which now carries operand descriptors and per-
//! operand-pair latency data from UIPack), produces a list
//! of `UopPlan` values that drive laminated-uop creation.
//!
//! The logic mirrors Python exactly:
//!   1. Classify port groups into mem-load, store-address, store-data, non-mem.
//!   2. Build load/store uop props with pseudo-operands for data flow.
//!   3. For non-mem uops: compute latency classes, create base + extra uops
//!      with correct input/output operand names and latencies.
//!   4. Handle the special 3-uop shift-by-CL case.
//!   5. The resulting UopPlans feed `expand_instr_instance_to_lam_uops_with_storage`.

use std::collections::{BTreeMap, HashMap, VecDeque};

use super::types::{FusedUop, InstrInstance, LaminatedUop, OperandKey, Uop, UopProperties};
use super::uop_storage::UopStorage;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Port of Python's `computeUopProperties` applied to one instruction.
/// Returns an ordered list of `(ports, inputs, outputs, latencies)` plans,
/// one per unfused uop, matching Python's `instr.UopPropertiesList`.
pub fn compute_uop_plans(record: &uica_data::InstructionRecord, arch_name: &str) -> Vec<UopPlan> {
    compute_uop_plans_inner(record, arch_name, None)
}

pub(crate) fn record_may_be_eliminated(record: &uica_data::InstructionRecord) -> bool {
    if record.perf.may_be_eliminated {
        return true;
    }
    // Python parity: `instructions.py` derives `mayBeEliminated` for MOVs
    // from the measured zero-uop form. `MOVZX` is included because Python's
    // predicate is `('MOV' in instrData['string'])`; only the decoded
    // `movzxSpecialCase` below disables elimination for specific low-8 aliases.
    record_zero_uop_single_reg_mov(record)
}

pub(crate) fn perf_may_be_eliminated_with_input_regs(
    record: &uica_data::InstructionRecord,
    perf: &uica_data::PerfRecord,
    input_regs: &[String],
    arch: &crate::micro_arch::MicroArchConfig,
) -> bool {
    if perf.may_be_eliminated {
        return true;
    }
    perf_zero_uop_single_reg_mov(record, perf)
        && !record_movzx_special_case_with_input_regs(record, input_regs, arch)
}

pub(crate) fn perf_uses_move_elim_fallback_with_input_regs(
    record: &uica_data::InstructionRecord,
    perf: &uica_data::PerfRecord,
    input_regs: &[String],
    arch: &crate::micro_arch::MicroArchConfig,
) -> bool {
    perf_may_be_eliminated_with_input_regs(record, perf, input_regs, arch)
        || record_movzx_special_case_with_input_regs(record, input_regs, arch)
}

pub(crate) fn record_movzx_special_case_with_input_regs(
    record: &uica_data::InstructionRecord,
    input_regs: &[String],
    arch: &crate::micro_arch::MicroArchConfig,
) -> bool {
    !arch.movzx_high8_alias_can_be_eliminated
        && matches!(
            record.string.as_str(),
            "MOVZX (R64, R8l)" | "MOVZX (R32, R8l)"
        )
        && input_regs.first().is_some_and(|reg| {
            matches!(
                reg.to_ascii_uppercase().as_str(),
                "SPL" | "BPL" | "SIL" | "DIL" | "R12B" | "R13B" | "R14B" | "R15B"
            )
        })
}

fn record_zero_uop_single_reg_mov(record: &uica_data::InstructionRecord) -> bool {
    perf_zero_uop_single_reg_mov(record, &record.perf)
}

fn perf_zero_uop_single_reg_mov(
    record: &uica_data::InstructionRecord,
    perf: &uica_data::PerfRecord,
) -> bool {
    record.string.contains("MOV")
        && perf.uops == 0
        && record_input_reg_operand_count(record) == 1
        && record_output_reg_operand_count(record) == 1
}

fn record_input_reg_operand_count(record: &uica_data::InstructionRecord) -> usize {
    record
        .perf
        .operands
        .iter()
        .filter(|op| {
            op.r#type == "reg"
                && (op.read
                    || record
                        .perf
                        .latencies
                        .iter()
                        .any(|latency| latency.start_op == op.name))
        })
        .count()
}

fn record_output_reg_operand_count(record: &uica_data::InstructionRecord) -> usize {
    record
        .perf
        .operands
        .iter()
        .filter(|op| op.r#type == "reg" && op.write)
        .count()
}

fn is_python_excluded_stack_or_ip_reg_operand(
    instr: &InstrInstance,
    _record: &uica_data::InstructionRecord,
    operand_name: &str,
) -> bool {
    let Some(idx) = operand_name
        .strip_prefix("REG")
        .and_then(|idx| idx.parse::<usize>().ok())
    else {
        return false;
    };
    if instr.implicit_rsp_change == 0 {
        return false;
    }

    match instr.mnemonic.as_str() {
        // Python filters operands whose XED reg name contains STACK or IP.
        // For call/ret/enter, uops.info REG ordering follows XED: stack pseudo
        // reg first; IP regs after explicit call target; ENTER keeps RBP at REG1.
        "enter" => idx == 0,
        "ret" => true,
        "call" => idx >= instr.explicit_reg_operands.len(),
        "push" | "pop" | "pushf" | "popf" => idx >= instr.explicit_reg_operands.len(),
        _ => false,
    }
}

/// An unfused uop's properties as computed by `computeUopProperties`.
#[derive(Clone, Debug)]
pub struct UopPlan {
    pub ports: Vec<String>,
    /// Input operand names (canonical register names or pseudo names __P_N).
    pub inputs: Vec<String>,
    /// Output operand names.
    pub outputs: Vec<String>,
    /// Latency from each output operand to execution result.
    pub latencies: BTreeMap<String, u32>,
    pub is_load: bool,
    pub is_store_address: bool,
    pub is_store_data: bool,
    pub is_first: bool,
    pub is_last: bool,
    pub mem_addr: Option<super::types::MemAddr>,
    pub mem_addr_index: Option<usize>,
}

// ---------------------------------------------------------------------------
// is_instr_supported
// ---------------------------------------------------------------------------

/// True iff we can produce a plan for this decoded instruction.
pub fn is_instr_supported(
    instr: &InstrInstance,
    arch_name: &str,
    index: &uica_data::DataPackIndex,
) -> bool {
    let _ = (instr, arch_name, index);
    // Python parity: missing `archData.instrData[iform]` becomes UnknownInstr,
    // which still enters the simulator as a single zero-port retire-slot uop.
    true
}

/// Look up (uops_mite, uops_ms) from the DataPack.
pub fn lookup_uops_mite_ms(
    mnemonic: &str,
    iform_signature: &str,
    max_op_size_bytes: u8,
    arch_name: &str,
    pack: &uica_data::DataPack,
) -> (u32, u32) {
    let owned;
    let index: &uica_data::DataPackIndex = {
        owned = uica_data::DataPackIndex::new(pack.clone());
        &owned
    };
    lookup_uops_mite_ms_indexed(
        mnemonic,
        iform_signature,
        max_op_size_bytes,
        arch_name,
        index,
    )
}

pub fn record_uops_mite(record: &uica_data::InstructionRecord) -> u32 {
    perf_uops_mite(&record.perf)
}

pub(crate) fn perf_uops_mite(perf: &uica_data::PerfRecord) -> u32 {
    // Python parity: `convertXML.py` clamps `uops_MITE` to at least 1 and
    // `instructions.py` defaults missing `uopsMITE` to 1. Older UIPacks encode
    // that missing/clamped value as 0; normalize at runtime without adding a
    // JSON fallback or regenerating datapacks.
    perf.uops_mite.max(1) as u32
}

pub(crate) fn python_decoder_shape_from_record(
    record: &uica_data::InstructionRecord,
    perf: &uica_data::PerfRecord,
    n_decoders: u32,
) -> (bool, u32) {
    // Python parity: `instructions.py getInstructions()` reads
    // `complDec`/`sDec` from perfData, then derives `complexDecoder` after
    // variants are applied:
    // `if (not complexDecoder) and (uopsMS or (uopsMITE + uopsMS > 1))`.
    let uops_mite = perf_uops_mite(perf);
    let uops_ms = perf.uops_ms.max(0) as u32;
    let derived_complex = !perf.complex_decoder && (uops_ms > 0 || uops_mite + uops_ms > 1);

    // Compatibility for UIPacks produced before data-gen preserved
    // measurement-level `complex_decoder`/`available_simple_decoders`.
    // Python marks CMOVcc as one-uop complex with `sDec = nDecoders - 1`.
    let legacy_cmov_complex = !perf.complex_decoder
        && perf.n_available_simple_decoders == 0
        && record.iform.starts_with("CMOV");

    let complex_decoder = perf.complex_decoder || derived_complex || legacy_cmov_complex;
    let n_available_simple_decoders = if legacy_cmov_complex {
        n_decoders.saturating_sub(1)
    } else if legacy_adc_sbb_complex_with_two_simple_decoders(record, perf) {
        2
    } else if derived_complex && perf.n_available_simple_decoders == 0 {
        n_decoders
    } else {
        perf.n_available_simple_decoders
    };
    (complex_decoder, n_available_simple_decoders)
}

fn legacy_adc_sbb_complex_with_two_simple_decoders(
    record: &uica_data::InstructionRecord,
    perf: &uica_data::PerfRecord,
) -> bool {
    // Python parity/legacy-pack compatibility: HSW perfData for ADC/SBB
    // two-uop flag arithmetic carries `complDec=1, sDec=2`. Older UIPacks
    // dropped both fields, so recover this Python decoder-shape fact from the
    // exact uops.info form instead of letting derived-complex default to all
    // simple decoders.
    !perf.complex_decoder
        && perf.n_available_simple_decoders == 0
        && perf_uops_mite(perf) == 2
        && perf.uops_ms <= 0
        && matches!(record.iform.split('_').next().unwrap_or(""), "ADC" | "SBB")
        && ((perf.ports.get("0156") == Some(&1) && perf.ports.get("06") == Some(&1))
            || (perf.ports.get("015") == Some(&1) && perf.ports.get("05") == Some(&1)))
}

pub(crate) fn perf_for_operands(
    record: &uica_data::InstructionRecord,
    uses_same_reg: bool,
    uses_indexed_addr: bool,
) -> uica_data::PerfRecord {
    let mut perf = record.perf.clone();
    // Python parity: `instructions.py getInstructions()` overlays `_SR`
    // fields before `_I` fields when both same-register and indexed-address
    // predicates apply.
    if uses_same_reg {
        if let Some(same_reg) = record.perf.variants.get("same_reg") {
            apply_perf_variant(&mut perf, same_reg);
        }
    }
    if uses_indexed_addr {
        if let Some(indexed) = record.perf.variants.get("indexed") {
            apply_perf_variant(&mut perf, indexed);
        }
    }
    perf
}

fn apply_perf_variant(perf: &mut uica_data::PerfRecord, variant: &uica_data::PerfVariantRecord) {
    if let Some(uops) = variant.uops {
        perf.uops = uops;
    }
    if let Some(retire_slots) = variant.retire_slots {
        perf.retire_slots = retire_slots;
    }
    if let Some(uops_mite) = variant.uops_mite {
        perf.uops_mite = uops_mite;
    }
    if let Some(uops_ms) = variant.uops_ms {
        perf.uops_ms = uops_ms;
    }
    if let Some(tp) = variant.tp {
        perf.tp = Some(tp);
    }
    if let Some(ports) = &variant.ports {
        perf.ports = ports.clone();
    }
    if let Some(div_cycles) = variant.div_cycles {
        perf.div_cycles = div_cycles;
    }
    if let Some(complex_decoder) = variant.complex_decoder {
        perf.complex_decoder = complex_decoder;
    }
    if let Some(n_available_simple_decoders) = variant.n_available_simple_decoders {
        perf.n_available_simple_decoders = n_available_simple_decoders;
    }
}

pub(crate) fn instr_uses_indexed_addr(instr: &InstrInstance) -> bool {
    instr.mem_addrs.iter().any(|addr| addr.index.is_some())
}

pub(crate) fn instr_uses_same_reg(instr: &InstrInstance) -> bool {
    explicit_regs_use_same_reg(&instr.explicit_reg_operands)
}

pub(crate) fn explicit_regs_use_same_reg(regs: &[String]) -> bool {
    let used_regs: Vec<String> = regs
        .iter()
        .filter(|reg| crate::x64::is_gp_reg(reg) || reg.to_ascii_uppercase().contains("MM"))
        .map(|reg| crate::x64::get_canonical_reg(reg))
        .collect();
    used_regs.len() > 1 && used_regs.iter().all(|reg| reg == &used_regs[0])
}

pub fn record_latency_cycles(
    record: &uica_data::InstructionRecord,
    latency: &uica_data::LatencyRecord,
    arch_name: &str,
) -> i32 {
    let mut cycles = latency.cycles;
    if let Some(addr) = latency.cycles_addr {
        cycles = cycles.max(addr);
    }
    if let Some(addr_index) = latency.cycles_addr_index {
        cycles = cycles.max(addr_index);
    }
    if let Some(mem) = latency.cycles_mem {
        cycles = cycles.max(mem);
    }
    cycles.max(old_uipack_complex_lea_latency(record, latency, arch_name).unwrap_or(cycles))
}

pub fn record_latency_cycles_for_start(
    record: &uica_data::InstructionRecord,
    latency: &uica_data::LatencyRecord,
    arch_name: &str,
    start_op: &str,
) -> i32 {
    let field_cycles = match start_op {
        "__AGEN_ADDR" => latency.cycles_addr,
        "__AGEN_ADDRI" => latency.cycles_addr_index,
        name if name.starts_with("__M_") => latency.cycles_mem,
        _ => None,
    };
    field_cycles
        .or_else(|| old_uipack_complex_lea_latency(record, latency, arch_name))
        .unwrap_or(latency.cycles)
}

fn old_uipack_complex_lea_latency(
    record: &uica_data::InstructionRecord,
    latency: &uica_data::LatencyRecord,
    arch_name: &str,
) -> Option<i32> {
    if latency.cycles_addr.is_none()
        && latency.cycles_addr_index.is_none()
        && latency.start_op == "AGEN"
        && record.iform == "LEA_GPRv_AGEN"
        && matches!(arch_name, "HSW" | "SKL")
        && record.string.starts_with("LEA_B_I")
        && record.perf.ports.len() == 1
        && record.perf.ports.get("1") == Some(&1)
    {
        Some(3)
    } else {
        None
    }
}

fn lea_agen_concrete_names(record: &uica_data::InstructionRecord) -> Vec<String> {
    let Some(form) = record
        .string
        .strip_prefix("LEA_")
        .and_then(|rest| rest.split_whitespace().next())
    else {
        return vec!["AGEN".to_string()];
    };
    let mut names = Vec::new();
    if form.split('_').any(|part| matches!(part, "B" | "R")) {
        names.push("__AGEN_ADDR".to_string());
    }
    if form.split('_').any(|part| matches!(part, "I" | "IS")) {
        names.push("__AGEN_ADDRI".to_string());
    }
    if names.is_empty() {
        names.push("AGEN".to_string());
    }
    names
}

pub fn lookup_uops_mite_ms_indexed(
    mnemonic: &str,
    iform_signature: &str,
    max_op_size_bytes: u8,
    arch_name: &str,
    index: &uica_data::DataPackIndex,
) -> (u32, u32) {
    use crate::matcher::{match_instruction_record, NormalizedInstr};
    let norm = NormalizedInstr {
        mnemonic: mnemonic.to_string(),
        iform_signature: iform_signature.to_string(),
        max_op_size_bytes,
        immediate: None,
        uses_high8_reg: false,
        explicit_reg_operands: Vec::new(),
        agen: None,
    };
    let candidates = index.candidates_for(&arch_name.to_ascii_uppercase(), mnemonic);
    match match_instruction_record(&norm, candidates) {
        Some(rec) => (record_uops_mite(rec), rec.perf.uops_ms as u32),
        None => (1, 0),
    }
}

// ---------------------------------------------------------------------------
// Main expand entry point
// ---------------------------------------------------------------------------

/// Expand one InstrInstance into laminated uops stored in UopStorage.
/// Returns the lam_idx list or an error string.
#[allow(clippy::too_many_arguments)]
pub fn expand_instr_instance_to_lam_uops_with_storage(
    instr: &InstrInstance,
    uop_idx_counter: &mut u64,
    fused_idx_counter: &mut u64,
    lam_idx_counter: &mut u64,
    storage: &mut UopStorage,
    arch_name: &str,
    pack: &uica_data::DataPack,
    pack_index: Option<&uica_data::DataPackIndex>,
) -> Result<Vec<u64>, String> {
    if instr.macro_fused_with_prev_instr {
        return Ok(vec![]);
    }
    use crate::matcher::{match_instruction_record, NormalizedInstr};

    // Use pre-built index if provided (avoids O(n) rebuild per call).
    let owned_index;
    let index = if let Some(idx) = pack_index {
        idx
    } else {
        owned_index = uica_data::DataPackIndex::new(pack.clone());
        &owned_index
    };
    let norm = NormalizedInstr {
        mnemonic: instr.mnemonic.clone(),
        iform_signature: instr.iform_signature.clone(),
        max_op_size_bytes: instr.max_op_size_bytes,
        immediate: instr.immediate,
        uses_high8_reg: instr.uses_high8_reg,
        explicit_reg_operands: instr.explicit_reg_operands.clone(),
        agen: instr.agen.clone(),
    };
    let candidates = index.candidates_for(&arch_name.to_ascii_uppercase(), &instr.mnemonic);
    let record = match match_instruction_record(&norm, candidates) {
        Some(rec) => rec,
        None => {
            // Python parity: `getInstructions()` creates `UnknownInstr` for
            // decoded iforms absent from `archData.instrData`; later
            // `computeUopProperties()` pads its single retire slot with one
            // zero-port uop.
            return Ok(emit_lam_uops(
                &[UopPlan {
                    ports: vec![],
                    inputs: vec![],
                    outputs: vec![],
                    latencies: BTreeMap::new(),
                    is_load: false,
                    is_store_address: false,
                    is_store_data: false,
                    is_first: true,
                    is_last: true,
                    mem_addr: None,
                    mem_addr_index: None,
                }],
                instr,
                uop_idx_counter,
                fused_idx_counter,
                lam_idx_counter,
                storage,
                arch_name,
                None,
            ));
        }
    };

    // Zero-idiom: same-register xor/sub/pxor → 1 zero-port lam for bookkeeping.
    let is_zero_idiom = {
        let m = instr.mnemonic.to_ascii_lowercase();
        ["xor", "sub", "pxor", "vxorps", "vxorpd", "vpxor"].contains(&m.as_str())
            && !instr.has_memory_read
            && !instr.has_memory_write
            && instr.input_regs.is_empty()
    };

    if is_zero_idiom {
        return Ok(emit_lam_uops(
            &[UopPlan {
                ports: vec![],
                inputs: vec![],
                outputs: instr
                    .output_regs
                    .iter()
                    .map(|r| crate::x64::get_canonical_reg(r))
                    .collect(),
                latencies: BTreeMap::new(),
                is_load: false,
                is_store_address: false,
                is_store_data: false,
                is_first: true,
                is_last: true,
                mem_addr: instr.mem_addrs.first().cloned(),
                mem_addr_index: None,
            }],
            instr,
            uop_idx_counter,
            fused_idx_counter,
            lam_idx_counter,
            storage,
            arch_name,
            Some(record),
        ));
    }

    let plans = compute_uop_plans_inner(record, arch_name, Some(instr));

    if plans.is_empty() {
        return Ok(vec![]);
    }

    Ok(emit_lam_uops(
        &plans,
        instr,
        uop_idx_counter,
        fused_idx_counter,
        lam_idx_counter,
        storage,
        arch_name,
        Some(record),
    ))
}

// ---------------------------------------------------------------------------
// Old DSB-slot compatibility shim (not data-driven; only used for slot counts)
// ---------------------------------------------------------------------------

pub fn expand_instr_instance_to_lam_uops(
    instr: &InstrInstance,
    uop_idx_counter: &mut u64,
    fused_idx_counter: &mut u64,
    lam_idx_counter: &mut u64,
) -> Vec<LaminatedUop> {
    if instr.macro_fused_with_prev_instr {
        return vec![];
    }
    let n = (instr.uops_mite as usize).max(1);
    (0..n)
        .map(|_i| {
            let lam_idx = *lam_idx_counter;
            *lam_idx_counter += 1;
            let fused_idx = *fused_idx_counter;
            *fused_idx_counter += 1;
            *uop_idx_counter += 1;
            LaminatedUop {
                idx: lam_idx,
                fused_uop_idxs: vec![fused_idx],
                added_to_idq: None,
                uop_source: None,
                instr_instance_idx: instr.idx,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// computeUopProperties 1:1 port
// ---------------------------------------------------------------------------

/// Pseudo-operand counter for this process (global; unique across all calls).
static PSEUDO_CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_pseudo() -> String {
    let n = PSEUDO_CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("__P_{n}")
}

fn compute_uop_plans_inner(
    record: &uica_data::InstructionRecord,
    arch_name: &str,
    instr: Option<&InstrInstance>,
) -> Vec<UopPlan> {
    let uses_same_reg = instr.is_some_and(instr_uses_same_reg);
    let selected_perf;
    let perf = if let Some(instr) = instr {
        selected_perf = perf_for_operands(record, uses_same_reg, instr_uses_indexed_addr(instr));
        &selected_perf
    } else {
        &record.perf
    };
    let arch = crate::micro_arch::get_micro_arch(arch_name);
    let may_be_eliminated = instr
        .zip(arch.as_ref())
        .map(|(instr, arch)| {
            perf_may_be_eliminated_with_input_regs(record, perf, &instr.input_regs, arch)
        })
        .unwrap_or_else(|| record_may_be_eliminated(record));
    let use_move_elim_fallback = instr
        .zip(arch.as_ref())
        .map(|(instr, arch)| {
            perf_uses_move_elim_fallback_with_input_regs(record, perf, &instr.input_regs, arch)
        })
        .unwrap_or(may_be_eliminated);

    // --- Port classification (mirrors Python's portData loop) ---
    // ports string "06" → [0,6]; "23" → [2,3]; "78" → [7,8]; "49" → [4,9]
    let mut load_pcs: Vec<Vec<String>> = Vec::new();
    let mut store_addr_pcs: Vec<Vec<String>> = Vec::new();
    let mut store_data_pcs: Vec<Vec<String>> = Vec::new();
    let mut non_mem_pcs: Vec<Vec<String>> = Vec::new();

    let mut sorted_ports: Vec<(&String, &i32)> = perf.ports.iter().collect();
    sorted_ports.sort_by_key(|(k, _)| k.as_str());
    let move_elim_ports;
    if sorted_ports.is_empty() && use_move_elim_fallback {
        move_elim_ports = crate::micro_arch::alu_ports(arch_name).join("");
        sorted_ports.push((&move_elim_ports, &1));
    }
    for (port_str, &count) in &sorted_ports {
        if count <= 0 {
            continue;
        }
        let ports: Vec<String> = port_str.chars().map(|c| c.to_string()).collect();
        let is_store_addr = ports.iter().any(|p| p == "7" || p == "8");
        let is_load = !is_store_addr && ports.iter().any(|p| p == "2" || p == "3");
        let is_store_data =
            !is_store_addr && !is_load && ports.iter().any(|p| p == "4" || p == "9");
        for _ in 0..count {
            if is_store_addr {
                store_addr_pcs.push(ports.clone());
            } else if is_load {
                load_pcs.push(ports.clone());
            } else if is_store_data {
                store_data_pcs.push(ports.clone());
            } else {
                non_mem_pcs.push(ports.clone());
            }
        }
    }

    // Balance store data / address counts (mirrors Python's while loop).
    while store_data_pcs.len() > store_addr_pcs.len() {
        if !load_pcs.is_empty() {
            store_addr_pcs.push(load_pcs.pop().unwrap());
        } else {
            store_data_pcs.pop();
        }
    }

    // --- Operand extraction ---
    // Python uses separate RegOperand / FlagOperand / MemOperand objects per
    // read/write role. Flags are split into Python's two rename domains: C and
    // SPAZO. Memory operands use stable per-instruction synthetic names that
    // resolve to OperandKey::Mem during emission.

    let mut input_reg_ops: Vec<String> = Vec::new();
    let mut output_reg_ops: Vec<String> = Vec::new();
    let mut input_flag_ops: Vec<String> = Vec::new();
    let mut output_flag_ops: Vec<String> = Vec::new();
    let mut input_mem_ops: Vec<String> = Vec::new();
    let mut output_mem_ops: Vec<String> = Vec::new();
    let mut mem_addr_ops: Vec<String> = Vec::new();
    let mut agen_ops: Vec<String> = Vec::new();
    let mut concrete_operand_names: HashMap<String, Vec<String>> = HashMap::new();
    let mut next_mem_id = 0u32;

    for op in &perf.operands {
        let concrete_names = if op.r#type == "flags" && !op.flags.is_empty() {
            op.flags.clone()
        } else if op.r#type == "mem" {
            let name = format!("__M_{}", next_mem_id);
            next_mem_id += 1;
            vec![name]
        } else if op.r#type == "agen" {
            lea_agen_concrete_names(record)
        } else {
            vec![op.name.clone()]
        };
        let read_names = if op.r#type == "flags" && !op.flags_read.is_empty() {
            op.flags_read.clone()
        } else {
            concrete_names.clone()
        };
        let write_names = if op.r#type == "flags" && !op.flags_write.is_empty() {
            op.flags_write.clone()
        } else {
            concrete_names.clone()
        };
        concrete_operand_names.insert(op.name.clone(), concrete_names.clone());

        match op.r#type.as_str() {
            "reg" => {
                if instr.is_some_and(|instr| {
                    is_python_excluded_stack_or_ip_reg_operand(instr, record, &op.name)
                }) {
                    // Python parity: `instructions.py` excludes XED STACKPUSH/
                    // STACKPOP pseudo-register operands from input/output regs;
                    // stack address RSP remains as an implicit memAddr operand.
                    continue;
                }
                let read_by_latency = perf.latencies.iter().any(|lr| lr.start_op == op.name);
                if op.read || read_by_latency {
                    // Python parity: `instructions.py` includes a register in
                    // `instrInputRegOperands` when XML marks it read OR when
                    // any latency row starts at that operand. Conditional-write
                    // operands like CMOV destination registers rely on latency
                    // rows to stay live as inputs.
                    input_reg_ops.extend(read_names.iter().cloned());
                }
                if op.write {
                    output_reg_ops.extend(write_names.iter().cloned());
                }
            }
            "flags" => {
                let read_by_latency = perf.latencies.iter().any(|lr| lr.start_op == op.name);
                // Python parity: input flags come from `flagsR`. Some existing
                // UIPacks lack flags_read for latency-start flag operands used by
                // Python's SHL/ROL latency-class split; fall back only for inputs.
                if !op.flags_read.is_empty() {
                    input_flag_ops.extend(op.flags_read.iter().cloned());
                } else if op.read || read_by_latency {
                    let mut flags = op.flags.clone();
                    flags.sort_by_key(|flag| if flag == "C" { 0 } else { 1 });
                    input_flag_ops.extend(flags);
                }
                // Outputs must come only from `flagsW`; read-only flags are not writes.
                if !op.flags_write.is_empty() {
                    output_flag_ops.extend(op.flags_write.iter().cloned());
                }
            }
            "mem" | "agen" => {
                let role = op.mem_operand_role.as_deref();
                let has_addr_metadata = op.mem_base.is_some()
                    || op.mem_index.is_some()
                    || op.mem_scale.is_some()
                    || op.mem_disp.is_some();
                if op.r#type == "agen"
                    || op.is_agen
                    || role == Some("agen")
                    || role == Some("address")
                {
                    agen_ops.extend(concrete_names.iter().cloned());
                }
                if op.r#type == "agen"
                    || op.is_agen
                    || role == Some("agen")
                    || role == Some("address")
                    || has_addr_metadata
                {
                    mem_addr_ops.extend(concrete_names.iter().cloned());
                }
                if op.r#type == "mem"
                    && (op.read || matches!(role, Some("read") | Some("read_write")))
                {
                    input_mem_ops.extend(read_names.iter().cloned());
                }
                if op.r#type == "mem"
                    && (op.write || matches!(role, Some("write") | Some("read_write")))
                {
                    output_mem_ops.extend(write_names.iter().cloned());
                }
            }
            _ => {} // imm handled separately
        }
    }

    // --- Build latency dict (inOp, outOp) -> cycles ---
    let mut lat_dict: HashMap<(String, String), i32> = HashMap::new();
    let mut lat_dict_no_sr: HashMap<(String, String), i32> = HashMap::new();
    for lr in &perf.latencies {
        let start_ops = concrete_operand_names
            .get(&lr.start_op)
            .cloned()
            .unwrap_or_else(|| vec![lr.start_op.clone()]);
        let target_ops = concrete_operand_names
            .get(&lr.target_op)
            .cloned()
            .unwrap_or_else(|| vec![lr.target_op.clone()]);
        for start_op in &start_ops {
            for target_op in &target_ops {
                let cycles = record_latency_cycles_for_start(record, lr, arch_name, start_op);
                lat_dict.insert((start_op.clone(), target_op.clone()), cycles);
                lat_dict_no_sr.insert((start_op.clone(), target_op.clone()), cycles);
                if let Some(sr) = lr.cycles_same_reg {
                    if use_move_elim_fallback || uses_same_reg {
                        // Python parity: `instructions.py` replaces `latData`
                        // with `lat_SR` only for same-register forms, eliminated
                        // MOV fallback uops, and `movzxSpecialCase`. Non-SR
                        // instructions keep base latency even when XML also
                        // carries a `cycles_same_reg` shortcut.
                        lat_dict.insert((start_op.clone(), target_op.clone()), sr);
                    }
                }
            }
        }
    }

    // Python parity: while constructing `Instr.inputRegOperands`, Python
    // drops read registers whose latency to every output operand is exactly 0
    // (zero idioms and some store-data forms). Do this after lat_dict exists
    // so store-data uops do not acquire dependencies Python omitted.
    if !may_be_eliminated {
        let output_operand_names: Vec<String> = output_reg_ops
            .iter()
            .chain(output_flag_ops.iter())
            .chain(output_mem_ops.iter())
            .cloned()
            .collect();
        let zero_filter_lat_dict = if use_move_elim_fallback || uses_same_reg {
            &lat_dict
        } else {
            &lat_dict_no_sr
        };
        input_reg_ops.retain(|inp| {
            !output_operand_names.iter().all(|out| {
                zero_filter_lat_dict
                    .get(&(inp.clone(), out.clone()))
                    .copied()
                    .unwrap_or(1)
                    == 0
            })
        });
    }

    // --- No ports at all → zero-port (zero idiom), NOP, or fence padding ---
    // Python parity: `computeUopProperties` still pads `UopPropertiesList` to
    // `retireSlots` after seeing no port data. LFENCE has one DSB/MITE slot plus
    // MS uops, but two retire slots, so it needs two zero-port properties.
    if non_mem_pcs.is_empty() && load_pcs.is_empty() && store_addr_pcs.is_empty() {
        let n = perf.retire_slots.max(1) as usize;
        return (0..n)
            .map(|i| UopPlan {
                ports: vec![],
                inputs: vec![],
                outputs: output_reg_ops.to_vec(),
                latencies: output_reg_ops.iter().map(|o| (o.clone(), 0)).collect(),
                is_load: false,
                is_store_address: false,
                is_store_data: false,
                is_first: i == 0,
                is_last: i == n - 1,
                mem_addr: None,
                mem_addr_index: None,
            })
            .collect();
    }

    let mut result: Vec<UopPlan> = Vec::new();

    // --- Load uops ---
    for (i, pc) in load_pcs.iter().enumerate() {
        let out_ops: Vec<String> = if !non_mem_pcs.is_empty() {
            vec![next_pseudo()] // load feeds a pseudo-op to non-mem chain
        } else {
            output_reg_ops.clone()
        };
        let lat_map: BTreeMap<String, u32> = out_ops.iter().map(|o| (o.clone(), 5)).collect(); // ~5-cycle load lat
        result.push(UopPlan {
            ports: pc.clone(),
            inputs: Vec::new(),
            outputs: out_ops,
            latencies: lat_map,
            is_load: true,
            is_store_address: false,
            is_store_data: false,
            is_first: false,
            is_last: false,
            mem_addr: None,
            mem_addr_index: Some(i),
        });
    }
    let load_pseudo_ops: Vec<String> = result
        .iter()
        .filter(|p| p.is_load)
        .flat_map(|p| p.outputs.iter().cloned())
        .collect();

    // --- Store uops (address then data) ---
    let mut store_uop_props: Vec<UopPlan> = Vec::new();
    for (i, (st_a_pc, st_d_pc)) in store_addr_pcs.iter().zip(store_data_pcs.iter()).enumerate() {
        let store_data_input: Vec<String> = if !non_mem_pcs.is_empty() {
            vec![next_pseudo()]
        } else {
            input_reg_ops
                .iter()
                .chain(input_flag_ops.iter())
                .cloned()
                .collect()
        };
        store_uop_props.push(UopPlan {
            ports: st_a_pc.clone(),
            inputs: Vec::new(),
            outputs: vec![],
            latencies: BTreeMap::new(),
            is_load: false,
            is_store_address: true,
            is_store_data: false,
            is_first: false,
            is_last: false,
            mem_addr: None,
            mem_addr_index: Some(i),
        });
        store_uop_props.push(UopPlan {
            ports: st_d_pc.clone(),
            inputs: store_data_input,
            outputs: vec![],
            latencies: BTreeMap::new(),
            is_load: false,
            is_store_address: false,
            is_store_data: true,
            is_first: false,
            is_last: false,
            mem_addr: None,
            mem_addr_index: None,
        });
    }
    let store_pseudo_ops: Vec<String> = store_uop_props
        .iter()
        .filter(|p| p.is_store_data)
        .flat_map(|p| p.inputs.iter().filter(|s| s.starts_with("__P_")).cloned())
        .collect();

    // --- Non-mem uops ---
    if !non_mem_pcs.is_empty() {
        let non_mem_plans = compute_non_mem_uop_plans(
            &non_mem_pcs,
            &input_reg_ops,
            &output_reg_ops,
            &input_flag_ops,
            &output_flag_ops,
            &input_mem_ops,
            &output_mem_ops,
            &mem_addr_ops,
            &agen_ops,
            &load_pseudo_ops,
            &store_pseudo_ops,
            &lat_dict,
            perf.div_cycles,
        );
        result.extend(non_mem_plans);
    }
    result.extend(store_uop_props);

    // Add retire-slot padding uops (no ports, no operands) if retire_slots > len.
    let n_extra = (perf.retire_slots as usize).saturating_sub(result.len());
    for _ in 0..n_extra {
        result.push(UopPlan {
            ports: vec![],
            inputs: vec![],
            outputs: vec![],
            latencies: BTreeMap::new(),
            is_load: false,
            is_store_address: false,
            is_store_data: false,
            is_first: false,
            is_last: false,
            mem_addr: None,
            mem_addr_index: None,
        });
    }

    if result.is_empty() {
        return result;
    }

    // Apply macro-fusion port override to the LAST uop of the instruction.
    // (dec+jnz fused: the dec's last uop gets branch unit ports.)
    // This is handled in emit_lam_uops using is_macro_fused_with_next_instr.

    // Mark first/last.
    let last_idx = result.len() - 1;
    for (i, plan) in result.iter_mut().enumerate() {
        plan.is_first = i == 0;
        plan.is_last = i == last_idx;
    }

    result
}

/// 1:1 port of the non-memory branch of `computeUopProperties`.
#[allow(clippy::too_many_arguments)]
fn compute_non_mem_uop_plans(
    non_mem_pcs: &[Vec<String>],
    input_reg_ops: &[String],
    output_reg_ops: &[String],
    input_flag_ops: &[String],
    output_flag_ops: &[String],
    input_mem_ops: &[String],
    output_mem_ops: &[String],
    mem_addr_ops: &[String],
    agen_ops: &[String],
    load_pseudo_ops: &[String],
    store_pseudo_ops: &[String],
    lat_dict: &HashMap<(String, String), i32>,
    div_cycles: u32,
) -> Vec<UopPlan> {
    // Special 3-uop case: SHL (R64, CL) pattern.
    // Python checks exact latency values; we detect the same conditions.
    if non_mem_pcs.len() == 3
        && input_mem_ops.is_empty()
        && output_mem_ops.is_empty()
        && mem_addr_ops.is_empty()
        && agen_ops.is_empty()
        && !input_reg_ops.is_empty()
        && !output_reg_ops.is_empty()
        && !input_flag_ops.is_empty()
        && !output_flag_ops.is_empty()
        && load_pseudo_ops.is_empty()
        && store_pseudo_ops.is_empty()
    {
        let all_reg_to_reg_1 = input_reg_ops.iter().all(|i| {
            output_reg_ops
                .iter()
                .all(|o| lat_dict.get(&(i.clone(), o.clone())).copied().unwrap_or(1) == 1)
        });
        let all_reg_to_flag_2 = input_reg_ops.iter().all(|i| {
            output_flag_ops
                .iter()
                .all(|o| lat_dict.get(&(i.clone(), o.clone())).copied().unwrap_or(2) == 2)
        });
        let all_flag_to_reg_0 = input_flag_ops.iter().all(|i| {
            output_reg_ops
                .iter()
                .all(|o| lat_dict.get(&(i.clone(), o.clone())).copied().unwrap_or(0) == 0)
        });
        let all_flag_to_flag_2 = input_flag_ops.iter().all(|i| {
            output_flag_ops
                .iter()
                .all(|o| lat_dict.get(&(i.clone(), o.clone())).copied().unwrap_or(2) == 2)
        });

        if all_reg_to_reg_1 && all_reg_to_flag_2 && all_flag_to_reg_0 && all_flag_to_flag_2 {
            let r_pseudo = next_pseudo();
            let f_pseudo = next_pseudo();

            let mut rout = output_reg_ops.to_vec();
            rout.push(r_pseudo.clone());
            let r_lat: BTreeMap<String, u32> = rout.iter().map(|o| (o.clone(), 1)).collect();

            let f_lat: BTreeMap<String, u32> = vec![(f_pseudo.clone(), 1)].into_iter().collect();

            let out_flag_lat: BTreeMap<String, u32> =
                output_flag_ops.iter().map(|o| (o.clone(), 1)).collect();

            return vec![
                UopPlan {
                    ports: non_mem_pcs[0].clone(),
                    inputs: input_reg_ops.to_vec(),
                    outputs: rout,
                    latencies: r_lat,
                    is_load: false,
                    is_store_address: false,
                    is_store_data: false,
                    is_first: false,
                    is_last: false,
                    mem_addr: None,
                    mem_addr_index: None,
                },
                UopPlan {
                    ports: non_mem_pcs[1].clone(),
                    inputs: input_flag_ops.to_vec(),
                    outputs: vec![f_pseudo.clone()],
                    latencies: f_lat,
                    is_load: false,
                    is_store_address: false,
                    is_store_data: false,
                    is_first: false,
                    is_last: false,
                    mem_addr: None,
                    mem_addr_index: None,
                },
                UopPlan {
                    ports: non_mem_pcs[2].clone(),
                    inputs: vec![r_pseudo, f_pseudo.clone()],
                    outputs: output_flag_ops.to_vec(),
                    latencies: out_flag_lat,
                    is_load: false,
                    is_store_address: false,
                    is_store_data: false,
                    is_first: false,
                    is_last: false,
                    mem_addr: None,
                    mem_addr_index: None,
                },
            ];
        }
    }

    // General case: compute latency classes.
    let non_mem_inputs: Vec<String> = input_reg_ops
        .iter()
        .chain(input_flag_ops.iter())
        .chain(if agen_ops.is_empty() {
            [].iter()
        } else {
            mem_addr_ops.iter()
        })
        .chain(load_pseudo_ops.iter())
        .cloned()
        .collect();
    let non_mem_outputs: Vec<String> = output_reg_ops
        .iter()
        .chain(output_flag_ops.iter())
        .chain(store_pseudo_ops.iter())
        .cloned()
        .collect();

    // adjusted latencies for non-mem operand pairs
    let mut adj_lat: HashMap<(String, String), i32> = HashMap::new();
    for in_op in input_reg_ops
        .iter()
        .chain(input_flag_ops.iter())
        .chain(if agen_ops.is_empty() {
            [].iter()
        } else {
            mem_addr_ops.iter()
        })
    {
        for out_op in output_reg_ops.iter().chain(output_flag_ops.iter()) {
            let v = lat_dict
                .get(&(in_op.clone(), out_op.clone()))
                .copied()
                .unwrap_or(1);
            adj_lat.insert((in_op.clone(), out_op.clone()), v);
        }
        for sp in store_pseudo_ops.iter() {
            let store_lat = output_mem_ops
                .iter()
                .map(|out_mem| {
                    (lat_dict
                        .get(&(in_op.clone(), out_mem.clone()))
                        .copied()
                        .unwrap_or(1)
                        - 4)
                    .max(1)
                })
                .max()
                .unwrap_or(1);
            adj_lat.insert((in_op.clone(), sp.clone()), store_lat);
        }
    }
    if agen_ops.is_empty() {
        for in_mem_op in mem_addr_ops.iter() {
            for lp in load_pseudo_ops.iter() {
                for out_op in output_reg_ops.iter().chain(output_flag_ops.iter()) {
                    let load_lat = (lat_dict
                        .get(&(in_mem_op.clone(), out_op.clone()))
                        .copied()
                        .unwrap_or(1)
                        - 5)
                    .max(1);
                    adj_lat.insert((lp.clone(), out_op.clone()), load_lat);
                }
            }
        }
    }
    for lp in load_pseudo_ops.iter() {
        for out_op in output_reg_ops.iter().chain(output_flag_ops.iter()) {
            adj_lat.entry((lp.clone(), out_op.clone())).or_insert(1);
        }
        for sp in store_pseudo_ops.iter() {
            let load_store_lat = input_mem_ops
                .iter()
                .flat_map(|in_mem| {
                    output_mem_ops.iter().map(move |out_mem| {
                        (lat_dict
                            .get(&(in_mem.clone(), out_mem.clone()))
                            .copied()
                            .unwrap_or(1)
                            - 5)
                        .max(1)
                    })
                })
                .max()
                .unwrap_or(1);
            adj_lat.insert((lp.clone(), sp.clone()), load_store_lat);
        }
    }

    // Latency class map: max latency from input to ANY output.
    let mut lat_classes: BTreeMap<i32, Vec<String>> = BTreeMap::new();
    for in_op in &non_mem_inputs {
        let max_lat = non_mem_outputs
            .iter()
            .map(|out_op| {
                adj_lat
                    .get(&(in_op.clone(), out_op.clone()))
                    .copied()
                    .unwrap_or(1)
            })
            .max()
            .unwrap_or(1);
        lat_classes.entry(max_lat).or_default().push(in_op.clone());
    }

    // Build plans: base uop (lowest latency class) + prepended extra uops.
    let mut remaining_levels: VecDeque<i32> = lat_classes.keys().copied().collect();
    let min_lat_level = remaining_levels.pop_front().unwrap_or(1);
    let min_lat_class = lat_classes.get(&min_lat_level).cloned().unwrap_or_default();

    let mut base_lat: BTreeMap<String, u32> = BTreeMap::new();
    for out_op in &non_mem_outputs {
        let v = if !min_lat_class.is_empty() {
            min_lat_class
                .iter()
                .map(|i| {
                    adj_lat
                        .get(&(i.clone(), out_op.clone()))
                        .copied()
                        .unwrap_or(1) as u32
                })
                .max()
                .unwrap_or(1)
        } else {
            1
        };
        base_lat.insert(out_op.clone(), v);
    }

    let mut base_plan = UopPlan {
        ports: non_mem_pcs.first().cloned().unwrap_or_default(),
        inputs: min_lat_class.clone(),
        outputs: non_mem_outputs.clone(),
        latencies: base_lat,
        is_load: false,
        is_store_address: false,
        is_store_data: false,
        is_first: false,
        is_last: false,
        mem_addr: None,
        mem_addr_index: None,
    };
    base_plan
        .latencies
        .insert("div_cycles".to_string(), div_cycles);

    // Python parity: `computeUopProperties` stores latency-class extras with
    // `nonMemUopProps.appendleft(...)`, but once latency classes are exhausted
    // it appends filler uops to the right of the base uop. Keep those two
    // Python deque directions separate.
    let mut prepended_extras: Vec<UopPlan> = Vec::new();
    let mut appended_extras: Vec<UopPlan> = Vec::new();

    for pc in non_mem_pcs.iter().skip(1) {
        if let Some(lat_level) = remaining_levels.pop_front() {
            let lat_class = lat_classes.get(&lat_level).cloned().unwrap_or_default();
            let pseudo = next_pseudo();
            base_plan.inputs.push(pseudo.clone());
            let delay = (lat_level - min_lat_level).max(0) as u32;
            let mut extra_lat = BTreeMap::new();
            extra_lat.insert(pseudo.clone(), delay);
            prepended_extras.push(UopPlan {
                ports: pc.clone(),
                inputs: lat_class,
                outputs: vec![pseudo],
                latencies: extra_lat,
                is_load: false,
                is_store_address: false,
                is_store_data: false,
                is_first: false,
                is_last: false,
                mem_addr: None,
                mem_addr_index: None,
            });
        } else {
            // No more latency levels: extra uop reads all inputs, writes nothing.
            appended_extras.push(UopPlan {
                ports: pc.clone(),
                inputs: non_mem_inputs.clone(),
                outputs: vec![],
                latencies: BTreeMap::new(),
                is_load: false,
                is_store_address: false,
                is_store_data: false,
                is_first: false,
                is_last: false,
                mem_addr: None,
                mem_addr_index: None,
            });
        }
    }

    // Append any remaining latency-class inputs to Python deque's last element:
    // right-appended filler if present, otherwise the base uop.
    while let Some(lat_level) = remaining_levels.pop_front() {
        if let Some(lat_class) = lat_classes.get(&lat_level) {
            if let Some(last) = appended_extras.last_mut() {
                last.inputs.extend(lat_class.iter().cloned());
            } else {
                base_plan.inputs.extend(lat_class.iter().cloned());
            }
        }
    }

    let mut plans: Vec<UopPlan> = prepended_extras.into_iter().rev().collect();
    plans.push(base_plan);
    plans.extend(appended_extras);
    plans
}

// ---------------------------------------------------------------------------
// Emit lam uops from plans
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn emit_lam_uops(
    plans: &[UopPlan],
    instr: &InstrInstance,
    uop_idx_counter: &mut u64,
    fused_idx_counter: &mut u64,
    lam_idx_counter: &mut u64,
    storage: &mut UopStorage,
    arch_name: &str,
    record: Option<&uica_data::InstructionRecord>,
) -> Vec<u64> {
    let n = plans.len();
    let mut lam_idxs = Vec::with_capacity(n);

    let decoded_inputs: Vec<String> = instr
        .input_regs
        .iter()
        .map(|r| crate::x64::get_canonical_reg(r))
        .collect();
    // Python parity: `Instr.inputRegOperands` excludes memory address
    // operands. `DecodedInstruction.input_regs` includes base/index regs, so
    // strip one address occurrence before resolving REGn placeholders; load
    // and store-address uops add memAddrOperands separately below.
    let mut decoded_reg_inputs = decoded_inputs.clone();
    for mem_addr in &instr.mem_addrs {
        if mem_addr.is_implicit_stack_operand {
            continue;
        }
        for addr_reg in [mem_addr.base.as_ref(), mem_addr.index.as_ref()]
            .into_iter()
            .flatten()
        {
            let canonical = crate::x64::get_canonical_reg(addr_reg);
            if let Some(pos) = decoded_reg_inputs.iter().position(|r| r == &canonical) {
                decoded_reg_inputs.remove(pos);
            }
        }
    }
    let decoded_outputs: Vec<String> = instr
        .output_regs
        .iter()
        .map(|r| crate::x64::get_canonical_reg(r))
        .collect();
    let flag_str = if instr.reads_flags || instr.writes_flags {
        "RFLAGS"
    } else {
        ""
    };

    // Python parity: `UopProperties` stores actual operand objects from
    // `Instr.inputRegOperands` / `Instr.outputRegOperands`. DataPack REGn names
    // must resolve through full instruction operand order, not through the
    // local uop input/output list (e.g. DIV REG2 remains EDX even when alone).
    let mut input_operand_map: HashMap<String, String> = HashMap::new();
    let mut output_operand_map: HashMap<String, String> = HashMap::new();
    if let Some(record) = record {
        let mut explicit_reg_operand_map: HashMap<String, String> = HashMap::new();
        let mut explicit_reg_idx = 0usize;
        for operand in &record.perf.operands {
            if operand.r#type == "reg" && !operand.implicit {
                if let Some(reg) = instr.explicit_reg_operands.get(explicit_reg_idx) {
                    // Python parity: `instrD['regOperands']` is keyed by XML
                    // operand names and preserves duplicate explicit operands
                    // (e.g. KANDW K2,K1,K1 has separate REG1/REG2 roles).
                    explicit_reg_operand_map
                        .insert(operand.name.clone(), crate::x64::get_canonical_reg(reg));
                }
                explicit_reg_idx += 1;
            }
        }

        let mut read_reg_idx = 0usize;
        let mut write_reg_idx = 0usize;
        for operand in &record.perf.operands {
            if operand.r#type == "reg" {
                let read_by_latency = record
                    .perf
                    .latencies
                    .iter()
                    .any(|latency| latency.start_op == operand.name);
                if operand.read || read_by_latency {
                    // Python parity: same predicate as `Instr.inputRegOperands`.
                    // Latency-start write operands map to their own decoded output
                    // (SETCC/MOVZX dest), not blindly to next input register.
                    if let Some(reg) = explicit_reg_operand_map.get(&operand.name) {
                        input_operand_map.insert(operand.name.clone(), reg.clone());
                        if operand.read
                            || (operand.write
                                && decoded_reg_inputs
                                    .get(read_reg_idx)
                                    .is_some_and(|decoded| decoded == reg))
                        {
                            read_reg_idx += 1;
                        }
                    } else if operand.read {
                        if let Some(reg) = decoded_reg_inputs.get(read_reg_idx) {
                            input_operand_map.insert(operand.name.clone(), reg.clone());
                        }
                        read_reg_idx += 1;
                    } else if operand.write {
                        if let Some(reg) = decoded_outputs.get(write_reg_idx) {
                            input_operand_map.insert(operand.name.clone(), reg.clone());
                        }
                        if decoded_reg_inputs
                            .get(read_reg_idx)
                            .is_some_and(|reg| decoded_outputs.get(write_reg_idx) == Some(reg))
                        {
                            read_reg_idx += 1;
                        }
                    } else if let Some(reg) = decoded_reg_inputs.get(read_reg_idx) {
                        input_operand_map.insert(operand.name.clone(), reg.clone());
                        read_reg_idx += 1;
                    }
                }
                if operand.write {
                    if let Some(reg) = explicit_reg_operand_map
                        .get(&operand.name)
                        .or_else(|| decoded_outputs.get(write_reg_idx))
                    {
                        output_operand_map.insert(operand.name.clone(), reg.clone());
                    }
                    write_reg_idx += 1;
                }
            }
        }
    }

    let fallback_resolve_name =
        |name: &str, all_ops: &[String], decoded: &[String], flag_name: &str| -> String {
            if name.starts_with("__") || matches!(name, "C" | "SPAZO") {
                return name.to_string();
            }
            let is_reg_placeholder = name.to_ascii_uppercase().starts_with("REG")
                && name.len() > 3
                && name[3..].chars().all(|c| c.is_ascii_digit());
            if is_reg_placeholder {
                let idx = all_ops
                    .iter()
                    .filter(|o| {
                        let u = o.to_ascii_uppercase();
                        u.starts_with("REG")
                            && u.len() > 3
                            && u[3..].chars().all(|c| c.is_ascii_digit())
                    })
                    .position(|o| o == name);
                if let Some(i) = idx {
                    if i < decoded.len() {
                        return decoded[i].clone();
                    }
                }
                if !flag_name.is_empty() {
                    return flag_name.to_string();
                }
                return name.to_string();
            }
            crate::x64::get_canonical_reg(name)
        };

    let resolve_input = |name: &str, all_ops: &[String]| -> String {
        if name.starts_with("__") || matches!(name, "C" | "SPAZO") {
            return name.to_string();
        }
        input_operand_map
            .get(name)
            .cloned()
            .unwrap_or_else(|| fallback_resolve_name(name, all_ops, &decoded_reg_inputs, flag_str))
    };
    let resolve_output = |name: &str, all_ops: &[String]| -> String {
        if name.starts_with("__") || matches!(name, "C" | "SPAZO") {
            return name.to_string();
        }
        output_operand_map
            .get(name)
            .cloned()
            .unwrap_or_else(|| fallback_resolve_name(name, all_ops, &decoded_outputs, flag_str))
    };

    let mut unfused_domain_uops = VecDeque::new();

    for (i, plan) in plans.iter().enumerate() {
        let is_first = i == 0;
        let is_last = i == n - 1;

        // Macro-fused dec+jnz: override last uop's ports to branch unit.
        let possible_ports = if instr.macro_fused_with_next_instr && is_last {
            match arch_name {
                "ICL" => vec!["0".to_string(), "6".to_string()],
                _ => vec!["6".to_string()],
            }
        } else {
            plan.ports.clone()
        };

        // Resolve inputs: use input_regs for positional REGn names. Load and
        // store-address uops also read address-generation registers.
        let mut inputs: Vec<String> = plan
            .inputs
            .iter()
            .map(|s| resolve_input(s, &plan.inputs))
            .collect();
        let reads_generic_address = plan.inputs.iter().any(|input| input == "AGEN");
        let reads_base_address = plan.is_load
            || plan.is_store_address
            || reads_generic_address
            || plan
                .inputs
                .iter()
                .any(|input| input == "__AGEN_ADDR" || input.starts_with("__M_"));
        let reads_index_address = plan.is_load
            || plan.is_store_address
            || reads_generic_address
            || plan
                .inputs
                .iter()
                .any(|input| input == "__AGEN_ADDRI" || input.starts_with("__M_"));
        let reads_address_operand = reads_base_address || reads_index_address;
        if reads_address_operand {
            inputs.retain(|input| {
                input != "AGEN"
                    && input != "__AGEN_ADDR"
                    && input != "__AGEN_ADDRI"
                    && !input.starts_with("__M_")
            });
        }
        let selected_mem_addr = plan.mem_addr.clone().or_else(|| {
            if plan.is_load || plan.is_store_address || reads_address_operand {
                let idx = plan.mem_addr_index.unwrap_or(0);
                instr
                    .mem_addrs
                    .get(idx.min(instr.mem_addrs.len().saturating_sub(1)))
                    .cloned()
            } else {
                None
            }
        });
        if plan.is_load || plan.is_store_address || reads_address_operand {
            if let Some(mem_addr) = selected_mem_addr.as_ref() {
                if reads_base_address {
                    if let Some(base) = &mem_addr.base {
                        let base = crate::x64::get_canonical_reg(base);
                        if !inputs.contains(&base) {
                            inputs.push(base);
                        }
                    }
                }
                if reads_index_address {
                    if let Some(index) = &mem_addr.index {
                        let index = crate::x64::get_canonical_reg(index);
                        if !inputs.contains(&index) {
                            inputs.push(index);
                        }
                    }
                }
            }
        }
        // Resolve outputs: use output_regs for positional REGn names.
        let outputs: Vec<String> = plan
            .outputs
            .iter()
            .map(|s| resolve_output(s, &plan.outputs))
            .collect();

        // Build latency map keyed on resolved output names.
        let latencies: BTreeMap<String, u32> = plan
            .latencies
            .iter()
            .filter(|(k, _)| k.as_str() != "div_cycles")
            .map(|(k, &v)| {
                let resolved = if k.starts_with("__") {
                    k.clone()
                } else {
                    resolve_output(k, &plan.outputs)
                };
                (resolved, v)
            })
            .collect();
        let input_operands: Vec<OperandKey> = inputs
            .iter()
            .map(|name| OperandKey::from_resolved_name(name))
            .collect();
        let output_operands: Vec<OperandKey> = outputs
            .iter()
            .map(|name| OperandKey::from_resolved_name(name))
            .collect();
        let latencies_by_operand: BTreeMap<OperandKey, u32> = latencies
            .iter()
            .map(|(name, &latency)| (OperandKey::from_resolved_name(name), latency))
            .collect();

        let prop = UopProperties {
            possible_ports,
            div_cycles: plan.latencies.get("div_cycles").copied().unwrap_or(0),
            is_load_uop: plan.is_load,
            is_store_address_uop: plan.is_store_address,
            is_store_data_uop: plan.is_store_data,
            is_first_uop_of_instr: is_first,
            is_last_uop_of_instr: is_last,
            is_reg_merge_uop: false,
            is_serializing_instr: instr.is_serializing_instr,
            input_reg_operands: inputs,
            output_reg_operands: outputs,
            may_be_eliminated: instr.may_be_eliminated,
            latencies,
            input_operands,
            output_operands,
            latencies_by_operand,
            instr_tp: instr.instr_tp,
            instr_str: instr.instr_str.clone(),
            immediate: instr.immediate,
            is_load_serializing: instr.is_load_serializing,
            is_store_serializing: instr.is_store_serializing,
            mem_addr: selected_mem_addr,
        };

        let uop_idx = *uop_idx_counter;
        *uop_idx_counter += 1;
        let uop = Uop {
            idx: uop_idx,
            queue_idx: uop_idx,
            prop,
            actual_port: None,
            eliminated: false,
            ready_for_dispatch: None,
            dispatched: None,
            executed: None,
            lat_reduced_due_to_fast_ptr_chasing: false,
            renamed_input_operands: vec![],
            renamed_output_operands: vec![],
            store_buffer_entry: None,
            fused_uop_idx: None,
            instr_instance_idx: instr.idx,
        };
        storage.add_uop(uop);
        unfused_domain_uops.push_back(uop_idx);
    }

    // Python parity: `InstrInstance.__generateUops` first groups unfused
    // uops into fused-domain uops using `retireSlots`, then groups those into
    // laminated-domain uops using `uopsMITE + uopsMS`. Memory-domain uops on
    // ports 2/3/7 may pull the following uop into the same fused/laminated
    // object. This preserves Python's ROB/IDQ issue shape for stores/loads.
    let mut fused_domain_uops = VecDeque::new();
    let retire_slots = instr.retire_slots.max(1) as usize;
    for i in 0..retire_slots.saturating_sub(1) {
        let Some(uop_idx) = unfused_domain_uops.pop_front() else {
            break;
        };
        let can_micro_fuse = storage.get_uop(uop_idx).is_some_and(|uop| {
            !uop.prop.possible_ports.is_empty()
                && uop
                    .prop
                    .possible_ports
                    .iter()
                    .any(|p| matches!(p.as_str(), "2" | "3" | "7"))
        }) && unfused_domain_uops.len() >= retire_slots - i;

        let mut members = vec![uop_idx];
        if can_micro_fuse {
            if let Some(next_uop_idx) = unfused_domain_uops.pop_front() {
                members.push(next_uop_idx);
            }
        }

        let fused_idx = *fused_idx_counter;
        *fused_idx_counter += 1;
        for &member in &members {
            if let Some(uop) = storage.get_uop_mut(member) {
                uop.fused_uop_idx = Some(fused_idx);
            }
        }
        storage.add_fused_uop(FusedUop {
            idx: fused_idx,
            unfused_uop_idxs: members,
            laminated_uop_idx: None,
            issued: None,
            retired: None,
            retire_idx: None,
        });
        fused_domain_uops.push_back(fused_idx);
    }

    if !unfused_domain_uops.is_empty() {
        let members: Vec<u64> = unfused_domain_uops.drain(..).collect();
        let fused_idx = *fused_idx_counter;
        *fused_idx_counter += 1;
        for &member in &members {
            if let Some(uop) = storage.get_uop_mut(member) {
                uop.fused_uop_idx = Some(fused_idx);
            }
        }
        storage.add_fused_uop(FusedUop {
            idx: fused_idx,
            unfused_uop_idxs: members,
            laminated_uop_idx: None,
            issued: None,
            retired: None,
            retire_idx: None,
        });
        fused_domain_uops.push_back(fused_idx);
    }

    let n_laminated_domain_uops =
        ((instr.uops_mite + instr.uops_ms) as usize).min(fused_domain_uops.len());
    if n_laminated_domain_uops == 0 {
        return lam_idxs;
    }

    for i in 0..n_laminated_domain_uops.saturating_sub(1) {
        let Some(fused_idx) = fused_domain_uops.pop_front() else {
            break;
        };
        let can_laminate = storage.get_fused_uop(fused_idx).is_some_and(|fused| {
            fused.unfused_uop_idxs.len() == 1
                && fused.unfused_uop_idxs.first().is_some_and(|uop_idx| {
                    storage.get_uop(*uop_idx).is_some_and(|uop| {
                        !uop.prop.possible_ports.is_empty()
                            && uop
                                .prop
                                .possible_ports
                                .iter()
                                .any(|p| matches!(p.as_str(), "2" | "3" | "7"))
                    })
                })
        }) && fused_domain_uops.len() >= n_laminated_domain_uops - i;

        let mut members = vec![fused_idx];
        if can_laminate {
            if let Some(next_fused_idx) = fused_domain_uops.pop_front() {
                members.push(next_fused_idx);
            }
        }

        let lam_idx = *lam_idx_counter;
        *lam_idx_counter += 1;
        for &member in &members {
            if let Some(fused) = storage.get_fused_uop_mut(member) {
                fused.laminated_uop_idx = Some(lam_idx);
            }
        }
        storage.add_laminated_uop(LaminatedUop {
            idx: lam_idx,
            fused_uop_idxs: members,
            added_to_idq: None,
            uop_source: None,
            instr_instance_idx: instr.idx,
        });
        lam_idxs.push(lam_idx);
    }

    if !fused_domain_uops.is_empty() {
        let members: Vec<u64> = fused_domain_uops.drain(..).collect();
        let lam_idx = *lam_idx_counter;
        *lam_idx_counter += 1;
        for &member in &members {
            if let Some(fused) = storage.get_fused_uop_mut(member) {
                fused.laminated_uop_idx = Some(lam_idx);
            }
        }
        storage.add_laminated_uop(LaminatedUop {
            idx: lam_idx,
            fused_uop_idxs: members,
            added_to_idq: None,
            uop_source: None,
            instr_instance_idx: instr.idx,
        });
        lam_idxs.push(lam_idx);
    }

    lam_idxs
}

#[cfg(test)]
mod tests {
    use super::{compute_uop_plans, compute_uop_plans_inner, python_decoder_shape_from_record};
    use std::collections::BTreeMap;
    use uica_data::{
        load_manifest_pack, DataPack, DataPackIndex, InstructionRecord, LatencyRecord,
        OperandRecord, PerfRecord, PerfVariantRecord,
    };

    fn operand(name: &str, kind: &str, read: bool, write: bool) -> OperandRecord {
        OperandRecord {
            name: name.to_string(),
            r#type: kind.to_string(),
            read,
            write,
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
        }
    }

    #[test]
    fn legacy_adc_sbb_decoder_shape_recovers_python_sdec() {
        let record = InstructionRecord {
            arch: "HSW".to_string(),
            iform: "SBB_GPRv_GPRv_19".to_string(),
            string: "SBB_19 (R32, R32)".to_string(),
            imm_zero: false,
            perf: PerfRecord {
                uops: 2,
                retire_slots: 2,
                uops_mite: 2,
                uops_ms: 0,
                tp: None,
                ports: BTreeMap::from([("0156".to_string(), 1), ("06".to_string(), 1)]),
                variants: Default::default(),
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
        };

        assert_eq!(
            python_decoder_shape_from_record(&record, &record.perf, 4),
            (true, 2)
        );
    }

    #[test]
    fn unmatched_iform_emits_unknown_instr_zero_port_uop() {
        let pack = DataPack {
            schema_version: uica_data::DATAPACK_SCHEMA_VERSION.to_string(),
            instructions: vec![InstructionRecord {
                arch: "HSW".to_string(),
                iform: "MULX_GPR64q_GPR64q_GPR64q".to_string(),
                string: "MULX (R64, R64, R64)".to_string(),
                imm_zero: false,
                perf: PerfRecord {
                    uops: 2,
                    retire_slots: 2,
                    uops_mite: 2,
                    uops_ms: 0,
                    tp: None,
                    ports: BTreeMap::from([("06".to_string(), 1), ("1".to_string(), 1)]),
                    variants: Default::default(),
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
        };
        let index = DataPackIndex::new(pack.clone());
        let mut instr = super::super::types::InstrInstance::new(
            0,
            0,
            0,
            0,
            5,
            "mulx".to_string(),
            "mulx rax, rbx, rcx".to_string(),
        );
        instr.iform_signature = "VGPR64q_VGPR64q_VGPR64q".to_string();
        instr.max_op_size_bytes = 8;
        instr.uops_mite = 1;
        instr.retire_slots = 1;

        let mut storage = super::super::uop_storage::UopStorage::new();
        let mut uop_idx = 0;
        let mut fused_idx = 0;
        let mut lam_idx = 0;
        let lam_idxs = super::expand_instr_instance_to_lam_uops_with_storage(
            &instr,
            &mut uop_idx,
            &mut fused_idx,
            &mut lam_idx,
            &mut storage,
            "HSW",
            &pack,
            Some(&index),
        )
        .expect("unknown instruction expansion should succeed");

        assert_eq!(lam_idxs.len(), 1);
        let uop = storage.get_uop(0).expect("zero-port uop");
        assert!(uop.prop.possible_ports.is_empty());
        assert!(uop.prop.input_operands.is_empty());
        assert!(uop.prop.output_operands.is_empty());
    }

    #[test]
    fn cmov_latency_class_pseudo_uop_precedes_base_uop() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../uica-data/generated/manifest.json");
        let pack = load_manifest_pack(manifest, "HSW").unwrap();
        let index = DataPackIndex::new(pack);
        let record = index
            .candidates_for("HSW", "CMOVG")
            .iter()
            .find(|record| record.iform == "CMOVNLE_GPRv_GPRv")
            .expect("CMOVNLE_GPRv_GPRv record");

        let plans = compute_uop_plans(record, "HSW");

        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].ports, vec!["0", "6"]);
        assert_eq!(plans[0].inputs, vec!["REG1", "SPAZO"]);
        assert!(plans[0].outputs.iter().all(|op| op.starts_with("__P_")));
        assert_eq!(plans[1].ports, vec!["0", "1", "5", "6"]);
        assert!(plans[1].inputs.iter().any(|op| op.starts_with("__P_")));
    }

    #[test]
    fn eliminated_mov_uses_same_reg_latency_for_fallback_uop() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../uica-data/generated/manifest.json");
        let pack = load_manifest_pack(manifest, "HSW").unwrap();
        let index = DataPackIndex::new(pack);
        let record = index
            .candidates_for("HSW", "MOV")
            .iter()
            .find(|record| {
                super::record_may_be_eliminated(record) && record.string == "MOV_89 (R64, R64)"
            })
            .expect("eliminable MOV_89 (R64, R64) record");

        assert!(super::record_may_be_eliminated(record));
        let plans = compute_uop_plans(record, "HSW");

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].latencies.get("REG0"), Some(&1));
    }

    #[test]
    fn non_same_reg_form_ignores_cycles_same_reg_latency() {
        let record = InstructionRecord {
            arch: "HSW".to_string(),
            iform: "SUB_GPRv_GPRv_29".to_string(),
            string: "SUB_29 (R64, R64)".to_string(),
            imm_zero: false,
            perf: PerfRecord {
                uops: 1,
                retire_slots: 1,
                uops_mite: 1,
                uops_ms: 0,
                tp: None,
                ports: BTreeMap::from([("0156".to_string(), 1)]),
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
                operands: vec![
                    operand("REG0", "reg", true, true),
                    operand("REG1", "reg", true, false),
                ],
                latencies: vec![LatencyRecord {
                    start_op: "REG0".to_string(),
                    target_op: "REG0".to_string(),
                    cycles: 1,
                    cycles_addr: None,
                    cycles_addr_index: None,
                    cycles_mem: None,
                    cycles_same_reg: Some(0),
                }],
                variants: Default::default(),
            },
        };
        let mut instr = super::super::types::InstrInstance::new(
            0,
            0,
            0,
            0,
            3,
            "sub".to_string(),
            "sub rcx, rdx".to_string(),
        );
        instr.input_regs = vec!["RCX".to_string(), "RDX".to_string()];
        instr.output_regs = vec!["RCX".to_string()];
        instr.explicit_reg_operands = vec!["RCX".to_string(), "RDX".to_string()];

        let plans = compute_uop_plans_inner(&record, "HSW", Some(&instr));

        assert_eq!(plans[0].latencies.get("REG0"), Some(&1));
    }

    #[test]
    fn same_reg_perf_variant_overrides_base_ports() {
        let record = InstructionRecord {
            arch: "HSW".to_string(),
            iform: "PCMPGTB_XMMdq_XMMdq".to_string(),
            string: "PCMPGTB (XMM, XMM)".to_string(),
            imm_zero: false,
            perf: PerfRecord {
                uops: 1,
                retire_slots: 1,
                uops_mite: 1,
                uops_ms: 0,
                tp: None,
                ports: BTreeMap::from([("15".to_string(), 1)]),
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
                operands: vec![
                    operand("REG0", "reg", false, true),
                    operand("REG1", "reg", true, false),
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
                variants: BTreeMap::from([(
                    "same_reg".to_string(),
                    PerfVariantRecord {
                        uops: Some(0),
                        retire_slots: Some(1),
                        uops_mite: None,
                        uops_ms: None,
                        tp: None,
                        ports: Some(BTreeMap::new()),
                        div_cycles: None,
                        complex_decoder: None,
                        n_available_simple_decoders: None,
                    },
                )]),
            },
        };
        let mut instr = super::super::types::InstrInstance::new(
            0,
            0,
            0,
            0,
            4,
            "pcmpgtb".to_string(),
            "pcmpgtb xmm0, xmm0".to_string(),
        );
        instr.input_regs = vec!["XMM0".to_string()];
        instr.output_regs = vec!["XMM0".to_string()];
        instr.explicit_reg_operands = vec!["XMM0".to_string(), "XMM0".to_string()];

        let plans = compute_uop_plans_inner(&record, "HSW", Some(&instr));

        assert_eq!(plans.len(), 1);
        assert!(plans[0].ports.is_empty());
    }

    #[test]
    fn explicit_duplicate_reg_operands_preserve_duplicate_input_roles() {
        let record = InstructionRecord {
            arch: "ICL".to_string(),
            iform: "KANDW_MASKmskw_MASKmskw_MASKmskw_AVX512".to_string(),
            string: "KANDW (K, K, K)".to_string(),
            imm_zero: false,
            perf: PerfRecord {
                uops: 1,
                retire_slots: 1,
                uops_mite: 1,
                uops_ms: 0,
                tp: None,
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
                operands: vec![
                    operand("REG0", "reg", false, true),
                    operand("REG1", "reg", true, false),
                    operand("REG2", "reg", true, false),
                ],
                latencies: vec![
                    LatencyRecord {
                        start_op: "REG1".to_string(),
                        target_op: "REG0".to_string(),
                        cycles: 1,
                        cycles_addr: None,
                        cycles_addr_index: None,
                        cycles_mem: None,
                        cycles_same_reg: None,
                    },
                    LatencyRecord {
                        start_op: "REG2".to_string(),
                        target_op: "REG0".to_string(),
                        cycles: 1,
                        cycles_addr: None,
                        cycles_addr_index: None,
                        cycles_mem: None,
                        cycles_same_reg: None,
                    },
                ],
                variants: Default::default(),
            },
        };
        let pack = DataPack {
            schema_version: uica_data::DATAPACK_SCHEMA_VERSION.to_string(),
            instructions: vec![record],
        };
        let index = DataPackIndex::new(pack.clone());
        let mut instr = super::super::types::InstrInstance::new(
            0,
            0,
            0,
            0,
            4,
            "kandw".to_string(),
            "kandw k2, k1, k1".to_string(),
        );
        instr.iform_signature = "MASKmskw_MASKmskw_MASKmskw_AVX512".to_string();
        instr.max_op_size_bytes = 8;
        instr.input_regs = vec!["K1".to_string()];
        instr.output_regs = vec!["K2".to_string()];
        instr.explicit_reg_operands = vec!["K2".to_string(), "K1".to_string(), "K1".to_string()];
        instr.uops_mite = 1;
        instr.retire_slots = 1;

        let mut storage = super::super::uop_storage::UopStorage::new();
        let mut uop_idx = 0;
        let mut fused_idx = 0;
        let mut lam_idx = 0;
        super::expand_instr_instance_to_lam_uops_with_storage(
            &instr,
            &mut uop_idx,
            &mut fused_idx,
            &mut lam_idx,
            &mut storage,
            "ICL",
            &pack,
            Some(&index),
        )
        .expect("expand should succeed");

        let uop = storage.get_uop(0).expect("uop should exist");
        assert_eq!(
            uop.prop.input_operands,
            vec![
                super::super::types::OperandKey::Reg("K1".to_string()),
                super::super::types::OperandKey::Reg("K1".to_string()),
            ]
        );
    }

    #[test]
    fn lfence_zero_port_plans_keep_retire_slot_padding() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../uica-data/generated/manifest.json");
        let pack = load_manifest_pack(manifest, "HSW").unwrap();
        let index = DataPackIndex::new(pack);
        let record = index
            .candidates_for("HSW", "LFENCE")
            .iter()
            .find(|record| record.iform == "LFENCE")
            .expect("LFENCE record");

        let plans = compute_uop_plans(record, "HSW");

        assert_eq!(record.perf.retire_slots, 2);
        assert_eq!(super::record_uops_mite(record), 1);
        assert_eq!(plans.len(), 2);
        assert!(plans.iter().all(|plan| plan.ports.is_empty()));
    }

    #[test]
    fn shift_by_cl_cw_flags_feed_special_pseudo_uop() {
        let mut ports = BTreeMap::new();
        ports.insert("06".to_string(), 3);
        let mut flags = operand("REG2", "flags", false, true);
        flags.flags = vec!["SPAZO".to_string(), "C".to_string()];
        flags.flags_read = vec!["C".to_string(), "SPAZO".to_string()];
        flags.flags_write = vec!["C".to_string(), "SPAZO".to_string()];

        let record = InstructionRecord {
            arch: "HSW".to_string(),
            iform: "SHL_GPRv_CL_D3r4".to_string(),
            string: "SHL (R64, CL)".to_string(),
            imm_zero: false,
            perf: PerfRecord {
                uops: 3,
                retire_slots: 3,
                uops_mite: 3,
                uops_ms: 0,
                tp: None,
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
                operands: vec![
                    operand("REG0", "reg", true, true),
                    operand("REG1", "reg", true, false),
                    flags,
                ],
                latencies: vec![
                    LatencyRecord {
                        start_op: "REG0".to_string(),
                        target_op: "REG0".to_string(),
                        cycles: 1,
                        cycles_addr: None,
                        cycles_addr_index: None,
                        cycles_mem: None,
                        cycles_same_reg: None,
                    },
                    LatencyRecord {
                        start_op: "REG0".to_string(),
                        target_op: "REG2".to_string(),
                        cycles: 2,
                        cycles_addr: None,
                        cycles_addr_index: None,
                        cycles_mem: None,
                        cycles_same_reg: None,
                    },
                    LatencyRecord {
                        start_op: "REG1".to_string(),
                        target_op: "REG0".to_string(),
                        cycles: 1,
                        cycles_addr: None,
                        cycles_addr_index: None,
                        cycles_mem: None,
                        cycles_same_reg: None,
                    },
                    LatencyRecord {
                        start_op: "REG1".to_string(),
                        target_op: "REG2".to_string(),
                        cycles: 2,
                        cycles_addr: None,
                        cycles_addr_index: None,
                        cycles_mem: None,
                        cycles_same_reg: None,
                    },
                    LatencyRecord {
                        start_op: "REG2".to_string(),
                        target_op: "REG0".to_string(),
                        cycles: 0,
                        cycles_addr: None,
                        cycles_addr_index: None,
                        cycles_mem: None,
                        cycles_same_reg: None,
                    },
                    LatencyRecord {
                        start_op: "REG2".to_string(),
                        target_op: "REG2".to_string(),
                        cycles: 2,
                        cycles_addr: None,
                        cycles_addr_index: None,
                        cycles_mem: None,
                        cycles_same_reg: None,
                    },
                ],
                variants: Default::default(),
            },
        };

        let plans = compute_uop_plans(&record, "HSW");

        assert_eq!(plans.len(), 3);
        assert_eq!(plans[0].inputs, vec!["REG0", "REG1"]);
        assert!(plans[0].outputs.iter().any(|op| op.starts_with("__P_")));
        assert_eq!(plans[1].inputs, vec!["C", "SPAZO"]);
        assert_eq!(plans[1].outputs.len(), 1);
        assert!(plans[1].outputs[0].starts_with("__P_"));
        assert_eq!(plans[2].inputs.len(), 2);
        assert!(plans[2].inputs.iter().all(|op| op.starts_with("__P_")));
        assert_eq!(plans[2].outputs, vec!["C", "SPAZO"]);
    }
}
