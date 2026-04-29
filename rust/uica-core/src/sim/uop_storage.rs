//! Central storage for all uops, fused uops, and laminated uops.
//!
//! Python uses shared mutable references everywhere. We use indexed storage
//! with explicit lookup to maintain structural parity while avoiding Rc<RefCell<T>>
//! everywhere.

use std::collections::HashMap;

use super::types::{FusedUop, LaminatedUop, Uop};

/// Central storage for all uops in the simulation.
#[derive(Debug, Default)]
pub struct UopStorage {
    pub uops: HashMap<u64, Uop>,
    pub fused_uops: HashMap<u64, FusedUop>,
    pub laminated_uops: HashMap<u64, LaminatedUop>,
}

impl UopStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_uop(&mut self, uop: Uop) {
        self.uops.insert(uop.idx, uop);
    }

    pub fn add_fused_uop(&mut self, fused: FusedUop) {
        self.fused_uops.insert(fused.idx, fused);
    }

    pub fn add_laminated_uop(&mut self, lam: LaminatedUop) {
        self.laminated_uops.insert(lam.idx, lam);
    }

    pub fn get_uop(&self, idx: u64) -> Option<&Uop> {
        self.uops.get(&idx)
    }

    pub fn get_uop_mut(&mut self, idx: u64) -> Option<&mut Uop> {
        self.uops.get_mut(&idx)
    }

    pub fn get_fused_uop(&self, idx: u64) -> Option<&FusedUop> {
        self.fused_uops.get(&idx)
    }

    pub fn get_fused_uop_mut(&mut self, idx: u64) -> Option<&mut FusedUop> {
        self.fused_uops.get_mut(&idx)
    }

    pub fn get_laminated_uop(&self, idx: u64) -> Option<&LaminatedUop> {
        self.laminated_uops.get(&idx)
    }

    pub fn get_laminated_uop_mut(&mut self, idx: u64) -> Option<&mut LaminatedUop> {
        self.laminated_uops.get_mut(&idx)
    }

    /// Get all unfused uops for a given fused uop.
    pub fn get_unfused_uops(&self, fused_idx: u64) -> Vec<&Uop> {
        if let Some(fused) = self.get_fused_uop(fused_idx) {
            fused
                .unfused_uop_idxs
                .iter()
                .filter_map(|&idx| self.get_uop(idx))
                .collect()
        } else {
            vec![]
        }
    }

    /// Get all fused uops for a given laminated uop.
    pub fn get_fused_uops_for_lam(&self, lam_idx: u64) -> Vec<&FusedUop> {
        if let Some(lam) = self.get_laminated_uop(lam_idx) {
            lam.fused_uop_idxs
                .iter()
                .filter_map(|&idx| self.get_fused_uop(idx))
                .collect()
        } else {
            vec![]
        }
    }
}
