#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use uica_core::engine;
use uica_model::Invocation;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn analyze_hex(hex_bytes: &str, arch: &str) -> Result<String, String> {
    let code = decode_hex(hex_bytes)?;
    let invocation = Invocation {
        arch: arch.to_string(),
        ..Invocation::default()
    };

    serde_json::to_string(&engine(&code, &invocation)).map_err(|err| err.to_string())
}

fn decode_hex(input: &str) -> Result<Vec<u8>, String> {
    let hex: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();

    if !hex.len().is_multiple_of(2) {
        return Err("hex input must contain even number of digits".to_string());
    }

    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for idx in (0..hex.len()).step_by(2) {
        let hi = decode_nibble(hex[idx], idx)?;
        let lo = decode_nibble(hex[idx + 1], idx + 1)?;
        bytes.push((hi << 4) | lo);
    }

    Ok(bytes)
}

fn decode_nibble(byte: u8, idx: usize) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!(
            "invalid hex digit '{}' at position {}",
            char::from(byte),
            idx
        )),
    }
}
