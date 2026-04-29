use tempfile::tempdir;

#[test]
fn returns_error_for_invalid_uipack() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("invalid.uipack");
    std::fs::write(&path, b"not a uipack").unwrap();

    let result = uica_data::load_pack(&path);

    assert!(result.is_err());
}
