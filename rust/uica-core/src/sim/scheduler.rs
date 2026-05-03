//! Full port of `class Scheduler` from uiCA.py (lines ~1098-1500).
//!
//! Allowing a handful of stylistic clippy lints during the port. The code
//! mirrors Python structure and will be cleaned up after parity.
#![allow(
    clippy::needless_borrow,
    clippy::unnecessary_unwrap,
    clippy::manual_is_multiple_of,
    clippy::collapsible_if,
    clippy::if_same_then_else,
    clippy::unnecessary_map_or,
    clippy::op_ref,
    clippy::unwrap_or_default,
    clippy::unnecessary_lazy_evaluations,
    clippy::let_and_return,
    clippy::needless_range_loop,
    clippy::too_many_arguments
)]
//!
//! Implements core scheduling logic with port selection,
//! ready queues, dispatch, and dependency tracking.

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::micro_arch::MicroArchConfig;

use super::uop_storage::UopStorage;

pub struct Scheduler {
    pub arch: MicroArchConfig,
    pub all_ports: Vec<String>,
    pub port_usage: HashMap<String, u32>,
    pub port_usage_at_start_of_cycle: BTreeMap<u32, HashMap<String, u32>>,
    pub next_p23_port: String,
    pub next_p49_port: String,
    pub next_p78_port: String,
    pub uops_dispatched_in_prev_cycle: Vec<u64>,
    pub ready_queue: HashMap<String, Vec<(u64, u64)>>, // min-heap by Python Uop.idx, uop_idx
    pub ready_div_uops: Vec<(u64, u64)>,
    pub uops_ready_in_cycle: BTreeMap<u32, Vec<u64>>,
    pub non_ready_uops: Vec<u64>,
    pub pending_uops: Vec<u64>,
    pub pending_store_fence_uops: VecDeque<u64>,
    pub store_uops_since_last_store_fence: Vec<u64>,
    pub pending_load_fence_uops: VecDeque<u64>,
    pub load_uops_since_last_load_fence: Vec<u64>,
    pub blocked_resources: HashMap<String, u32>,
    pub dependent_uops: HashMap<u64, Vec<u64>>,
    pub uops: Vec<u64>,
}

impl Scheduler {
    pub fn new(arch: MicroArchConfig, all_ports: Vec<String>) -> Self {
        let mut port_usage = HashMap::new();
        let mut ready_queue = HashMap::new();
        for port in &all_ports {
            port_usage.insert(port.clone(), 0);
            ready_queue.insert(port.clone(), Vec::new());
        }

        let mut blocked_resources = HashMap::new();
        blocked_resources.insert("div".to_string(), 0);

        Self {
            arch,
            all_ports,
            port_usage,
            port_usage_at_start_of_cycle: BTreeMap::new(),
            next_p23_port: "2".to_string(),
            next_p49_port: "4".to_string(),
            next_p78_port: "7".to_string(),
            uops_dispatched_in_prev_cycle: Vec::new(),
            ready_queue,
            ready_div_uops: Vec::new(),
            uops_ready_in_cycle: BTreeMap::new(),
            non_ready_uops: Vec::new(),
            pending_uops: Vec::new(),
            pending_store_fence_uops: VecDeque::new(),
            store_uops_since_last_store_fence: Vec::new(),
            pending_load_fence_uops: VecDeque::new(),
            load_uops_since_last_load_fence: Vec::new(),
            blocked_resources,
            dependent_uops: HashMap::new(),
            uops: Vec::new(),
        }
    }

    pub fn is_full(&self) -> bool {
        self.uops.len() + self.arch.issue_width as usize > self.arch.rs_width as usize
    }

