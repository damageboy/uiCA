//! Microcode sequencer — ports `class MicrocodeSequencer` from `uiCA.py` lines 866-888.
//!
//! Mirrors MS uop queue with MITE/DSB stall accounting.

use std::collections::VecDeque;

use crate::micro_arch::MicroArchConfig;

pub struct MicrocodeSequencer {
    pub arch: MicroArchConfig,
    pub uop_queue: VecDeque<u64>,
    pub stalled: u32,
    pub post_stall: u32,
}

impl MicrocodeSequencer {
    pub fn new(arch: MicroArchConfig) -> Self {
        Self {
            arch,
            uop_queue: VecDeque::new(),
            stalled: 0,
            post_stall: 0,
        }
    }

    /// Port of MicrocodeSequencer.cycle from uiCA.py.
    pub fn cycle(&mut self) -> Vec<u64> {
        let mut uops = Vec::new();
        if self.stalled > 0 {
            self.stalled -= 1;
        } else if !self.uop_queue.is_empty() {
            while !self.uop_queue.is_empty() && uops.len() < 4 {
                uops.push(self.uop_queue.pop_front().unwrap());
            }
            if self.uop_queue.is_empty() {
                self.stalled = self.post_stall;
            }
        }
        uops
    }

    /// Port of MicrocodeSequencer.addUops from uiCA.py.
    pub fn add_lam_idxs(&mut self, lam_idxs: Vec<u64>, prev_uop_source: &str) {
        self.uop_queue.extend(lam_idxs);
        if prev_uop_source == "MITE" {
            self.stalled = 1;
            self.post_stall = 1;
        } else if prev_uop_source == "DSB" {
            self.stalled = self.arch.dsb_ms_stall;
            self.post_stall = 0;
        }
    }

    pub fn is_busy(&self) -> bool {
        self.stalled > 0 || !self.uop_queue.is_empty()
    }
}
