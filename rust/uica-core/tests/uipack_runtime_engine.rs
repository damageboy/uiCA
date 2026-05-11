use uica_core::engine::engine_output_with_uipack_runtime;
use uica_data::load_manifest_runtime;
use uica_model::Invocation;

#[test]
fn mapped_runtime_engine_is_stable_for_skl_add() {
    let code = vec![0x48, 0x01, 0xd8];
    let invocation = Invocation {
        arch: "SKL".to_string(),
        min_cycles: 8,
        min_iterations: 1,
        ..Invocation::default()
    };
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let runtime = load_manifest_runtime(&manifest, "SKL").unwrap();

    let first = engine_output_with_uipack_runtime(&code, &invocation, &runtime, false)
        .unwrap()
        .result;
    let second = engine_output_with_uipack_runtime(&code, &invocation, &runtime, false)
        .unwrap()
        .result;

    assert_eq!(first.summary, second.summary);
    assert_eq!(first.parameters, second.parameters);
    assert_eq!(first.instructions, second.instructions);
    assert_eq!(first.cycles, second.cycles);
}
