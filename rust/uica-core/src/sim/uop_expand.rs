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
    compute_uop_plans_inner(record, arch_name)
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
// is_mnemonic_supported
// ---------------------------------------------------------------------------

/// True iff we can produce a non-empty plan for this instruction.
pub fn is_mnemonic_supported(
    mnemonic: &str,
    is_macro_fused_next: bool,
    is_macro_fused_prev: bool,
    has_memory: bool,
    arch_name: &str,
    pack: &uica_data::DataPack,
) -> bool {
    if is_macro_fused_prev {
        return true;
    }
    let _ = (is_macro_fused_next, has_memory);
    // Zero-idiom mnemonics are always supported; they produce 0 real uops.
    let m = mnemonic.to_ascii_lowercase();
    if ["xor", "sub", "pxor", "vxorps", "vxorpd", "vpxor"].contains(&m.as_str()) {
        return true;
    }
    use crate::matcher::{match_instruction_record, NormalizedInstr};
    use uica_data::DataPackIndex;
    let index = DataPackIndex::new(pack.clone());
    let norm = NormalizedInstr {
        mnemonic: mnemonic.to_string(),
        iform_signature: String::new(),
        max_op_size_bytes: 0,
    };
    let candidates = index.candidates_for(&arch_name.to_ascii_uppercase(), mnemonic);
    match_instruction_record(&norm, candidates).is_some()
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
    };
    let candidates = index.candidates_for(&arch_name.to_ascii_uppercase(), mnemonic);
    match match_instruction_record(&norm, candidates) {
        Some(rec) => {
            let mite = if rec.perf.uops_mite > 0 {
                rec.perf.uops_mite as u32
            } else {
                rec.perf.uops.max(0) as u32
            };
            (mite, rec.perf.uops_ms as u32)
        }
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
    };
    let candidates = index.candidates_for(&arch_name.to_ascii_uppercase(), &instr.mnemonic);
    let record = match match_instruction_record(&norm, candidates) {
        Some(rec) => rec,
        None => {
            return Err(format!(
                "no DataPack record for {}: {}",
                arch_name, instr.mnemonic
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
        ));
    }

    let plans = compute_uop_plans_inner(record, arch_name);

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
    _arch_name: &str,
) -> Vec<UopPlan> {
    let perf = &record.perf;

    // --- Port classification (mirrors Python's portData loop) ---
    // ports string "06" → [0,6]; "23" → [2,3]; "78" → [7,8]; "49" → [4,9]
    let mut load_pcs: Vec<Vec<String>> = Vec::new();
    let mut store_addr_pcs: Vec<Vec<String>> = Vec::new();
    let mut store_data_pcs: Vec<Vec<String>> = Vec::new();
    let mut non_mem_pcs: Vec<Vec<String>> = Vec::new();

    let mut sorted_ports: Vec<(&String, &i32)> = perf.ports.iter().collect();
    sorted_ports.sort_by_key(|(k, _)| k.as_str());
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
                if op.read {
                    input_reg_ops.extend(read_names.iter().cloned());
                }
                if op.write {
                    output_reg_ops.extend(write_names.iter().cloned());
                }
            }
            "flags" => {
                if op.read {
                    input_flag_ops.extend(read_names.iter().cloned());
                }
                if op.write {
                    output_flag_ops.extend(write_names.iter().cloned());
                }
            }
            "mem" => {
                let role = op.mem_operand_role.as_deref();
                let has_addr_metadata = op.mem_base.is_some()
                    || op.mem_index.is_some()
                    || op.mem_scale.is_some()
                    || op.mem_disp.is_some();
                if op.is_agen || role == Some("agen") || role == Some("address") {
                    agen_ops.extend(concrete_names.iter().cloned());
                }
                if op.is_agen
                    || role == Some("agen")
                    || role == Some("address")
                    || has_addr_metadata
                {
                    mem_addr_ops.extend(concrete_names.iter().cloned());
                }
                if op.read || matches!(role, Some("read") | Some("read_write")) {
                    input_mem_ops.extend(read_names.iter().cloned());
                }
                if op.write || matches!(role, Some("write") | Some("read_write")) {
                    output_mem_ops.extend(write_names.iter().cloned());
                }
            }
            _ => {} // imm handled separately
        }
    }

    // --- Build latency dict (inOp, outOp) -> cycles ---
    let mut lat_dict: HashMap<(String, String), i32> = HashMap::new();
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
                lat_dict.insert((start_op.clone(), target_op.clone()), lr.cycles);
                if let Some(sr) = lr.cycles_same_reg {
                    // lat_SR: use for same-register special case (Python's lat_SR dict).
                    // We use the lower of normal vs same-reg cycles.
                    let cur = lat_dict
                        .entry((start_op.clone(), target_op.clone()))
                        .or_insert(lr.cycles);
                    *cur = (*cur).min(sr);
                }
            }
        }
    }

    // --- No ports at all → zero-port (move-eliminated) or NOP ---
    // Still create uops_mite zero-port lam uops for IDQ bookkeeping.
    if non_mem_pcs.is_empty() && load_pcs.is_empty() && store_addr_pcs.is_empty() {
        let n = perf.uops_mite.max(1) as usize;
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

    let mut extras: Vec<UopPlan> = Vec::new();

    for (extra_idx, pc) in non_mem_pcs.iter().enumerate().skip(1) {
        if let Some(lat_level) = remaining_levels.pop_front() {
            let lat_class = lat_classes.get(&lat_level).cloned().unwrap_or_default();
            let pseudo = next_pseudo();
            base_plan.inputs.push(pseudo.clone());
            let delay = (lat_level - min_lat_level).max(0) as u32;
            let mut extra_lat = BTreeMap::new();
            extra_lat.insert(pseudo.clone(), delay);
            extras.push(UopPlan {
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
            extras.push(UopPlan {
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
        let _ = extra_idx;
    }

    // Append any remaining latency-class inputs to the last extra (or base).
    while let Some(lat_level) = remaining_levels.pop_front() {
        if let Some(lat_class) = lat_classes.get(&lat_level) {
            if let Some(last) = extras.last_mut().or(Some(&mut base_plan)) {
                last.inputs.extend(lat_class.iter().cloned());
            }
        }
    }

    // Python uses appendleft for extras; result is [extra_last..extra_first, base].
    let mut plans: Vec<UopPlan> = extras.into_iter().rev().collect();
    plans.push(base_plan);
    plans
}

// ---------------------------------------------------------------------------
// Emit lam uops from plans
// ---------------------------------------------------------------------------

fn emit_lam_uops(
    plans: &[UopPlan],
    instr: &InstrInstance,
    uop_idx_counter: &mut u64,
    fused_idx_counter: &mut u64,
    lam_idx_counter: &mut u64,
    storage: &mut UopStorage,
    arch_name: &str,
) -> Vec<u64> {
    let n = plans.len();
    let mut lam_idxs = Vec::with_capacity(n);

    // Build a map from DataPack operand placeholder names (REG0, REG1, REG2)
    // to actual decoded register names (RAX, R15, RFLAGS, ...).
    //
    // The DataPack uses generic names keyed by XML operand order.
    // The decoder gives us actual registers split into:
    //   input_regs:   registers that are read
    //   output_regs:  registers that are written
    //   reads_flags / writes_flags: RFLAGS involvement
    //
    // We build the map by walking the InstrInstance fields.
    let placeholder_to_real: HashMap<String, String> = HashMap::new();
    // We don't have per-operand decoded names mapped to REGn names here,
    // but the operand resolver below handles this positionally.
    let _ = placeholder_to_real;

    // Resolve a DataPack operand placeholder name to an actual canonical register.
    // - Pseudo-ops (__P_N): pass through unchanged.
    // - Flag operand names: resolve to RFLAGS.
    // - REGn names: resolve to actual registers from InstrInstance using order.
    // For input operands: use instr.input_regs in order, then RFLAGS.
    // For output operands: use instr.output_regs in order, then RFLAGS.
    let resolve_name =
        |name: &str, all_ops: &[String], decoded: &[String], flag_name: &str| -> String {
            if name.starts_with("__") || matches!(name, "C" | "SPAZO") {
                return name.to_string();
            }
            // Pseudo-operand role: flag operand if the DataPack name is not in the
            // decoded register list AND we have a flag involved.
            let is_reg_placeholder = name.to_ascii_uppercase().starts_with("REG")
                && name.len() > 3
                && name[3..].chars().all(|c| c.is_ascii_digit());
            if is_reg_placeholder {
                // Find index of this REGn among all REGn names in the ops list.
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
                // Couldn't find in decoded regs; fall back to RFLAGS if flag involved.
                if !flag_name.is_empty() {
                    return flag_name.to_string();
                }
                return name.to_string();
            }
            // Real register name: canonicalize.
            crate::x64::get_canonical_reg(name)
        };

    let decoded_inputs: Vec<String> = instr
        .input_regs
        .iter()
        .map(|r| crate::x64::get_canonical_reg(r))
        .collect();
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
            .map(|s| resolve_name(s, &plan.inputs, &decoded_inputs, flag_str))
            .collect();
        let selected_mem_addr = plan.mem_addr.clone().or_else(|| {
            if plan.is_load || plan.is_store_address {
                plan.mem_addr_index.and_then(|idx| {
                    instr
                        .mem_addrs
                        .get(idx.min(instr.mem_addrs.len().saturating_sub(1)))
                        .cloned()
                })
            } else {
                None
            }
        });
        if plan.is_load || plan.is_store_address {
            if let Some(mem_addr) = selected_mem_addr.as_ref() {
                if let Some(base) = &mem_addr.base {
                    let base = crate::x64::get_canonical_reg(base);
                    if !inputs.contains(&base) {
                        inputs.push(base);
                    }
                }
                if let Some(index) = &mem_addr.index {
                    let index = crate::x64::get_canonical_reg(index);
                    if !inputs.contains(&index) {
                        inputs.push(index);
                    }
                }
            }
        }
        // Resolve outputs: use output_regs for positional REGn names.
        let outputs: Vec<String> = plan
            .outputs
            .iter()
            .map(|s| resolve_name(s, &plan.outputs, &decoded_outputs, flag_str))
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
                    resolve_name(k, &plan.outputs, &decoded_outputs, flag_str)
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
        let fused_idx = *fused_idx_counter;
        *fused_idx_counter += 1;
        let lam_idx = *lam_idx_counter;
        *lam_idx_counter += 1;

        let uop = Uop {
            idx: uop_idx,
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
            fused_uop_idx: Some(fused_idx),
            instr_instance_idx: instr.idx,
        };
        let fused = FusedUop {
            idx: fused_idx,
            unfused_uop_idxs: vec![uop_idx],
            laminated_uop_idx: Some(lam_idx),
            issued: None,
            retired: None,
            retire_idx: None,
        };
        let lam = LaminatedUop {
            idx: lam_idx,
            fused_uop_idxs: vec![fused_idx],
            added_to_idq: None,
            uop_source: None,
            instr_instance_idx: instr.idx,
        };
        storage.add_uop(uop);
        storage.add_fused_uop(fused);
        storage.add_laminated_uop(lam);
        lam_idxs.push(lam_idx);
    }

    lam_idxs
}