    pub fn cycle(&mut self, clock: u32, new_fused_uop_idxs: &[u64], storage: &mut UopStorage) {
        // 1. Move ready uops from uopsReadyInCycle to ready queues

        if let Some(ready_uops) = self.uops_ready_in_cycle.remove(&clock) {
            for uop_idx in ready_uops {
                if let Some(uop) = storage.get_uop(uop_idx) {
                    if std::env::var("D8").is_ok() {
                        eprintln!(
                            "C{clock} move to ready_queue: uop={} port={:?} div={}",
                            uop.idx, uop.actual_port, uop.prop.div_cycles
                        );
                    }
                    if uop.prop.div_cycles > 0 {
                        self.ready_div_uops.push((uop.queue_idx, uop_idx));
                    } else if let Some(ref port) = uop.actual_port {
                        if let Some(queue) = self.ready_queue.get_mut(port) {
                            queue.push((uop.queue_idx, uop_idx));
                        }
                    }
                }
            }
        }

        // 2-7: Other cycle steps
        self.add_new_uops(clock, new_fused_uop_idxs, storage);
        self.dispatch_uops(clock, storage);
        self.process_pending_uops(storage);
        self.process_non_ready_uops(clock, storage);
        self.process_pending_fences(clock, storage);
        self.update_blocked_resources();
    }

    fn add_new_uops(&mut self, clock: u32, new_fused_uop_idxs: &[u64], storage: &mut UopStorage) {
        self.port_usage_at_start_of_cycle
            .insert(clock, self.port_usage.clone());

        let mut port_combinations: HashMap<Vec<String>, usize> = HashMap::new();

        // Collect all unfused uop indices first
        let mut all_unfused_uop_idxs = Vec::new();
        for &fused_idx in new_fused_uop_idxs {
            if let Some(fused) = storage.get_fused_uop(fused_idx) {
                for &uop_idx in &fused.unfused_uop_idxs {
                    all_unfused_uop_idxs.push((
                        new_fused_uop_idxs
                            .iter()
                            .position(|&x| x == fused_idx)
                            .unwrap(),
                        uop_idx,
                    ));
                }
            }
        }

        // Now process each uop
        for (issue_slot, uop_idx) in all_unfused_uop_idxs {
            // Get uop properties (immutable borrow)
            let (
                possible_ports,
                eliminated,
                is_first,
                is_load_ser,
                is_store_ser,
                _is_load,
                _is_store_addr,
                _is_store_data,
            ) = {
                if let Some(uop) = storage.get_uop(uop_idx) {
                    (
                        uop.prop.possible_ports.clone(),
                        uop.eliminated,
                        uop.prop.is_first_uop_of_instr,
                        uop.prop.is_load_serializing,
                        uop.prop.is_store_serializing,
                        uop.prop.is_load_uop,
                        uop.prop.is_store_address_uop,
                        uop.prop.is_store_data_uop,
                    )
                } else {
                    continue;
                }
            };

            if possible_ports.is_empty() || eliminated {
                continue;
            }

            // Select port
            let port = if possible_ports.len() == 1 {
                possible_ports[0].clone()
            } else if self.arch.simple_port_assignment {
                possible_ports[0].clone()
            } else if self.all_ports.len() == 10 {
                self.select_port_10port_style(
                    clock,
                    issue_slot,
                    &possible_ports,
                    &mut port_combinations,
                )
            } else if self.all_ports.len() == 8 {
                self.select_port_hsw_style(clock, issue_slot, &possible_ports)
            } else {
                self.select_port_python_style(clock, issue_slot, &possible_ports)
            };

            // Now update the uop (mutable borrow)
            if let Some(uop) = storage.get_uop_mut(uop_idx) {
                uop.actual_port = Some(port.clone());
            }

            *self.port_usage.entry(port).or_insert(0) += 1;
            if !self.uops.contains(&uop_idx) {
                self.uops.push(uop_idx);
            }

            // Check dependencies
            self.check_depending_uops_executed(uop_idx, storage);

            // Handle fences
            if is_first {
                if is_store_ser {
                    self.pending_store_fence_uops.push_back(uop_idx);
                }
                if is_load_ser {
                    self.pending_load_fence_uops.push_back(uop_idx);
                }
            }
        }
    }

