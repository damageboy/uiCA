//! Public wasm-bindgen facade for uiCA analysis without embedding XED.
//!
//! Audience map:
//!
//! - Pure wasm/browser callers use [`analyze_decoded_json_with_uipack`]. They decode
//!   instructions elsewhere, fetch manifest-selected `.uipack` bytes, and pass both
//!   into this crate.
//! - Rust/native smoke tests and transitional callers may use
//!   [`analyze_decoded_json`]. It depends on core default data lookup and can fall
//!   back when data is absent, so it is not the preferred browser API.
//! - Raw-byte browser analysis with XED belongs to sibling `uica-emscripten`, not
//!   this crate. [`analyze_hex`] exists only as a wasm-bindgen-compatible stub for
//!   callers probing this older/raw-byte shape.
//!
//! All exported functions return JSON strings on success and JavaScript exceptions
//! (via `Result<_, String>`) on failure in wasm-bindgen builds.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use uica_core::engine;
use uica_data::MappedUiPackRuntime;
use uica_decode_ir::DecodedInstruction;
use uica_model::Invocation;

/// Analyze pre-decoded instruction IR using core's default data lookup.
///
/// Who uses this: Rust/native smoke tests and legacy/transitional callers that
/// already have `DecodedInstruction` JSON but do not control `.uipack` bytes.
///
/// Pure wasm/browser code should prefer [`analyze_decoded_json_with_uipack`]
/// because that path makes the manifest-selected `.uipack` explicit. This
/// function may fall back to a reduced result when default data is unavailable.
///
/// `decoded_json` must be a JSON array of `uica_decode_ir::DecodedInstruction`.
/// `arch` is the requested microarchitecture, e.g. `"SKL"`. The returned string
/// is serialized `uica_model::UicaResult` JSON.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn analyze_decoded_json(decoded_json: &str, arch: &str) -> Result<String, String> {
    let decoded: Vec<DecodedInstruction> =
        serde_json::from_str(decoded_json).map_err(|err| err.to_string())?;
    let invocation = Invocation {
        arch: arch.to_string(),
        ..Invocation::default()
    };

    serde_json::to_string(&engine::engine_with_decoded(&decoded, &invocation))
        .map_err(|err| err.to_string())
}

/// Analyze pre-decoded instruction IR with caller-supplied `.uipack` bytes.
///
/// Who uses this: pure wasm/browser integrations produced by `wasm-bindgen`,
/// including `web/pure-wasm.js`, where JavaScript fetches/caches architecture
/// packs and supplies decoded IR. This is the preferred public API for non-XED
/// wasm execution.
///
/// `decoded_json` must be a JSON array of `uica_decode_ir::DecodedInstruction`.
/// `arch` must match the architecture encoded in `uipack_bytes`; mismatches are
/// rejected before analysis. The returned string is serialized
/// `uica_model::UicaResult` JSON.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn analyze_decoded_json_with_uipack(
    decoded_json: &str,
    arch: &str,
    uipack_bytes: &[u8],
) -> Result<String, String> {
    let decoded: Vec<DecodedInstruction> =
        serde_json::from_str(decoded_json).map_err(|err| err.to_string())?;
    let invocation = Invocation {
        arch: arch.to_string(),
        ..Invocation::default()
    };
    let runtime = MappedUiPackRuntime::from_bytes_verified(uipack_bytes.to_vec())
        .map_err(|err| err.to_string())?;
    let pack_arch = runtime
        .view()
        .map_err(|err| err.to_string())?
        .arch()
        .to_string();
    if !pack_arch.eq_ignore_ascii_case(arch) {
        return Err(format!(
            "UIPack architecture {pack_arch} does not match requested architecture {}",
            arch.trim().to_ascii_uppercase()
        ));
    }
    let output =
        engine::engine_output_with_decoded_uipack_runtime(&decoded, &invocation, &runtime, false)?;

    serde_json::to_string(&output.result).map_err(|err| err.to_string())
}

/// Validate raw hex input, then report that XED-backed wasm is required.
///
/// Who uses this: compatibility callers probing the old/raw-byte wasm API shape.
/// It is not a pure wasm analysis path. Browser flows that need raw x86 bytes
/// should use the Emscripten/XED build in sibling crate `uica-emscripten`; pure
/// wasm callers should decode elsewhere and use
/// [`analyze_decoded_json_with_uipack`].
///
/// Whitespace is ignored while validating `hex_bytes`. After successful hex
/// validation, this always returns an error explaining that raw-byte analysis
/// requires an XED-enabled wasm build.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn analyze_hex(hex_bytes: &str, _arch: &str) -> Result<String, String> {
    let _code = decode_hex(hex_bytes)?;
    Err("raw x86 byte analysis requires an XED-enabled wasm build; use analyze_decoded_json_with_uipack for pre-decoded IR".to_string())
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
