//! Renamer — structural port of Python `class Renamer`.
//!
//! This keeps Python's core rename semantics: per-instruction deferred output
//! commits, pseudo operands visible inside one instruction, move elimination
//! accounting, and store-buffer key plumbing.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use crate::micro_arch::{MicroArchConfig, MoveElimSlots};
use crate::x64::{get_canonical_reg, get_reg_size, is_gp_reg};

use super::reorder_buffer::ReorderBuffer;
use super::types::{
    next_renamed_operand_identity, share, AbstractValueKey, InstrInstance, MemAddr, OperandKey,
    RenamedOperand, Shared, StoreBufferEntry,
};
use super::uop_storage::UopStorage;

#[derive(Clone)]
pub struct RenameDictEntry {
    pub renamed_op: Shared<RenamedOperand>,
    pub renamed_by_elim_32_bit_move: bool,
}

type StoreBufferKey = (Option<AbstractValueKey>, Option<AbstractValueKey>, i32, i64);

pub struct AbstractValueGenerator {
    init_policy: String,
    next_value: u64,
    init_value: AbstractValueKey,
    values: HashMap<String, AbstractValueKey>,
    cur_instr_values: HashMap<String, AbstractValueKey>,
}

impl AbstractValueGenerator {
    pub fn new(init_policy: impl Into<String>) -> Self {
        let init_policy = init_policy.into();
        let mut gen = Self {
            init_policy,
            next_value: 0,
            init_value: (0, 0),
            values: HashMap::new(),
            cur_instr_values: HashMap::new(),
        };
        gen.init_value = gen.generate_fresh_abstract_value();
        if gen.init_policy == "stack" {
            let rsp = gen.generate_fresh_abstract_value();
            let rbp = gen.generate_fresh_abstract_value();
            gen.values.insert("RSP".to_string(), rsp);
            gen.values.insert("RBP".to_string(), rbp);
        }
        gen
    }

    pub fn get_abstract_value_for_reg(&mut self, reg: Option<&String>) -> Option<AbstractValueKey> {
        let reg = reg?;
        let key = get_canonical_reg(reg);
        if !self.values.contains_key(&key) {
            let value = if self.init_policy == "diff" {
                self.generate_fresh_abstract_value()
            } else {
                self.init_value
            };
            self.values.insert(key.clone(), value);
        }
        self.values.get(&key).copied()
    }

    pub fn set_abstract_value_for_cur_instr(
        &mut self,
        key: String,
        instr_str: &str,
        input_ops: &[OperandKey],
        immediate: Option<i64>,
    ) {
        let value = self.compute_abstract_value(instr_str, input_ops, immediate);
        self.cur_instr_values.insert(key, value);
    }

    pub fn finish_cur_instr(&mut self) {
        for (key, value) in self.cur_instr_values.drain() {
            self.values.insert(key, value);
        }
    }

    fn generate_fresh_abstract_value(&mut self) -> AbstractValueKey {
        let value = (self.next_value, 0);
        self.next_value += 1;
        value
    }

    fn compute_abstract_value(
        &mut self,
        instr_str: &str,
        input_ops: &[OperandKey],
        immediate: Option<i64>,
    ) -> AbstractValueKey {
        let first_reg = input_ops.iter().find_map(|op| match op {
            OperandKey::Reg(reg) => Some(reg.clone()),
            _ => None,
        });
        if let Some(reg) = first_reg {
            if let Some((base, offset)) = self.get_abstract_value_for_reg(Some(&reg)) {
                if instr_str.contains("MOV") && !instr_str.contains("CMOV") {
                    return (base, offset);
                }
                if instr_str.contains("ADD") {
                    if let Some(imm) = immediate {
                        return (base, offset + imm);
                    }
                }
                if instr_str.contains("SUB") {
                    if let Some(imm) = immediate {
                        return (base, offset - imm);
                    }
                }
                if instr_str.contains("INC") {
                    return (base, offset + 1);
                }
                if instr_str.contains("DEC") {
                    return (base, offset - 1);
                }
            }
        }
        self.generate_fresh_abstract_value()
    }
}

