//! DSB (Decoded Stream Buffer) — ports `class DSB` from `uiCA.py` lines 775-863.
//!
//! Consumes DSBBlock queues, handles paired-block width tricks (SKL/ICL),
//! and emits laminated uops to the IDQ.

use std::collections::VecDeque;

use crate::micro_arch::MicroArchConfig;

use super::cache_blocks::{
    split_32byte_block_to_16byte_blocks, split_64byte_block_to_32byte_blocks,
};
use super::ms::MicrocodeSequencer;
use super::types::InstrInstance;

/// DSB entry (mirrors `DSBEntry` namedtuple in `uiCA.py`).
#[derive(Clone, Debug)]
pub struct DsbEntry {
    pub slot: usize,
    pub instr_i: InstrInstance,
    pub lam_idx: Option<u64>,
    pub ms_lam_idxs: Vec<u64>,
    pub requires_extra_entry: bool,
}

/// Ports `class DSB` from `uiCA.py`.
pub struct Dsb {
    pub arch: MicroArchConfig,
    pub dsb_block_queue: VecDeque<VecDeque<DsbEntry>>,
    pub delay_in_prev_cycle: bool,
}

impl Dsb {
    pub fn new(arch: MicroArchConfig) -> Self {
        Self {
            arch,
            dsb_block_queue: VecDeque::new(),
            delay_in_prev_cycle: false,
        }
    }

    /// Port of DSB.cycle from uiCA.py.
    ///
    /// Returns list of (InstrInstance, Option<lam_idx>) tuples.
    /// When the lam_idx is Some, it should be added to the IDQ.
    /// When MS lam IDs are present, they are queued in the MS.
    pub fn cycle(&mut self, ms: &mut MicrocodeSequencer) -> Vec<(InstrInstance, Option<u64>)> {
        if self.dsb_block_queue.is_empty() {
            return vec![];
        }

        let mut ret_list: Vec<(InstrInstance, Option<u64>)> = Vec::new();
        let new_dsb_block_started = self.dsb_block_queue[0].front().is_some_and(|e| e.slot == 0);
        let mut second_dsb_block_loaded = false;
        let mut need_load_after_empty = false;
        let mut remaining_slots = self.arch.dsb_width as usize;
        let delay_in_prev_cycle = self.delay_in_prev_cycle;
        self.delay_in_prev_cycle = false;

        while remaining_slots > 0 {
            if need_load_after_empty {
                if !second_dsb_block_loaded
                    && !self.dsb_block_queue.is_empty()
                    && self.dsb_block_queue[0]
                        .back()
                        .is_none_or(|e| e.ms_lam_idxs.is_empty())
                {
                    second_dsb_block_loaded = true;
                    if let Some(prev) = ret_list.last() {
                        let prev_instr_i = &prev.0;
                        if let Some(first) = self.dsb_block_queue[0].front() {
                            let next_addr = prev_instr_i.address + prev_instr_i.size;
                            if next_addr != first.instr_i.address
                                && !prev_instr_i.is_last_decoded_instr
                            {
                                return ret_list;
                            }
                        }
                    }
                    need_load_after_empty = false;
                } else {
                    return ret_list;
                }
            }

            if self.dsb_block_queue.is_empty() {
                return ret_list;
            }

            if self.dsb_block_queue[0]
                .front()
                .is_some_and(|entry| entry.requires_extra_entry)
                && remaining_slots < 2
            {
                return ret_list;
            }

            let entry = self.dsb_block_queue[0]
                .pop_front()
                .expect("non-empty DSB block");
            let entry_slot = entry.slot;
            let entry_instr_addr = entry.instr_i.address;
            let entry_is_last_decoded_instr = entry.instr_i.is_last_decoded_instr;

            if let Some(lam_idx) = entry.lam_idx {
                ret_list.push((entry.instr_i, Some(lam_idx)));
                if entry.requires_extra_entry {
                    remaining_slots = 0;
                    self.delay_in_prev_cycle = true;
                } else {
                    remaining_slots = remaining_slots.saturating_sub(1);
                }
            }

            if !entry.ms_lam_idxs.is_empty() {
                ms.add_lam_idxs(entry.ms_lam_idxs, "DSB");
                remaining_slots = 0;
            }

            if self.dsb_block_queue[0].is_empty() {
                self.dsb_block_queue.pop_front();
                need_load_after_empty = remaining_slots > 0;
                if remaining_slots > 0
                    && !self.dsb_block_queue.is_empty()
                    && self.arch.dsb_width == 6
                {
                    if let Some(next_first) = self.dsb_block_queue[0].front() {
                        let next_instr_addr = next_first.instr_i.address;
                        let next_instr_in_same_memory_block = next_instr_addr
                            / self.arch.dsb_block_size
                            == entry_instr_addr / self.arch.dsb_block_size;

                        if self.arch.dsb_block_size == 32
                            && next_instr_in_same_memory_block
                            && entry_is_last_decoded_instr
                            && !delay_in_prev_cycle
                        {
                            remaining_slots = 0;
                            need_load_after_empty = false;
                        } else if !next_instr_in_same_memory_block {
                            if new_dsb_block_started {
                                remaining_slots = match ret_list.len() {
                                    1 | 2 => 4,
                                    3 | 4 => 2,
                                    5 => 1,
                                    _ => remaining_slots,
                                };
                            } else if entry_is_last_decoded_instr {
                                if ret_list.len() == 1 || (ret_list.len() == 2 && entry_slot >= 4) {
                                    remaining_slots = 4;
                                } else {
                                    remaining_slots = remaining_slots.min(2);
                                }
                            }
                        }
                    }
                }

                if need_load_after_empty && self.dsb_block_queue.is_empty() {
                    return ret_list;
                }
            }
        }

        ret_list
    }
}