    fn select_port_hsw_style(
        &mut self,
        clock: u32,
        issue_slot: usize,
        possible_ports: &[String],
    ) -> String {
        let usage = self.port_usage_at_start_of_cycle.get(&clock).unwrap();
        let applicable: Vec<_> = usage
            .iter()
            .filter(|(p, _)| possible_ports.contains(p))
            .collect();

        if applicable.is_empty() {
            return possible_ports[0].clone();
        }

        let (min_port, min_usage) = applicable
            .iter()
            .min_by_key(|(p, &u)| (u, -(p.parse::<i32>().unwrap_or(0))))
            .map(|(p, &u)| ((*p).clone(), u))
            .unwrap();

        // P23 alternation
        if possible_ports == &["2".to_string(), "3".to_string()] {
            let port = self.next_p23_port.clone();
            self.next_p23_port = if self.next_p23_port == "2" {
                "3".to_string()
            } else {
                "2".to_string()
            };
            return port;
        }

        // Even issue slots use min port
        if issue_slot % 2 == 0 {
            return min_port;
        }

        // Odd issue slots: check second-best port
        let rem_applicable: Vec<_> = applicable.iter().filter(|(p, _)| **p != min_port).collect();

        if let Some((min2_port, &min2_usage)) = rem_applicable
            .iter()
            .min_by_key(|(p, &u)| (u, -(p.parse::<i32>().unwrap_or(0))))
        {
            if min2_usage >= min_usage + 3 {
                min_port
            } else {
                (*min2_port).clone()
            }
        } else {
            min_port
        }
    }

    fn select_port_10port_style(
        &mut self,
        clock: u32,
        issue_slot: usize,
        possible_ports: &[String],
        port_combinations: &mut HashMap<Vec<String>, usize>,
    ) -> String {
        // Get port usage from previous cycle (clock-1), fall back to current cycle
        let usage = if clock > 0 {
            self.port_usage_at_start_of_cycle
                .get(&(clock - 1))
                .or_else(|| self.port_usage_at_start_of_cycle.get(&clock))
                .unwrap()
        } else {
            self.port_usage_at_start_of_cycle.get(&clock).unwrap()
        };

        // Filter to applicable ports
        let mut applicable: Vec<_> = usage
            .iter()
            .filter(|(p, _)| possible_ports.contains(p))
            .collect();

        if applicable.is_empty() {
            return possible_ports[0].clone();
        }

        // Sort by (usage ascending, -port_hex descending)
        applicable.sort_by_key(|(p, &u)| (u, -(p.parse::<i32>().unwrap_or(0))));

        let min_port_usage = *applicable[0].1;

        // Filter to ports within minPortUsage + 5
        let sorted_ports: Vec<String> = applicable
            .iter()
            .filter(|(_, &u)| u < min_port_usage + 5)
            .map(|(p, _)| (*p).clone())
            .collect();

        // Track port combinations in current cycle
        let mut pc_key = possible_ports.to_vec();
        pc_key.sort();
        let n_pc = *port_combinations.get(&pc_key).unwrap_or(&0);
        port_combinations.insert(pc_key, n_pc + 1);

        // Special case: p23 alternation
        if possible_ports == ["2".to_string(), "3".to_string()]
            || possible_ports == ["3".to_string(), "2".to_string()]
        {
            let port = self.next_p23_port.clone();
            self.next_p23_port = if self.next_p23_port == "2" {
                "3".to_string()
            } else {
                "2".to_string()
            };
            return port;
        }

        // Special case: p49 alternation
        if possible_ports == ["4".to_string(), "9".to_string()]
            || possible_ports == ["9".to_string(), "4".to_string()]
        {
            let port = self.next_p49_port.clone();
            self.next_p49_port = if self.next_p49_port == "4" {
                "9".to_string()
            } else {
                "4".to_string()
            };
            return port;
        }

        // Special case: p78 alternation
        if possible_ports == ["7".to_string(), "8".to_string()]
            || possible_ports == ["8".to_string(), "7".to_string()]
        {
            let port = self.next_p78_port.clone();
            self.next_p78_port = if self.next_p78_port == "7" {
                "8".to_string()
            } else {
                "7".to_string()
            };
            return port;
        }

        // Issue slot 4: always use first sorted port
        if issue_slot == 4 {
            return sorted_ports[0].clone();
        }

        // Issue slot 3: use second port if this is first occurrence and multiple ports available
        if issue_slot == 3 && n_pc == 0 && sorted_ports.len() > 1 {
            return sorted_ports[1].clone();
        }

        // Otherwise: round-robin through sorted ports
        sorted_ports[n_pc % sorted_ports.len()].clone()
    }

    fn ready_queue_head(&mut self, port: &str) -> Option<u64> {
        let queue = self.ready_queue.get_mut(port)?;
        queue.sort_by_key(|(idx, _)| *idx);
        queue.first().map(|(_, uop_idx)| *uop_idx)
    }