pub struct Renamer {
    pub arch: MicroArchConfig,
    pub renamer_active_cycle: u32,
    pub rename_dict: HashMap<String, RenameDictEntry>,
    /// Pending rename_dict updates keyed by instr_instance_idx.
    /// Mirrors Python's curInstrRndRenameDict.
    pub pending_commits: HashMap<u64, HashMap<String, RenameDictEntry>>,
    pub pending_commit_eliminated: HashMap<u64, HashMap<String, bool>>,
    pub abs_val_gen: AbstractValueGenerator,

    pub n_gpr_move_elim_in_cycle: HashMap<u32, u32>,
    pub multi_use_gpr_dict: HashMap<u64, BTreeSet<String>>,
    pub multi_use_gpr_dict_use_in_cycle: HashMap<u32, u32>,
    pub n_simd_move_elim_in_cycle: HashMap<u32, u32>,
    pub multi_use_simd_dict: HashMap<u64, BTreeSet<String>>,
    pub multi_use_simd_dict_use_in_cycle: HashMap<u32, u32>,

    pub cur_store_buffer_entry: Option<Shared<StoreBufferEntry>>,
    pub store_buffer_entry_dict: HashMap<StoreBufferKey, Shared<StoreBufferEntry>>,
    pub last_reg_merge_issued: Option<u64>,
    /// Python parity: `Renamer.curInstrPseudoOpDict` persists pseudo operands
    /// across rename cycles until `isLastUopOfInstr` clears it.
    pub cur_instr_pseudo_op_dict: HashMap<OperandKey, Shared<RenamedOperand>>,
}

impl Renamer {
    pub fn new(arch: MicroArchConfig) -> Self {
        Self::new_with_init_policy(arch, "diff")
    }

    pub fn new_with_init_policy(arch: MicroArchConfig, init_policy: impl Into<String>) -> Self {
        Self {
            arch,
            renamer_active_cycle: 0,
            rename_dict: HashMap::new(),
            pending_commits: HashMap::new(),
            pending_commit_eliminated: HashMap::new(),
            abs_val_gen: AbstractValueGenerator::new(init_policy),
            n_gpr_move_elim_in_cycle: HashMap::new(),
            multi_use_gpr_dict: HashMap::new(),
            multi_use_gpr_dict_use_in_cycle: HashMap::new(),
            n_simd_move_elim_in_cycle: HashMap::new(),
            multi_use_simd_dict: HashMap::new(),
            multi_use_simd_dict_use_in_cycle: HashMap::new(),
            cur_store_buffer_entry: None,
            store_buffer_entry_dict: HashMap::new(),
            last_reg_merge_issued: None,
            cur_instr_pseudo_op_dict: HashMap::new(),
        }
    }

    pub fn cycle(
        &mut self,
        idq: &mut VecDeque<u64>,
        storage: &mut UopStorage,
        reorder_buffer: &ReorderBuffer,
        next_uop_queue_idx: &mut u64,
        all_generated_instr_instances: &mut [InstrInstance],
    ) -> Vec<u64> {
        self.renamer_active_cycle += 1;

        let mut fused_uop_idxs = Vec::new();
        let mut uops_to_issue = 0;

        while !idq.is_empty() && uops_to_issue < self.arch.issue_width {
            let lam_idx = idq[0];
            let fused_uops = storage.get_fused_uops_for_lam(lam_idx);
            let Some(first_fused) = fused_uops.first() else {
                break;
            };
            let Some(first_uop_idx) = first_fused.unfused_uop_idxs.first().copied() else {
                break;
            };
            let Some(first_uop) = storage.get_uop(first_uop_idx) else {
                break;
            };

            // Python parity: register-merge uops are renamer-injected, not
            // IDQ entries. When the first real uop of an instruction reaches
            // IDQ head, issue all merge uops as a standalone batch, leave the
            // real lam in IDQ, and use last_reg_merge_issued to avoid
            // reinjecting next cycle.
            if first_uop.prop.is_first_uop_of_instr && !first_uop.prop.is_reg_merge_uop {
                let merge_fused_idxs =
                    self.reg_merge_fused_idxs(first_uop.instr_instance_idx, storage);
                if !merge_fused_idxs.is_empty() {
                    if !fused_uop_idxs.is_empty() {
                        break;
                    }
                    if self.last_reg_merge_issued != Some(first_uop_idx) {
                        self.assign_reg_merge_queue_idxs(
                            &merge_fused_idxs,
                            storage,
                            next_uop_queue_idx,
                            all_generated_instr_instances,
                        );
                        fused_uop_idxs.extend(merge_fused_idxs);
                        self.last_reg_merge_issued = Some(first_uop_idx);
                        break;
                    }
                }
            }

            if first_uop.prop.is_first_uop_of_instr
                && first_uop.prop.is_serializing_instr
                && !reorder_buffer.is_empty()
            {
                break;
            }

            if uops_to_issue + fused_uops.len() as u32 > self.arch.issue_width {
                break;
            }

            idq.pop_front();
            for fused in fused_uops {
                fused_uop_idxs.push(fused.idx);
                uops_to_issue += 1;
            }
        }

        self.apply_move_elimination(&fused_uop_idxs, storage);
        self.rename_uops(&fused_uop_idxs, storage);
        self.finish_move_elim_cycle_bookkeeping();

        fused_uop_idxs
    }

