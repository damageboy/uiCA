//! ReorderBuffer - ports `class ReorderBuffer` from uiCA.py lines 1056-1093.
//!
//! Short class: tracks fused uops in program order until they can retire.

use std::collections::VecDeque;

use crate::micro_arch::MicroArchConfig;

use super::uop_storage::UopStorage;

pub struct ReorderBuffer {
    pub arch: MicroArchConfig,
    pub uops: VecDeque<u64>, // fused uop indices
    /// Python parity: `ReorderBuffer.retireQueue`, drained by runSimulation.
    pub retire_queue: VecDeque<u64>,
}

impl ReorderBuffer {
    pub fn new(arch: MicroArchConfig) -> Self {
        Self {
            arch,
            uops: VecDeque::new(),
            retire_queue: VecDeque::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.uops.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.uops.len() + self.arch.issue_width as usize > self.arch.rb_width as usize
    }

    /// Port of ReorderBuffer.cycle from uiCA.py.
    ///
    /// Calls retireUops then addUops.
    pub fn cycle(&mut self, clock: u32, new_uop_idxs: &[u64], storage: &mut UopStorage) {
        self.retire_uops(clock, storage);
        self.add_uops(clock, new_uop_idxs, storage);
    }

    /// Port of ReorderBuffer.retireUops from uiCA.py.
    ///
    /// Retires up to retireWidth fused uops whose unfused uops are all executed.
    fn retire_uops(&mut self, clock: u32, storage: &mut UopStorage) {
        let mut n_retired_in_same_cycle = 0;

        while n_retired_in_same_cycle < self.arch.retire_width {
            if self.uops.is_empty() {
                break;
            }

            let fused_idx = self.uops[0];
            let _fused = storage.get_fused_uop(fused_idx).unwrap();

            // Check if all unfused uops are executed
            let unfused_uops = storage.get_unfused_uops(fused_idx);
            let all_executed = unfused_uops
                .iter()
                .all(|uop| uop.executed.is_some() && uop.executed.unwrap() < clock);

            if all_executed {
                self.uops.pop_front();
                self.retire_queue.push_back(fused_idx);
                let fused = storage.get_fused_uop_mut(fused_idx).unwrap();
                fused.retired = Some(clock);
                fused.retire_idx = Some(n_retired_in_same_cycle);
                n_retired_in_same_cycle += 1;
            } else {
                break;
            }
        }
    }

    /// Port of ReorderBuffer.addUops from uiCA.py.
    ///
    /// Adds newly issued fused uops. For uops without ports or that are
    /// eliminated, sets executed = clock immediately.
    fn add_uops(&mut self, clock: u32, new_uop_idxs: &[u64], storage: &mut UopStorage) {
        for &fused_idx in new_uop_idxs {
            self.uops.push_back(fused_idx);

            // For each unfused uop, check if it should be marked as executed immediately
            let unfused_uop_idxs = {
                let fused = storage.get_fused_uop(fused_idx).unwrap();
                fused.unfused_uop_idxs.clone()
            };

            for uop_idx in unfused_uop_idxs {
                let uop = storage.get_uop_mut(uop_idx).unwrap();
                if uop.prop.possible_ports.is_empty() || uop.eliminated {
                    // Python parity: port-less / move-eliminated uops execute
                    // at issue time, but are not dispatched to any port.
                    uop.executed = Some(clock);
                }
            }
        }
    }
}
