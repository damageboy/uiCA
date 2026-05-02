//! FrontEnd driver — ports `class FrontEnd` from `uiCA.py` lines 461-755.
//!
//! Central cycle controller that orchestrates:
//!   1. Renamer issue (when ROB and RS have room)
//!   2. ROB cycle
//!   3. Scheduler cycle
//!   4. Front-end fill (DSB / MITE / LSD / MS depending on state)

use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use crate::micro_arch::MicroArchConfig;

use super::cache_blocks::*;
use super::decoder::Decoder;
use super::dsb::*;
use super::ms::MicrocodeSequencer;
use super::predecoder::PreDecoder;
use super::renamer::Renamer;
use super::reorder_buffer::ReorderBuffer;
use super::scheduler::Scheduler;
use super::types::{
    recompute_macro_fusion_and_is_last, FusedUop, InstrInstance, LaminatedUop, OperandKey, Uop,
    UopProperties, UopSource,
};
use super::uop_expand::{
    expand_instr_instance_to_lam_uops_with_storage, instr_uses_indexed_addr, instr_uses_same_reg,
    perf_for_operands, perf_for_python_getinstructions, perf_uops_mite,
    python_decoder_shape_from_record, python_may_be_eliminated_for_getinstructions,
    record_may_be_eliminated,
};
use super::uop_storage::UopStorage;

fn populate_instr_instance_metadata(
    instr_i: &mut InstrInstance,
    arch_name: &str,
    pack_index: &uica_data::DataPackIndex,
    no_micro_fusion: bool,
    no_macro_fusion: bool,
) {
    let norm = crate::matcher::NormalizedInstr {
        mnemonic: instr_i.mnemonic.clone(),
        iform_signature: instr_i.iform_signature.clone(),
        max_op_size_bytes: instr_i.max_op_size_bytes,
        immediate: instr_i.immediate,
        uses_high8_reg: instr_i.uses_high8_reg,
        explicit_reg_operands: instr_i.explicit_reg_operands.clone(),
        agen: instr_i.agen.clone(),
    };
    let candidates = pack_index.candidates_for(&arch_name.to_ascii_uppercase(), &instr_i.mnemonic);
    if let Some(record) = crate::matcher::match_instruction_record(&norm, candidates) {
        let arch = crate::micro_arch::get_micro_arch(arch_name);
        let perf = if let Some(arch) = arch.as_ref() {
            perf_for_python_getinstructions(
                record,
                instr_uses_same_reg(instr_i),
                instr_uses_indexed_addr(instr_i),
                &instr_i.input_regs,
                arch,
            )
        } else {
            perf_for_operands(
                record,
                instr_uses_same_reg(instr_i),
                instr_uses_indexed_addr(instr_i),
            )
        };
        instr_i.uops_mite = perf_uops_mite(&perf);
        instr_i.uops_ms = perf.uops_ms.max(0) as u32;
        instr_i.div_cycles = perf.div_cycles;
        instr_i.retire_slots = perf.retire_slots.max(1) as u32;
        instr_i.instr_tp = perf.tp.map(|tp| tp.ceil().max(0.0) as u32);
        instr_i.instr_str = record.string.clone();
        instr_i.macro_fusible_with = record.perf.macro_fusible_with.clone();
        instr_i.is_macro_fusible_with_next = !instr_i.macro_fusible_with.is_empty();
        if instr_i.implicit_rsp_change == 0 {
            instr_i.implicit_rsp_change = record.perf.implicit_rsp_change;
        }
        instr_i.may_be_eliminated = arch
            .as_ref()
            .map(|arch| {
                python_may_be_eliminated_for_getinstructions(
                    record,
                    instr_uses_same_reg(instr_i),
                    instr_uses_indexed_addr(instr_i),
                    &instr_i.input_regs,
                    arch,
                )
            })
            .unwrap_or_else(|| record_may_be_eliminated(record));
        let (complex_decoder, n_available_simple_decoders) = python_decoder_shape_from_record(
            record,
            &perf,
            crate::micro_arch::get_micro_arch(arch_name).map_or(4, |arch| arch.n_decoders),
        );
        instr_i.complex_decoder = complex_decoder;
        instr_i.n_available_simple_decoders = n_available_simple_decoders;
        instr_i.lcp_stall |= perf.lcp_stall;
        instr_i.can_be_used_by_lsd = perf.uops_ms <= 0
            && instr_i.implicit_rsp_change == 0
            && !instr_i
                .output_regs
                .iter()
                .any(|reg| crate::x64::is_high8_reg(reg));
        instr_i.cannot_be_in_dsb_due_to_jcc_erratum =
            record.perf.cannot_be_in_dsb_due_to_jcc_erratum;
        instr_i.no_micro_fusion = perf.no_micro_fusion || no_micro_fusion;
        instr_i.no_macro_fusion = perf.no_macro_fusion || no_macro_fusion;
        if instr_i.no_macro_fusion {
            instr_i.macro_fusible_with.clear();
            instr_i.is_macro_fusible_with_next = false;
        }
        if no_micro_fusion {
            instr_i.retire_slots = (perf.uops.max(0) as u32)
                .max(instr_i.uops_mite + instr_i.uops_ms)
                .max(1);
            instr_i.uops_mite = instr_i.retire_slots.saturating_sub(instr_i.uops_ms);
            if instr_i.uops_mite > 4 {
                instr_i.uops_ms += instr_i.uops_mite - 4;
                instr_i.uops_mite = 4;
            }
            if instr_i.uops_mite > 1 {
                instr_i.complex_decoder = true;
                let ms_limit = if instr_i.uops_ms > 0 { 0 } else { 3 };
                instr_i.n_available_simple_decoders = instr_i
                    .n_available_simple_decoders
                    .min(5u32.saturating_sub(instr_i.uops_mite))
                    .min(ms_limit);
            }
        }
    }
    // Keep structural exclusions that Python canBeUsedByLSD() enforces even
    // when older packs lack explicit metadata.
    instr_i.can_be_used_by_lsd &= instr_i.uops_ms == 0 && instr_i.implicit_rsp_change == 0;
}

