//! Decoder - ports `class Decoder` from uiCA.py lines 908-970.
//!
//! Consumes instructions from the instruction queue, respects decoder
//! constraints (complex decoder, macro fusion), and emits instructions
//! to be processed into laminated uops.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::micro_arch::MicroArchConfig;
use crate::sim::types::InstrInstance;

type SharedQueue = Rc<RefCell<VecDeque<InstrInstance>>>;

/// Mirrors Python `class Decoder` exactly.
pub struct Decoder {
    pub arch: MicroArchConfig,
    pub instruction_queue: SharedQueue,
}

impl Decoder {
    pub fn new(arch: MicroArchConfig, instruction_queue: SharedQueue) -> Self {
        Self {
            arch,
            instruction_queue,
        }
    }

    /// Port of Decoder.cycle from uiCA.py.
    ///
    /// Returns decoded InstrInstances; FrontEnd maps them to existing laminated uops.
    pub fn cycle(&mut self, clock: u32) -> Vec<InstrInstance> {
        let mut decoded_instrs = Vec::new();
        let mut n_decoded_instrs = 0;
        let mut remaining_decoder_slots = self.arch.n_decoders;

        loop {
            let instr_i = match self.instruction_queue.borrow().front() {
                Some(i) => i.clone(),
                None => break,
            };
            // Skip macro-fused instructions (the first of the pair)
            if instr_i.macro_fused_with_prev_instr {
                let mut inst = self.instruction_queue.borrow_mut().pop_front().unwrap();
                inst.removed_from_iq = Some(clock);
                continue;
            }

            // Check predecode delay
            if let Some(predecoded) = instr_i.predecoded {
                if predecoded + self.arch.predecode_decode_delay > clock {
                    break;
                }
            } else {
                break;
            }

            // Complex decoder constraint: can't decode more instructions after a complex one
            if !decoded_instrs.is_empty() && instr_i.complex_decoder {
                break;
            }

            // Macro-fusible instruction at last decoder slot constraint
            if instr_i.is_macro_fusible_with_next {
                if !self.arch.macro_fusible_instr_can_be_decoded_as_last_instr
                    && n_decoded_instrs == self.arch.n_decoders - 1
                {
                    break;
                }
                // Need to check if next instruction is ready
                let queue = self.instruction_queue.borrow();
                if queue.len() <= 1 {
                    break;
                }
                if let Some(next_instr) = queue.get(1) {
                    if let Some(next_predecoded) = next_instr.predecoded {
                        if next_predecoded + self.arch.predecode_decode_delay > clock {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            // All checks passed, decode this instruction
            let mut inst = self.instruction_queue.borrow_mut().pop_front().unwrap();
            inst.removed_from_iq = Some(clock);
            decoded_instrs.push(inst);

            if instr_i.uops_ms > 0 {
                break;
            }

            // Update decoder slot tracking
            if instr_i.complex_decoder {
                remaining_decoder_slots =
                    (remaining_decoder_slots - 1).min(instr_i.n_available_simple_decoders);
            } else {
                remaining_decoder_slots -= 1;
            }
            n_decoded_instrs += 1;

            if remaining_decoder_slots == 0 {
                break;
            }

            // Branch or macro-fused instruction ends the decode group
            if instr_i.is_branch_instr || instr_i.macro_fused_with_next_instr {
                break;
            }
        }

        decoded_instrs
    }

    pub fn is_empty(&self) -> bool {
        self.instruction_queue.borrow().is_empty()
    }
}