/// Port of `getDSBBlocks` from `uiCA.py` lines 1941-1997.
pub fn get_dsb_blocks(
    cache_block: &[InstrInstance],
    lam_idxs_per_instr: &[Vec<u64>],
) -> Vec<VecDeque<DsbEntry>> {
    let mut remaining_entries_in_cur_block = 0usize;
    let mut dsb_blocks = Vec::new();

    for (i, instr_i) in cache_block.iter().enumerate() {
        if instr_i.macro_fused_with_prev_instr {
            continue;
        }

        // Use the DataPack uops_mite count to determine how many DSB slots
        // this instruction occupies. `uops_mite` is set from the DataPack
        // record in build_instruction_instances (via uop_expand). Fall back
        // to 1 if not set so instructions without a record still get a slot.
        let n_required_entries = (instr_i.uops_mite as usize).max(1);
        let requires_extra_entry = instr_i.immediate.is_some_and(|imm| {
            !(i32::MIN as i64..=i32::MAX as i64).contains(&imm)
                || (!(i16::MIN as i64..=i16::MAX as i64).contains(&imm)
                    && !instr_i.mem_addrs.is_empty())
        });

        if instr_i.uops_ms > 0
            || n_required_entries + if requires_extra_entry { 1 } else { 0 }
                > remaining_entries_in_cur_block
        {
            let cur_block = VecDeque::new();
            remaining_entries_in_cur_block = 6;
            dsb_blocks.push(cur_block);
        }

        let lam_idxs = &lam_idxs_per_instr[i];

        if instr_i.uops_mite > 0 {
            for (j, &lam_idx) in lam_idxs.iter().take(instr_i.uops_mite as usize).enumerate() {
                let is_last = j == instr_i.uops_mite as usize - 1;
                let ms_lam_idxs = if is_last && instr_i.uops_ms > 0 {
                    lam_idxs
                        .get(instr_i.uops_mite as usize..)
                        .unwrap_or(&[])
                        .to_vec()
                } else {
                    vec![]
                };

                let entry = DsbEntry {
                    slot: dsb_blocks.last().unwrap().len(),
                    instr_i: instr_i.clone(),
                    lam_idx: Some(lam_idx),
                    ms_lam_idxs,
                    requires_extra_entry: is_last && requires_extra_entry,
                };

                dsb_blocks.last_mut().unwrap().push_back(entry);
                remaining_entries_in_cur_block = remaining_entries_in_cur_block.saturating_sub(1);
            }
        } else if instr_i.uops_ms > 0 {
            let entry = DsbEntry {
                slot: dsb_blocks.last().unwrap().len(),
                instr_i: instr_i.clone(),
                lam_idx: None,
                ms_lam_idxs: lam_idxs.to_vec(),
                requires_extra_entry: false,
            };
            dsb_blocks.last_mut().unwrap().push_back(entry);
            remaining_entries_in_cur_block = 0;
        } else {
            let entry = DsbEntry {
                slot: dsb_blocks.last().unwrap().len(),
                instr_i: instr_i.clone(),
                lam_idx: None,
                ms_lam_idxs: vec![],
                requires_extra_entry: false,
            };
            dsb_blocks.last_mut().unwrap().push_back(entry);
            remaining_entries_in_cur_block = remaining_entries_in_cur_block.saturating_sub(1);
        }

        if requires_extra_entry {
            remaining_entries_in_cur_block = remaining_entries_in_cur_block.saturating_sub(1);
        }
        if instr_i.uops_ms > 0 {
            remaining_entries_in_cur_block = 0;
        }
    }

    dsb_blocks
}

/// Port of `canBeInDSB` from `uiCA.py` lines 1999-2038.
pub fn can_be_in_dsb(
    block: &[InstrInstance],
    lam_idxs_per_instr: &[Vec<u64>],
    dsb_block_size: u32,
) -> bool {
    let dsb_blocks = get_dsb_blocks(block, lam_idxs_per_instr);

    if dsb_block_size == 32 && dsb_blocks.len() > 3 {
        return false;
    }
    if dsb_block_size == 64 && dsb_blocks.len() > 6 {
        return false;
    }

    if block
        .last()
        .is_some_and(|instr| instr.cannot_be_in_dsb_due_to_jcc_erratum)
    {
        return false;
    }

    if dsb_block_size == 32 {
        if lcp_macro_fusion_overflow(block.to_vec()) {
            return false;
        }
    } else {
        for block32 in split_64byte_block_to_32byte_blocks(block) {
            if !block32.is_empty() && lcp_macro_fusion_overflow(block32) {
                return false;
            }
        }
    }

    true
}

fn lcp_macro_fusion_overflow(block32: Vec<InstrInstance>) -> bool {
    let [mut b16_1, mut b16_2] = split_32byte_block_to_16byte_blocks(&block32);
    if !b16_1.is_empty() && !b16_2.is_empty() {
        let last = b16_1.last().unwrap();
        if (last.address % 16) + last.pos_nominal_opcode >= 16 {
            if let Some(moved) = b16_1.pop() {
                b16_2.insert(0, moved);
            }
        }
    }

    b16_1
        .last()
        .is_some_and(|instr| instr.lcp_stall && instr.is_macro_fusible_with_next)
        && b16_2
            .iter()
            .filter(|instr| instr.lcp_stall && instr.is_macro_fusible_with_next)
            .count()
            >= 2
}
