use diffy::patch_set::{FileMode as DiffyFileMode, FileOperation, ParseOptions, PatchSet};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PatchError {
    #[error("invalid patch request JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("invalid change for {path}: {message}")]
    InvalidChange { path: String, message: String },
    #[error("invalid patch option {name}: {message}")]
    InvalidOption { name: String, message: String },
    #[error("invalid apply patch request JSON: {0}")]
    InvalidApplyJson(#[source] serde_json::Error),
    #[error("duplicate normalized path {normalized:?} from {first:?} and {second:?}")]
    DuplicatePath {
        normalized: String,
        first: String,
        second: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchRequest {
    pub changes: BTreeMap<String, FileChange>,
    #[serde(default)]
    pub options: PatchOptions,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub moved: Option<Moved>,
    #[serde(default)]
    pub mode: Option<ModeInput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum ModeInput {
    Shorthand(String),
    Change {
        before: Option<String>,
        after: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileMode {
    Regular,
    Executable,
}

impl FileMode {
    fn parse(path: &str, value: &str) -> Result<Self, PatchError> {
        match value {
            "100644" => Ok(Self::Regular),
            "100755" => Ok(Self::Executable),
            _ => invalid(path, "mode must be 100644 or 100755"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "100644",
            Self::Executable => "100755",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ModeTransition {
    before: Option<FileMode>,
    after: Option<FileMode>,
}

impl ModeTransition {
    fn changed(self) -> Option<(FileMode, FileMode)> {
        match (self.before, self.after) {
            (Some(before), Some(after)) if before != after => Some((before, after)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Moved {
    From(String),
    Detail {
        from: String,
        similarity: Option<i64>,
    },
}

impl Moved {
    fn source_path(&self) -> &str {
        match self {
            Self::From(path) => path,
            Self::Detail { from, .. } => from,
        }
    }

    fn similarity(&self) -> Option<i64> {
        match self {
            Self::From(_) => None,
            Self::Detail { similarity, .. } => *similarity,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchOptions {
    #[serde(default = "default_context_lines")]
    pub context_lines: usize,
    #[serde(default)]
    pub rename_similarity_threshold: Option<i64>,
}

impl Default for PatchOptions {
    fn default() -> Self {
        Self {
            context_lines: default_context_lines(),
            rename_similarity_threshold: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPatchRequest {
    pub files: BTreeMap<String, FileSnapshotInput>,
    pub patch: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPatchRequest {
    pub patch: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FileSnapshotInput {
    Content(String),
    Entry(FileSnapshotEntry),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSnapshotEntry {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "_tag")]
pub enum ApplyPatchResult {
    Applied {
        files: BTreeMap<String, FileSnapshotEntry>,
        changes: Vec<AppliedChange>,
    },
    Rejected {
        files: BTreeMap<String, FileSnapshotEntry>,
        rejects: Vec<PatchReject>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "_tag")]
pub enum AppliedChange {
    Added {
        path: String,
        after: FileSnapshotEntry,
    },
    Modified {
        path: String,
        before: FileSnapshotEntry,
        after: FileSnapshotEntry,
    },
    Deleted {
        path: String,
        before: FileSnapshotEntry,
    },
    Renamed {
        from: String,
        to: String,
        before: FileSnapshotEntry,
        after: FileSnapshotEntry,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "_tag")]
pub struct PatchSummary {
    pub files: Vec<PatchFileSummary>,
    pub rejects: Vec<PatchReject>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "_tag")]
pub enum PatchFileSummary {
    Added { path: String },
    Modified { path: String },
    Deleted { path: String },
    Renamed { from: String, to: String },
    Copied { from: String, to: String },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "_tag")]
pub enum PatchReject {
    MissingFile {
        path: String,
        operation: String,
        patch: String,
        message: String,
    },
    AlreadyExists {
        path: String,
        operation: String,
        patch: String,
        message: String,
    },
    ContentMismatch {
        path: String,
        operation: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        hunk: Option<usize>,
        patch: String,
        message: String,
    },
    Unsupported {
        operation: String,
        patch: String,
        message: String,
    },
}

fn default_context_lines() -> usize {
    3
}

pub fn generate_patch_from_json(input: &str) -> Result<String, PatchError> {
    let request: PatchRequest = serde_json::from_str(input)?;
    generate_patch(&request)
}

pub fn apply_patch_from_json(input: &str) -> Result<String, PatchError> {
    let request: ApplyPatchRequest =
        serde_json::from_str(input).map_err(PatchError::InvalidApplyJson)?;
    serde_json::to_string(&apply_patch(&request)?).map_err(PatchError::InvalidJson)
}

pub fn inspect_patch_from_json(input: &str) -> Result<String, PatchError> {
    let request: InspectPatchRequest =
        serde_json::from_str(input).map_err(PatchError::InvalidApplyJson)?;
    serde_json::to_string(&inspect_patch(&request)).map_err(PatchError::InvalidJson)
}

pub fn generate_patch(request: &PatchRequest) -> Result<String, PatchError> {
    validate_options(&request.options)?;

    let mut normalized_changes = BTreeMap::<String, NormalizedChange>::new();

    for (path, change) in &request.changes {
        validate_change(path, change)?;
        let normalized = normalize_and_validate_path(path, path)?;
        if let Some(first) = normalized_changes.insert(
            normalized.clone(),
            NormalizedChange {
                original_path: path.clone(),
                change: change.clone(),
            },
        ) {
            return Err(PatchError::DuplicatePath {
                normalized,
                first: first.original_path,
                second: path.to_owned(),
            });
        }
    }

    if let Some(threshold) = request.options.rename_similarity_threshold {
        detect_renames(&mut normalized_changes, threshold as u8)?;
    }

    let mut out = String::new();
    for (normalized_path, normalized) in normalized_changes {
        emit_file_patch(
            &normalized_path,
            &normalized.change,
            &request.options,
            &mut out,
        )?;
    }
    Ok(out)
}

pub fn inspect_patch(request: &InspectPatchRequest) -> PatchSummary {
    let mut files = Vec::new();
    let mut rejects = Vec::new();

    if request.patch.contains('\0') {
        rejects.push(PatchReject::Unsupported {
            operation: "Parse".to_owned(),
            patch: request.patch.clone(),
            message: "NUL bytes are not supported; this API parses text patches only".to_owned(),
        });
        return PatchSummary { files, rejects };
    }

    for file_patch in PatchSet::parse(&request.patch, ParseOptions::gitdiff()) {
        let file_patch = match file_patch {
            Ok(file_patch) => file_patch,
            Err(error) => {
                rejects.push(PatchReject::Unsupported {
                    operation: "Parse".to_owned(),
                    patch: request.patch.clone(),
                    message: error.to_string(),
                });
                continue;
            }
        };

        let operation = file_patch.operation().strip_prefix(1);
        match &operation {
            FileOperation::Create(path) => {
                let patch =
                    operation_patchlet(&operation, file_patch.old_mode(), file_patch.new_mode());
                if let Some(path) =
                    normalize_patch_path_or_reject(path.as_ref(), "Create", &patch, &mut rejects)
                {
                    files.push(PatchFileSummary::Added { path });
                }
            }
            FileOperation::Delete(path) => {
                let patch =
                    operation_patchlet(&operation, file_patch.old_mode(), file_patch.new_mode());
                if let Some(path) =
                    normalize_patch_path_or_reject(path.as_ref(), "Delete", &patch, &mut rejects)
                {
                    files.push(PatchFileSummary::Deleted { path });
                }
            }
            FileOperation::Modify { original, modified } => {
                let patch =
                    operation_patchlet(&operation, file_patch.old_mode(), file_patch.new_mode());
                let Some(original) = normalize_patch_path_or_reject(
                    original.as_ref(),
                    "Modify",
                    &patch,
                    &mut rejects,
                ) else {
                    continue;
                };
                let Some(modified) = normalize_patch_path_or_reject(
                    modified.as_ref(),
                    "Modify",
                    &patch,
                    &mut rejects,
                ) else {
                    continue;
                };
                if original == modified {
                    files.push(PatchFileSummary::Modified { path: original });
                } else {
                    files.push(PatchFileSummary::Renamed {
                        from: original,
                        to: modified,
                    });
                }
            }
            FileOperation::Rename { from, to } => {
                let patch =
                    operation_patchlet(&operation, file_patch.old_mode(), file_patch.new_mode());
                let Some(from) =
                    normalize_patch_path_or_reject(from.as_ref(), "Rename", &patch, &mut rejects)
                else {
                    continue;
                };
                let Some(to) =
                    normalize_patch_path_or_reject(to.as_ref(), "Rename", &patch, &mut rejects)
                else {
                    continue;
                };
                files.push(PatchFileSummary::Renamed { from, to });
            }
            FileOperation::Copy { from, to } => {
                let patch =
                    operation_patchlet(&operation, file_patch.old_mode(), file_patch.new_mode());
                if normalize_patch_path_or_reject(from.as_ref(), "Copy", &patch, &mut rejects)
                    .is_some()
                    && normalize_patch_path_or_reject(to.as_ref(), "Copy", &patch, &mut rejects)
                        .is_some()
                {
                    rejects.push(PatchReject::Unsupported {
                        operation: "Copy".to_owned(),
                        patch,
                        message: "copy patches are not supported".to_owned(),
                    });
                }
            }
        }
    }

    PatchSummary { files, rejects }
}

pub fn apply_patch(request: &ApplyPatchRequest) -> Result<ApplyPatchResult, PatchError> {
    let files = normalize_snapshot(&request.files)?;
    let mut next_files = files.clone();
    let mut changes = Vec::new();
    let mut rejects = Vec::new();

    if request.patch.contains('\0') {
        return Ok(ApplyPatchResult::Rejected {
            files,
            rejects: vec![PatchReject::Unsupported {
                operation: "Parse".to_owned(),
                patch: request.patch.clone(),
                message: "NUL bytes are not supported; this API applies text patches only"
                    .to_owned(),
            }],
        });
    }

    let parsed = PatchSet::parse(&request.patch, ParseOptions::gitdiff());
    for file_patch in parsed {
        let file_patch = match file_patch {
            Ok(file_patch) => file_patch,
            Err(error) => {
                rejects.push(PatchReject::Unsupported {
                    operation: "Parse".to_owned(),
                    patch: request.patch.clone(),
                    message: error.to_string(),
                });
                continue;
            }
        };

        if file_patch.patch().is_binary() {
            rejects.push(PatchReject::Unsupported {
                operation: operation_name(file_patch.operation()),
                patch: request.patch.clone(),
                message: "binary patches are not supported".to_owned(),
            });
            continue;
        }

        let operation = file_patch.operation().strip_prefix(1);
        let Some(text_patch) = file_patch.patch().as_text() else {
            rejects.push(PatchReject::Unsupported {
                operation: operation_name(&operation),
                patch: operation_patchlet(&operation, file_patch.old_mode(), file_patch.new_mode()),
                message: "patch entry does not contain a text patch".to_owned(),
            });
            continue;
        };
        let patch_text = text_patchlet(
            &operation,
            text_patch,
            file_patch.old_mode(),
            file_patch.new_mode(),
        );
        match operation {
            FileOperation::Create(path) => {
                let Some(path) = normalize_patch_path_or_reject(
                    path.as_ref(),
                    "Create",
                    &patch_text,
                    &mut rejects,
                ) else {
                    continue;
                };
                if next_files.contains_key(&path) {
                    rejects.push(PatchReject::AlreadyExists {
                        path,
                        operation: "Create".to_owned(),
                        patch: patch_text,
                        message: "target file already exists".to_owned(),
                    });
                    continue;
                }
                match diffy::apply("", text_patch) {
                    Ok(content) => {
                        let after = FileSnapshotEntry {
                            content,
                            mode: mode_to_string(file_patch.new_mode())
                                .or(Some("100644".to_owned())),
                        };
                        next_files.insert(path.clone(), after.clone());
                        changes.push(AppliedChange::Added { path, after });
                    }
                    Err(error) => rejects.push(content_mismatch(
                        path,
                        "Create",
                        None,
                        patch_text,
                        error.to_string(),
                    )),
                }
            }
            FileOperation::Delete(path) => {
                let Some(path) = normalize_patch_path_or_reject(
                    path.as_ref(),
                    "Delete",
                    &patch_text,
                    &mut rejects,
                ) else {
                    continue;
                };
                let Some(before) = next_files.get(&path).cloned() else {
                    rejects.push(PatchReject::MissingFile {
                        path,
                        operation: "Delete".to_owned(),
                        patch: patch_text,
                        message: "source file is missing".to_owned(),
                    });
                    continue;
                };
                match diffy::apply(&before.content, text_patch) {
                    Ok(content) if content.is_empty() => {
                        next_files.remove(&path);
                        changes.push(AppliedChange::Deleted { path, before });
                    }
                    Ok(_) => rejects.push(content_mismatch(
                        path,
                        "Delete",
                        None,
                        patch_text,
                        "delete patch did not produce empty content".to_owned(),
                    )),
                    Err(error) => rejects.push(content_mismatch(
                        path,
                        "Delete",
                        None,
                        patch_text,
                        error.to_string(),
                    )),
                }
            }
            FileOperation::Modify { original, modified } => {
                let Some(original) = normalize_patch_path_or_reject(
                    original.as_ref(),
                    "Modify",
                    &patch_text,
                    &mut rejects,
                ) else {
                    continue;
                };
                let Some(modified) = normalize_patch_path_or_reject(
                    modified.as_ref(),
                    "Modify",
                    &patch_text,
                    &mut rejects,
                ) else {
                    continue;
                };
                apply_modify_or_rename(
                    &mut next_files,
                    &mut changes,
                    &mut rejects,
                    ApplyFilePatch {
                        original,
                        modified,
                        text_patch,
                        patch_text: &patch_text,
                        new_mode: mode_to_string(file_patch.new_mode()),
                        operation: "Modify",
                    },
                );
            }
            FileOperation::Rename { from, to } => {
                let Some(from) = normalize_patch_path_or_reject(
                    from.as_ref(),
                    "Rename",
                    &patch_text,
                    &mut rejects,
                ) else {
                    continue;
                };
                let Some(to) = normalize_patch_path_or_reject(
                    to.as_ref(),
                    "Rename",
                    &patch_text,
                    &mut rejects,
                ) else {
                    continue;
                };
                apply_modify_or_rename(
                    &mut next_files,
                    &mut changes,
                    &mut rejects,
                    ApplyFilePatch {
                        original: from,
                        modified: to,
                        text_patch,
                        patch_text: &patch_text,
                        new_mode: mode_to_string(file_patch.new_mode()),
                        operation: "Rename",
                    },
                );
            }
            FileOperation::Copy { .. } => rejects.push(PatchReject::Unsupported {
                operation: "Copy".to_owned(),
                patch: patch_text,
                message: "copy patches are not supported".to_owned(),
            }),
        }
    }

    if rejects.is_empty() {
        Ok(ApplyPatchResult::Applied {
            files: next_files,
            changes,
        })
    } else {
        Ok(ApplyPatchResult::Rejected { files, rejects })
    }
}

struct ApplyFilePatch<'a> {
    original: String,
    modified: String,
    text_patch: &'a diffy::Patch<'a, str>,
    patch_text: &'a str,
    new_mode: Option<String>,
    operation: &'static str,
}

fn apply_modify_or_rename(
    files: &mut BTreeMap<String, FileSnapshotEntry>,
    changes: &mut Vec<AppliedChange>,
    rejects: &mut Vec<PatchReject>,
    patch: ApplyFilePatch<'_>,
) {
    let Some(before) = files.get(&patch.original).cloned() else {
        rejects.push(PatchReject::MissingFile {
            path: patch.original,
            operation: patch.operation.to_owned(),
            patch: patch.patch_text.to_owned(),
            message: "source file is missing".to_owned(),
        });
        return;
    };

    if patch.original != patch.modified && files.contains_key(&patch.modified) {
        rejects.push(PatchReject::AlreadyExists {
            path: patch.modified,
            operation: patch.operation.to_owned(),
            patch: patch.patch_text.to_owned(),
            message: "target file already exists".to_owned(),
        });
        return;
    }

    match diffy::apply(&before.content, patch.text_patch) {
        Ok(content) => {
            let after = FileSnapshotEntry {
                content,
                mode: patch.new_mode.or_else(|| before.mode.clone()),
            };
            if patch.original == patch.modified {
                files.insert(patch.modified.clone(), after.clone());
                changes.push(AppliedChange::Modified {
                    path: patch.modified,
                    before,
                    after,
                });
            } else {
                files.remove(&patch.original);
                files.insert(patch.modified.clone(), after.clone());
                changes.push(AppliedChange::Renamed {
                    from: patch.original,
                    to: patch.modified,
                    before,
                    after,
                });
            }
        }
        Err(error) => rejects.push(content_mismatch(
            patch.original,
            patch.operation,
            parse_hunk_number(&error.to_string()),
            patch.patch_text.to_owned(),
            error.to_string(),
        )),
    }
}

fn normalize_snapshot(
    files: &BTreeMap<String, FileSnapshotInput>,
) -> Result<BTreeMap<String, FileSnapshotEntry>, PatchError> {
    let mut normalized = BTreeMap::new();
    let mut original_paths = BTreeMap::<String, String>::new();

    for (path, file) in files {
        let original_path = path.clone();
        let path = normalize_and_validate_path(path, path)?;
        let entry = match file {
            FileSnapshotInput::Content(content) => FileSnapshotEntry {
                content: content.clone(),
                mode: None,
            },
            FileSnapshotInput::Entry(entry) => entry.clone(),
        };
        if entry.content.contains('\0') {
            return invalid(
                &path,
                "NUL bytes are not supported; this API applies text patches only",
            );
        }
        if let Some(mode) = entry.mode.as_deref() {
            FileMode::parse(&path, mode)?;
        }
        if let Some(first) = original_paths.insert(path.clone(), original_path.clone()) {
            return Err(PatchError::DuplicatePath {
                normalized: path,
                first,
                second: original_path,
            });
        }
        normalized.insert(path, entry);
    }

    Ok(normalized)
}

fn normalize_patch_path_or_reject(
    path: &str,
    operation: &str,
    patch: &str,
    rejects: &mut Vec<PatchReject>,
) -> Option<String> {
    match normalize_and_validate_path("patch", path) {
        Ok(path) => Some(path),
        Err(error) => {
            rejects.push(PatchReject::Unsupported {
                operation: operation.to_owned(),
                patch: patch.to_owned(),
                message: error.to_string(),
            });
            None
        }
    }
}

fn text_patchlet(
    operation: &FileOperation<'_, str>,
    text_patch: &diffy::Patch<'_, str>,
    old_mode: Option<&DiffyFileMode>,
    new_mode: Option<&DiffyFileMode>,
) -> String {
    let mut out = operation_patchlet(operation, old_mode, new_mode);
    out.push_str(&text_patch.to_string());
    out
}

fn operation_patchlet(
    operation: &FileOperation<'_, str>,
    old_mode: Option<&DiffyFileMode>,
    new_mode: Option<&DiffyFileMode>,
) -> String {
    let (old_path, new_path) = match operation {
        FileOperation::Create(path) => (path.as_ref(), path.as_ref()),
        FileOperation::Delete(path) => (path.as_ref(), path.as_ref()),
        FileOperation::Modify { original, modified } => (original.as_ref(), modified.as_ref()),
        FileOperation::Rename { from, to } | FileOperation::Copy { from, to } => {
            (from.as_ref(), to.as_ref())
        }
    };

    let mut out = String::new();
    out.push_str("diff --git ");
    out.push_str(&patch_path(Some("a"), old_path));
    out.push(' ');
    out.push_str(&patch_path(Some("b"), new_path));
    out.push('\n');

    match operation {
        FileOperation::Create(_) => {
            out.push_str("new file mode ");
            out.push_str(mode_to_string(new_mode).as_deref().unwrap_or("100644"));
            out.push('\n');
        }
        FileOperation::Delete(_) => {
            out.push_str("deleted file mode ");
            out.push_str(mode_to_string(old_mode).as_deref().unwrap_or("100644"));
            out.push('\n');
        }
        FileOperation::Modify { .. } => {
            if let (Some(old), Some(new)) = (mode_to_string(old_mode), mode_to_string(new_mode))
                && old != new
            {
                out.push_str("old mode ");
                out.push_str(&old);
                out.push_str("\nnew mode ");
                out.push_str(&new);
                out.push('\n');
            }
        }
        FileOperation::Rename { from, to } => {
            out.push_str("similarity index 100%\nrename from ");
            out.push_str(&patch_path(None, from.as_ref()));
            out.push_str("\nrename to ");
            out.push_str(&patch_path(None, to.as_ref()));
            out.push('\n');
        }
        FileOperation::Copy { from, to } => {
            out.push_str("similarity index 100%\ncopy from ");
            out.push_str(&patch_path(None, from.as_ref()));
            out.push_str("\ncopy to ");
            out.push_str(&patch_path(None, to.as_ref()));
            out.push('\n');
        }
    }

    out
}

fn content_mismatch(
    path: String,
    operation: &str,
    hunk: Option<usize>,
    patch: String,
    message: String,
) -> PatchReject {
    PatchReject::ContentMismatch {
        path,
        operation: operation.to_owned(),
        hunk,
        patch,
        message,
    }
}

fn parse_hunk_number(message: &str) -> Option<usize> {
    message
        .strip_prefix("error applying hunk #")
        .and_then(|value| value.parse().ok())
}

fn mode_to_string(mode: Option<&DiffyFileMode>) -> Option<String> {
    match mode {
        Some(DiffyFileMode::Regular) => Some("100644".to_owned()),
        Some(DiffyFileMode::Executable) => Some("100755".to_owned()),
        _ => None,
    }
}

fn operation_name(operation: &FileOperation<'_, str>) -> String {
    match operation {
        FileOperation::Delete(_) => "Delete",
        FileOperation::Create(_) => "Create",
        FileOperation::Modify { .. } => "Modify",
        FileOperation::Rename { .. } => "Rename",
        FileOperation::Copy { .. } => "Copy",
    }
    .to_owned()
}

fn validate_options(options: &PatchOptions) -> Result<(), PatchError> {
    if options.context_lines == 0 {
        return Err(PatchError::InvalidOption {
            name: "contextLines".to_owned(),
            message: "must be at least 1 so patches apply with default git apply".to_owned(),
        });
    }
    if options
        .rename_similarity_threshold
        .is_some_and(|threshold| !(0..=100).contains(&threshold))
    {
        return Err(PatchError::InvalidOption {
            name: "renameSimilarityThreshold".to_owned(),
            message: "must be between 0 and 100".to_owned(),
        });
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct NormalizedChange {
    original_path: String,
    change: FileChange,
}

#[derive(Clone, Debug)]
struct RenameSide {
    path: String,
    content: String,
    mode: FileMode,
}

#[derive(Clone, Debug)]
struct RenameCandidate {
    delete_index: usize,
    add_index: usize,
    delete_path: String,
    add_path: String,
    similarity: u8,
    basename_match: bool,
    size_delta: usize,
}

#[derive(Clone, Debug)]
struct SelectedRename {
    delete_path: String,
    add_path: String,
    similarity: u8,
}

fn validate_change(path: &str, change: &FileChange) -> Result<(), PatchError> {
    let before = change.before.as_deref();
    let after = change.after.as_deref();

    if before.is_none() && after.is_none() {
        return invalid(path, "at least one of before or after is required");
    }

    if before.is_some_and(|content| content.contains('\0'))
        || after.is_some_and(|content| content.contains('\0'))
    {
        return invalid(
            path,
            "NUL bytes are not supported; this API emits text patches only",
        );
    }

    resolve_mode_transition(path, change)?;

    if let Some(moved) = change.moved.as_ref() {
        normalize_and_validate_path(path, moved.source_path())?;
        if moved
            .similarity()
            .is_some_and(|similarity| !(0..=100).contains(&similarity))
        {
            return invalid(path, "similarity must be between 0 and 100");
        }
        if before.is_none() || after.is_none() {
            return invalid(path, "moved requires both before and after content");
        }
    }

    Ok(())
}

fn resolve_mode_transition(path: &str, change: &FileChange) -> Result<ModeTransition, PatchError> {
    let is_add = change.before.is_none() && change.after.is_some();
    let is_delete = change.before.is_some() && change.after.is_none();
    let is_existing = change.before.is_some() && change.after.is_some();

    match change.mode.as_ref() {
        None => Ok(ModeTransition::default()),
        Some(ModeInput::Shorthand(mode)) => {
            let mode = FileMode::parse(path, mode)?;
            if is_add {
                Ok(ModeTransition {
                    before: None,
                    after: Some(mode),
                })
            } else if is_delete {
                Ok(ModeTransition {
                    before: Some(mode),
                    after: None,
                })
            } else {
                invalid(
                    path,
                    "scalar mode is only supported for additions and deletions; use mode.before and mode.after for mode changes",
                )
            }
        }
        Some(ModeInput::Change { before, after }) => {
            let before_mode = parse_optional_mode(path, before.as_deref())?;
            let after_mode = parse_optional_mode(path, after.as_deref())?;

            if is_add {
                if before_mode.is_some() {
                    return invalid(path, "mode.before is not valid for file additions");
                }
                if after_mode.is_none() {
                    return invalid(
                        path,
                        "mode.after is required when mode is provided for additions",
                    );
                }
                Ok(ModeTransition {
                    before: None,
                    after: after_mode,
                })
            } else if is_delete {
                if after_mode.is_some() {
                    return invalid(path, "mode.after is not valid for file deletions");
                }
                if before_mode.is_none() {
                    return invalid(
                        path,
                        "mode.before is required when mode is provided for deletions",
                    );
                }
                Ok(ModeTransition {
                    before: before_mode,
                    after: None,
                })
            } else if is_existing {
                match (before_mode, after_mode) {
                    (Some(before), Some(after)) => Ok(ModeTransition {
                        before: Some(before),
                        after: Some(after),
                    }),
                    _ => invalid(
                        path,
                        "mode changes for existing files require both mode.before and mode.after",
                    ),
                }
            } else {
                invalid(path, "at least one of before or after is required")
            }
        }
    }
}

fn parse_optional_mode(path: &str, mode: Option<&str>) -> Result<Option<FileMode>, PatchError> {
    mode.map(|mode| FileMode::parse(path, mode)).transpose()
}

fn invalid<T>(path: &str, message: &str) -> Result<T, PatchError> {
    Err(PatchError::InvalidChange {
        path: path.to_owned(),
        message: message.to_owned(),
    })
}

fn detect_renames(
    changes: &mut BTreeMap<String, NormalizedChange>,
    threshold: u8,
) -> Result<(), PatchError> {
    let mut deletes = Vec::new();
    let mut adds = Vec::new();

    for (path, normalized) in changes.iter() {
        let change = &normalized.change;
        if change.moved.is_some() {
            continue;
        }
        let mode = resolve_mode_transition(path, change)?;
        match (change.before.as_ref(), change.after.as_ref()) {
            (Some(before), None) => deletes.push(RenameSide {
                path: path.clone(),
                content: before.clone(),
                mode: mode.before.unwrap_or(FileMode::Regular),
            }),
            (None, Some(after)) => adds.push(RenameSide {
                path: path.clone(),
                content: after.clone(),
                mode: mode.after.unwrap_or(FileMode::Regular),
            }),
            _ => {}
        }
    }

    if deletes.is_empty() || adds.is_empty() {
        return Ok(());
    }

    let selected = select_rename_matches(&deletes, &adds, threshold);

    let delete_modes: BTreeMap<String, FileMode> = deletes
        .into_iter()
        .map(|side| (side.path, side.mode))
        .collect();
    let add_modes: BTreeMap<String, FileMode> = adds
        .into_iter()
        .map(|side| (side.path, side.mode))
        .collect();

    for rename in selected {
        let Some(delete) = changes.remove(&rename.delete_path) else {
            continue;
        };
        let Some(mut add) = changes.remove(&rename.add_path) else {
            changes.insert(rename.delete_path.clone(), delete);
            continue;
        };

        let before_mode = delete_modes
            .get(&rename.delete_path)
            .copied()
            .unwrap_or(FileMode::Regular);
        let after_mode = add_modes
            .get(&rename.add_path)
            .copied()
            .unwrap_or(FileMode::Regular);

        add.change = FileChange {
            before: delete.change.before,
            after: add.change.after,
            moved: Some(Moved::Detail {
                from: rename.delete_path.clone(),
                similarity: Some(i64::from(rename.similarity)),
            }),
            mode: if before_mode == after_mode {
                None
            } else {
                Some(ModeInput::Change {
                    before: Some(before_mode.as_str().to_owned()),
                    after: Some(after_mode.as_str().to_owned()),
                })
            },
        };

        changes.insert(rename.add_path, add);
    }

    Ok(())
}

fn select_rename_matches(
    deletes: &[RenameSide],
    adds: &[RenameSide],
    threshold: u8,
) -> Vec<SelectedRename> {
    let mut matcher = RenameMatcher::new(deletes.len(), adds.len());

    for (delete_index, delete) in deletes.iter().enumerate() {
        for (add_index, add) in adds.iter().enumerate() {
            let similarity = text_similarity_percent(&delete.content, &add.content);
            if similarity >= threshold {
                matcher.add_candidate(RenameCandidate {
                    delete_index,
                    add_index,
                    delete_path: delete.path.clone(),
                    add_path: add.path.clone(),
                    similarity,
                    basename_match: basename(&delete.path) == basename(&add.path),
                    size_delta: delete.content.len().abs_diff(add.content.len()),
                });
            }
        }
    }

    matcher.solve()
}

// Computes a maximum-weight one-to-one matching over eligible delete/add pairs.
// Every real edge carries a large cardinality bonus, so the primary objective is
// maximum rename count; similarity, basename, and size delta only break ties.
struct RenameMatcher {
    graph: Vec<Vec<FlowEdge>>,
    candidates: Vec<RenameCandidate>,
    candidate_edges: Vec<(usize, usize)>,
    add_offset: usize,
    sink: usize,
}

#[derive(Clone, Debug)]
struct FlowEdge {
    to: usize,
    rev: usize,
    cap: i32,
    cost: i64,
    candidate_index: Option<usize>,
}

impl RenameMatcher {
    fn new(delete_count: usize, add_count: usize) -> Self {
        let add_offset = 1 + delete_count;
        let sink = add_offset + add_count;
        let mut matcher = Self {
            graph: vec![Vec::new(); sink + 1],
            candidates: Vec::new(),
            candidate_edges: Vec::new(),
            add_offset,
            sink,
        };

        for delete_index in 0..delete_count {
            matcher.add_edge(0, 1 + delete_index, 1, 0, None);
        }
        for add_index in 0..add_count {
            matcher.add_edge(add_offset + add_index, sink, 1, 0, None);
        }

        matcher
    }

    fn add_candidate(&mut self, candidate: RenameCandidate) {
        let from = 1 + candidate.delete_index;
        let to = self.add_offset + candidate.add_index;
        let candidate_index = self.candidates.len();
        let edge_index = self.graph[from].len();
        let score = rename_candidate_score(&candidate);

        self.candidates.push(candidate);
        self.candidate_edges.push((from, edge_index));
        self.add_edge(from, to, 1, -score, Some(candidate_index));
    }

    fn add_edge(
        &mut self,
        from: usize,
        to: usize,
        cap: i32,
        cost: i64,
        candidate_index: Option<usize>,
    ) {
        let reverse_from = self.graph[to].len();
        let reverse_to = self.graph[from].len();
        self.graph[from].push(FlowEdge {
            to,
            rev: reverse_from,
            cap,
            cost,
            candidate_index,
        });
        self.graph[to].push(FlowEdge {
            to: from,
            rev: reverse_to,
            cap: 0,
            cost: -cost,
            candidate_index: None,
        });
    }

    fn solve(mut self) -> Vec<SelectedRename> {
        while self.augment_best_negative_path() {}

        let mut selected = Vec::new();
        for (from, edge_index) in &self.candidate_edges {
            let edge = &self.graph[*from][*edge_index];
            if edge.cap == 0 {
                let Some(candidate_index) = edge.candidate_index else {
                    continue;
                };
                let candidate = &self.candidates[candidate_index];
                selected.push(SelectedRename {
                    delete_path: candidate.delete_path.clone(),
                    add_path: candidate.add_path.clone(),
                    similarity: candidate.similarity,
                });
            }
        }
        selected.sort_by(|left, right| left.add_path.cmp(&right.add_path));
        selected
    }

    fn augment_best_negative_path(&mut self) -> bool {
        let node_count = self.graph.len();
        let mut distance = vec![i64::MAX; node_count];
        let mut parent = vec![None::<(usize, usize)>; node_count];
        let mut queued = vec![false; node_count];
        let mut queue = std::collections::VecDeque::new();

        distance[0] = 0;
        queued[0] = true;
        queue.push_back(0);

        while let Some(node) = queue.pop_front() {
            queued[node] = false;
            for (edge_index, edge) in self.graph[node].iter().enumerate() {
                if edge.cap <= 0 || distance[node] == i64::MAX {
                    continue;
                }
                let next_distance = distance[node] + edge.cost;
                if next_distance < distance[edge.to] {
                    distance[edge.to] = next_distance;
                    parent[edge.to] = Some((node, edge_index));
                    if !queued[edge.to] {
                        queued[edge.to] = true;
                        queue.push_back(edge.to);
                    }
                }
            }
        }

        if distance[self.sink] >= 0 {
            return false;
        }

        let mut node = self.sink;
        while let Some((from, edge_index)) = parent[node] {
            let to = self.graph[from][edge_index].to;
            let reverse = self.graph[from][edge_index].rev;
            self.graph[from][edge_index].cap -= 1;
            self.graph[to][reverse].cap += 1;
            node = from;
        }

        true
    }
}

fn rename_candidate_score(candidate: &RenameCandidate) -> i64 {
    const CARDINALITY_WEIGHT: i64 = 1_000_000_000_000;
    const SIMILARITY_WEIGHT: i64 = 10_000_000;
    const BASENAME_WEIGHT: i64 = 1_000_000;
    const SIZE_SCORE_MAX: i64 = 999_999;

    let size_score = SIZE_SCORE_MAX - (candidate.size_delta as i64).min(SIZE_SCORE_MAX);
    CARDINALITY_WEIGHT
        + i64::from(candidate.similarity) * SIMILARITY_WEIGHT
        + if candidate.basename_match {
            BASENAME_WEIGHT
        } else {
            0
        }
        + size_score
}

fn text_similarity_percent(before: &str, after: &str) -> u8 {
    if before == after {
        return 100;
    }
    if before.is_empty() || after.is_empty() {
        return 0;
    }

    let line_ratio = TextDiff::from_lines(before, after).ratio();
    let token_ratio = token_multiset_similarity(before, after);
    let ratio = line_ratio.max(token_ratio);
    ((ratio * 100.0).floor() as u8).min(99)
}

fn token_multiset_similarity(before: &str, after: &str) -> f32 {
    let before_tokens = token_counts(before);
    let after_tokens = token_counts(after);
    let mut intersection = 0usize;
    let mut union = 0usize;

    for (token, before_count) in &before_tokens {
        let after_count = after_tokens.get(token).copied().unwrap_or(0);
        intersection += (*before_count).min(after_count);
        union += (*before_count).max(after_count);
    }
    for (token, after_count) in &after_tokens {
        if !before_tokens.contains_key(token) {
            union += *after_count;
        }
    }

    if union == 0 {
        return 0.0;
    }
    intersection as f32 / union as f32
}

fn token_counts(value: &str) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for token in value.split_whitespace() {
        *counts.entry(token).or_insert(0) += 1;
    }
    counts
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn emit_file_patch(
    path: &str,
    change: &FileChange,
    options: &PatchOptions,
    out: &mut String,
) -> Result<(), PatchError> {
    let before = change.before.as_deref();
    let after = change.after.as_deref();

    let mode = resolve_mode_transition(path, change)?;

    if before == after && change.moved.is_none() && mode.changed().is_none() {
        return Ok(());
    }

    let old_path = match change.moved.as_ref() {
        Some(moved) => normalize_and_validate_path(path, moved.source_path())?,
        None => path.to_owned(),
    };
    let new_path = path;

    out.push_str("diff --git ");
    out.push_str(&patch_path(Some("a"), &old_path));
    out.push(' ');
    out.push_str(&patch_path(Some("b"), new_path));
    out.push('\n');

    match (before, after, change.moved.as_ref()) {
        (None, Some(_), _) => {
            out.push_str("new file mode ");
            out.push_str(mode.after.unwrap_or(FileMode::Regular).as_str());
            out.push('\n');
        }
        (Some(_), None, _) => {
            out.push_str("deleted file mode ");
            out.push_str(mode.before.unwrap_or(FileMode::Regular).as_str());
            out.push('\n');
        }
        (Some(_), Some(_), moved) => {
            if let Some((before_mode, after_mode)) = mode.changed() {
                out.push_str("old mode ");
                out.push_str(before_mode.as_str());
                out.push_str("\nnew mode ");
                out.push_str(after_mode.as_str());
                out.push('\n');
            }
            if let Some(moved) = moved {
                out.push_str("similarity index ");
                out.push_str(&moved.similarity().unwrap_or(100).to_string());
                out.push_str("%\nrename from ");
                out.push_str(&patch_path(None, &old_path));
                out.push_str("\nrename to ");
                out.push_str(&patch_path(None, new_path));
                out.push('\n');
            }
        }
        _ => {}
    }

    let (old_label, new_label) = match (before, after) {
        (None, Some(_)) => ("/dev/null".to_owned(), patch_path(Some("b"), new_path)),
        (Some(_), None) => (patch_path(Some("a"), &old_path), "/dev/null".to_owned()),
        (Some(_), Some(_)) => (
            patch_path(Some("a"), &old_path),
            patch_path(Some("b"), new_path),
        ),
        (None, None) => unreachable!(),
    };

    let old_text = before.unwrap_or("");
    let new_text = after.unwrap_or("");
    let body = unified_diff(
        old_text,
        new_text,
        &old_label,
        &new_label,
        options.context_lines,
    );
    out.push_str(&body);
    Ok(())
}

fn patch_path(prefix: Option<&str>, normalized_path: &str) -> String {
    let path = match prefix {
        Some(prefix) => format!("{prefix}/{normalized_path}"),
        None => normalized_path.to_owned(),
    };

    if !needs_quoting(&path) {
        return path;
    }

    let mut quoted = String::with_capacity(path.len() + 2);
    quoted.push('"');
    for char in path.chars() {
        match char {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            control if control.is_control() => push_octal_utf8(control, &mut quoted),
            _ => quoted.push(char),
        }
    }
    quoted.push('"');
    quoted
}

fn needs_quoting(path: &str) -> bool {
    path.chars()
        .any(|char| matches!(char, '\\' | '"') || char.is_control())
}

fn push_octal_utf8(char: char, out: &mut String) {
    let mut buffer = [0; 4];
    for byte in char.encode_utf8(&mut buffer).bytes() {
        out.push('\\');
        out.push(char::from(b'0' + ((byte >> 6) & 0o7)));
        out.push(char::from(b'0' + ((byte >> 3) & 0o7)));
        out.push(char::from(b'0' + (byte & 0o7)));
    }
}

fn normalize_and_validate_path(owner_path: &str, path: &str) -> Result<String, PatchError> {
    let normalized = path.replace('\\', "/");

    if normalized.is_empty() {
        return invalid(owner_path, "path must not be empty");
    }
    if normalized.starts_with('/') {
        return invalid(owner_path, "absolute paths are not supported");
    }
    if normalized.contains('\0') {
        return invalid(owner_path, "path must not contain NUL");
    }
    if normalized
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return invalid(
            owner_path,
            "path components must not be empty, '.', or '..'",
        );
    }

    Ok(normalized)
}

fn unified_diff(
    old_text: &str,
    new_text: &str,
    old_label: &str,
    new_label: &str,
    context: usize,
) -> String {
    if old_text == new_text {
        return String::new();
    }

    let diff = TextDiff::from_lines(old_text, new_text);
    let groups = diff.grouped_ops(context);
    if groups.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("--- ");
    out.push_str(old_label);
    out.push('\n');
    out.push_str("+++ ");
    out.push_str(new_label);
    out.push('\n');

    for group in groups {
        let Some(first) = group.first() else { continue };
        let old_start_idx = first.old_range().start;
        let new_start_idx = first.new_range().start;
        let mut old_len = 0usize;
        let mut new_len = 0usize;
        let mut lines = Vec::new();

        for op in group {
            for change in diff.iter_changes(&op) {
                match change.tag() {
                    ChangeTag::Delete => {
                        old_len += 1;
                        lines.push(('-', change.value()));
                    }
                    ChangeTag::Insert => {
                        new_len += 1;
                        lines.push(('+', change.value()));
                    }
                    ChangeTag::Equal => {
                        old_len += 1;
                        new_len += 1;
                        lines.push((' ', change.value()));
                    }
                }
            }
        }

        out.push_str("@@ -");
        out.push_str(&range_header_start(old_start_idx, old_len).to_string());
        out.push(',');
        out.push_str(&old_len.to_string());
        out.push_str(" +");
        out.push_str(&range_header_start(new_start_idx, new_len).to_string());
        out.push(',');
        out.push_str(&new_len.to_string());
        out.push_str(" @@\n");

        for (prefix, value) in lines {
            emit_line(prefix, value, &mut out);
        }
    }

    out
}

fn range_header_start(start_idx: usize, len: usize) -> usize {
    if len == 0 { start_idx } else { start_idx + 1 }
}

fn emit_line(prefix: char, value: &str, out: &mut String) {
    out.push(prefix);
    out.push_str(value);
    if !value.ends_with('\n') {
        out.push('\n');
        out.push_str("\\ No newline at end of file\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(input: &str) -> Result<String, PatchError> {
        generate_patch_from_json(input)
    }

    fn apply(input: &str) -> Result<ApplyPatchResult, PatchError> {
        let output = apply_patch_from_json(input)?;
        serde_json::from_str(&output).map_err(PatchError::InvalidJson)
    }

    fn inspect(input: &str) -> Result<PatchSummary, PatchError> {
        let output = inspect_patch_from_json(input)?;
        serde_json::from_str(&output).map_err(PatchError::InvalidJson)
    }

    #[test]
    fn rejects_mixed_patch_atomically() {
        let patch = patch(
            r#"{
              "changes": {
                "a-success.txt": { "before": "one\n", "after": "two\n" },
                "z-fail.txt": { "before": "old\n", "after": "new\n" }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));

        let request = serde_json::json!({
            "files": {
                "a-success.txt": "one\n",
                "z-fail.txt": "stale\n"
            },
            "patch": patch,
        });

        match apply(&request.to_string()).unwrap_or_else(|error| panic!("{error}")) {
            ApplyPatchResult::Rejected { files, rejects } => {
                assert_eq!(
                    files.get("a-success.txt").map(|file| file.content.as_str()),
                    Some("one\n")
                );
                assert_eq!(
                    files.get("z-fail.txt").map(|file| file.content.as_str()),
                    Some("stale\n")
                );
                assert_eq!(rejects.len(), 1);
                assert!(matches!(rejects[0], PatchReject::ContentMismatch { .. }));
            }
            ApplyPatchResult::Applied { .. } => panic!("expected atomic reject"),
        }
    }

    #[test]
    fn reject_patchlet_replays_against_corrected_snapshot() {
        let patch = patch(
            r#"{
              "changes": {
                "a.txt": { "before": "one\n", "after": "two\n" }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));

        let stale = serde_json::json!({ "files": { "a.txt": "stale\n" }, "patch": patch });
        let patchlet = match apply(&stale.to_string()).unwrap_or_else(|error| panic!("{error}")) {
            ApplyPatchResult::Rejected { rejects, .. } => match &rejects[0] {
                PatchReject::ContentMismatch { patch, .. } => patch.clone(),
                reject => panic!("unexpected reject: {reject:?}"),
            },
            ApplyPatchResult::Applied { .. } => panic!("expected reject"),
        };

        let corrected = serde_json::json!({ "files": { "a.txt": "one\n" }, "patch": patchlet });
        match apply(&corrected.to_string()).unwrap_or_else(|error| panic!("{error}")) {
            ApplyPatchResult::Applied { files, .. } => {
                assert_eq!(
                    files.get("a.txt").map(|file| file.content.as_str()),
                    Some("two\n")
                );
            }
            ApplyPatchResult::Rejected { rejects, .. } => panic!("unexpected rejects: {rejects:?}"),
        }
    }

    #[test]
    fn applies_patch_entries_sequentially() {
        let create = patch(r#"{ "changes": { "a.txt": { "after": "one\n" } } }"#)
            .unwrap_or_else(|error| panic!("{error}"));
        let modify =
            patch(r#"{ "changes": { "a.txt": { "before": "one\n", "after": "two\n" } } }"#)
                .unwrap_or_else(|error| panic!("{error}"));
        let request = serde_json::json!({ "files": {}, "patch": format!("{create}{modify}") });

        match apply(&request.to_string()).unwrap_or_else(|error| panic!("{error}")) {
            ApplyPatchResult::Applied { files, changes } => {
                assert_eq!(
                    files.get("a.txt").map(|file| file.content.as_str()),
                    Some("two\n")
                );
                assert_eq!(changes.len(), 2);
            }
            ApplyPatchResult::Rejected { rejects, .. } => panic!("unexpected rejects: {rejects:?}"),
        }
    }

    #[test]
    fn unsupported_operation_rejects_atomically() {
        let modify =
            patch(r#"{ "changes": { "a.txt": { "before": "one\n", "after": "two\n" } } }"#)
                .unwrap_or_else(|error| panic!("{error}"));
        let copy = "diff --git a/source.txt b/copy.txt\nsimilarity index 100%\ncopy from source.txt\ncopy to copy.txt\n";
        let patch = format!("{modify}{copy}");
        let summary = inspect(&serde_json::json!({ "patch": patch.clone() }).to_string())
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(
            summary
                .files
                .iter()
                .all(|file| !matches!(file, PatchFileSummary::Copied { .. }))
        );
        assert!(summary.rejects.iter().any(|reject| matches!(reject, PatchReject::Unsupported { operation, .. } if operation == "Copy")));

        let request = serde_json::json!({
            "files": { "a.txt": "one\n", "source.txt": "same\n" },
            "patch": patch,
        });

        match apply(&request.to_string()).unwrap_or_else(|error| panic!("{error}")) {
            ApplyPatchResult::Rejected { files, rejects } => {
                assert_eq!(
                    files.get("a.txt").map(|file| file.content.as_str()),
                    Some("one\n")
                );
                assert_eq!(
                    files.get("source.txt").map(|file| file.content.as_str()),
                    Some("same\n")
                );
                assert!(rejects.iter().any(|reject| matches!(reject, PatchReject::Unsupported { operation, .. } if operation == "Copy")));
            }
            ApplyPatchResult::Applied { .. } => panic!("expected unsupported reject"),
        }
    }

    #[test]
    fn hostile_patch_paths_reject_atomically() {
        let valid =
            patch(r#"{ "changes": { "good.txt": { "before": "one\n", "after": "two\n" } } }"#)
                .unwrap_or_else(|error| panic!("{error}"));
        let hostile = "diff --git a/../evil.txt b/../evil.txt\n--- a/../evil.txt\n+++ b/../evil.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let patch = format!("{valid}{hostile}");

        let summary = inspect(&serde_json::json!({ "patch": patch.clone() }).to_string())
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(summary.files.len(), 1);
        assert_eq!(summary.rejects.len(), 1);
        assert!(matches!(
            summary.rejects[0],
            PatchReject::Unsupported { .. }
        ));

        let request = serde_json::json!({ "files": { "good.txt": "one\n" }, "patch": patch });
        match apply(&request.to_string()).unwrap_or_else(|error| panic!("{error}")) {
            ApplyPatchResult::Rejected { files, rejects } => {
                assert_eq!(
                    files.get("good.txt").map(|file| file.content.as_str()),
                    Some("one\n")
                );
                assert_eq!(rejects.len(), 1);
                assert!(matches!(rejects[0], PatchReject::Unsupported { .. }));
            }
            ApplyPatchResult::Applied { .. } => panic!("expected hostile path reject"),
        }
    }

    #[test]
    fn inspect_reports_parse_rejects_without_throwing() {
        let summary =
            inspect("{ \"patch\": \"a\\u0000b\" }").unwrap_or_else(|error| panic!("{error}"));
        assert!(summary.files.is_empty());
        assert_eq!(summary.rejects.len(), 1);
        assert!(matches!(
            summary.rejects[0],
            PatchReject::Unsupported { .. }
        ));
    }

    #[test]
    fn applies_modify_add_delete_and_rename() {
        let patch = patch(
            r#"{
              "changes": {
                "modified.txt": { "before": "one\n", "after": "two\n" },
                "added.txt": { "after": "new\n" },
                "deleted.txt": { "before": "old\n" },
                "new.txt": { "before": "same\n", "after": "same\n", "moved": "old.txt" }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));

        let request = serde_json::json!({
            "files": {
                "modified.txt": "one\n",
                "deleted.txt": "old\n",
                "old.txt": "same\n"
            },
            "patch": patch,
        });

        match apply(&request.to_string()).unwrap_or_else(|error| panic!("{error}")) {
            ApplyPatchResult::Applied { files, changes } => {
                assert_eq!(
                    files.get("modified.txt").map(|file| file.content.as_str()),
                    Some("two\n")
                );
                assert_eq!(
                    files.get("added.txt").map(|file| file.content.as_str()),
                    Some("new\n")
                );
                assert!(!files.contains_key("deleted.txt"));
                assert!(!files.contains_key("old.txt"));
                assert_eq!(
                    files.get("new.txt").map(|file| file.content.as_str()),
                    Some("same\n")
                );
                assert_eq!(changes.len(), 4);
            }
            ApplyPatchResult::Rejected { rejects, .. } => panic!("unexpected rejects: {rejects:?}"),
        }
    }

    #[test]
    fn applies_mode_only_patch() {
        let patch = patch(
            r#"{
              "changes": {
                "script.sh": { "before": "echo hi\n", "after": "echo hi\n", "mode": { "before": "100644", "after": "100755" } }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));

        let request = serde_json::json!({
            "files": { "script.sh": { "content": "echo hi\n", "mode": "100644" } },
            "patch": patch,
        });

        match apply(&request.to_string()).unwrap_or_else(|error| panic!("{error}")) {
            ApplyPatchResult::Applied { files, changes } => {
                assert_eq!(
                    files.get("script.sh").and_then(|file| file.mode.as_deref()),
                    Some("100755")
                );
                assert_eq!(changes.len(), 1);
            }
            ApplyPatchResult::Rejected { rejects, .. } => panic!("unexpected rejects: {rejects:?}"),
        }
    }

    #[test]
    fn rejects_duplicate_normalized_snapshot_paths() {
        let result = apply(
            r#"{
              "files": {
                "dir/file.txt": "one\n",
                "dir\\file.txt": "two\n"
              },
              "patch": ""
            }"#,
        );
        match result {
            Ok(output) => panic!("expected duplicate path rejection, got {output:?}"),
            Err(error) => assert!(error.to_string().contains("duplicate normalized path")),
        }
    }

    #[test]
    fn returns_rejects_without_mutating_snapshot() {
        let patch = patch(
            r#"{
              "changes": {
                "a.txt": { "before": "one\n", "after": "two\n" }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));

        let request = serde_json::json!({
            "files": { "a.txt": "changed\n" },
            "patch": patch,
        });

        match apply(&request.to_string()).unwrap_or_else(|error| panic!("{error}")) {
            ApplyPatchResult::Rejected { files, rejects } => {
                assert_eq!(
                    files.get("a.txt").map(|file| file.content.as_str()),
                    Some("changed\n")
                );
                assert_eq!(rejects.len(), 1);
                assert!(matches!(rejects[0], PatchReject::ContentMismatch { .. }));
            }
            ApplyPatchResult::Applied { .. } => panic!("expected reject"),
        }
    }

    #[test]
    fn emits_modify_patch() {
        let output = patch(
            r#"{
              "changes": {
                "src/main.ts": { "before": "a\nb\n", "after": "a\nc\n" }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(output.contains("diff --git a/src/main.ts b/src/main.ts\n"));
        assert!(output.contains("--- a/src/main.ts\n+++ b/src/main.ts\n"));
        assert!(output.contains("-b\n+c\n"));
    }

    #[test]
    fn emits_valid_hunk_lengths_for_repeated_lines() {
        let output = patch(
            r#"{
              "changes": {
                "repeated.txt": { "before": "a\na\na\nb\n", "after": "a\na\nb\nb\n" }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(output.contains("@@ -1,4 +1,4 @@\n"));
    }

    #[test]
    fn emits_exact_edge_hunk_headers() {
        let added = patch(r#"{ "changes": { "one.txt": { "after": "one\n" } } }"#)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(added.contains("@@ -0,0 +1,1 @@\n+one\n"));

        let deleted = patch(r#"{ "changes": { "one.txt": { "before": "one\n" } } }"#)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(deleted.contains("@@ -1,1 +0,0 @@\n-one\n"));
    }

    #[test]
    fn emits_rename_chmod_edit_headers_in_git_order() {
        let output = patch(
            r#"{
              "changes": {
                "new.sh": {
                  "before": "echo old\n",
                  "after": "echo new\n",
                  "moved": { "from": "old.sh", "similarity": 80 },
                  "mode": { "before": "100644", "after": "100755" }
                }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(output.starts_with(
            "diff --git a/old.sh b/new.sh\nold mode 100644\nnew mode 100755\nsimilarity index 80%\nrename from old.sh\nrename to new.sh\n--- a/old.sh\n+++ b/new.sh\n"
        ));
    }

    #[test]
    fn emits_add_delete_and_rename_headers() {
        let output = patch(
            r#"{
              "changes": {
                "added.txt": { "after": "hello\n" },
                "deleted.txt": { "before": "bye\n" },
                "new.txt": { "before": "same\n", "after": "same\n", "moved": "old.txt" }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(output.contains("new file mode 100644\n"));
        assert!(output.contains("deleted file mode 100644\n"));
        assert!(output.contains("rename from old.txt\nrename to new.txt\n"));
    }

    #[test]
    fn auto_detects_renames_from_delete_add_pairs() {
        let output = patch(
            r#"{
              "changes": {
                "old.txt": { "before": "one\ntwo\nthree\nfour\n" },
                "new.txt": { "after": "one\ntwo\nTHREE\nfour\n" }
              },
              "options": { "renameSimilarityThreshold": 70 }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(output.contains("similarity index 75%\nrename from old.txt\nrename to new.txt\n"));
        assert!(output.contains("--- a/old.txt\n+++ b/new.txt\n"));
        assert!(!output.contains("deleted file mode"));
        assert!(!output.contains("new file mode"));
    }

    #[test]
    fn auto_detected_renames_use_global_optimum() {
        let output = patch(
            r#"{
              "changes": {
                "old-a.txt": { "before": "a\na\na\nb\n" },
                "old-b.txt": { "before": "a\na\na\na\n" },
                "new-x.txt": { "after": "a\na\na\nb\n" },
                "new-y.txt": { "after": "a\na\nb\nb\n" }
              },
              "options": { "renameSimilarityThreshold": 70 }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));

        assert!(output.contains("rename from old-b.txt\nrename to new-x.txt\n"));
        assert!(output.contains("rename from old-a.txt\nrename to new-y.txt\n"));
        assert_eq!((output.match_indices("rename from").count()), 2);
        assert!(!output.contains("deleted file mode"));
        assert!(!output.contains("new file mode"));
    }

    #[test]
    fn explicit_renames_are_not_auto_remapped() {
        let output = patch(
            r#"{
              "changes": {
                "explicit-new.txt": { "before": "same\n", "after": "same\n", "moved": { "from": "explicit-old.txt", "similarity": 88 } },
                "auto-old.txt": { "before": "auto\n" },
                "auto-new.txt": { "after": "auto\n" }
              },
              "options": { "renameSimilarityThreshold": 100 }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));

        assert!(
            output.contains(
                "similarity index 100%\nrename from auto-old.txt\nrename to auto-new.txt\n"
            )
        );
        assert!(output.contains(
            "similarity index 88%\nrename from explicit-old.txt\nrename to explicit-new.txt\n"
        ));
        assert_eq!((output.match_indices("rename from").count()), 2);
    }

    #[test]
    fn auto_detected_renames_preserve_mode_changes() {
        let output = patch(
            r#"{
              "changes": {
                "old.sh": { "before": "echo hi\n", "mode": { "before": "100644" } },
                "new.sh": { "after": "echo hi\n", "mode": { "after": "100755" } }
              },
              "options": { "renameSimilarityThreshold": 100 }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(output.contains("old mode 100644\nnew mode 100755\nsimilarity index 100%\n"));
        assert!(output.contains("rename from old.sh\nrename to new.sh\n"));
    }

    #[test]
    fn quotes_paths_that_need_git_c_style_escapes() {
        let output = patch(
            "{\n  \"changes\": {\n    \"tab\\tquote\\\"file.txt\": { \"after\": \"x\\n\" },\n    \"bell\\u0007file.txt\": { \"after\": \"x\\n\" },\n    \"newline\\nfile.txt\": { \"before\": \"same\\n\", \"after\": \"same\\n\", \"moved\": \"old\\tname.txt\" }\n  }\n}",
        )
        .unwrap_or_else(|error| panic!("{error}"));

        assert!(
            output.contains(
                "diff --git \"a/tab\\tquote\\\"file.txt\" \"b/tab\\tquote\\\"file.txt\"\n"
            )
        );
        assert!(output.contains("+++ \"b/tab\\tquote\\\"file.txt\"\n"));
        assert!(output.contains("diff --git \"a/bell\\007file.txt\" \"b/bell\\007file.txt\"\n"));
        assert!(
            output.contains("rename from \"old\\tname.txt\"\nrename to \"newline\\nfile.txt\"\n")
        );
    }

    #[test]
    fn emits_existing_file_mode_changes() {
        let chmod_only = patch(
            r#"{
              "changes": {
                "script.sh": { "before": "echo hi\n", "after": "echo hi\n", "mode": { "before": "100644", "after": "100755" } }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            chmod_only,
            "diff --git a/script.sh b/script.sh\nold mode 100644\nnew mode 100755\n"
        );

        let edit_and_chmod = patch(
            r#"{
              "changes": {
                "script.sh": { "before": "echo hi\n", "after": "echo bye\n", "mode": { "before": "100644", "after": "100755" } }
              }
            }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(
            edit_and_chmod
                .contains("old mode 100644\nnew mode 100755\n--- a/script.sh\n+++ b/script.sh\n")
        );
    }

    #[test]
    fn rejects_duplicate_normalized_paths() {
        let result = patch(
            r#"{
              "changes": {
                "dir/file.txt": { "after": "one\n" },
                "dir\\file.txt": { "after": "two\n" }
              }
            }"#,
        );
        match result {
            Ok(output) => panic!("expected duplicate path rejection, got {output}"),
            Err(error) => assert!(error.to_string().contains("duplicate normalized path")),
        }
    }

    #[test]
    fn rejects_unsupported_text_and_path_inputs() {
        for input in [
            r#"{ "changes": { "bad.txt": { "after": "a\u0000b" } } }"#,
            r#"{ "changes": { "bad path.txt": { "after": "x\n" } } }"#,
            r#"{ "changes": { "../bad.txt": { "after": "x\n" } } }"#,
            r#"{ "changes": { "bad.txt": { "after": "x\n", "mode": "100600" } } }"#,
            r#"{ "changes": { "bad.txt": { "before": "x\n", "after": "x\n", "mode": "100755" } } }"#,
            r#"{ "changes": { "bad.txt": { "before": "x\n", "after": "x\n", "moved": { "from": "old.txt", "similarity": 101 } } } }"#,
            r#"{ "changes": { "bad.txt": { "before": "x\n", "after": "x\n", "moved": { "from": "old.txt", "similarity": -1 } } } }"#,
            r#"{ "changes": { "bad.txt": { "before": "x\n", "after": "x\n", "moved": { "from": "old.txt", "similarity": 999999 } } } }"#,
        ] {
            assert!(patch(input).is_err(), "expected rejection for {input}");
        }
    }
}
