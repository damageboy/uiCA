use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Invocation {
    pub arch: String,
    #[serde(rename = "alignmentOffset")]
    pub alignment_offset: u32,
    #[serde(rename = "initPolicy")]
    pub init_policy: String,
    #[serde(rename = "noMicroFusion")]
    pub no_micro_fusion: bool,
    #[serde(rename = "noMacroFusion")]
    pub no_macro_fusion: bool,
    #[serde(rename = "simpleFrontEnd")]
    pub simple_front_end: bool,
    #[serde(rename = "minIterations")]
    pub min_iterations: u32,
    #[serde(rename = "minCycles")]
    pub min_cycles: u32,
}

impl Default for Invocation {
    fn default() -> Self {
        Self {
            arch: String::new(),
            alignment_offset: 0,
            init_policy: "diff".to_string(),
            no_micro_fusion: false,
            no_macro_fusion: false,
            simple_front_end: false,
            min_iterations: 10,
            min_cycles: 500,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Summary {
    pub throughput_cycles_per_iteration: Option<f64>,
    pub iterations_simulated: u32,
    pub cycles_simulated: u32,
    pub mode: String,
    pub bottlenecks_predicted: Vec<String>,
    pub limits: BTreeMap<String, Option<f64>>,
}

impl Default for Summary {
    fn default() -> Self {
        Self {
            throughput_cycles_per_iteration: None,
            iterations_simulated: 0,
            cycles_simulated: 0,
            mode: "loop".to_string(),
            bottlenecks_predicted: Vec::new(),
            limits: default_limits(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UicaResult {
    pub schema_version: String,
    pub engine: String,
    pub engine_version: String,
    pub uica_commit: String,
    pub invocation: Invocation,
    pub summary: Summary,
    pub parameters: Value,
    pub instructions: Vec<Value>,
    pub cycles: Vec<Value>,
}

impl Default for UicaResult {
    fn default() -> Self {
        Self {
            schema_version: "uica-result-v1".to_string(),
            engine: "rust".to_string(),
            engine_version: "uiCA-rust".to_string(),
            uica_commit: "unknown".to_string(),
            invocation: Invocation::default(),
            summary: Summary::default(),
            parameters: Value::Object(Default::default()),
            instructions: Vec::new(),
            cycles: Vec::new(),
        }
    }
}

fn default_limits() -> BTreeMap<String, Option<f64>> {
    [
        "predecoder",
        "decoder",
        "dsb",
        "lsd",
        "issue",
        "ports",
        "dependencies",
    ]
    .into_iter()
    .map(|name| (name.to_string(), None))
    .collect()
}