    fn remove_applicable_port(applicable_ports: &mut Vec<String>, port: &str) {
        applicable_ports.retain(|p| p != port);
    }

    fn store_pair_conflicts(storage: &UopStorage, left_uop_idx: u64, right_uop_idx: u64) -> bool {
        let left_key = storage
            .get_uop(left_uop_idx)
            .and_then(|uop| uop.store_buffer_entry.as_ref())
            .and_then(|entry| entry.borrow().key);
        let right_key = storage
            .get_uop(right_uop_idx)
            .and_then(|uop| uop.store_buffer_entry.as_ref())
            .and_then(|entry| entry.borrow().key);

        match (left_key, right_key) {
            (
                Some((left_base, left_index, left_scale, left_disp)),
                Some((right_base, right_index, right_scale, right_disp)),
            ) => {
                left_base != right_base
                    || left_index != right_index
                    || left_scale != right_scale
                    || (left_disp - right_disp).abs() >= 64
            }
            _ => false,
        }
    }

    fn is_slow_256_load(storage: &UopStorage, uop_idx: u64) -> bool {
        storage
            .get_uop(uop_idx)
            .is_some_and(|uop| uop.prop.is_load_uop && uop.prop.instr_str.contains("M256"))
    }

    fn select_port_python_style(
        &mut self,
        clock: u32,
        issue_slot: usize,
        possible_ports: &[String],
    ) -> String {
        let usage = self.port_usage_at_start_of_cycle.get(&clock).unwrap();
        let mut applicable: Vec<_> = usage
            .iter()
            .filter(|(p, _)| possible_ports.contains(p))
            .collect();
        if applicable.is_empty() {
            return possible_ports[0].clone();
        }
        applicable.sort_by_key(|(p, &u)| (u, p.parse::<i32>().unwrap_or(0)));
        let min_port = applicable[0].0.clone();
        if possible_ports == ["2".to_string(), "3".to_string()] {
            let port = self.next_p23_port.clone();
            self.next_p23_port = if self.next_p23_port == "2" {
                "3".to_string()
            } else {
                "2".to_string()
            };
            return port;
        }

        if applicable
            .iter()
            .any(|(_, &u1)| applicable.iter().any(|(_, &u2)| u1.abs_diff(u2) >= 3))
        {
            return min_port;
        }

        if possible_ports == ["0".to_string(), "1".to_string(), "5".to_string()] {
            let table = match min_port.as_str() {
                "0" => ["0", "5", "1", "0"],
                "1" => ["1", "5", "0", "1"],
                "5" => ["5", "1", "0", "5"],
                _ => [
                    min_port.as_str(),
                    min_port.as_str(),
                    min_port.as_str(),
                    min_port.as_str(),
                ],
            };
            return table[issue_slot.min(3)].to_string();
        }

        if issue_slot % 2 == 0 {
            min_port
        } else {
            applicable
                .iter()
                .max_by_key(|(p, &u)| (u, p.parse::<i32>().unwrap_or(0)))
                .map(|(p, _)| (*p).clone())
                .unwrap_or(min_port)
        }
    }

