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
  /** Git-style similarity percentage. Defaults to 100 when omitted. */
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
}

export interface GeneratePatchRequest {
  changes: Changes;
  options?: GeneratePatchOptions;
}

export interface NativeBinding {
  generatePatchJson(inputJson: string): string;
}
