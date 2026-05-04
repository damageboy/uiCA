use std::fs;
use std::process::Command;

use serde_json::Value;
use tempfile::tempdir;

#[test]
fn raw_cli_accepts_run_config_flags_and_writes_v1_json() {
    let temp = tempdir().expect("tempdir should exist");
    let input = temp.path().join("loop.bin");
    let output = temp.path().join("result.json");

    fs::write(&input, [0x90]).expect("input should be writable");

    let status = Command::new(env!("CARGO_BIN_EXE_uica-cli"))
        .arg(&input)
        .arg("--raw")
        .arg("--arch")
        .arg("SKL")
        .arg("--json")
        .arg(&output)
        .arg("--tp-only")
        .arg("--alignment-offset")
        .arg("4")
        .arg("--init-policy")
        .arg("same")
        .arg("--min-iterations")
        .arg("20")
        .arg("--min-cycles")
        .arg("600")
        .arg("--no-micro-fusion")
        .arg("--no-macro-fusion")
        .arg("--simple-front-end")
        .output()
        .expect("cli should run");

    assert!(status.status.success());
    assert_eq!(String::from_utf8(status.stdout).unwrap(), "0.25\n");

    let value: Value =
        serde_json::from_slice(&fs::read(&output).expect("json output should be written"))
            .expect("json output should parse");

    assert_eq!(value["engine"], "rust");
    assert_eq!(value["schema_version"], "uica-result-v1");
    assert_eq!(value["invocation"]["arch"], "SKL");
    assert_eq!(value["invocation"]["alignmentOffset"], 4);
    assert_eq!(value["invocation"]["initPolicy"], "same");
    assert_eq!(value["invocation"]["minIterations"], 20);
    assert_eq!(value["invocation"]["minCycles"], 600);
    assert_eq!(value["invocation"]["noMicroFusion"], true);
    assert_eq!(value["invocation"]["noMacroFusion"], true);
    assert_eq!(value["invocation"]["simpleFrontEnd"], true);
    assert_eq!(value["summary"]["throughput_cycles_per_iteration"], 0.25);
    assert_eq!(value["summary"]["iterations_simulated"], 2400);
    assert_eq!(value["summary"]["cycles_simulated"], 601);
}

#[test]
fn raw_cli_writes_trace_from_non_repo_cwd_without_datapack_env() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let raw = temp.path().join("loop.bin");
    let trace = temp.path().join("trace.html");
    std::fs::write(&raw, [0x48, 0x01, 0xd8]).expect("raw input should be written");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_uica-cli"))
        .current_dir(temp.path())
        .env_remove("UICA_RUST_DATAPACK")
        .arg("loop.bin")
        .arg("--raw")
        .arg("--arch")
        .arg("SKL")
        .arg("--min-cycles")
        .arg("8")
        .arg("--min-iterations")
        .arg("1")
        .arg("--trace")
        .arg(&trace)
        .output()
        .expect("cli should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trace_html = std::fs::read_to_string(&trace).expect("trace report should be written");
    assert!(trace_html.contains("Execution Trace"));
}

#[test]
fn raw_cli_writes_trace_and_graph_html_reports() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let raw = temp.path().join("loop.bin");
    let trace = temp.path().join("trace.html");
    let graph = temp.path().join("graph.html");
    std::fs::write(&raw, [0x48, 0x01, 0xd8]).expect("raw input should be written");

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_uica-cli"))
        .env("UICA_RUST_DATAPACK", manifest)
        .arg(&raw)
        .arg("--raw")
        .arg("--arch")
        .arg("SKL")
        .arg("--min-cycles")
        .arg("8")
        .arg("--min-iterations")
        .arg("1")
        .arg("--trace")
        .arg(&trace)
        .arg("--graph")
        .arg(&graph)
        .output()
        .expect("cli should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trace_html = std::fs::read_to_string(&trace).expect("trace report should be written");
    let graph_html = std::fs::read_to_string(&graph).expect("graph report should be written");
    assert!(trace_html.contains("var tableData = ["));
    assert!(trace_html.contains("Execution Trace"));
    assert!(graph_html.contains("Plotly.newPlot"));
    assert!(graph_html.contains("Toggle interpolation mode"));
}