    fn reg_merge_fused_idxs(&self, instr_instance_idx: u64, storage: &UopStorage) -> Vec<u64> {
        let mut lam_idxs: Vec<u64> = storage
            .laminated_uops
            .values()
            .filter(|lam| lam.instr_instance_idx == instr_instance_idx)
            .filter(|lam| {
                lam.fused_uop_idxs
                    .first()
                    .and_then(|fused_idx| storage.get_fused_uop(*fused_idx))
                    .and_then(|fused| fused.unfused_uop_idxs.first())
                    .and_then(|uop_idx| storage.get_uop(*uop_idx))
                    .is_some_and(|uop| uop.prop.is_reg_merge_uop)
            })
            .map(|lam| lam.idx)
            .collect();
        lam_idxs.sort_unstable();
        lam_idxs
            .into_iter()
            .filter_map(|lam_idx| storage.get_laminated_uop(lam_idx))
            .flat_map(|lam| lam.fused_uop_idxs.iter().copied())
            .collect()
    }

    fn assign_reg_merge_queue_idxs(
        &mut self,
        fused_uop_idxs: &[u64],
        storage: &mut UopStorage,
        next_uop_queue_idx: &mut u64,
        all_generated_instr_instances: &mut [InstrInstance],
    ) {
        // Python parity: `Renamer.cycle()` constructs `Uop(mergeProp, ...)`
        // when merge uops are injected. Rust stores merge uops earlier so the
        // IDQ can reference them; assign scheduler heap order at Python's
        // injection point instead of using early storage ids.
        for &fused_idx in fused_uop_idxs {
            let Some(fused) = storage.get_fused_uop(fused_idx).cloned() else {
                continue;
            };
            let lam_idx = fused.laminated_uop_idx;
            for uop_idx in fused.unfused_uop_idxs {
                let Some(uop) = storage.get_uop_mut(uop_idx) else {
                    continue;
                };
                if uop.prop.is_reg_merge_uop {
                    uop.queue_idx = *next_uop_queue_idx;
                    *next_uop_queue_idx += 1;
                    if let Some(lam_idx) = lam_idx {
                        if let Some(instr_i) = all_generated_instr_instances
                            .iter_mut()
                            .find(|instr_i| instr_i.idx == uop.instr_instance_idx)
                        {
                            if !instr_i.reg_merge_uops.contains(&lam_idx) {
                                instr_i.reg_merge_uops.push(lam_idx);
                            }
                        }
                    }
                }
            }
        }
    }

