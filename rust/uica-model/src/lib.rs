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
    pub regular: RegularOutputReport,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularOutputReport {
    pub schema_version: String,
    pub arch: String,
    pub throughput_cycles_per_iteration: Option<f64>,
    pub bottlenecks: Vec<String>,
    pub limits: BTreeMap<String, Option<f64>>,
    pub limit_lines: Vec<RegularLimitLine>,
    pub columns: Vec<RegularColumn>,
    pub rows: Vec<RegularOutputRow>,
    pub totals: RegularOutputMetrics,
    pub notes: Vec<RegularNote>,
}

impl Default for RegularOutputReport {
    fn default() -> Self {
        Self {
            schema_version: "uica-regular-report-v1".to_string(),
            arch: String::new(),
            throughput_cycles_per_iteration: None,
            bottlenecks: Vec::new(),
            limits: BTreeMap::new(),
            limit_lines: Vec::new(),
            columns: Vec::new(),
            rows: Vec::new(),
            totals: RegularOutputMetrics::default(),
            notes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularLimitLine {
    pub key: String,
    pub label: String,
    pub throughput: f64,
    pub is_bottleneck: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegularColumnKind {
    #[default]
    Frontend,
    Issue,
    Execute,
    Port,
    Divider,
    Notes,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularColumn {
    pub key: String,
    pub label: String,
    pub kind: RegularColumnKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegularRowKind {
    #[default]
    Instruction,
    RegisterMerge,
    StackSync,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularOutputRow {
    pub row_id: String,
    pub kind: RegularRowKind,
    pub instr_id: Option<u32>,
    pub asm: String,
    pub opcode: Option<String>,
    pub url: Option<String>,
    pub notes: Vec<String>,
    pub metrics: RegularOutputMetrics,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularOutputMetrics {
    pub mite: f64,
    pub ms: f64,
    pub dsb: f64,
    pub lsd: f64,
    pub issued: f64,
    pub executed: f64,
    pub div: f64,
    pub ports: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegularNote {
    pub key: String,
    pub label: String,
    pub url: Option<String>,
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
            ..ReportBundle::default()
        };

        let json = serde_json::to_value(&bundle).unwrap();
        assert_eq!(json["graph"]["series"][0]["name"], "IQ");
        assert_eq!(json["graph"]["series"][0]["lineShape"], "hv");
        assert_eq!(json["graph"]["interpolationToggle"], true);
    }

    #[test]
    fn regular_report_serializes_structured_rows() {
        use super::{
            RegularColumn, RegularColumnKind, RegularNote, RegularOutputMetrics,
            RegularOutputReport, RegularOutputRow, RegularRowKind,
        };

        let metrics = RegularOutputMetrics {
            dsb: 1.0,
            issued: 1.0,
            executed: 1.0,
            ports: BTreeMap::from([("0".to_string(), 0.5)]),
            ..RegularOutputMetrics::default()
        };

        let report = RegularOutputReport {
            arch: "SKL".to_string(),
            throughput_cycles_per_iteration: Some(2.0),
            bottlenecks: vec!["Dependencies".to_string()],
            columns: vec![RegularColumn {
                key: "port_0".to_string(),
                label: "Port 0".to_string(),
                kind: RegularColumnKind::Port,
            }],
            rows: vec![RegularOutputRow {
                row_id: "instr-0".to_string(),
                kind: RegularRowKind::Instruction,
                instr_id: Some(0),
                asm: "add rax, rbx".to_string(),
                opcode: Some("4801D8".to_string()),
                url: Some("https://www.uops.info/html-instr/ADD_R64_R64.html".to_string()),
                notes: Vec::new(),
                metrics,
            }],
            notes: vec![RegularNote {
                key: "M".to_string(),
                label: "Macro-fused with previous instruction".to_string(),
                url: None,
            }],
            ..RegularOutputReport::default()
        };

        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["schema_version"], "uica-regular-report-v1");
        assert_eq!(json["arch"], "SKL");
        assert_eq!(json["rows"][0]["kind"], "instruction");
        assert_eq!(json["rows"][0]["metrics"]["ports"]["0"], 0.5);
        assert_eq!(json["columns"][0]["kind"], "port");
    }
}