    fn dispatch_uops(&mut self, clock: u32, storage: &mut UopStorage) {
        let mut applicable_ports: Vec<String> = self.all_ports.clone();

        if applicable_ports.iter().any(|p| p == "4") && applicable_ports.iter().any(|p| p == "9") {
            if let (Some(uop4), Some(uop9)) =
                (self.ready_queue_head("4"), self.ready_queue_head("9"))
            {
                if Self::store_pair_conflicts(storage, uop4, uop9) {
                    if uop4 <= uop9 {
                        Self::remove_applicable_port(&mut applicable_ports, "9");
                    } else {
                        Self::remove_applicable_port(&mut applicable_ports, "4");
                    }
                }
            }
        }

        if self.arch.slow_256bit_mem_acc {
            if let (Some(uop2), Some(uop3)) =
                (self.ready_queue_head("2"), self.ready_queue_head("3"))
            {
                if Self::is_slow_256_load(storage, uop2) && Self::is_slow_256_load(storage, uop3) {
                    if uop2 < uop3 {
                        Self::remove_applicable_port(&mut applicable_ports, "3");
                    } else {
                        Self::remove_applicable_port(&mut applicable_ports, "2");
                    }
                }
            }
        }

        let mut uops_dispatched = Vec::new();

        for port in applicable_ports {
            let use_div_queue = port == "0"
                && self.blocked_resources.get("div").copied().unwrap_or(0) == 0
                && !self.ready_div_uops.is_empty()
                && self
                    .ready_queue
                    .get(&port)
                    .is_none_or(|q| q.is_empty() || self.ready_div_uops[0].0 < q[0].0);

            if self
                .blocked_resources
                .get(&format!("port{}", port))
                .copied()
                .unwrap_or(0)
                > 0
            {
                continue;
            }

            let uop_idx = if use_div_queue {
                self.ready_div_uops.sort_by_key(|(idx, _)| *idx);
                if self.ready_div_uops.is_empty() {
                    continue;
                }
                self.ready_div_uops.remove(0).1
            } else {
                let queue = self.ready_queue.get_mut(&port).unwrap();
                if queue.is_empty() {
                    continue;
                }
                queue.sort_by_key(|(idx, _)| *idx);
                queue.remove(0).1
            };

            // Get dispatch resource effects before mutable borrow
            let (div_cycles, blocks_slow_256_store) = storage
                .get_uop(uop_idx)
                .map(|u| {
                    (
                        u.prop.div_cycles,
                        self.arch.slow_256bit_mem_acc
                            && port == "4"
                            && u.prop.instr_str.contains("M256"),
                    )
                })
                .unwrap_or((0, false));

            // Update uop
            if let Some(uop) = storage.get_uop_mut(uop_idx) {
                uop.dispatched = Some(clock);
            }

            self.uops.retain(|&idx| idx != uop_idx);
            uops_dispatched.push(uop_idx);
            if !self.pending_uops.contains(&uop_idx) {
                self.pending_uops.push(uop_idx);
            }

            *self.blocked_resources.entry("div".to_string()).or_insert(0) += div_cycles;
            if blocks_slow_256_store {
                self.blocked_resources.insert(format!("port{}", port), 2);
            }
        }

        // Decrease port usage for previous cycle's dispatches
        for &uop_idx in &self.uops_dispatched_in_prev_cycle {
            if let Some(uop) = storage.get_uop(uop_idx) {
                if let Some(ref port) = uop.actual_port {
                    if let Some(usage) = self.port_usage.get_mut(port) {
                        *usage = usage.saturating_sub(1);
                    }
                }
            }
        }

        self.uops_dispatched_in_prev_cycle = uops_dispatched;
    }

    fn process_pending_uops(&mut self, storage: &mut UopStorage) {
        let mut to_remove = Vec::new();
        let mut all_dep_uops = Vec::new();

        for &uop_idx in &self.pending_uops {
            // Collect all info needed (immutable borrows)
            let (
                dispatched,
                is_first,
                instr_tp,
                ren_out_ready,
                is_store_addr,
                is_store_data,
                sb_entry,
            ) = {
                if let Some(uop) = storage.get_uop(uop_idx) {
                    let mut all_ready = true;
                    for ren_out in &uop.renamed_output_operands {
                        if ren_out.borrow().get_ready_cycle(storage).is_none() {
                            all_ready = false;
                            break;
                        }
                    }

                    let max_ready = uop
                        .renamed_output_operands
                        .iter()
                        .filter_map(|r| r.borrow().get_ready_cycle(storage))
                        .max()
                        .unwrap_or(0);

                    (
                        uop.dispatched,
                        uop.prop.is_first_uop_of_instr,
                        uop.prop.instr_tp,
                        if all_ready { Some(max_ready) } else { None },
                        uop.prop.is_store_address_uop,
                        uop.prop.is_store_data_uop,
                        uop.store_buffer_entry.clone(),
                    )
                } else {
                    continue;
                }
            };

            if ren_out_ready.is_none() {
                continue;
            }

            let mut finish_time = dispatched.unwrap() + 2;
            if is_first {
                if let Some(tp) = instr_tp {
                    finish_time = finish_time.max(dispatched.unwrap() + tp);
                }
            }
            finish_time = finish_time.max(ren_out_ready.unwrap());

            // Store buffer updates
            if is_store_addr {
                let addr_ready = dispatched.unwrap() + 5;
                if let Some(ref sb) = sb_entry {
                    sb.borrow_mut().address_ready_cycle = Some(addr_ready);
                }
                finish_time = finish_time.max(addr_ready);
            }
            if is_store_data {
                let data_ready = dispatched.unwrap() + 1;
                if let Some(ref sb) = sb_entry {
                    sb.borrow_mut().data_ready_cycle = Some(data_ready);
                }
                finish_time = finish_time.max(data_ready);
            }

            // Collect dependent uops
            if let Some(dep_uops) = self.dependent_uops.remove(&uop_idx) {
                all_dep_uops.extend(dep_uops);
            }

            to_remove.push(uop_idx);

            // Set executed (mutable borrow)
            if let Some(uop) = storage.get_uop_mut(uop_idx) {
                uop.executed = Some(finish_time);
            }
        }

        for uop_idx in to_remove {
            self.pending_uops.retain(|&idx| idx != uop_idx);
        }

        // Now check dependent uops
        for dep_uop_idx in all_dep_uops {
            self.check_depending_uops_executed(dep_uop_idx, storage);
        }
    }