    fn apply_move_elimination(&mut self, fused_uop_idxs: &[u64], storage: &mut UopStorage) {
        let mut n_gpr = 0;
        let mut n_simd = 0;

        for &fused_idx in fused_uop_idxs {
            let Some(fused) = storage.get_fused_uop(fused_idx).cloned() else {
                continue;
            };
            for uop_idx in fused.unfused_uop_idxs {
                let can_eliminate = storage
                    .get_uop(uop_idx)
                    .map(|u| u.prop.may_be_eliminated && !u.prop.is_reg_merge_uop)
                    .unwrap_or(false);
                if !can_eliminate {
                    continue;
                }
                let input = storage
                    .get_uop(uop_idx)
                    .and_then(|u| u.prop.input_operands.first().cloned());
                let Some(OperandKey::Reg(input_reg)) = input else {
                    continue;
                };
                let canonical = get_canonical_reg(&input_reg);
                let possible = if is_gp_reg(&canonical) {
                    self.move_elim_slots_available(true, n_gpr)
                } else if canonical.contains("MM") {
                    self.move_elim_slots_available(false, n_simd)
                } else {
                    0
                };
                if possible > 0 {
                    if let Some(uop) = storage.get_uop_mut(uop_idx) {
                        uop.eliminated = true;
                    }
                    if is_gp_reg(&canonical) {
                        n_gpr += 1;
                    } else {
                        n_simd += 1;
                    }
                }
            }
        }

        if n_gpr == 0 && !self.arch.move_elim_gpr_all_aliases_must_be_overwritten {
            self.multi_use_gpr_dict
                .retain(|_, aliases| aliases.len() > 1);
        }
        if n_simd == 0 {
            self.multi_use_simd_dict
                .retain(|_, aliases| aliases.len() > 1);
        }

        self.n_gpr_move_elim_in_cycle
            .insert(self.renamer_active_cycle, n_gpr);
        self.n_simd_move_elim_in_cycle
            .insert(self.renamer_active_cycle, n_simd);
    }

    fn move_elim_slots_available(&self, gpr: bool, used_this_cycle: u32) -> u32 {
        let slots = if gpr {
            self.arch.move_elim_gpr_slots
        } else {
            self.arch.move_elim_simd_slots
        };
        match slots {
            MoveElimSlots::None => 0,
            MoveElimSlots::Unlimited => 1,
            MoveElimSlots::Finite(n) => {
                let hist = (1..self.arch.move_elim_pipeline_length)
                    .map(|i| {
                        if gpr {
                            self.n_gpr_move_elim_in_cycle
                                .get(&(self.renamer_active_cycle.saturating_sub(i)))
                        } else {
                            self.n_simd_move_elim_in_cycle
                                .get(&(self.renamer_active_cycle.saturating_sub(i)))
                        }
                        .copied()
                        .unwrap_or(0)
                    })
                    .sum::<u32>();
                let multi_use_pressure = if gpr {
                    self.multi_use_gpr_dict_use_in_cycle.get(
                        &self
                            .renamer_active_cycle
                            .saturating_sub(self.arch.move_elim_pipeline_length),
                    )
                } else {
                    self.multi_use_simd_dict_use_in_cycle.get(
                        &self
                            .renamer_active_cycle
                            .saturating_sub(self.arch.move_elim_pipeline_length),
                    )
                }
                .copied()
                .unwrap_or(0);
                n.saturating_sub(used_this_cycle + hist + multi_use_pressure)
            }
        }
    }

