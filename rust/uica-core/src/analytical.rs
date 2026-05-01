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
pub struct AnalyticalLatencyInstruction {
    pub input_operands: Vec<String>,
    pub output_operands: Vec<String>,
    pub latencies: BTreeMap<(String, String), i32>,
    pub may_be_eliminated: bool,
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

pub fn generate_latency_graph(instructions: &[AnalyticalLatencyInstruction]) -> LatencyGraph {
    let mut prev_write: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for (idx, instr) in instructions.iter().enumerate() {
        for output in &instr.output_operands {
            prev_write.insert(output.clone(), (idx, output.clone()));
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
            let Some((prev_idx, prev_output)) = prev_write.get(input).cloned() else {
                continue;
            };
            let prev_instr = &instructions[prev_idx];
            for prev_input in &prev_instr.input_operands {
                let latency = if prev_instr.may_be_eliminated {
                    Some(0)
                } else {
                    prev_instr
                        .latencies
                        .get(&(prev_input.clone(), prev_output.clone()))
                        .copied()
                        .filter(|lat| *lat > 0)
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

        for output in &instr.output_operands {
            prev_write.insert(output.clone(), (idx, output.clone()));
        }
    }

    LatencyGraph {
        nodes_for_instr,
        edges_for_node,
    }
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
