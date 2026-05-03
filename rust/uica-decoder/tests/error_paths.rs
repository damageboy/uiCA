use std::fs;

use tempfile::tempdir;
use uica_decoder::{decode_raw, extract_text_from_object};

#[test]
fn truncated_decode_input_returns_error() {
    let bytes = [0x0f];
    let err = decode_raw(bytes.as_slice()).expect_err("truncated decode should fail");

    assert!(err.to_string().contains("truncated instruction stream"));
}

#[test]
fn extract_text_from_object_missing_file_returns_error() {
    let temp = tempdir().expect("tempdir should exist");
    let path = temp.path().join("missing.o");
    let err = extract_text_from_object(&path).expect_err("missing file should fail");

    assert!(err.to_string().contains("failed to read object file"));
}

#[test]
fn extract_text_from_object_malformed_object_returns_error() {
    let temp = tempdir().expect("tempdir should exist");
    let path = temp.path().join("malformed.o");
    fs::write(&path, b"not an object file").expect("temp object should be writable");

    let err = extract_text_from_object(&path).expect_err("malformed object should fail");

    assert!(err.to_string().contains("failed to parse object file"));
}
