//! Cache block generation and splitting helpers.
//!
//! Ports `CacheBlockGenerator`, `CacheBlocksForNextRoundGenerator`,
//! `split64ByteBlockTo32ByteBlocks`, `split32ByteBlockTo16ByteBlocks`,
//! and `split64ByteBlockTo16ByteBlocks` from `uiCA.py`.

use super::types::InstrInstance;

/// Split 64-byte cache block into four 16-byte blocks.
pub fn split_64byte_block_to_16byte_blocks(
    cache_block: &[InstrInstance],
) -> [Vec<InstrInstance>; 4] {
    let mut blocks: [Vec<InstrInstance>; 4] = Default::default();
    for ii in cache_block {
        let b = ((ii.address % 64) / 16) as usize;
        blocks[b].push(ii.clone());
    }
    blocks
}

/// Split 32-byte block into two 16-byte blocks.
pub fn split_32byte_block_to_16byte_blocks(b32_block: &[InstrInstance]) -> [Vec<InstrInstance>; 2] {
    let mut blocks: [Vec<InstrInstance>; 2] = Default::default();
    for ii in b32_block {
        let b = ((ii.address % 32) / 16) as usize;
        blocks[b].push(ii.clone());
    }
    blocks
}

/// Split 64-byte cache block into two 32-byte blocks.
pub fn split_64byte_block_to_32byte_blocks(
    cache_block: &[InstrInstance],
) -> [Vec<InstrInstance>; 2] {
    let mut blocks: [Vec<InstrInstance>; 2] = Default::default();
    for ii in cache_block {
        let b = ((ii.address % 64) / 32) as usize;
        blocks[b].push(ii.clone());
    }
    blocks
}

/// Generate cache blocks for unroll mode or loop mode.
///
/// Python generator equivalent: `CacheBlockGenerator(instructions, unroll, alignmentOffset)`.
pub struct CacheBlockGenerator {
    instructions: Vec<InstrInstance>,
    unroll: bool,
    alignment_offset: u32,
    next_addr: u32,
    rnd: u32,
    idx_in_instr_list: usize,
    cache_block: Vec<InstrInstance>,
    next_instance_idx: u64,
}

impl CacheBlockGenerator {
    pub fn new(instructions: Vec<InstrInstance>, unroll: bool, alignment_offset: u32) -> Self {
        // Start from instructions.len() so round 0 indices from
        // build_instruction_instances (0..n-1) don't conflict with
        // round 1+ indices generated here. Every round gets globally
        // unique InstrInstance.idx values.
        let start_idx = instructions.len() as u64;
        Self {
            instructions,
            unroll,
            alignment_offset,
            next_addr: alignment_offset,
            rnd: 0,
            idx_in_instr_list: 0,
            cache_block: Vec::new(),
            next_instance_idx: start_idx,
        }
    }
}

impl Iterator for CacheBlockGenerator {
    type Item = Vec<InstrInstance>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.idx_in_instr_list >= self.instructions.len() {
                self.idx_in_instr_list = 0;
                self.rnd += 1;
            }

            let base = &self.instructions[self.idx_in_instr_list];
            let mut inst = base.clone();
            inst.address = self.next_addr;
            inst.rnd = self.rnd;
            inst.idx = self.next_instance_idx;
            self.next_instance_idx += 1;

            let inst_size = inst.size;
            self.cache_block.push(inst);

            let is_last = self.idx_in_instr_list == self.instructions.len() - 1;
            self.idx_in_instr_list += 1;

            if (!self.unroll) && is_last {
                let ret = std::mem::take(&mut self.cache_block);
                self.next_addr = self.alignment_offset;
                return Some(ret);
            }

            let prev_addr = self.next_addr;
            self.next_addr = prev_addr + inst_size;
            if prev_addr / 64 != self.next_addr / 64 {
                let ret = std::mem::take(&mut self.cache_block);
                return Some(ret);
            }
        }
    }
}

/// Generate cache blocks for one round (without unrolling).
///
/// Python generator equivalent: `CacheBlocksForNextRoundGenerator(instructions, alignmentOffset)`.
pub struct CacheBlocksForNextRoundGenerator {
    gen: CacheBlockGenerator,
    prev_rnd: Option<u32>,
    cache_blocks: Vec<Vec<InstrInstance>>,
}

impl CacheBlocksForNextRoundGenerator {
    pub fn new(instructions: Vec<InstrInstance>, alignment_offset: u32) -> Self {
        Self {
            gen: CacheBlockGenerator::new(instructions, false, alignment_offset),
            prev_rnd: None,
            cache_blocks: Vec::new(),
        }
    }
}

impl Iterator for CacheBlocksForNextRoundGenerator {
    type Item = Vec<Vec<InstrInstance>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let cache_block = self.gen.next()?;
            let cur_rnd = cache_block.last()?.rnd;

            if let Some(prev_rnd) = self.prev_rnd {
                if prev_rnd != cur_rnd {
                    let ret = std::mem::take(&mut self.cache_blocks);
                    self.prev_rnd = Some(cur_rnd);
                    self.cache_blocks.push(cache_block);
                    return Some(ret);
                }
            } else {
                self.prev_rnd = Some(cur_rnd);
            }

            self.cache_blocks.push(cache_block);
        }
    }
}
