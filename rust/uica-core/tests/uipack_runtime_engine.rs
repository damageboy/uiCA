use uica_core::engine::{engine_output_with_pack, engine_output_with_uipack_runtime};
use uica_data::{load_manifest_pack, load_manifest_runtime};
use uica_model::Invocation;

#[test]
fn mapped_runtime_engine_matches_owned_pack_for_skl_add() {
    let code = vec![0x48, 0x01, 0xd8];
    let invocation = Invocation {
        arch: "SKL".to_string(),
        min_cycles: 8,
        min_iterations: 1,
        ..Invocation::default()
    };
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../uica-data/generated/manifest.json");
    let pack = load_manifest_pack(&manifest, "SKL").unwrap();
    let runtime = load_manifest_runtime(&manifest, "SKL").unwrap();

    let owned = engine_output_with_pack(&code, &invocation, &pack, false)
        .unwrap()
        .result;
    let mapped = engine_output_with_uipack_runtime(&code, &invocation, &runtime, false)
        .unwrap()
        .result;

    assert_eq!(mapped.summary, owned.summary);
    assert_eq!(mapped.parameters, owned.parameters);
    assert_eq!(mapped.instructions, owned.instructions);
    assert_eq!(mapped.cycles, owned.cycles);
}