    fn rename_uops(&mut self, fused_uop_idxs: &[u64], storage: &mut UopStorage) {
        let mut instr_groups: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for &fused_idx in fused_uop_idxs {
            if let Some(fused) = storage.get_fused_uop(fused_idx) {
                if let Some(uop_idx) = fused.unfused_uop_idxs.first().copied() {
                    if let Some(uop) = storage.get_uop(uop_idx) {
                        instr_groups
                            .entry(uop.instr_instance_idx)
                            .or_default()
                            .push(fused_idx);
                    }
                }
            }
        }

        for group_fused_idxs in instr_groups.values() {
            let mut all_uop_idxs = Vec::new();
            for &fused_idx in group_fused_idxs {
                if let Some(fused) = storage.get_fused_uop(fused_idx) {
                    all_uop_idxs.extend(fused.unfused_uop_idxs.iter().copied());
                }
            }

            let mut pseudo_dict = std::mem::take(&mut self.cur_instr_pseudo_op_dict);
            let mut group_finished = false;

            for &uop_idx in &all_uop_idxs {
                let (
                    input_ops,
                    instr_input_ops,
                    output_ops,
                    eliminated,
                    inst_idx,
                    is_last,
                    is_zero_port,
                    mem_addr,
                    is_store_addr,
                    is_store_data,
                    is_load,
                    instr_str,
                    immediate,
                    output_reg_operands,
                ) = {
                    let Some(uop) = storage.get_uop(uop_idx) else {
                        continue;
                    };
                    (
                        uop.prop.input_operands.clone(),
                        uop.prop.instr_input_operands.clone(),
                        uop.prop.output_operands.clone(),
                        uop.eliminated,
                        uop.instr_instance_idx,
                        uop.prop.is_last_uop_of_instr || uop.prop.is_reg_merge_uop,
                        uop.prop.possible_ports.is_empty(),
                        uop.prop.mem_addr.clone(),
                        uop.prop.is_store_address_uop,
                        uop.prop.is_store_data_uop,
                        uop.prop.is_load_uop,
                        uop.prop.instr_str.clone(),
                        uop.prop.immediate,
                        uop.prop.output_reg_operands.clone(),
                    )
                };

                let mut renamed_inputs = Vec::new();
                for input in input_ops.iter() {
                    let renamed = if let Some(op) = pseudo_dict.get(input) {
                        op.clone()
                    } else if let Some(key) = self.rename_dict_key(input) {
                        self.rename_dict
                            .entry(key)
                            .or_insert_with(|| RenameDictEntry {
                                renamed_op: share(RenamedOperand::new()),
                                renamed_by_elim_32_bit_move: false,
                            })
                            .renamed_op
                            .clone()
                    } else {
                        share(RenamedOperand::new())
                    };
                    renamed_inputs.push(renamed);
                }

                let lat_reduced_due_to_fast_ptr_chasing =
                    self.lat_reduced_due_to_fast_ptr_chasing(is_load, mem_addr.as_ref(), storage);
                let sb_entry = self.update_store_buffer_for_uop(
                    is_store_addr,
                    is_store_data,
                    is_load,
                    mem_addr.as_ref(),
                    uop_idx,
                );

                let mut renamed_outputs = Vec::new();
                for (i, output) in output_ops.iter().enumerate() {
                    let renamed = if eliminated {
                        self.renamed_op_for_eliminated_move(&input_ops, output)
                    } else if is_zero_port && input_ops.is_empty() && !output_ops.is_empty() {
                        // Python parity: zero-uop instructions (e.g. `xor r,r`)
                        // still publish `RenamedOperand(uop=uop, ready=-1)` so
                        // later consumers keep dependency identity in traces.
                        share(RenamedOperand {
                            ready: Some(-1),
                            uop_idx: Some(uop_idx),
                            latency: None,
                            operand: Some(output.clone()),
                            identity: next_renamed_operand_identity(),
                        })
                    } else if is_zero_port {
                        renamed_inputs
                            .get(i)
                            .or_else(|| renamed_inputs.first())
                            .cloned()
                            .unwrap_or_else(|| share(RenamedOperand::new()))
                    } else {
                        let latency = storage.get_uop(uop_idx).and_then(|u| {
                            u.prop
                                .latencies_by_operand
                                .get(output)
                                .copied()
                                .or_else(|| {
                                    self.rename_dict_key(output)
                                        .and_then(|key| u.prop.latencies.get(&key).copied())
                                })
                        });
                        share(RenamedOperand {
                            ready: None,
                            uop_idx: Some(uop_idx),
                            latency,
                            operand: Some(output.clone()),
                            identity: next_renamed_operand_identity(),
                        })
                    };

                    if matches!(output, OperandKey::Pseudo(_)) {
                        pseudo_dict.insert(output.clone(), renamed.clone());
                    } else if let Some(key) = self.rename_dict_key(output) {
                        let renamed_by_elim_32_bit_move = eliminated
                            && output_reg_operands
                                .get(i)
                                .map(|reg| get_reg_size(reg) == 32)
                                .unwrap_or_else(|| {
                                    matches!(output, OperandKey::Reg(reg) if get_reg_size(reg) == 32)
                                });
                        self.pending_commits.entry(inst_idx).or_default().insert(
                            key.clone(),
                            RenameDictEntry {
                                renamed_op: renamed.clone(),
                                renamed_by_elim_32_bit_move,
                            },
                        );
                        self.pending_commit_eliminated
                            .entry(inst_idx)
                            .or_default()
                            .insert(key.clone(), eliminated);
                        self.abs_val_gen.set_abstract_value_for_cur_instr(
                            key,
                            &instr_str,
                            &instr_input_ops,
                            immediate,
                        );
                    }
                    renamed_outputs.push(renamed);
                }

                if let Some(uop) = storage.get_uop_mut(uop_idx) {
                    uop.renamed_input_operands.extend(renamed_inputs);
                    uop.renamed_output_operands.extend(renamed_outputs);
                    uop.store_buffer_entry = sb_entry;
                    uop.lat_reduced_due_to_fast_ptr_chasing = lat_reduced_due_to_fast_ptr_chasing;
                }

                if is_last {
                    group_finished = true;
                    let pending_eliminated = self
                        .pending_commit_eliminated
                        .remove(&inst_idx)
                        .unwrap_or_default();
                    if let Some(pending) = self.pending_commits.remove(&inst_idx) {
                        self.remove_overwritten_multi_use_aliases(&pending, &pending_eliminated);
                        self.rename_dict.extend(pending);
                    }
                    self.abs_val_gen.finish_cur_instr();
                    pseudo_dict.clear();
                }
            }

            if !group_finished {
                self.cur_instr_pseudo_op_dict = pseudo_dict;
            }
        }
    }