fn populate_and_recompute_cache_blocks(
    cache_blocks: &mut [Vec<InstrInstance>],
    arch_name: &str,
    pack_index: &uica_data::DataPackIndex,
    no_micro_fusion: bool,
    no_macro_fusion: bool,
) {
    let lengths: Vec<usize> = cache_blocks.iter().map(Vec::len).collect();
    let mut all: Vec<InstrInstance> = cache_blocks
        .iter()
        .flat_map(|block| block.iter().cloned())
        .collect();
    for inst in all.iter_mut() {
        populate_instr_instance_metadata(
            inst,
            arch_name,
            pack_index,
            no_micro_fusion,
            no_macro_fusion,
        );
    }
    recompute_macro_fusion_and_is_last(&mut all);

    let mut pos = 0;
    for (block, len) in cache_blocks.iter_mut().zip(lengths) {
        *block = all[pos..pos + len].to_vec();
        pos += len;
    }
}

fn lam_idxs_for_block(block: &[InstrInstance], all_instances: &[InstrInstance]) -> Vec<Vec<u64>> {
    block
        .iter()
        .map(|inst| {
            let n = all_instances
                .iter()
                .find(|i| i.idx == inst.idx)
                .map(|i| {
                    if i.macro_fused_with_prev_instr {
                        0
                    } else if i.uops_mite > 0 {
                        i.uops_mite as usize + i.uops_ms as usize
                    } else {
                        i.uops_ms as usize
                    }
                })
                .unwrap_or(0);
            (0..n as u64).collect()
        })
        .collect()
}

fn all_first_round_blocks_cacheable(
    cache_blocks: &[Vec<InstrInstance>],
    all_instances: &[InstrInstance],
    arch: &MicroArchConfig,
) -> bool {
    if arch.dsb_block_size == 32 {
        cache_blocks.iter().all(|cache_block| {
            split_64byte_block_to_32byte_blocks(cache_block)
                .into_iter()
                .filter(|block| !block.is_empty())
                .all(|block| {
                    let lam_idxs = lam_idxs_for_block(&block, all_instances);
                    can_be_in_dsb(&block, &lam_idxs, arch.dsb_block_size)
                })
        })
    } else {
        cache_blocks.iter().all(|block| {
            let lam_idxs = lam_idxs_for_block(block, all_instances);
            can_be_in_dsb(block, &lam_idxs, arch.dsb_block_size)
        })
    }
}

fn find_cacheable_addresses_for_first_round(
    cache_blocks: &[Vec<InstrInstance>],
    all_instances: &[InstrInstance],
    arch: &MicroArchConfig,
) -> HashSet<u32> {
    let mut addresses = HashSet::new();
    for cache_block in cache_blocks {
        let split_blocks: Vec<Vec<InstrInstance>> = if arch.dsb_block_size == 32 {
            split_64byte_block_to_32byte_blocks(cache_block)
                .into_iter()
                .filter(|block| !block.is_empty())
                .collect()
        } else {
            vec![cache_block.clone()]
        };

        if arch.dsb_block_size == 32
            && arch.both_32byte_blocks_must_be_cacheable
            && split_blocks.iter().any(|block| {
                let lam_idxs = lam_idxs_for_block(block, all_instances);
                !can_be_in_dsb(block, &lam_idxs, arch.dsb_block_size)
            })
        {
            return addresses;
        }

        for block in split_blocks {
            let lam_idxs = lam_idxs_for_block(&block, all_instances);
            if can_be_in_dsb(&block, &lam_idxs, arch.dsb_block_size) {
                for instr_i in block {
                    addresses.insert(instr_i.address);
                }
            } else {
                return addresses;
            }
        }
    }
    addresses
}

pub struct FrontEnd {
    pub arch: MicroArchConfig,
    pub renamer: Renamer,
    pub reorder_buffer: ReorderBuffer,
    pub scheduler: Scheduler,
    pub decoder: Decoder,
    pub predecoder: PreDecoder,
    pub dsb: Dsb,
    pub ms: MicrocodeSequencer,
    pub idq: VecDeque<u64>, // laminated uop indices
    pub unroll: bool,
    pub no_micro_fusion: bool,
    pub no_macro_fusion: bool,
    pub uop_source: Option<String>,
    pub lsd_unroll_count: u32,
    pub addresses_in_dsb: HashSet<u32>,
    pub alignment_offset: u32,
    pub cache_block_generator: Option<CacheBlockGenerator>,
    pub pending_cache_block: Option<Vec<InstrInstance>>,
    pub cache_blocks_generator: Option<CacheBlocksForNextRoundGenerator>,
    pub all_generated_instr_instances: Vec<InstrInstance>,
    pub uop_idx_counter: u64,
    pub fused_idx_counter: u64,
    pub lam_idx_counter: u64,
    pub rsp_offset: i32,
    pub high8_reg_clean: std::collections::HashMap<String, bool>,
    pub reg_merge_issued_for: HashSet<u64>,
    pub uop_storage: UopStorage,
    pub pack: uica_data::DataPack,
    pub pack_index: uica_data::DataPackIndex,
}

