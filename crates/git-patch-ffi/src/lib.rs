use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

/// Generate a JSON result envelope from a UTF-8 patch request JSON string.
///
/// # Safety
/// `input_json` must be null or point to a valid NUL-terminated C string for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn git_patch_generate_patch_json_result(
    input_json: *const c_char,
) -> *mut c_char {
    let result = read_input(input_json).and_then(|input| {
        git_patch_core::generate_patch_from_json(input).map_err(|error| error.to_string())
    });

    into_c_string(match result {
        Ok(value) => serde_json::json!({ "ok": true, "value": value }).to_string(),
        Err(error) => serde_json::json!({ "ok": false, "error": error }).to_string(),
    })
}

/// Free a string returned by this library.
///
/// # Safety
/// `value` must be null or a pointer returned by this library that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn git_patch_free_string(value: *mut c_char) {
    if value.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(value));
    }
}

fn read_input<'a>(input_json: *const c_char) -> Result<&'a str, String> {
    if input_json.is_null() {
        return Err("input_json must not be null".to_owned());
    }

    unsafe { CStr::from_ptr(input_json) }
        .to_str()
        .map_err(|error| format!("input_json must be valid UTF-8: {error}"))
}

fn into_c_string(value: String) -> *mut c_char {
    let bytes = value
        .into_bytes()
        .into_iter()
        .filter(|byte| *byte != 0)
        .collect();
    unsafe { CString::from_vec_unchecked(bytes) }.into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn git_patch_null() -> *mut c_char {
    ptr::null_mut()
}
