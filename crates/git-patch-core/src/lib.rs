use serde::Deserialize;
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PatchError {
    #[error("invalid patch request JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("invalid change for {path}: {message}")]
    InvalidChange { path: String, message: String },
    #[error("invalid patch option {name}: {message}")]
    InvalidOption { name: String, message: String },
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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Moved {
    From(String),
    Detail {
        from: String,
        similarity: Option<u8>,
    },
}

impl Moved {
    fn source_path(&self) -> &str {
        match self {
            Self::From(path) => path,
            Self::Detail { from, .. } => from,
        }
    }

    fn similarity(&self) -> Option<u8> {
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
}

impl Default for PatchOptions {
    fn default() -> Self {
        Self {
            context_lines: default_context_lines(),
        }
    }
}

fn default_context_lines() -> usize {
    3
}

pub fn generate_patch_from_json(input: &str) -> Result<String, PatchError> {
    let request: PatchRequest = serde_json::from_str(input)?;
    generate_patch(&request)
}

pub fn generate_patch(request: &PatchRequest) -> Result<String, PatchError> {
    validate_options(&request.options)?;

    let mut normalized_changes = BTreeMap::<String, (&str, &FileChange)>::new();

    for (path, change) in &request.changes {
        validate_change(path, change)?;
        let normalized = normalize_and_validate_path(path, path)?;
        if let Some((first, _)) = normalized_changes.insert(normalized.clone(), (path, change)) {
            return Err(PatchError::DuplicatePath {
                normalized,
                first: first.to_owned(),
                second: path.to_owned(),
            });
        }
    }

    let mut out = String::new();
    for (normalized_path, (_, change)) in normalized_changes {
        emit_file_patch(&normalized_path, change, &request.options, &mut out)?;
    }
    Ok(out)
}

fn validate_options(options: &PatchOptions) -> Result<(), PatchError> {
    if options.context_lines == 0 {
        return Err(PatchError::InvalidOption {
            name: "contextLines".to_owned(),
            message: "must be at least 1 so patches apply with default git apply".to_owned(),
        });
    }
    Ok(())
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
            .is_some_and(|similarity| similarity > 100)
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
        let Some(last) = group.last() else { continue };
        let old_start_idx = first.old_range().start;
        let new_start_idx = first.new_range().start;
        let old_len = last.old_range().end.saturating_sub(old_start_idx);
        let new_len = last.new_range().end.saturating_sub(new_start_idx);

        out.push_str("@@ -");
        out.push_str(&range_header_start(old_start_idx, old_len).to_string());
        out.push(',');
        out.push_str(&old_len.to_string());
        out.push_str(" +");
        out.push_str(&range_header_start(new_start_idx, new_len).to_string());
        out.push(',');
        out.push_str(&new_len.to_string());
        out.push_str(" @@\n");

        for op in group {
            for change in diff.iter_changes(&op) {
                match change.tag() {
                    ChangeTag::Delete => emit_line('-', change.value(), &mut out),
                    ChangeTag::Insert => emit_line('+', change.value(), &mut out),
                    ChangeTag::Equal => emit_line(' ', change.value(), &mut out),
                }
            }
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
        ] {
            assert!(patch(input).is_err(), "expected rejection for {input}");
        }
    }
}
