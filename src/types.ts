/** Relative repository path. Backslashes are normalized to `/`; quotes/control characters are Git-quoted in output. */
export type Path = string;

export type FileContent = string;

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
  /** File mode header for adds/deletes. Defaults to 100644. */
  mode?: string;
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
