use napi_derive::napi;

#[napi]
pub fn generate_patch_json(input_json: String) -> napi::Result<String> {
    git_patch_core::generate_patch_from_json(&input_json)
        .map_err(|error| napi::Error::from_reason(error.to_string()))
}

#[napi]
pub fn apply_patch_json(input_json: String) -> napi::Result<String> {
    git_patch_core::apply_patch_from_json(&input_json)
        .map_err(|error| napi::Error::from_reason(error.to_string()))
}

#[napi]
pub fn inspect_patch_json(input_json: String) -> napi::Result<String> {
    git_patch_core::inspect_patch_from_json(&input_json)
        .map_err(|error| napi::Error::from_reason(error.to_string()))
}