    fn process_pending_fences(&mut self, clock: u32, storage: &UopStorage) {
        // Load fences
        while let Some(&uop_idx) = self.pending_load_fence_uops.front() {
            if let Some(uop) = storage.get_uop(uop_idx) {
                if let Some(executed_cycle) = uop.executed {
                    if executed_cycle <= clock {
                        self.pending_load_fence_uops.pop_front();
                        self.load_uops_since_last_load_fence.clear();
                        continue;
                    }
                }
            }
            break;
        }

        // Store fences
        while let Some(&uop_idx) = self.pending_store_fence_uops.front() {
            if let Some(uop) = storage.get_uop(uop_idx) {
                if let Some(executed_cycle) = uop.executed {
                    if executed_cycle <= clock {
                        self.pending_store_fence_uops.pop_front();
                        self.store_uops_since_last_store_fence.clear();
                        continue;
                    }
                }
            }
            break;
        }
    }

    fn process_non_ready_uops(&mut self, clock: u32, storage: &mut UopStorage) {
        let mut new_ready_uops = Vec::new();
        let non_ready_clone = self.non_ready_uops.clone();

        for &uop_idx in &non_ready_clone {
            if self.check_uop_ready(clock, uop_idx, storage) {
                new_ready_uops.push(uop_idx);
            }
        }

        self.non_ready_uops
            .retain(|idx| !new_ready_uops.contains(idx));
    }

    fn update_blocked_resources(&mut self) {
        for (_, val) in self.blocked_resources.iter_mut() {
            *val = val.saturating_sub(1);
        }
    }

