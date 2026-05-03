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
    assert_eq!(String::from_utf8(status.stdout).unwrap(), "1\n");

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
    assert_eq!(value["summary"]["throughput_cycles_per_iteration"], 1.0);
    assert_eq!(value["summary"]["iterations_simulated"], 20);
    assert_eq!(value["summary"]["cycles_simulated"], 600);
}
