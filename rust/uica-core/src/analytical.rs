use std::collections::{BTreeMap, BTreeSet};

use crate::micro_arch::{LsdUnrollEntry, MicroArchConfig};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionPortUsage {
    pub port_data: BTreeMap<String, i32>,
    pub uops: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalyticalInstruction {
    pub size: u32,
    pub macro_fused_with_prev: bool,
    pub macro_fused_with_next: bool,
    pub macro_fusible_with_next: bool,
    pub is_branch: bool,
    pub complex_decoder: bool,
    pub n_available_simple_decoders: u32,
    pub uops_mite: u32,
    pub uops_ms: u32,
    pub can_be_used_by_lsd: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrontendLimits {
    pub decoder: Option<f64>,
    pub dsb: Option<f64>,
    pub lsd: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatencyGraphEdge {
    pub source: String,
    pub target: String,
    pub cost: i32,
    pub time: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LatencyGraph {
    pub nodes_for_instr: Vec<Vec<String>>,
    pub edges_for_node: BTreeMap<String, Vec<LatencyGraphEdge>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalyticalMemOperand {
    pub base: Option<String>,
    pub index: Option<String>,
    pub disp: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalyticalLatencyInstruction {
    pub instr_str: String,
    pub uops: i32,
    pub input_operands: Vec<String>,
    pub output_operands: Vec<String>,
    pub input_mem_operands: Vec<AnalyticalMemOperand>,
    pub mem_addr_operands: Vec<String>,
    pub mem_addr_latency_pairs: BTreeSet<(String, String)>,
    pub latencies: BTreeMap<(String, String), i32>,
    pub implicit_rsp_change: i32,
    pub non_implicit_input_operands: BTreeSet<String>,
    pub may_be_eliminated: bool,
    pub eliminated_move_input: Option<String>,
    pub eliminated_move_output_is_32_bit: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaximumLatencyResult {
    pub max_cycle_ratio: f64,
    pub edges_on_max_cycle: Vec<LatencyGraphEdge>,
    pub strongly_connected_components: Vec<Vec<String>>,
}

pub fn compute_port_usage_limit(instructions: &[InstructionPortUsage]) -> f64 {
    let mut port_usage: Vec<(BTreeSet<char>, i32)> = Vec::new();

    for instruction in instructions {
        for (ports, n_uops) in &instruction.port_data {
            let port_set = normalize_port_set(ports);
            if port_set.is_empty() {
                continue;
            }

            if let Some((_, total)) = port_usage.iter_mut().find(|(set, _)| *set == port_set) {
                *total += *n_uops;
            } else {
                port_usage.push((port_set, *n_uops));
            }
        }
    }

    if port_usage.is_empty() {
        return 0.0;
    }

    let mut limit: f64 = 0.0;

    for (left_set, _) in &port_usage {
        for (right_set, _) in &port_usage {
            let candidate: BTreeSet<char> = left_set.union(right_set).copied().collect();
            if candidate.is_empty() {
                continue;
            }

            let total_uops: i32 = port_usage
                .iter()
                .filter(|(set, _)| set.is_subset(&candidate))
                .map(|(_, n_uops)| *n_uops)
                .sum();

            limit = limit.max(total_uops as f64 / candidate.len() as f64);
        }
    }

    limit
}

pub fn compute_issue_limit(total_uops: i32, issue_width: i32) -> f64 {
    if issue_width <= 0 {
        return if total_uops > 0 { f64::INFINITY } else { 0.0 };
    }

    total_uops as f64 / issue_width as f64
}

pub fn compute_frontend_limits(
    instructions: &[AnalyticalInstruction],
    arch: &MicroArchConfig,
    alignment_offset: u32,
) -> FrontendLimits {
    FrontendLimits {
        decoder: compute_decoder_limit(instructions, arch),
        dsb: compute_dsb_limit(instructions, alignment_offset, arch),
        lsd: compute_lsd_limit(instructions, arch),
    }
}

pub fn compute_decoder_limit(
    instructions: &[AnalyticalInstruction],
    arch: &MicroArchConfig,
) -> Option<f64> {
    let instructions: Vec<&AnalyticalInstruction> = instructions
        .iter()
        .filter(|instr| !instr.macro_fused_with_prev)
        .collect();
    if instructions.is_empty() || arch.n_decoders == 0 {
        return None;
    }

    let mut first_instr_on_decoder: BTreeMap<u32, u32> = BTreeMap::new();
    let mut n_avail_simple_dec = 0u32;
    let mut cur_dec = arch.n_decoders.saturating_sub(1);
    let mut complex_per_round: BTreeMap<u32, u32> = BTreeMap::new();

    for round in 0..10_000u32 {
        complex_per_round.insert(round, 0);
        for (idx, instr) in instructions.iter().enumerate() {
            if instr.complex_decoder {
                cur_dec = 0;
                n_avail_simple_dec = instr.n_available_simple_decoders;
            } else if n_avail_simple_dec == 0
                || (cur_dec + 1 == arch.n_decoders.saturating_sub(1)
                    && instr.macro_fusible_with_next
                    && !arch.macro_fusible_instr_can_be_decoded_as_last_instr)
            {
                cur_dec = 0;
                n_avail_simple_dec = arch.n_decoders.saturating_sub(1);
            } else {
                cur_dec += 1;
                n_avail_simple_dec = n_avail_simple_dec.saturating_sub(1);
            }

            if instr.is_branch || instr.macro_fused_with_next {
                n_avail_simple_dec = 0;
            }

            if cur_dec == 0 {
                *complex_per_round.entry(round).or_default() += 1;
            }

            if idx == 0 {
                if let Some(first_round) = first_instr_on_decoder.get(&cur_dec).copied() {
                    if round == first_round {
                        return None;
                    }
                    let total: u32 = (first_round..round)
                        .map(|r| complex_per_round.get(&r).copied().unwrap_or(0))
                        .sum();
                    return Some(total as f64 / (round - first_round) as f64);
                }
                first_instr_on_decoder.insert(cur_dec, round);
            }
        }
    }

    None
}

pub fn compute_lsd_limit(
    instructions: &[AnalyticalInstruction],
    arch: &MicroArchConfig,
) -> Option<f64> {
    if !arch.lsd_enabled || arch.issue_width == 0 {
        return None;
    }
    if instructions.iter().any(|instr| !instr.can_be_used_by_lsd) {
        return None;
    }
    let n_uops: u32 = instructions
        .iter()
        .filter(|instr| !instr.macro_fused_with_prev)
        .map(|instr| instr.uops_mite + instr.uops_ms)
        .sum();
    if n_uops == 0 || n_uops > arch.idq_width {
        return None;
    }
    let unroll = lsd_unroll_count(arch.lsd_unrolling, n_uops);
    Some(((n_uops * unroll) as f64 / arch.issue_width as f64).ceil() / unroll as f64)
}

pub fn compute_dsb_limit(
    instructions: &[AnalyticalInstruction],
    alignment_offset: u32,
    arch: &MicroArchConfig,
) -> Option<f64> {
    if arch.dsb_width == 0 || instructions.is_empty() {
        return None;
    }
    let n_uops: u32 = instructions
        .iter()
        .filter(|instr| !instr.macro_fused_with_prev)
        .map(|instr| instr.uops_mite)
        .sum();
    let code_length: u32 = instructions
        .iter()
        .take(instructions.len() - 1)
        .map(|i| i.size)
        .sum();
    // Python parity: `facile.computeDSBLimit()` uses six DSB entries per
    // 32-byte block for this analytical limit, independent of DSB issue width.
    const DSB_ENTRIES_PER_BLOCK: f64 = 6.0;
    if (code_length + alignment_offset) / 32 == alignment_offset / 32 {
        Some((n_uops as f64 / DSB_ENTRIES_PER_BLOCK).ceil())
    } else {
        Some(n_uops as f64 / DSB_ENTRIES_PER_BLOCK)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnalyticalPrediction {
    pub throughput: f64,
    pub bottlenecks: Vec<String>,
}

pub fn compute_final_prediction(limits: &BTreeMap<String, Option<f64>>) -> AnalyticalPrediction {
    let throughput = limits
        .values()
        .filter_map(|value| *value)
        .fold(0.0, f64::max);
    let mut bottlenecks = Vec::new();
    if throughput.is_finite() && throughput > 0.0 {
        for (name, maybe_limit) in limits {
            if maybe_limit.is_some_and(|limit| limit >= 0.98 * throughput) {
                bottlenecks.push(match name.as_str() {
                    "predecoder" => "Predecoder".to_string(),
                    "decoder" => "Decoder".to_string(),
                    "dsb" => "DSB".to_string(),
                    "lsd" => "LSD".to_string(),
                    "issue" => "Issue".to_string(),
                    "ports" => "Ports".to_string(),
                    "dependencies" => "Dependencies".to_string(),
                    other => other.to_string(),
                });
            }
        }
    }
    bottlenecks.sort();
    AnalyticalPrediction {
        throughput,
        bottlenecks,
    }
}

pub fn generate_latency_graph(
    instructions: &[AnalyticalLatencyInstruction],
    fast_pointer_chasing: bool,
) -> LatencyGraph {
    let prev_write_for_move = python_prev_write_for_move(instructions);
    let mut prev_write: BTreeMap<String, (usize, String, bool)> = BTreeMap::new();

    let process_outputs = |idx: usize,
                           instr: &AnalyticalLatencyInstruction,
                           prev_write: &mut BTreeMap<String, (usize, String, bool)>,
                           rsp_implicitly_changed: &mut bool| {
        let fast_ptr_chasing = analytical_fast_ptr_chasing(
            instructions,
            &prev_write_for_move,
            &*prev_write,
            instr,
            fast_pointer_chasing,
        );
        // Python parity: `facile.generateLatencyGraph()` keeps
        // `RSPImplicitlyChanged` while processing outputs. Implicit stack
        // changes (PUSH/POP/CALL/RET) make the next explicit RSP self-edge
        // one cycle longer; any explicit RSP input/output clears it.
        if instr.implicit_rsp_change != 0 {
            *rsp_implicitly_changed = true;
        } else if instr.non_implicit_input_operands.contains("RSP")
            || instr.output_operands.iter().any(|output| output == "RSP")
        {
            *rsp_implicitly_changed = false;
        }
        for output in &instr.output_operands {
            prev_write.insert(output.clone(), (idx, output.clone(), fast_ptr_chasing));
        }
    };

    let mut rsp_implicitly_changed = false;

    // Python parity: `facile.generateLatencyGraph()` primes `prevWriteToKey`
    // by processing `instructions * 2` before adding graph edges.
    for _ in 0..2 {
        for (idx, instr) in instructions.iter().enumerate() {
            process_outputs(idx, instr, &mut prev_write, &mut rsp_implicitly_changed);
        }
    }

    let mut nodes_for_instr = Vec::with_capacity(instructions.len());
    let mut edges_for_node: BTreeMap<String, Vec<LatencyGraphEdge>> = BTreeMap::new();

    for (idx, instr) in instructions.iter().enumerate() {
        let nodes: Vec<String> = instr
            .input_operands
            .iter()
            .map(|operand| format!("i{idx}:{operand}"))
            .collect();
        nodes_for_instr.push(nodes);

        for input in &instr.input_operands {
            let Some((prev_idx, prev_output, fast_ptr_chasing)) = prev_write.get(input).cloned()
            else {
                continue;
            };
            let prev_instr = &instructions[prev_idx];
            for prev_input in &prev_instr.input_operands {
                let latency = if prev_instr.may_be_eliminated {
                    Some(0)
                } else if !is_memory_operand_key(prev_input) && is_memory_operand_key(input) {
                    // Python parity: `facile.generateLatencyGraph()` uses a
                    // zero edge when a previous non-memory input contributes to
                    // a later memory operand (`not MemOperand -> MemOperand`).
                    // This keeps store-address inputs from adding data latency
                    // to a later load from the same abstract address.
                    Some(0)
                } else {
                    prev_instr
                        .latencies
                        .get(&(prev_input.clone(), prev_output.clone()))
                        .copied()
                        .filter(|lat| *lat > 0)
                        .map(|lat| {
                            if fast_ptr_chasing
                                && prev_instr
                                    .mem_addr_latency_pairs
                                    .contains(&(prev_input.clone(), prev_output.clone()))
                                && !prev_instr.input_mem_operands.is_empty()
                            {
                                lat - 1
                            } else if rsp_implicitly_changed
                                && prev_idx == idx
                                && instr.non_implicit_input_operands.contains(prev_input)
                                && prev_input == "RSP"
                            {
                                // Python parity: when `RSPImplicitlyChanged`
                                // is set, an explicit non-stack RSP input on
                                // the same instruction gets one extra cycle.
                                lat + 1
                            } else {
                                lat
                            }
                        })
                };
                let Some(latency) = latency else {
                    continue;
                };
                let source = format!("i{prev_idx}:{prev_input}");
                let target = format!("i{idx}:{input}");
                let time = if prev_idx < idx { 0 } else { 1 };
                edges_for_node
                    .entry(source.clone())
                    .or_default()
                    .push(LatencyGraphEdge {
                        source,
                        target,
                        cost: latency,
                        time,
                    });
            }
        }

        process_outputs(idx, instr, &mut prev_write, &mut rsp_implicitly_changed);
    }

    LatencyGraph {
        nodes_for_instr,
        edges_for_node,
    }
}

fn analytical_fast_ptr_chasing(
    instructions: &[AnalyticalLatencyInstruction],
    prev_write_for_move: &BTreeMap<usize, Option<usize>>,
    prev_write: &BTreeMap<String, (usize, String, bool)>,
    instr: &AnalyticalLatencyInstruction,
    fast_pointer_chasing: bool,
) -> bool {
    let Some(mem) = instr.input_mem_operands.first() else {
        return false;
    };
    if !fast_pointer_chasing || mem.disp < 0 || mem.disp >= 2048 {
        return false;
    }
    let Some(base) = &mem.base else {
        return false;
    };
    let Some((base_idx, _, _)) = prev_write.get(base) else {
        return false;
    };
    let Some((base_idx, base_renamed_by_32_bit_move)) =
        resolve_python_non_eliminated_move_writer(instructions, prev_write_for_move, *base_idx)
    else {
        return false;
    };
    if base_renamed_by_32_bit_move {
        return false;
    }
    let base_instr = &instructions[base_idx];
    if !matches!(
        base_instr.instr_str.as_str(),
        "MOV (R64, M64)"
            | "MOV (RAX, M64)"
            | "MOV (R32, M32)"
            | "MOV (EAX, M32)"
            | "MOVSXD (R64, M32)"
            | "POP (R64)"
    ) {
        return false;
    }
    mem.index.as_ref().is_none_or(|index| {
        prev_write.get(index).is_some_and(|(index_idx, _, _)| {
            resolve_python_non_eliminated_move_writer(instructions, prev_write_for_move, *index_idx)
                .is_some_and(|(resolved_idx, _)| instructions[resolved_idx].uops == 0)
        })
    })
}

fn resolve_python_non_eliminated_move_writer(
    instructions: &[AnalyticalLatencyInstruction],
    prev_write_for_move: &BTreeMap<usize, Option<usize>>,
    mut idx: usize,
) -> Option<(usize, bool)> {
    // Python parity: `facile.generateLatencyGraph()` follows
    // `prevNonEliminatedWriteForMove` for eliminated MOV producers and tracks
    // whether the path contains a 32-bit eliminated move.
    let mut seen = BTreeSet::new();
    let mut renamed_by_32_bit_move = false;
    loop {
        if !seen.insert(idx) {
            return None;
        }
        let instr = &instructions[idx];
        if !instr.may_be_eliminated {
            return Some((idx, renamed_by_32_bit_move));
        }
        if instr.eliminated_move_output_is_32_bit {
            renamed_by_32_bit_move = true;
        }
        idx = (*prev_write_for_move.get(&idx)?)?;
    }
}

fn python_prev_write_for_move(
    instructions: &[AnalyticalLatencyInstruction],
) -> BTreeMap<usize, Option<usize>> {
    let mut prev_write_to_reg = BTreeMap::new();
    let mut prev_write_for_move = BTreeMap::new();
    for _ in 0..2 {
        for (idx, instr) in instructions.iter().enumerate() {
            if instr.may_be_eliminated {
                let prev = instr
                    .eliminated_move_input
                    .as_ref()
                    .and_then(|input| prev_write_to_reg.get(input).copied());
                prev_write_for_move.insert(idx, prev);
            }
            for output in &instr.output_operands {
                prev_write_to_reg.insert(output.clone(), idx);
            }
        }
    }
    prev_write_for_move
}

pub fn compute_maximum_latency_for_graph(graph: &LatencyGraph) -> MaximumLatencyResult {
    let components = find_strongly_connected_components(graph);
    let mut max_cycle_ratio = 0.0;
    let mut edges_on_max_cycle = Vec::new();

    for component in &components {
        let component_set: BTreeSet<String> = component.iter().cloned().collect();
        let edges: Vec<LatencyGraphEdge> = component
            .iter()
            .flat_map(|node| graph.edges_for_node.get(node).into_iter().flatten())
            .filter(|edge| component_set.contains(&edge.target))
            .cloned()
            .collect();
        if edges.is_empty() {
            continue;
        }
        let (ratio, cycle_edges) = maximum_cycle_ratio(component, &edges);
        if ratio > max_cycle_ratio {
            max_cycle_ratio = ratio;
            edges_on_max_cycle = cycle_edges;
        }
    }

    MaximumLatencyResult {
        max_cycle_ratio,
        edges_on_max_cycle,
        strongly_connected_components: components,
    }
}

fn is_memory_operand_key(operand: &str) -> bool {
    operand.starts_with("MEM:")
}

fn find_strongly_connected_components(graph: &LatencyGraph) -> Vec<Vec<String>> {
    struct Tarjan<'a> {
        graph: &'a LatencyGraph,
        index: usize,
        stack: Vec<String>,
        on_stack: BTreeSet<String>,
        indices: BTreeMap<String, usize>,
        lowlinks: BTreeMap<String, usize>,
        components: Vec<Vec<String>>,
    }

    impl<'a> Tarjan<'a> {
        fn strong_connect(&mut self, node: String) {
            let idx = self.index;
            self.index += 1;
            self.indices.insert(node.clone(), idx);
            self.lowlinks.insert(node.clone(), idx);
            self.stack.push(node.clone());
            self.on_stack.insert(node.clone());

            let edges = self
                .graph
                .edges_for_node
                .get(&node)
                .cloned()
                .unwrap_or_default();
            for edge in edges {
                let target = edge.target;
                if !self.indices.contains_key(&target) {
                    self.strong_connect(target.clone());
                    let low_node = self.lowlinks[&node].min(self.lowlinks[&target]);
                    self.lowlinks.insert(node.clone(), low_node);
                } else if self.on_stack.contains(&target) {
                    let low_node = self.lowlinks[&node].min(self.indices[&target]);
                    self.lowlinks.insert(node.clone(), low_node);
                }
            }

            if self.lowlinks[&node] == self.indices[&node] {
                let mut component = Vec::new();
                while let Some(top) = self.stack.pop() {
                    self.on_stack.remove(&top);
                    component.push(top.clone());
                    if top == node {
                        break;
                    }
                }
                self.components.push(component);
            }
        }
    }

    let mut nodes = BTreeSet::new();
    for node_list in &graph.nodes_for_instr {
        nodes.extend(node_list.iter().cloned());
    }
    for (source, edges) in &graph.edges_for_node {
        nodes.insert(source.clone());
        for edge in edges {
            nodes.insert(edge.target.clone());
        }
    }

    let mut tarjan = Tarjan {
        graph,
        index: 0,
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        indices: BTreeMap::new(),
        lowlinks: BTreeMap::new(),
        components: Vec::new(),
    };

    for node in nodes {
        if !tarjan.indices.contains_key(&node) {
            tarjan.strong_connect(node);
        }
    }

    tarjan.components
}

fn maximum_cycle_ratio(
    nodes: &[String],
    edges: &[LatencyGraphEdge],
) -> (f64, Vec<LatencyGraphEdge>) {
    let mut best_ratio = 0.0;
    let mut best_edges = Vec::new();
    for start in nodes {
        let mut stack = vec![(start.clone(), Vec::<LatencyGraphEdge>::new(), 0i32, 0i32)];
        while let Some((node, path, cost, time)) = stack.pop() {
            if path.len() > nodes.len() {
                continue;
            }
            for edge in edges.iter().filter(|edge| edge.source == node) {
                let mut next_path = path.clone();
                next_path.push(edge.clone());
                let next_cost = cost + edge.cost;
                let next_time = time + edge.time;
                if edge.target == *start && next_time > 0 {
                    let ratio = next_cost as f64 / next_time as f64;
                    if ratio > best_ratio {
                        best_ratio = ratio;
                        best_edges = next_path.clone();
                    }
                } else if !path.iter().any(|path_edge| path_edge.source == edge.target) {
                    stack.push((edge.target.clone(), next_path, next_cost, next_time));
                }
            }
        }
    }
    (best_ratio, best_edges)
}

fn lsd_unroll_count(entries: &[LsdUnrollEntry], n_uops: u32) -> u32 {
    entries
        .iter()
        .find(|entry| entry.nuops == n_uops)
        .map(|entry| entry.unroll)
        .unwrap_or(1)
}

fn normalize_port_set(ports: &str) -> BTreeSet<char> {
    ports.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_graph_mirrors_python_non_mem_to_mem_zero_edge() {
        let push = AnalyticalLatencyInstruction {
            input_operands: vec!["RBP".to_string(), "RSP".to_string()],
            output_operands: vec!["MEM:RSP::0:0".to_string()],
            latencies: BTreeMap::from([
                (("RBP".to_string(), "MEM:RSP::0:0".to_string()), 2),
                (("RSP".to_string(), "MEM:RSP::0:0".to_string()), 11),
            ]),
            ..Default::default()
        };
        let pop = AnalyticalLatencyInstruction {
            input_operands: vec!["RSP".to_string(), "MEM:RSP::0:0".to_string()],
            output_operands: vec!["RBP".to_string()],
            latencies: BTreeMap::from([
                (("MEM:RSP::0:0".to_string(), "RBP".to_string()), 2),
                (("RSP".to_string(), "RBP".to_string()), 5),
            ]),
            ..Default::default()
        };

        let graph = generate_latency_graph(&[push, pop], false);
        let mem_edges = graph.edges_for_node.get("i0:RBP").unwrap();
        assert!(mem_edges
            .iter()
            .any(|edge| { edge.target == "i1:MEM:RSP::0:0" && edge.cost == 0 && edge.time == 0 }));
        assert_eq!(
            compute_maximum_latency_for_graph(&graph).max_cycle_ratio,
            2.0
        );
    }

    #[test]
    fn latency_graph_mirrors_python_rsp_implicitly_changed_extra_self_edge() {
        let add_rsp = AnalyticalLatencyInstruction {
            instr_str: "ADD (R64, I32)".to_string(),
            input_operands: vec!["RSP".to_string()],
            output_operands: vec!["RSP".to_string()],
            non_implicit_input_operands: BTreeSet::from(["RSP".to_string()]),
            latencies: BTreeMap::from([(("RSP".to_string(), "RSP".to_string()), 1)]),
            ..Default::default()
        };
        let pop = AnalyticalLatencyInstruction {
            instr_str: "POP (R64)".to_string(),
            implicit_rsp_change: 8,
            input_operands: vec!["RSP".to_string(), "MEM:RSP::1:0".to_string()],
            output_operands: vec!["RBX".to_string()],
            latencies: BTreeMap::from([
                (("RSP".to_string(), "RBX".to_string()), 5),
                (("MEM:RSP::1:0".to_string(), "RBX".to_string()), 2),
            ]),
            ..Default::default()
        };

        let graph = generate_latency_graph(&[add_rsp, pop], false);
        let rsp_edges = graph.edges_for_node.get("i0:RSP").unwrap();
        assert!(rsp_edges
            .iter()
            .any(|edge| edge.target == "i0:RSP" && edge.cost == 2 && edge.time == 1));
        assert_eq!(
            compute_maximum_latency_for_graph(&graph).max_cycle_ratio,
            2.0
        );
    }

    #[test]
    fn latency_graph_mirrors_python_fast_pointer_chasing_reduction() {
        let load_ptr = AnalyticalLatencyInstruction {
            instr_str: "MOV (R64, M64)".to_string(),
            uops: 1,
            input_operands: vec!["RAX".to_string(), "MEM:RAX::1:0".to_string()],
            output_operands: vec!["RAX".to_string()],
            input_mem_operands: vec![AnalyticalMemOperand {
                base: Some("RAX".to_string()),
                index: None,
                disp: 0,
            }],
            mem_addr_operands: vec!["RAX".to_string()],
            latencies: BTreeMap::from([
                (("RAX".to_string(), "RAX".to_string()), 5),
                (("MEM:RAX::1:0".to_string(), "RAX".to_string()), 2),
            ]),
            ..Default::default()
        };
        let load_value = AnalyticalLatencyInstruction {
            instr_str: "MOV (R32, M32)".to_string(),
            uops: 1,
            input_operands: vec!["RAX".to_string(), "MEM:RAX::1:8".to_string()],
            output_operands: vec!["RAX".to_string()],
            input_mem_operands: vec![AnalyticalMemOperand {
                base: Some("RAX".to_string()),
                index: None,
                disp: 8,
            }],
            mem_addr_operands: vec!["RAX".to_string()],
            mem_addr_latency_pairs: BTreeSet::from([("RAX".to_string(), "RAX".to_string())]),
            latencies: BTreeMap::from([
                (("RAX".to_string(), "RAX".to_string()), 5),
                (("MEM:RAX::1:8".to_string(), "RAX".to_string()), 2),
            ]),
            ..Default::default()
        };
        let shr = AnalyticalLatencyInstruction {
            instr_str: "SHR (R32, I8)".to_string(),
            uops: 1,
            input_operands: vec!["RAX".to_string()],
            output_operands: vec!["RAX".to_string()],
            latencies: BTreeMap::from([(("RAX".to_string(), "RAX".to_string()), 1)]),
            ..Default::default()
        };

        let graph = generate_latency_graph(&[load_ptr, load_value, shr], true);
        let addr_edges = graph.edges_for_node.get("i1:RAX").unwrap();
        assert!(addr_edges
            .iter()
            .any(|edge| edge.target == "i2:RAX" && edge.cost == 4 && edge.time == 0));
    }

    #[test]
    fn fast_pointer_chasing_follows_python_eliminated_move_chain() {
        let load_ptr = AnalyticalLatencyInstruction {
            instr_str: "MOV (R64, M64)".to_string(),
            uops: 1,
            input_operands: vec!["RAX".to_string(), "MEM:RAX::1:0".to_string()],
            output_operands: vec!["RAX".to_string()],
            input_mem_operands: vec![AnalyticalMemOperand {
                base: Some("RAX".to_string()),
                index: None,
                disp: 0,
            }],
            mem_addr_latency_pairs: BTreeSet::from([("RAX".to_string(), "RAX".to_string())]),
            latencies: BTreeMap::from([(("RAX".to_string(), "RAX".to_string()), 5)]),
            ..Default::default()
        };
        let eliminated_mov64 = AnalyticalLatencyInstruction {
            instr_str: "MOV (R64, R64)".to_string(),
            input_operands: vec!["RAX".to_string()],
            output_operands: vec!["RBX".to_string()],
            may_be_eliminated: true,
            eliminated_move_input: Some("RAX".to_string()),
            ..Default::default()
        };
        let load_value = AnalyticalLatencyInstruction {
            instr_str: "MOV (R32, M32)".to_string(),
            uops: 1,
            input_operands: vec!["RBX".to_string(), "MEM:RBX::1:8".to_string()],
            output_operands: vec!["RCX".to_string()],
            input_mem_operands: vec![AnalyticalMemOperand {
                base: Some("RBX".to_string()),
                index: None,
                disp: 8,
            }],
            mem_addr_latency_pairs: BTreeSet::from([("RBX".to_string(), "RCX".to_string())]),
            latencies: BTreeMap::from([(("RBX".to_string(), "RCX".to_string()), 5)]),
            ..Default::default()
        };
        let use_value = AnalyticalLatencyInstruction {
            input_operands: vec!["RCX".to_string()],
            output_operands: vec!["RCX".to_string()],
            latencies: BTreeMap::from([(("RCX".to_string(), "RCX".to_string()), 1)]),
            ..Default::default()
        };

        let graph = generate_latency_graph(
            &[
                load_ptr.clone(),
                eliminated_mov64,
                load_value.clone(),
                use_value.clone(),
            ],
            true,
        );
        assert!(graph
            .edges_for_node
            .get("i2:RBX")
            .unwrap()
            .iter()
            .any(|edge| edge.target == "i3:RCX" && edge.cost == 4));

        let eliminated_mov32 = AnalyticalLatencyInstruction {
            instr_str: "MOV (R32, R32)".to_string(),
            input_operands: vec!["RAX".to_string()],
            output_operands: vec!["RBX".to_string()],
            may_be_eliminated: true,
            eliminated_move_input: Some("RAX".to_string()),
            eliminated_move_output_is_32_bit: true,
            ..Default::default()
        };
        let graph =
            generate_latency_graph(&[load_ptr, eliminated_mov32, load_value, use_value], true);
        assert!(graph
            .edges_for_node
            .get("i2:RBX")
            .unwrap()
            .iter()
            .any(|edge| edge.target == "i3:RCX" && edge.cost == 5));
    }
}