    fn check_uop_ready(&mut self, clock: u32, uop_idx: u64, storage: &mut UopStorage) -> bool {
        // Get uop info (immutable)
        let (
            already_ready,
            is_first,
            is_load_ser,
            is_store_ser,
            is_load,
            is_store_addr,
            is_store_data,
            instr_str,
            instr_tp,
        ) = {
            if let Some(uop) = storage.get_uop(uop_idx) {
                (
                    uop.ready_for_dispatch.is_some(),
                    uop.prop.is_first_uop_of_instr,
                    uop.prop.is_load_serializing,
                    uop.prop.is_store_serializing,
                    uop.prop.is_load_uop,
                    uop.prop.is_store_address_uop,
                    uop.prop.is_store_data_uop,
                    uop.prop.instr_str.clone(),
                    uop.prop.instr_tp,
                )
            } else {
                return false;
            }
        };

        if already_ready {
            return true;
        }

        if is_load_ser {
            if is_first {
                let is_fence_head = self
                    .pending_load_fence_uops
                    .front()
                    .is_some_and(|&first_fence| first_fence == uop_idx);
                let prior_load_pending = self.load_uops_since_last_load_fence.iter().any(|idx| {
                    storage
                        .get_uop(*idx)
                        .and_then(|uop| uop.executed)
                        .is_none_or(|executed| executed > clock)
                });
                if !is_fence_head || prior_load_pending {
                    return false;
                }
            }
        } else if is_store_ser {
            if is_first {
                let is_fence_head = self
                    .pending_store_fence_uops
                    .front()
                    .is_some_and(|&first_fence| first_fence == uop_idx);
                let prior_store_pending =
                    self.store_uops_since_last_store_fence.iter().any(|idx| {
                        storage
                            .get_uop(*idx)
                            .and_then(|uop| uop.executed)
                            .is_none_or(|executed| executed > clock)
                    });
                if !is_fence_head || prior_store_pending {
                    return false;
                }
            }
        } else {
            if is_load {
                if let Some(&first_fence) = self.pending_load_fence_uops.front() {
                    if first_fence < uop_idx {
                        return false;
                    }
                }
            }
            if is_store_addr || is_store_data {
                if let Some(&first_fence) = self.pending_store_fence_uops.front() {
                    if first_fence < uop_idx {
                        return false;
                    }
                }
            }
        }

        // Blocked resources
        if is_first && !instr_str.is_empty() {
            if self.blocked_resources.get(&instr_str).copied().unwrap_or(0) > 0 {
                return false;
            }
        }

        // Get ready for dispatch cycle
        let ready_for_dispatch_cycle = self.get_ready_for_dispatch_cycle(clock, uop_idx, storage);
        if ready_for_dispatch_cycle.is_none() {
            return false;
        }

        let ready_cycle = ready_for_dispatch_cycle.unwrap();

        // Set ready (mutable)
        if let Some(uop) = storage.get_uop_mut(uop_idx) {
            uop.ready_for_dispatch = Some(ready_cycle);
        }

        let ready_list = self
            .uops_ready_in_cycle
            .entry(ready_cycle)
            .or_insert_with(Vec::new);
        if !ready_list.contains(&uop_idx) {
            ready_list.push(uop_idx);
        }

        // Block resource if needed
        if is_first {
            if let Some(tp) = instr_tp {
                self.blocked_resources.insert(instr_str, tp);
            }
        }

        // Track fence uops
        if is_load {
            self.load_uops_since_last_load_fence.push(uop_idx);
        }
        if is_store_addr || is_store_data {
            self.store_uops_since_last_store_fence.push(uop_idx);
        }

        true
    }

    fn get_ready_for_dispatch_cycle(
        &self,
        clock: u32,
        uop_idx: u64,
        storage: &UopStorage,
    ) -> Option<u32> {
        let uop = storage.get_uop(uop_idx)?;
        let fused_uop = storage.get_fused_uop(uop.fused_uop_idx?)?;

        let mut op_ready_cycle = 0;
        for ren_inp_op in &uop.renamed_input_operands {
            let ren_inp = ren_inp_op.borrow();
            let ready = ren_inp.get_ready_cycle(storage)?;
            op_ready_cycle = op_ready_cycle.max(ready);
        }

        let issued = fused_uop.issued.unwrap_or(0);
        let mut ready_cycle = op_ready_cycle;

        if op_ready_cycle < issued + self.arch.issue_dispatch_delay {
            ready_cycle = issued + self.arch.issue_dispatch_delay;
        } else if op_ready_cycle == issued + self.arch.issue_dispatch_delay
            || op_ready_cycle == issued + self.arch.issue_dispatch_delay + 1
        {
            ready_cycle = op_ready_cycle + 1;
        }

        Some(ready_cycle.max(clock + 1))
    }

    fn check_depending_uops_executed(&mut self, uop_idx: u64, storage: &UopStorage) {
        // Get uop info (immutable)
        let mut has_unexecuted_dep = false;
        let mut dep_src_uop_idx = None;

        if let Some(uop) = storage.get_uop(uop_idx) {
            for ren_inp_op in &uop.renamed_input_operands {
                let ren_inp = ren_inp_op.borrow();
                if ren_inp.get_ready_cycle(storage).is_none() {
                    if let Some(src_idx) = ren_inp.uop_idx {
                        if let Some(src_uop) = storage.get_uop(src_idx) {
                            if src_uop.executed.is_none() {
                                has_unexecuted_dep = true;
                                dep_src_uop_idx = Some(src_idx);
                                break;
                            }
                        }
                    }
                }
            }
        }

        if has_unexecuted_dep {
            if let Some(src_idx) = dep_src_uop_idx {
                let deps = self.dependent_uops.entry(src_idx).or_insert_with(Vec::new);
                if !deps.contains(&uop_idx) {
                    deps.push(uop_idx);
                }
            }
        } else {
            if !self.non_ready_uops.contains(&uop_idx) {
                self.non_ready_uops.push(uop_idx);
            }
        }
    }
}