    fn renamed_op_for_eliminated_move(
        &mut self,
        input_ops: &[OperandKey],
        output: &OperandKey,
    ) -> Shared<RenamedOperand> {
        let Some(OperandKey::Reg(input_reg)) = input_ops.first() else {
            return share(RenamedOperand::new());
        };
        let canonical_input = get_canonical_reg(input_reg);
        let entry = self
            .rename_dict
            .entry(canonical_input.clone())
            .or_insert_with(|| RenameDictEntry {
                renamed_op: share(RenamedOperand::new()),
                renamed_by_elim_32_bit_move: false,
            })
            .clone();

        if let OperandKey::Reg(out_reg) = output {
            let canonical_output = get_canonical_reg(out_reg);
            let dict = if is_gp_reg(&canonical_input) {
                &mut self.multi_use_gpr_dict
            } else {
                &mut self.multi_use_simd_dict
            };
            dict.entry(entry.renamed_op.borrow().identity)
                .or_default()
                .extend([canonical_input, canonical_output]);
        }

        entry.renamed_op
    }

    fn lat_reduced_due_to_fast_ptr_chasing(
        &self,
        is_load: bool,
        mem_addr: Option<&MemAddr>,
        storage: &UopStorage,
    ) -> bool {
        if !is_load || !self.arch.fast_pointer_chasing {
            return false;
        }
        let Some(mem_addr) = mem_addr else {
            return false;
        };
        if !(0..2048).contains(&mem_addr.disp) {
            return false;
        }

        let Some(base_key) = mem_addr.base.as_ref().map(|reg| get_canonical_reg(reg)) else {
            return false;
        };
        let Some(base_entry) = self.rename_dict.get(&base_key) else {
            return false;
        };
        if base_entry.renamed_by_elim_32_bit_move {
            return false;
        }
        let Some(base_uop_idx) = base_entry.renamed_op.borrow().uop_idx else {
            return false;
        };
        let Some(base_uop) = storage.get_uop(base_uop_idx) else {
            return false;
        };
        if !matches!(
            base_uop.prop.instr_str.as_ref(),
            "MOV (R64, M64)"
                | "MOV (RAX, M64)"
                | "MOV (R32, M32)"
                | "MOV (EAX, M32)"
                | "MOVSXD (R64, M32)"
                | "POP (R64)"
        ) {
            return false;
        }

        if let Some(index_reg) = &mem_addr.index {
            let index_key = get_canonical_reg(index_reg);
            let Some(index_entry) = self.rename_dict.get(&index_key) else {
                return false;
            };
            let Some(index_uop_idx) = index_entry.renamed_op.borrow().uop_idx else {
                return false;
            };
            let Some(index_uop) = storage.get_uop(index_uop_idx) else {
                return false;
            };
            if !index_uop.prop.possible_ports.is_empty() {
                return false;
            }
        }

        true
    }

