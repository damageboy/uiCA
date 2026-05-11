use std::ffi::{c_char, CStr, CString};
use std::slice;

fn main() {}

/// Runs uiCA analysis for an Emscripten caller.
///
/// This is the Rust entry point behind JavaScript `Module._uica_run` in
/// `web/main.js`. Emscripten prefixes exported C ABI symbols with `_` on the
/// generated JS `Module`, so the `uica_run` symbol here is called as
/// `_uica_run` from the browser. This wrapper only handles pointer/string
/// ownership and delegates analysis to `uica_emscripten::run_request_json`.
///
/// # Safety
///
/// `request_ptr` must point to a valid NUL-terminated UTF-8/ASCII C string.
/// `uipack_ptr` must point to `uipack_len` readable bytes and `uipack_len` must be nonzero.
/// The returned pointer must be released exactly once with `uica_free_string`.
#[no_mangle]
pub unsafe extern "C" fn uica_run(
    request_ptr: *const c_char,
    uipack_ptr: *const u8,
    uipack_len: usize,
) -> *mut c_char {
    let response = if request_ptr.is_null() {
        r#"{"schema_version":"uica-error-v1","engine":"rust-emscripten-xed","error":"request pointer is null"}"#.to_string()
    } else if uipack_len == 0 {
        r#"{"schema_version":"uica-error-v1","engine":"rust-emscripten-xed","error":"uipack is empty"}"#.to_string()
    } else if uipack_ptr.is_null() {
        r#"{"schema_version":"uica-error-v1","engine":"rust-emscripten-xed","error":"uipack pointer is null"}"#.to_string()
    } else {
        let request = unsafe { CStr::from_ptr(request_ptr) }.to_string_lossy();
        let uipack = unsafe { slice::from_raw_parts(uipack_ptr, uipack_len) };
        uica_emscripten::run_analysis_with_request_json(&request, uipack)
    };

    CString::new(response)
        .unwrap_or_else(|_| {
            CString::new(
                r#"{"schema_version":"uica-error-v1","engine":"rust-emscripten-xed","error":"nul byte in response"}"#,
            )
            .expect("static error string has no nul")
        })
        .into_raw()
}

/// Frees a string returned by `uica_run`.
///
/// # Safety
///
/// `ptr` must be null or a pointer previously returned by `uica_run` that has not been freed.
#[no_mangle]
pub unsafe extern "C" fn uica_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)) };
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};

    use serde_json::Value;

    use super::{uica_free_string, uica_run};

    #[test]
    fn abi_reports_null_request_as_json_error() {
        let ptr = unsafe { uica_run(std::ptr::null(), std::ptr::null(), 0) };
        assert!(!ptr.is_null());
        let response = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { uica_free_string(ptr) };

        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["schema_version"], "uica-error-v1");
        assert_eq!(value["engine"], "rust-emscripten-xed");
        assert_eq!(value["error"], "request pointer is null");
    }

    #[test]
    fn abi_reports_empty_uipack_as_json_error() {
        let request = CString::new(r#"{"hex":"48 01 d8","arch":"SKL"}"#).unwrap();
        let ptr = unsafe { uica_run(request.as_ptr(), std::ptr::null(), 0) };
        assert!(!ptr.is_null());
        let response = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { uica_free_string(ptr) };

        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["schema_version"], "uica-error-v1");
        assert_eq!(value["engine"], "rust-emscripten-xed");
        assert_eq!(value["error"], "uipack is empty");
    }
}
