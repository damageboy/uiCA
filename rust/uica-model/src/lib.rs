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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReportBundle {
    pub trace: TraceReport,
    pub graph: GraphReport,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TraceReport {
    #[serde(rename = "tableData")]
    pub table_data: Vec<Vec<TraceInstructionRow>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TraceInstructionRow {
    #[serde(rename = "str")]
    pub display: String,
    pub uops: Vec<TraceUopRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TraceUopRow {
    #[serde(rename = "possiblePorts")]
    pub possible_ports: String,
    #[serde(rename = "actualPort")]
    pub actual_port: String,
    pub events: BTreeMap<u32, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphReport {
    pub series: Vec<GraphSeries>,
    #[serde(rename = "interpolationToggle")]
    pub interpolation_toggle: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphSeries {
    pub name: String,
    pub y: Vec<i64>,
    pub mode: String,
    #[serde(rename = "lineShape")]
    pub line_shape: String,
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

#[cfg(test)]
mod report_tests {
    use super::{
        GraphReport, GraphSeries, ReportBundle, TraceInstructionRow, TraceReport, TraceUopRow,
    };
    use std::collections::BTreeMap;

    #[test]
    fn trace_report_serializes_like_python_table_data() {
        let trace = TraceReport {
            table_data: vec![vec![TraceInstructionRow {
                display: "add rax, rbx".to_string(),
                uops: vec![TraceUopRow {
                    possible_ports: "{0,1}".to_string(),
                    actual_port: "0".to_string(),
                    events: BTreeMap::from([(0, "P".to_string()), (2, "D".to_string())]),
                }],
            }]],
        };

        let json = serde_json::to_value(&trace).unwrap();
        assert_eq!(json["tableData"][0][0]["str"], "add rax, rbx");
        assert_eq!(json["tableData"][0][0]["uops"][0]["possiblePorts"], "{0,1}");
        assert_eq!(json["tableData"][0][0]["uops"][0]["actualPort"], "0");
        assert_eq!(json["tableData"][0][0]["uops"][0]["events"]["2"], "D");
    }

    #[test]
    fn report_bundle_serializes_graph_series() {
        let bundle = ReportBundle {
            trace: TraceReport {
                table_data: Vec::new(),
            },
            graph: GraphReport {
                series: vec![GraphSeries {
                    name: "IQ".to_string(),
                    y: vec![0, 1, 0],
                    mode: "lines+markers".to_string(),
                    line_shape: "hv".to_string(),
                }],
                interpolation_toggle: true,
            },
        };

        let json = serde_json::to_value(&bundle).unwrap();
        assert_eq!(json["graph"]["series"][0]["name"], "IQ");
        assert_eq!(json["graph"]["series"][0]["lineShape"], "hv");
        assert_eq!(json["graph"]["interpolationToggle"], true);
    }
}
