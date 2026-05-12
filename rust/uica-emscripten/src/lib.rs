use serde::{Deserialize, Serialize};
use uica_core::{simulate, SimulationInput, SimulationOptions, SimulationRequest, UipackSource};
use uica_data::MappedUiPackRuntime;
use uica_model::Invocation;

#[derive(Debug, Deserialize)]
#[serde(default)]
#[derive(Default)]
struct RunRequest {
    hex: String,
    arch: String,
    invocation: Invocation,
}

#[derive(Serialize)]
struct RunError {
    schema_version: &'static str,
    engine: &'static str,
    error: String,
}

#[derive(Serialize)]
struct WebRunResult {
    schema_version: &'static str,
    engine: &'static str,
    result: uica_model::UicaResult,
    trace_html: String,
    regular_text: String,
    regular_html: String,
}

/// Handles the JSON request passed through `uica_run` / `Module._uica_run`.
///
/// Caller chain:
/// `web/main.js::callRun` -> Emscripten `Module._uica_run` ->
/// `rust/uica-emscripten/src/main.rs::uica_run` -> this function.
///
/// This layer owns uiCA-specific request parsing, UIPack validation, XED-backed
/// decode/analysis, and web response JSON generation. ABI pointer ownership stays
/// in `main.rs`.
pub fn run_analysis_with_request_json(request_json: &str, uipack_bytes: &[u8]) -> String {
    match run_request_json_inner(request_json, uipack_bytes) {
        Ok(json) => json,
        Err(error) => error_json(error),
    }
}

fn run_request_json_inner(request_json: &str, uipack_bytes: &[u8]) -> Result<String, String> {
    let request: RunRequest = serde_json::from_str(request_json).map_err(|err| err.to_string())?;
    let code = decode_hex(&request.hex)?;
    let mut invocation = request.invocation;
    let arch = if request.arch.trim().is_empty() {
        invocation.arch.trim().to_ascii_uppercase()
    } else {
        request.arch.trim().to_ascii_uppercase()
    };
    invocation.arch = arch;

    let uipack = MappedUiPackRuntime::from_bytes_verified(uipack_bytes.to_vec())
        .map_err(|err| err.to_string())?;
    let pack_arch = uipack
        .view()
        .map_err(|err| err.to_string())?
        .arch()
        .to_string();
    if !pack_arch.eq_ignore_ascii_case(&invocation.arch) {
        return Err(format!(
            "UIPack architecture {pack_arch} does not match requested architecture {}",
            invocation.arch
        ));
    }

    let output = simulate(SimulationRequest {
        input: SimulationInput::Bytes(&code),
        invocation: &invocation,
        uipack: UipackSource::Runtime(&uipack),
        options: SimulationOptions {
            include_reports: true,
            ..SimulationOptions::default()
        },
    })?;
    let reports = output
        .reports
        .as_ref()
        .ok_or_else(|| "report engine did not produce trace data".to_string())?;
    let trace_html = uica_core::report::render_trace_html(&reports.trace)?;
    let regular_text = uica_core::report::render_regular_text(&reports.regular);
    let regular_html = uica_core::report::render_regular_html(&reports.regular)?;
    let response = WebRunResult {
        schema_version: "uica-web-result-v1",
        engine: "rust-emscripten-xed",
        result: output.result,
        trace_html,
        regular_text,
        regular_html,
    };
    serde_json::to_string(&response).map_err(|err| err.to_string())
}

fn error_json(error: String) -> String {
    serde_json::to_string(&RunError {
        schema_version: "uica-error-v1",
        engine: "rust-emscripten-xed",
        error,
    })
    .unwrap_or_else(|_| {
        "{\"schema_version\":\"uica-error-v1\",\"engine\":\"rust-emscripten-xed\",\"error\":\"serialization failed\"}".to_string()
    })
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
