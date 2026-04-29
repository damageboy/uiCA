use std::collections::BTreeSet;

use serde_json::json;
use uica_model::UicaResult;

#[test]
fn deserializes_with_defaults_and_serializes_expected_contract_shape() {
    let result: UicaResult = serde_json::from_value(json!({
        "engine": "rust",
        "invocation": {
            "arch": "SKL"
        },
        "summary": {
            "throughput_cycles_per_iteration": 1.25
        }
    }))
    .unwrap();

    assert_eq!(result.schema_version, "uica-result-v1");
    assert_eq!(result.engine, "rust");
    assert_eq!(result.engine_version, "uiCA-rust");
    assert_eq!(result.uica_commit, "unknown");

    assert_eq!(result.invocation.arch, "SKL");
    assert_eq!(result.invocation.alignment_offset, 0);
    assert_eq!(result.invocation.init_policy, "diff");
    assert!(!result.invocation.no_micro_fusion);
    assert!(!result.invocation.no_macro_fusion);
    assert!(!result.invocation.simple_front_end);
    assert_eq!(result.invocation.min_iterations, 10);
    assert_eq!(result.invocation.min_cycles, 500);

    assert_eq!(result.summary.throughput_cycles_per_iteration, Some(1.25));
    assert_eq!(result.summary.iterations_simulated, 0);
    assert_eq!(result.summary.cycles_simulated, 0);
    assert_eq!(result.summary.mode, "loop");
    assert!(result.summary.bottlenecks_predicted.is_empty());
    assert_eq!(
        result.summary.limits,
        [
            ("decoder".to_string(), None),
            ("dependencies".to_string(), None),
            ("dsb".to_string(), None),
            ("issue".to_string(), None),
            ("lsd".to_string(), None),
            ("ports".to_string(), None),
            ("predecoder".to_string(), None),
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(result.parameters, json!({}));
    assert!(result.instructions.is_empty());
    assert!(result.cycles.is_empty());

    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "schema_version".to_string(),
            "engine".to_string(),
            "engine_version".to_string(),
            "uica_commit".to_string(),
            "invocation".to_string(),
            "summary".to_string(),
            "parameters".to_string(),
            "instructions".to_string(),
            "cycles".to_string(),
        ])
    );
    assert_eq!(value["schema_version"], "uica-result-v1");
    assert_eq!(value["engine"], "rust");
    assert_eq!(value["engine_version"], "uiCA-rust");
    assert_eq!(value["uica_commit"], "unknown");
    assert_eq!(
        value["invocation"],
        json!({
            "arch": "SKL",
            "alignmentOffset": 0,
            "initPolicy": "diff",
            "noMicroFusion": false,
            "noMacroFusion": false,
            "simpleFrontEnd": false,
            "minIterations": 10,
            "minCycles": 500
        })
    );
    assert_eq!(
        value["summary"],
        json!({
            "throughput_cycles_per_iteration": 1.25,
            "iterations_simulated": 0,
            "cycles_simulated": 0,
            "mode": "loop",
            "bottlenecks_predicted": [],
            "limits": {
                "decoder": null,
                "dependencies": null,
                "dsb": null,
                "issue": null,
                "lsd": null,
                "ports": null,
                "predecoder": null
            }
        })
    );
    assert_eq!(value["parameters"], json!({}));
    assert_eq!(value["instructions"], json!([]));
    assert_eq!(value["cycles"], json!([]));
}
