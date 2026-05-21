/** Relative repository path. Backslashes are normalized to `/`; quotes/control characters are Git-quoted in output. */
export type Path = string;

export type FileContent = string;

export type GitFileMode = "100644" | "100755";

export interface FileModeChange {
  /** Existing mode for deletes, chmods, and rename/chmod combinations. */
  before?: GitFileMode | null;
  /** Resulting mode for adds, chmods, and rename/chmod combinations. */
  after?: GitFileMode | null;
}

export interface RenameDetail {
  from: Path;
  /** Git-style integer similarity percentage from 0 to 100. Defaults to 100 when omitted. */
  similarity?: number;
}

export interface FileChange {
  /** Previous file contents. Omit for additions. */
  before?: FileContent | null;
  /** Next file contents. Omit for deletions. */
  after?: FileContent | null;
  /** Previous path when the record key is the new path. */
  moved?: Path | RenameDetail;
  /** File mode metadata. Scalar shorthand is only valid for additions/deletions. */
  mode?: GitFileMode | FileModeChange;
}

export type Changes = Record<Path, FileChange>;

export interface GeneratePatchOptions {
  /** Unified-diff context lines. Defaults to 3. Must be at least 1 for default git apply compatibility. */
  contextLines?: number;
  /**
   * Opt-in rename detection threshold as an integer from 0 to 100.
   * When set, plain delete/add pairs with text similarity at or above this percentage
   * are emitted as rename patches with the computed similarity index.
   */
  renameSimilarityThreshold?: number;
}

export interface GeneratePatchRequest {
  changes: Changes;
  options?: GeneratePatchOptions;
}

export interface FileEntry {
  content: FileContent;
  mode?: GitFileMode;
}

export type FileSnapshot = Record<Path, FileContent | FileEntry>;

export interface ApplyPatchRequest {
  files: FileSnapshot;
  patch: string;
}

export interface InspectPatchRequest {
  patch: string;
}

export interface PatchSummary {
  files: PatchFileSummary[];
  rejects: PatchReject[];
}

export type PatchFileSummary =
  | { _tag: "Added"; path: Path }
  | { _tag: "Modified"; path: Path }
  | { _tag: "Deleted"; path: Path }
  | { _tag: "Renamed"; from: Path; to: Path }
  | { _tag: "Copied"; from: Path; to: Path };

export type ApplyPatchResult =
  | {
      _tag: "Applied";
      files: Record<Path, FileEntry>;
      changes: AppliedPatchChange[];
    }
  | {
      _tag: "Rejected";
      files: Record<Path, FileEntry>;
      rejects: PatchReject[];
    };

export type AppliedPatchChange =
  | { _tag: "Added"; path: Path; after: FileEntry }
  | { _tag: "Modified"; path: Path; before: FileEntry; after: FileEntry }
  | { _tag: "Deleted"; path: Path; before: FileEntry }
  | { _tag: "Renamed"; from: Path; to: Path; before: FileEntry; after: FileEntry };

export type PatchReject =
  | { _tag: "MissingFile"; path: Path; operation: string; patch: string; message: string }
  | { _tag: "AlreadyExists"; path: Path; operation: string; patch: string; message: string }
  | { _tag: "ContentMismatch"; path: Path; operation: string; hunk?: number; patch: string; message: string }
  | { _tag: "Unsupported"; operation: string; patch: string; message: string };

export interface NativeBinding {
  generatePatchJson(inputJson: string): string;
  applyPatchJson(inputJson: string): string;
  inspectPatchJson(inputJson: string): string;
}