    fn update_store_buffer_for_uop(
        &mut self,
        is_store_addr: bool,
        is_store_data: bool,
        is_load: bool,
        mem_addr: Option<&MemAddr>,
        uop_idx: u64,
    ) -> Option<Shared<StoreBufferEntry>> {
        if is_store_addr {
            let key = self.get_store_buffer_key(mem_addr);
            let entry = share(StoreBufferEntry {
                key,
                address_ready_cycle: None,
                data_ready_cycle: None,
                uop_idxs: vec![uop_idx],
            });
            if let Some(key) = key {
                self.store_buffer_entry_dict.insert(key, entry.clone());
            }
            self.cur_store_buffer_entry = Some(entry.clone());
            Some(entry)
        } else if is_store_data {
            if let Some(entry) = &self.cur_store_buffer_entry {
                entry.borrow_mut().uop_idxs.push(uop_idx);
            }
            self.cur_store_buffer_entry.clone()
        } else if is_load {
            self.get_store_buffer_key(mem_addr)
                .and_then(|key| self.store_buffer_entry_dict.get(&key).cloned())
        } else {
            None
        }
    }

    fn get_store_buffer_key(&mut self, mem_addr: Option<&MemAddr>) -> Option<StoreBufferKey> {
        let mem_addr = mem_addr?;
        Some((
            self.abs_val_gen
                .get_abstract_value_for_reg(mem_addr.base.as_ref()),
            self.abs_val_gen
                .get_abstract_value_for_reg(mem_addr.index.as_ref()),
            mem_addr.scale,
            mem_addr.disp,
        ))
    }

    fn rename_dict_key(&self, op: &OperandKey) -> Option<String> {
        match op {
            OperandKey::Reg(reg) => Some(get_canonical_reg(reg)),
            OperandKey::Flag(flags) => Some(flags.clone()),
            OperandKey::Mem(_) | OperandKey::Pseudo(_) => None,
        }
    }

    fn remove_overwritten_multi_use_aliases(
        &mut self,
        pending: &HashMap<String, RenameDictEntry>,
        pending_eliminated: &HashMap<String, bool>,
    ) {
        for (key, new_entry) in pending {
            let Some(prev) = self.rename_dict.get(key) else {
                continue;
            };
            let was_eliminated = pending_eliminated.get(key).copied().unwrap_or(false);
            let same_mapping =
                prev.renamed_op.borrow().identity == new_entry.renamed_op.borrow().identity;
            if was_eliminated && same_mapping {
                continue;
            }

            let prev_identity = prev.renamed_op.borrow().identity;
            if is_gp_reg(key) {
                if let Some(set) = self.multi_use_gpr_dict.get_mut(&prev_identity) {
                    set.remove(key);
                }
            } else if key.contains("MM") {
                if let Some(set) = self.multi_use_simd_dict.get_mut(&prev_identity) {
                    if !set.is_empty() {
                        set.remove(key);
                    }
                }
            }
        }
    }

    fn finish_move_elim_cycle_bookkeeping(&mut self) {
        self.multi_use_gpr_dict
            .retain(|_, aliases| !aliases.is_empty());
        self.multi_use_simd_dict
            .retain(|_, aliases| !aliases.is_empty());
        if !self.multi_use_gpr_dict.is_empty() {
            self.multi_use_gpr_dict_use_in_cycle.insert(
                self.renamer_active_cycle,
                self.multi_use_gpr_dict.len() as u32,
            );
        }
        if !self.multi_use_simd_dict.is_empty() {
            self.multi_use_simd_dict_use_in_cycle.insert(
                self.renamer_active_cycle,
                self.multi_use_simd_dict.len() as u32,
            );
        }
    }
}
