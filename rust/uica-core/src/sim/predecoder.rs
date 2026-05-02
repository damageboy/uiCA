//! PreDecoder — ports `class PreDecoder` from uiCA.py lines 973-1037.
//!
//! Processes 16-byte instruction blocks, handles LCP stalls, partial
//! instructions crossing block boundaries, and feeds the instruction queue.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::micro_arch::MicroArchConfig;
use crate::sim::types::InstrInstance;

type SharedQueue = Rc<RefCell<VecDeque<InstrInstance>>>;

/// Mirrors Python `class PreDecoder` exactly.
pub struct PreDecoder {
    pub arch: MicroArchConfig,
    pub b16_block_queue: VecDeque<VecDeque<InstrInstance>>,
    pub instruction_queue: SharedQueue,
    pub cur_block: Option<VecDeque<InstrInstance>>,
    pub non_stalled_predec_cycles_for_cur_block: u32,
    pub pre_dec_queue: VecDeque<InstrInstance>,
    pub stalled: u32,
    pub partial_instr_i: Option<InstrInstance>,
}

impl PreDecoder {
    pub fn new(arch: MicroArchConfig, instruction_queue: SharedQueue) -> Self {
        Self {
            arch,
            b16_block_queue: VecDeque::new(),
            instruction_queue,
            cur_block: None,
            non_stalled_predec_cycles_for_cur_block: 0,
            pre_dec_queue: VecDeque::new(),
            stalled: 0,
            partial_instr_i: None,
        }
    }

    /// Port of PreDecoder.cycle from uiCA.py.
    pub fn cycle(&mut self, clock: u32, all_generated_instr_instances: &mut [InstrInstance]) {
        if self.stalled == 0 {
            if self.pre_dec_queue.is_empty()
                && (self.partial_instr_i.is_some() || !self.b16_block_queue.is_empty())
                && self.instruction_queue.borrow().len() + self.arch.predecode_width as usize
                    <= self.arch.iq_width as usize
            {
                if let Some(partial) = self.partial_instr_i.take() {
                    self.pre_dec_queue.push_back(partial);
                }

                if self.cur_block.as_ref().is_none_or(|block| block.is_empty()) {
                    if self.b16_block_queue.len() < (self.arch.predecode_block_size / 16) as usize {
                        return;
                    }
                    let mut new_block = VecDeque::new();
                    for _ in 0..(self.arch.predecode_block_size / 16) {
                        if let Some(b16) = self.b16_block_queue.pop_front() {
                            new_block.extend(b16);
                        }
                    }
                    self.stalled = self
                        .stalled
                        .max(new_block.iter().filter(|ii| ii.lcp_stall).count() as u32 * 3)
                        .saturating_sub(
                            self.non_stalled_predec_cycles_for_cur_block
                                .saturating_sub(1),
                        );
                    self.non_stalled_predec_cycles_for_cur_block = 0;
                    self.cur_block = Some(new_block);
                }

                if let Some(ref mut block) = self.cur_block {
                    while !block.is_empty()
                        && self.pre_dec_queue.len() < self.arch.predecode_width as usize
                    {
                        if instr_instance_crosses_predec_block_boundary(
                            &block[0],
                            self.arch.predecode_block_size,
                        ) {
                            break;
                        }
                        if let Some(inst) = block.pop_front() {
                            self.pre_dec_queue.push_back(inst);
                        }
                    }

                    // Handle partial instruction (crosses block boundary)
                    if block.len() == 1 {
                        let instr_i = &block[0];
                        if instr_instance_crosses_predec_block_boundary(
                            instr_i,
                            self.arch.predecode_block_size,
                        ) {
                            let offset_of_nominal_opcode =
                                (instr_i.address % 16) + instr_i.pos_nominal_opcode;
                            if self.pre_dec_queue.len() < self.arch.predecode_width as usize
                                || offset_of_nominal_opcode >= 16
                            {
                                self.partial_instr_i = block.pop_front();
                            }
                        }
                    }

                    self.non_stalled_predec_cycles_for_cur_block += 1;
                }
            }

            if self.stalled == 0 {
                let mut queue = self.instruction_queue.borrow_mut();
                for mut instr_i in self.pre_dec_queue.drain(..) {
                    instr_i.predecoded = Some(clock);
                    // Python parity: `allGeneratedInstrInstances` and the
                    // predecoder queue hold the same InstrInstance object.
                    // Rust queues carry cloned values, so mirror the mutation
                    // at PreDecoder timing on the canonical generated instance.
                    if let Some(generated) = all_generated_instr_instances
                        .iter_mut()
                        .find(|generated| generated.idx == instr_i.idx)
                    {
                        generated.predecoded = Some(clock);
                    }
                    queue.push_back(instr_i);
                }
            }
        }

        self.stalled = self.stalled.saturating_sub(1);
    }

    pub fn is_empty(&self) -> bool {
        self.b16_block_queue.is_empty()
            && self.pre_dec_queue.is_empty()
            && self.partial_instr_i.is_none()
    }
}

/// Port of `instrInstanceCrossesPredecBlockBoundary` from uiCA.py line 1902.
fn instr_instance_crosses_predec_block_boundary(instr_i: &InstrInstance, block_size: u32) -> bool {
    let instr_len = instr_i.size;
    (instr_i.address % block_size) + instr_len > block_size
}