impl FrontEnd {
    pub fn new(
        arch: MicroArchConfig,
        unroll: bool,
        base_instructions: Vec<InstrInstance>,
        alignment_offset: u32,
        pack: &uica_data::DataPack,
    ) -> Self {
        Self::new_with_init_policy(
            arch,
            unroll,
            base_instructions,
            alignment_offset,
            pack,
            "diff",
            false,
            false,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_init_policy(
        arch: MicroArchConfig,
        unroll: bool,
        base_instructions: Vec<InstrInstance>,
        alignment_offset: u32,
        pack: &uica_data::DataPack,
        init_policy: impl Into<String>,
        simple_front_end: bool,
        no_micro_fusion: bool,
        no_macro_fusion: bool,
    ) -> Self {
        let init_policy = init_policy.into();
        let instruction_queue = Rc::new(RefCell::new(VecDeque::<InstrInstance>::new()));

        let mut uop_source = if simple_front_end {
            None
        } else {
            Some("MITE".to_string())
        };
        let mut lsd_unroll_count = 1;
        let mut addresses_in_dsb = HashSet::new();

        let cache_block_generator = if unroll || simple_front_end {
            Some(CacheBlockGenerator::new(
                base_instructions.clone(),
                true,
                alignment_offset,
            ))
        } else {
            None
        };
        let cache_blocks_generator = if unroll || simple_front_end {
            None
        } else {
            Some(CacheBlocksForNextRoundGenerator::new(
                base_instructions.clone(),
                alignment_offset,
            ))
        };

        // For loop mode, check if we can use DSB or LSD using the first
        // generated round, matching Python FrontEnd.__init__.
        if !unroll && !simple_front_end {
            let pack_index = uica_data::DataPackIndex::new(pack.clone());
            let mut first_round_blocks =
                CacheBlocksForNextRoundGenerator::new(base_instructions.clone(), alignment_offset)
                    .next()
                    .unwrap_or_default();

            populate_and_recompute_cache_blocks(
                &mut first_round_blocks,
                arch.name,
                &pack_index,
                no_micro_fusion,
                no_macro_fusion,
            );

            let test_instances: Vec<InstrInstance> = first_round_blocks
                .iter()
                .flat_map(|block| block.iter().cloned())
                .collect();
            let can_cache =
                all_first_round_blocks_cacheable(&first_round_blocks, &test_instances, &arch);
            let all_can_lsd = test_instances.iter().all(|i| i.can_be_used_by_lsd);
            let n_uops: usize = test_instances
                .iter()
                .map(|inst| {
                    if inst.macro_fused_with_prev_instr {
                        0
                    } else if inst.uops_mite > 0 {
                        inst.uops_mite as usize + inst.uops_ms as usize
                    } else {
                        inst.uops_ms as usize
                    }
                })
                .sum();
            let use_lsd =
                can_cache && arch.lsd_enabled && all_can_lsd && (n_uops as u32 <= arch.idq_width);

            if use_lsd {
                uop_source = Some("LSD".to_string());
                lsd_unroll_count = arch
                    .lsd_unrolling
                    .iter()
                    .find(|e| e.nuops == n_uops as u32)
                    .map(|e| e.unroll)
                    .unwrap_or(1);
            } else {
                addresses_in_dsb = find_cacheable_addresses_for_first_round(
                    &first_round_blocks,
                    &test_instances,
                    &arch,
                );
                if addresses_in_dsb.contains(&alignment_offset) {
                    uop_source = Some("DSB".to_string());
                }
            }
        }

        let mut frontend = Self {
            renamer: Renamer::new_with_init_policy(arch.clone(), init_policy),
            reorder_buffer: ReorderBuffer::new(arch.clone()),
            scheduler: Scheduler::new(arch.clone()),
            decoder: Decoder::new(arch.clone(), instruction_queue.clone()),
            predecoder: PreDecoder::new(arch.clone(), instruction_queue),
            dsb: Dsb::new(arch.clone()),
            ms: MicrocodeSequencer::new(arch.clone()),
            idq: VecDeque::new(),
            arch,
            unroll,
            no_micro_fusion,
            no_macro_fusion,
            uop_source,
            lsd_unroll_count,
            addresses_in_dsb,
            alignment_offset,
            cache_block_generator,
            pending_cache_block: None,
            cache_blocks_generator,
            all_generated_instr_instances: Vec::new(),
            uop_idx_counter: 0,
            fused_idx_counter: 0,
            lam_idx_counter: 0,
            rsp_offset: 0,
            high8_reg_clean: [
                ("RAX".to_string(), true),
                ("RBX".to_string(), true),
                ("RCX".to_string(), true),
                ("RDX".to_string(), true),
            ]
            .into_iter()
            .collect(),
            reg_merge_issued_for: HashSet::new(),
            uop_storage: UopStorage::new(),
            pack_index: uica_data::DataPackIndex::new(pack.clone()),
            pack: pack.clone(),
        };

        // Pre-load LSD iterations (ported from uiCA.py FrontEnd.__init__).
        if !unroll && !simple_front_end && frontend.uop_source.as_deref() == Some("LSD") {
            // Collect all cache blocks to pre-load
            let mut all_blocks = Vec::new();
            if let Some(ref mut gen) = frontend.cache_blocks_generator {
                // First round
                if let Some(mut first_round) = gen.next() {
                    populate_and_recompute_cache_blocks(
                        &mut first_round,
                        frontend.arch.name,
                        &frontend.pack_index,
                        frontend.no_micro_fusion,
                        frontend.no_macro_fusion,
                    );
                    all_blocks.extend(first_round);
                }
                // (LSDUnrollCount - 1) additional rounds
                for _ in 0..(frontend.lsd_unroll_count - 1) {
                    if let Some(mut additional_round) = gen.next() {
                        populate_and_recompute_cache_blocks(
                            &mut additional_round,
                            frontend.arch.name,
                            &frontend.pack_index,
                            frontend.no_micro_fusion,
                            frontend.no_macro_fusion,
                        );
                        all_blocks.extend(additional_round);
                    }
                }
            }
            // Add all blocks to IDQ
            for cache_block in all_blocks {
                frontend.add_new_cache_block(cache_block, 0);
            }
        } else if !unroll && !simple_front_end {
            // DSB or MITE: load first round
            let mut first_round_blocks = Vec::new();
            if let Some(ref mut gen) = frontend.cache_blocks_generator {
                if let Some(mut first_round) = gen.next() {
                    populate_and_recompute_cache_blocks(
                        &mut first_round,
                        frontend.arch.name,
                        &frontend.pack_index,
                        frontend.no_micro_fusion,
                        frontend.no_macro_fusion,
                    );
                    first_round_blocks.extend(first_round);
                }
            }
            for cache_block in first_round_blocks {
                frontend.add_new_cache_block(cache_block, 0);
            }
        }

        frontend
    }

    fn prepare_instr_instance_uops(&mut self, instr_i: &mut InstrInstance, source: UopSource) {
        populate_instr_instance_metadata(
            instr_i,
            self.arch.name,
            &self.pack_index,
            self.no_micro_fusion,
            self.no_macro_fusion,
        );

        if instr_i.macro_fused_with_prev_instr || !instr_i.laminated_uops.is_empty() {
            return;
        }

        if let Ok(lam_idxs) = expand_instr_instance_to_lam_uops_with_storage(
            instr_i,
            &mut self.uop_idx_counter,
            &mut self.fused_idx_counter,
            &mut self.lam_idx_counter,
            &mut self.uop_storage,
            self.arch.name,
            &self.pack,
            Some(&self.pack_index),
        ) {
            for &lam_idx in &lam_idxs {
                if let Some(lam_uop) = self.uop_storage.get_laminated_uop_mut(lam_idx) {
                    lam_uop.added_to_idq = None;
                    lam_uop.uop_source = Some(source);
                }
            }
            instr_i.laminated_uops = lam_idxs;
        }
    }

    fn next_cache_block_with_macro_context(&mut self) -> Option<Vec<InstrInstance>> {
        let block = if let Some(block) = self.pending_cache_block.take() {
            block
        } else {
            self.cache_block_generator.as_mut()?.next()?
        };
        let first_was_fused_with_prev = block
            .first()
            .map(|inst| inst.macro_fused_with_prev_instr)
            .unwrap_or(false);

        let mut blocks = vec![block];
        if let Some(next_block) = self
            .cache_block_generator
            .as_mut()
            .and_then(|gen| gen.next())
        {
            blocks.push(next_block);
        }
        populate_and_recompute_cache_blocks(
            &mut blocks,
            self.arch.name,
            &self.pack_index,
            self.no_micro_fusion,
            self.no_macro_fusion,
        );
        if first_was_fused_with_prev {
            if let Some(first) = blocks[0].first_mut() {
                first.macro_fused_with_prev_instr = true;
                first.is_last_decoded_instr = true;
            }
        }

        let ret = blocks.remove(0);
        if let Some(next_block) = blocks.pop() {
            self.pending_cache_block = Some(next_block);
        }
        Some(ret)
    }

    /// Port of FrontEnd.cycle from uiCA.py.
    ///
    /// Cycle order matches Python exactly:
    /// 1. issueUops = renamer.cycle() iff ROB and RS both have room
    /// 2. mark issued cycle on each fused uop
    /// 3. reorderBuffer.cycle(clock, issueUops)
    /// 4. scheduler.cycle(clock, issueUops)
    /// 5. front-end fill: DSB / MITE / LSD / MS
    pub fn cycle(&mut self, clock: u32) {
        // 1. Issue stage (renamer)
        let issue_uops = if !self.reorder_buffer.is_full() && !self.scheduler.is_full() {
            self.renamer.cycle(
                &mut self.idq,
                &mut self.uop_storage,
                &self.reorder_buffer,
                &mut self.uop_idx_counter,
                &mut self.all_generated_instr_instances,
            )
        } else {
            vec![]
        };

        // 2. Mark issued cycle on each fused uop
        for &fused_idx in &issue_uops {
            if let Some(fused) = self.uop_storage.get_fused_uop_mut(fused_idx) {
                fused.issued = Some(clock);
            }
        }

        // 3. ROB cycle
        self.reorder_buffer
            .cycle(clock, &issue_uops, &mut self.uop_storage);

        // 4. Scheduler cycle
        self.scheduler
            .cycle(clock, &issue_uops, &mut self.uop_storage);

        // 5. Check if IDQ is full
        if self.idq.len() + self.arch.dsb_width as usize > self.arch.idq_width as usize {
            return;
        }

        // 4. Front-end fill
        if self.uop_source.is_none() {
            while self.idq.len() < self.arch.issue_width as usize {
                let Some(mut cache_block) = self.next_cache_block_with_macro_context() else {
                    break;
                };
                for instr_i in cache_block.iter_mut() {
                    self.prepare_instr_instance_uops(instr_i, UopSource::Mite);
                    self.all_generated_instr_instances.push(instr_i.clone());
                    for lam_idx in instr_i.laminated_uops.clone() {
                        self.add_reg_merge_uops(clock, lam_idx);
                        self.add_stack_sync_uop(clock, lam_idx);
                        if let Some(lam_uop) = self.uop_storage.get_laminated_uop_mut(lam_idx) {
                            lam_uop.added_to_idq = Some(clock);
                        }
                        self.idq.push_back(lam_idx);
                    }
                }
            }
        } else if self.uop_source.as_deref() == Some("LSD") {
            // LSD path: refill when loop-stream detector queue drains.
            if self.idq.is_empty() {
                // Refill for next round
                let mut blocks_to_add = Vec::new();
                if let Some(ref mut gen) = self.cache_blocks_generator {
                    for _ in 0..self.lsd_unroll_count {
                        if let Some(mut cache_blocks) = gen.next() {
                            populate_and_recompute_cache_blocks(
                                &mut cache_blocks,
                                self.arch.name,
                                &self.pack_index,
                                self.no_micro_fusion,
                                self.no_macro_fusion,
                            );
                            blocks_to_add.extend(cache_blocks);
                        }
                    }
                }
                for cache_block in blocks_to_add {
                    self.add_new_cache_block(cache_block, clock);
                }
            }
        } else {
            // Add new cache blocks to keep queues full
            while self.dsb.dsb_block_queue.len() < 2 && self.predecoder.b16_block_queue.len() < 4 {
                let mut blocks_to_add = Vec::new();
                let should_break = if self.unroll {
                    if let Some(cache_block) = self.next_cache_block_with_macro_context() {
                        blocks_to_add.push(cache_block);
                        false
                    } else {
                        true
                    }
                } else if let Some(ref mut gen) = self.cache_blocks_generator {
                    if let Some(mut cache_blocks) = gen.next() {
                        populate_and_recompute_cache_blocks(
                            &mut cache_blocks,
                            self.arch.name,
                            &self.pack_index,
                            self.no_micro_fusion,
                            self.no_macro_fusion,
                        );
                        blocks_to_add = cache_blocks;
                        false
                    } else {
                        true
                    }
                } else {
                    true
                };

                if should_break {
                    break;
                }

                for cache_block in blocks_to_add {
                    self.add_new_cache_block(cache_block, clock);
                }
            }

            // Add existing laminated uop IDs to IDQ.
            let new_lam_idxs = if self.ms.is_busy() {
                let lam_idxs = self.ms.cycle();
                for &lam_idx in &lam_idxs {
                    if let Some(lam_uop) = self.uop_storage.get_laminated_uop_mut(lam_idx) {
                        lam_uop.added_to_idq = Some(clock);
                        lam_uop.uop_source = Some(UopSource::Ms);
                    }
                }
                lam_idxs
            } else if self.uop_source.as_deref() == Some("MITE") {
                self.predecoder
                    .cycle(clock, &mut self.all_generated_instr_instances);
                let decoded = self
                    .decoder
                    .cycle(clock, &mut self.all_generated_instr_instances);
                let mut lam_idxs = Vec::new();
                for instr_i in &decoded {
                    if instr_i.macro_fused_with_prev_instr {
                        continue;
                    }
                    let mite_count = instr_i.uops_mite as usize;
                    for &lam_idx in instr_i.laminated_uops.iter().take(mite_count) {
                        if let Some(lam_uop) = self.uop_storage.get_laminated_uop_mut(lam_idx) {
                            lam_uop.added_to_idq = Some(clock);
                            lam_uop.uop_source = Some(UopSource::Mite);
                        }
                        lam_idxs.push(lam_idx);
                    }
                    if instr_i.uops_ms > 0 {
                        self.ms.add_lam_idxs(
                            instr_i
                                .laminated_uops
                                .iter()
                                .skip(mite_count)
                                .copied()
                                .collect(),
                            "MITE",
                        );
                        break;
                    }
                }

                // Check if we should switch to DSB
                if !self.unroll && !decoded.is_empty() {
                    let cur_instr_i = &decoded[decoded.len() - 1];
                    if cur_instr_i.is_last_decoded_instr
                        && (cur_instr_i.is_branch_instr || cur_instr_i.macro_fused_with_next_instr)
                        && self.addresses_in_dsb.contains(&self.alignment_offset)
                    {
                        self.uop_source = Some("DSB".to_string());
                    }
                }

                lam_idxs
            } else if self.uop_source.as_deref() == Some("DSB") {
                let new_instr_i_lam_idxs = self.dsb.cycle(&mut self.ms);
                let mut lam_idxs = Vec::new();

                for (_, maybe_lam_idx) in &new_instr_i_lam_idxs {
                    if let Some(lam_idx) = maybe_lam_idx {
                        if let Some(lam_uop) = self.uop_storage.get_laminated_uop_mut(*lam_idx) {
                            lam_uop.added_to_idq = Some(clock);
                            lam_uop.uop_source = Some(UopSource::Dsb);
                        }
                        lam_idxs.push(*lam_idx);
                    }
                }

                // Check if we should switch to MITE. Python only checks after the
                // emitted DSB uop is the last uop of its instruction.
                if !lam_idxs.is_empty()
                    && !new_instr_i_lam_idxs.is_empty()
                    && self.laminated_uop_is_last_uop_of_instr(*lam_idxs.last().unwrap())
                {
                    let (cur_instr_i, _) = &new_instr_i_lam_idxs[new_instr_i_lam_idxs.len() - 1];
                    if cur_instr_i.is_last_decoded_instr {
                        let next_addr = self.alignment_offset;
                        if !self.addresses_in_dsb.contains(&next_addr) {
                            self.uop_source = Some("MITE".to_string());
                        }
                    } else {
                        let next_addr = cur_instr_i.address + cur_instr_i.size;
                        if !self.addresses_in_dsb.contains(&next_addr) {
                            self.uop_source = Some("MITE".to_string());
                        }
                    }
                }

                lam_idxs
            } else {
                vec![]
            };

            for lam_idx in new_lam_idxs {
                self.add_reg_merge_uops(clock, lam_idx);
                self.add_stack_sync_uop(clock, lam_idx);
                self.idq.push_back(lam_idx);
            }
        }
    }

    /// Port of FrontEnd.addNewCacheBlock from uiCA.py lines 707-738.
    fn add_new_cache_block(&mut self, mut cache_block: Vec<InstrInstance>, _clock: u32) {
        // Match Python: set instrI.source before the block is consumed by
        // LSD/DSB/MITE so that stored instances carry the right source tag.
        let src_tag = match self.uop_source.as_deref() {
            Some("LSD") => Some(UopSource::Lsd),
            Some("DSB") => Some(UopSource::Dsb),
            Some("MITE") => Some(UopSource::Mite),
            _ => None,
        };
        if let Some(src) = src_tag {
            for inst in cache_block.iter_mut() {
                if inst.source.is_none() {
                    inst.source = Some(src);
                }
                self.prepare_instr_instance_uops(inst, src);
            }
        }

        self.all_generated_instr_instances
            .extend(cache_block.clone());

        if self.uop_source.as_deref() == Some("LSD") {
            // LSD path uses existing lam IDs; added_to_idq stays None (no Q events).
            for instr_i in cache_block {
                for lam_idx in instr_i.laminated_uops {
                    if let Some(lam_uop) = self.uop_storage.get_laminated_uop_mut(lam_idx) {
                        lam_uop.added_to_idq = None;
                        lam_uop.uop_source = Some(UopSource::Lsd);
                    }
                    self.idq.push_back(lam_idx);
                }
            }
        } else {
            // Split into 32-byte or 64-byte blocks depending on arch.
            let blocks: Vec<Vec<InstrInstance>> = if self.arch.dsb_block_size == 32 {
                split_64byte_block_to_32byte_blocks(&cache_block)
                    .into_iter()
                    .filter(|b| !b.is_empty())
                    .collect()
            } else {
                vec![cache_block]
            };

            for block in blocks {
                if block.is_empty() {
                    continue;
                }

                if self.addresses_in_dsb.contains(&block[0].address) {
                    let lam_idxs_per_instr: Vec<Vec<u64>> = block
                        .iter()
                        .map(|inst| inst.laminated_uops.clone())
                        .collect();
                    let dsb_blocks = get_dsb_blocks(&block, &lam_idxs_per_instr);
                    self.dsb.dsb_block_queue.extend(dsb_blocks);
                } else {
                    // MITE path.
                    let b16_blocks: Vec<Vec<InstrInstance>> = if self.arch.dsb_block_size == 32 {
                        split_32byte_block_to_16byte_blocks(&block)
                            .into_iter()
                            .collect()
                    } else {
                        split_64byte_block_to_16byte_blocks(&block)
                            .into_iter()
                            .collect()
                    };

                    for b16_block in b16_blocks {
                        if b16_block.is_empty() {
                            continue;
                        }
                        self.predecoder
                            .b16_block_queue
                            .push_back(VecDeque::from(b16_block.clone()));

                        // Handle branch instruction that ends in next block.
                        let last_instr_i = &b16_block[b16_block.len() - 1];
                        if last_instr_i.is_branch_instr
                            && (last_instr_i.address % 16) + last_instr_i.size > 16
                        {
                            self.predecoder.b16_block_queue.push_back(VecDeque::new());
                        }
                    }
                }
            }
        }
    }

    fn laminated_uop_is_last_uop_of_instr(&self, lam_idx: u64) -> bool {
        let Some(lam) = self.uop_storage.get_laminated_uop(lam_idx) else {
            return false;
        };
        let Some(fused_idx) = lam.fused_uop_idxs.last().copied() else {
            return false;
        };
        let Some(fused) = self.uop_storage.get_fused_uop(fused_idx) else {
            return false;
        };
        let Some(uop_idx) = fused.unfused_uop_idxs.last().copied() else {
            return false;
        };
        self.uop_storage
            .get_uop(uop_idx)
            .is_some_and(|uop| uop.prop.is_last_uop_of_instr)
    }

    fn create_single_uop_lam(
        &mut self,
        clock: u32,
        instr_idx: u64,
        prop: UopProperties,
        source: UopSource,
    ) -> u64 {
        let uop_idx = self.uop_idx_counter;
        self.uop_idx_counter += 1;
        let fused_idx = self.fused_idx_counter;
        self.fused_idx_counter += 1;
        let lam_idx = self.lam_idx_counter;
        self.lam_idx_counter += 1;

        self.uop_storage.add_uop(Uop {
            idx: uop_idx,
            queue_idx: uop_idx,
            prop,
            actual_port: None,
            eliminated: false,
            ready_for_dispatch: None,
            dispatched: None,
            executed: None,
            lat_reduced_due_to_fast_ptr_chasing: false,
            renamed_input_operands: Vec::new(),
            renamed_output_operands: Vec::new(),
            store_buffer_entry: None,
            fused_uop_idx: Some(fused_idx),
            instr_instance_idx: instr_idx,
        });
        self.uop_storage.add_fused_uop(FusedUop {
            idx: fused_idx,
            unfused_uop_idxs: vec![uop_idx],
            laminated_uop_idx: Some(lam_idx),
            issued: None,
            retired: None,
            retire_idx: None,
        });
        self.uop_storage.add_laminated_uop(LaminatedUop {
            idx: lam_idx,
            fused_uop_idxs: vec![fused_idx],
            added_to_idq: Some(clock),
            uop_source: Some(source),
            instr_instance_idx: instr_idx,
        });
        lam_idx
    }

    fn apply_high8_clean_latency_penalty(&mut self, instr: &InstrInstance) {
        let penalized_regs: Vec<String> = instr
            .input_regs
            .iter()
            .filter(|reg| crate::x64::is_high8_reg(reg))
            .map(|reg| crate::x64::get_canonical_reg(reg))
            .filter(|canonical| self.high8_reg_clean.get(canonical).copied().unwrap_or(true))
            .collect();
        if penalized_regs.is_empty() {
            return;
        }

        for lam_idx in &instr.laminated_uops {
            let Some(lam) = self.uop_storage.get_laminated_uop(*lam_idx).cloned() else {
                continue;
            };
            for fused_idx in lam.fused_uop_idxs {
                let Some(fused) = self.uop_storage.get_fused_uop(fused_idx).cloned() else {
                    continue;
                };
                for uop_idx in fused.unfused_uop_idxs {
                    let Some(uop) = self.uop_storage.get_uop_mut(uop_idx) else {
                        continue;
                    };
                    let reads_penalized = uop.prop.input_operands.iter().any(|op| {
                        matches!(op, OperandKey::Reg(reg) if penalized_regs.iter().any(|p| p == reg))
                    });
                    if !reads_penalized {
                        continue;
                    }
                    for latency in uop.prop.latencies.values_mut() {
                        *latency += 1;
                    }
                    for latency in uop.prop.latencies_by_operand.values_mut() {
                        *latency += 1;
                    }
                }
            }
        }
    }

    fn add_reg_merge_uops(&mut self, clock: u32, lam_idx: u64) {
        if !self.arch.high8_renamed_separately {
            return;
        }
        let Some(lam) = self.uop_storage.get_laminated_uop(lam_idx).cloned() else {
            return;
        };
        let Some(first_fused_idx) = lam.fused_uop_idxs.first().copied() else {
            return;
        };
        let Some(first_fused) = self.uop_storage.get_fused_uop(first_fused_idx) else {
            return;
        };
        let Some(first_uop_idx) = first_fused.unfused_uop_idxs.first().copied() else {
            return;
        };
        let Some(first_uop) = self.uop_storage.get_uop(first_uop_idx) else {
            return;
        };
        if !first_uop.prop.is_first_uop_of_instr
            || !self
                .reg_merge_issued_for
                .insert(first_uop.instr_instance_idx)
        {
            return;
        }
        let Some(instr) = self
            .all_generated_instr_instances
            .iter()
            .find(|instr| instr.idx == first_uop.instr_instance_idx)
            .cloned()
        else {
            return;
        };

        self.apply_high8_clean_latency_penalty(&instr);

        let mut regs_to_merge = Vec::new();
        for reg in instr.input_regs.iter() {
            let canonical = crate::x64::get_canonical_reg(reg);
            if matches!(canonical.as_str(), "RAX" | "RBX" | "RCX" | "RDX")
                && crate::x64::get_reg_size(reg) > 8
                && !self
                    .high8_reg_clean
                    .get(&canonical)
                    .copied()
                    .unwrap_or(true)
            {
                regs_to_merge.push(canonical);
            }
        }
        for mem_addr in &instr.mem_addrs {
            for reg in [&mem_addr.base, &mem_addr.index].into_iter().flatten() {
                let canonical = crate::x64::get_canonical_reg(reg);
                if matches!(canonical.as_str(), "RAX" | "RBX" | "RCX" | "RDX")
                    && !self
                        .high8_reg_clean
                        .get(&canonical)
                        .copied()
                        .unwrap_or(true)
                {
                    regs_to_merge.push(canonical);
                }
            }
        }
        regs_to_merge.sort();
        regs_to_merge.dedup();

        for canonical in regs_to_merge {
            let sync_lam_idx = self.create_single_uop_lam(
                clock,
                instr.idx,
                UopProperties {
                    possible_ports: vec!["1".to_string(), "5".to_string()],
                    div_cycles: 0,
                    is_load_uop: false,
                    is_store_address_uop: false,
                    is_store_data_uop: false,
                    is_first_uop_of_instr: true,
                    is_last_uop_of_instr: true,
                    is_reg_merge_uop: true,
                    is_serializing_instr: false,
                    input_reg_operands: vec![canonical.clone()],
                    output_reg_operands: vec![canonical.clone()],
                    may_be_eliminated: false,
                    latencies: [(canonical.clone(), 1)].into_iter().collect(),
                    input_operands: vec![OperandKey::Reg(canonical.clone())],
                    output_operands: vec![OperandKey::Reg(canonical.clone())],
                    latencies_by_operand: [(OperandKey::Reg(canonical.clone()), 1)]
                        .into_iter()
                        .collect(),
                    instr_tp: None,
                    instr_str: String::new(),
                    immediate: None,
                    is_load_serializing: false,
                    is_store_serializing: false,
                    mem_addr: None,
                },
                UopSource::Se,
            );
            if let Some(lam) = self.uop_storage.get_laminated_uop_mut(sync_lam_idx) {
                lam.added_to_idq = None;
            }
            // Python parity: `instrI.regMergeUops` is appended by
            // `Renamer.cycle()` when merge uops are injected, not when the
            // front end discovers merge properties.
        }

        for reg in instr.input_regs.iter().chain(instr.output_regs.iter()) {
            let canonical = crate::x64::get_canonical_reg(reg);
            if matches!(canonical.as_str(), "RAX" | "RBX" | "RCX" | "RDX")
                && crate::x64::get_reg_size(reg) > 8
            {
                self.high8_reg_clean.insert(canonical, true);
            }
        }
        for mem_addr in &instr.mem_addrs {
            for reg in [&mem_addr.base, &mem_addr.index].into_iter().flatten() {
                let canonical = crate::x64::get_canonical_reg(reg);
                if matches!(canonical.as_str(), "RAX" | "RBX" | "RCX" | "RDX")
                    && crate::x64::get_reg_size(reg) > 8
                {
                    self.high8_reg_clean.insert(canonical, true);
                }
            }
        }
        for reg in &instr.output_regs {
            if crate::x64::is_high8_reg(reg) {
                self.high8_reg_clean
                    .insert(crate::x64::get_canonical_reg(reg), false);
            }
        }
    }

    fn add_stack_sync_uop(&mut self, clock: u32, lam_idx: u64) {
        let Some(lam) = self.uop_storage.get_laminated_uop(lam_idx).cloned() else {
            return;
        };
        let Some(first_fused_idx) = lam.fused_uop_idxs.first().copied() else {
            return;
        };
        let Some(first_fused) = self.uop_storage.get_fused_uop(first_fused_idx) else {
            return;
        };
        let Some(first_uop_idx) = first_fused.unfused_uop_idxs.first().copied() else {
            return;
        };
        let Some(first_uop) = self.uop_storage.get_uop(first_uop_idx) else {
            return;
        };
        if !first_uop.prop.is_first_uop_of_instr {
            return;
        }

        let Some(instr) = self
            .all_generated_instr_instances
            .iter()
            .find(|instr| instr.idx == first_uop.instr_instance_idx)
            .cloned()
        else {
            return;
        };

        let mut requires_sync = false;
        if self.rsp_offset != 0 {
            let uses_rsp = instr
                .input_regs
                .iter()
                .any(|r| crate::x64::get_canonical_reg(r) == "RSP")
                || first_uop.prop.mem_addr.as_ref().is_some_and(|m| {
                    !m.is_implicit_stack_operand
                        && (m.base.as_deref() == Some("RSP") || m.index.as_deref() == Some("RSP"))
                });
            if uses_rsp {
                requires_sync = true;
                self.rsp_offset = 0;
            }
        }

        self.rsp_offset += instr.implicit_rsp_change;
        if self.rsp_offset > 192 {
            requires_sync = true;
            self.rsp_offset = 0;
        }
        if instr
            .output_regs
            .iter()
            .any(|r| crate::x64::get_canonical_reg(r) == "RSP")
        {
            self.rsp_offset = 0;
        }

        if !requires_sync {
            return;
        }

        let uop_idx = self.uop_idx_counter;
        self.uop_idx_counter += 1;
        let fused_idx = self.fused_idx_counter;
        self.fused_idx_counter += 1;
        let sync_lam_idx = self.lam_idx_counter;
        self.lam_idx_counter += 1;

        let mut latencies = std::collections::BTreeMap::new();
        latencies.insert("RSP".to_string(), 1);
        let mut latencies_by_operand = std::collections::BTreeMap::new();
        latencies_by_operand.insert(OperandKey::Reg("RSP".to_string()), 1);
        let prop = UopProperties {
            possible_ports: crate::micro_arch::alu_ports(self.arch.name)
                .iter()
                .map(|p| (*p).to_string())
                .collect(),
            div_cycles: 0,
            is_load_uop: false,
            is_store_address_uop: false,
            is_store_data_uop: false,
            is_first_uop_of_instr: true,
            is_last_uop_of_instr: true,
            is_reg_merge_uop: false,
            is_serializing_instr: false,
            input_reg_operands: vec!["RSP".to_string()],
            output_reg_operands: vec!["RSP".to_string()],
            may_be_eliminated: false,
            latencies,
            input_operands: vec![OperandKey::Reg("RSP".to_string())],
            output_operands: vec![OperandKey::Reg("RSP".to_string())],
            latencies_by_operand,
            instr_tp: None,
            // Python parity: `StackSyncUop` stores the original `instrI.instr`.
            // Renamer abstract-value updates therefore use that instruction's
            // `instrStr`/immediate, so a stack sync before `MOV RBP, RSP`
            // preserves the RSP abstract value and keeps store-forwarding keys
            // for PUSH/POP stack slots aligned.
            instr_str: instr.instr_str.clone(),
            immediate: instr.immediate,
            is_load_serializing: false,
            is_store_serializing: false,
            mem_addr: None,
        };
        self.uop_storage.add_uop(Uop {
            idx: uop_idx,
            queue_idx: uop_idx,
            prop,
            actual_port: None,
            eliminated: false,
            ready_for_dispatch: None,
            dispatched: None,
            executed: None,
            lat_reduced_due_to_fast_ptr_chasing: false,
            renamed_input_operands: Vec::new(),
            renamed_output_operands: Vec::new(),
            store_buffer_entry: None,
            fused_uop_idx: Some(fused_idx),
            instr_instance_idx: instr.idx,
        });
        self.uop_storage.add_fused_uop(FusedUop {
            idx: fused_idx,
            unfused_uop_idxs: vec![uop_idx],
            laminated_uop_idx: Some(sync_lam_idx),
            issued: None,
            retired: None,
            retire_idx: None,
        });
        self.uop_storage.add_laminated_uop(LaminatedUop {
            idx: sync_lam_idx,
            fused_uop_idxs: vec![fused_idx],
            added_to_idq: Some(clock),
            uop_source: Some(UopSource::Se),
            instr_instance_idx: instr.idx,
        });
        if let Some(stored) = self
            .all_generated_instr_instances
            .iter_mut()
            .find(|stored| stored.idx == instr.idx)
        {
            stored.stack_sync_uops.push(sync_lam_idx);
        }
        self.idq.push_back(sync_lam_idx);
    }
}
