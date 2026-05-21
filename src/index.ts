import { loadNativeBinding, nativeBindingExists } from "./native.js";
import type { ApplyPatchRequest, ApplyPatchResult, Changes, FileSnapshot, GeneratePatchOptions, GeneratePatchRequest, InspectPatchRequest, PatchSummary } from "./types.js";

export type {
  AppliedPatchChange,
  ApplyPatchRequest,
  ApplyPatchResult,
  Changes,
  FileChange,
  FileContent,
  FileEntry,
  FileModeChange,
  FileSnapshot,
  GeneratePatchOptions,
  GitFileMode,
  GeneratePatchRequest,
  InspectPatchRequest,
  PatchFileSummary,
  PatchReject,
  PatchSummary,
  Path,
  RenameDetail,
} from "./types.js";
export { getFfiFilename, getNativeFilename, getNpmPackageName, getTriple } from "./platform.js";
export { nativeBindingExists };

export function generatePatch(changes: Changes, options?: GeneratePatchOptions): string;
export function generatePatch(request: GeneratePatchRequest): string;
export function generatePatch(
  changesOrRequest: Changes | GeneratePatchRequest,
  options?: GeneratePatchOptions,
): string {
  const request = isGeneratePatchRequest(changesOrRequest)
    ? changesOrRequest
    : { changes: changesOrRequest, options };
  return loadNativeBinding().generatePatchJson(serializeRequest(normalizeRequest(request)));
}

export function applyPatch(files: FileSnapshot, patch: string): ApplyPatchResult;
export function applyPatch(request: ApplyPatchRequest): ApplyPatchResult;
export function applyPatch(filesOrRequest: FileSnapshot | ApplyPatchRequest, patch?: string): ApplyPatchResult {
  const request = patch !== undefined
    ? { files: filesOrRequest as FileSnapshot, patch }
    : isApplyPatchRequest(filesOrRequest)
      ? filesOrRequest
      : { files: filesOrRequest, patch };
  if (typeof request.patch !== "string") throw new TypeError("applyPatch requires a patch string");
  return JSON.parse(loadNativeBinding().applyPatchJson(JSON.stringify(request))) as ApplyPatchResult;
}

export function inspectPatch(patch: string): PatchSummary;
export function inspectPatch(request: InspectPatchRequest): PatchSummary;
export function inspectPatch(patchOrRequest: string | InspectPatchRequest): PatchSummary {
  const request = typeof patchOrRequest === "string" ? { patch: patchOrRequest } : patchOrRequest;
  if (typeof request.patch !== "string") throw new TypeError("inspectPatch requires a patch string");
  return JSON.parse(loadNativeBinding().inspectPatchJson(JSON.stringify(request))) as PatchSummary;
}

function isGeneratePatchRequest(value: Changes | GeneratePatchRequest): value is GeneratePatchRequest {
  if (!isRecord(value) || !("changes" in value)) return false;
  if ("options" in value) return true;
  return isRecord(value.changes) && !isFileChangeLike(value.changes);
}

function isApplyPatchRequest(value: FileSnapshot | ApplyPatchRequest): value is ApplyPatchRequest {
  return isRecord(value) && isRecord(value.files) && typeof value.patch === "string";
}

function normalizeRequest(request: GeneratePatchRequest): GeneratePatchRequest {
  return {
    changes: Object.fromEntries(
      Object.entries(request.changes).map(([path, change]) => [
        path,
        {
          ...change,
          before: change.before ?? undefined,
          after: change.after ?? undefined,
        },
      ]),
    ),
    options: request.options,
  };
}

function serializeRequest(request: GeneratePatchRequest): string {
  assertGeneratePatchIntegers(request);
  return JSON.stringify(request, (_key, value) => {
    if (typeof value === "number" && !Number.isFinite(value)) {
      throw new TypeError("generatePatch only accepts finite numeric options");
    }
    return value;
  });
}

function assertGeneratePatchIntegers(request: GeneratePatchRequest): void {
  if (request.options?.contextLines !== undefined) {
    assertInteger("contextLines", request.options.contextLines, 1);
  }
  if (request.options?.renameSimilarityThreshold !== undefined) {
    assertInteger("renameSimilarityThreshold", request.options.renameSimilarityThreshold, 0, 100);
  }

  for (const [path, change] of Object.entries(request.changes)) {
    const moved = change.moved;
    if (isRecord(moved) && "similarity" in moved && moved.similarity !== undefined) {
      assertInteger(`${path}.moved.similarity`, moved.similarity, 0, 100);
    }
  }
}

function assertInteger(name: string, value: unknown, min: number, max?: number): void {
  if (typeof value !== "number" || !Number.isInteger(value) || value < min || (max !== undefined && value > max)) {
    const range = max === undefined ? `at least ${min}` : `between ${min} and ${max}`;
    throw new TypeError(`${name} must be an integer ${range}`);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isFileChangeLike(value: unknown): boolean {
  if (!isRecord(value)) return false;
  const hasFileChangeKey = "before" in value || "after" in value || "moved" in value || "mode" in value;
  return hasFileChangeKey && isOptionalString(value.before) && isOptionalString(value.after) && isMovedLike(value.moved) && isModeLike(value.mode);
}

function isOptionalString(value: unknown): boolean {
  return value === undefined || value === null || typeof value === "string";
}

function isMovedLike(value: unknown): boolean {
  if (value === undefined || typeof value === "string") return true;
  return isRecord(value) && (value.from === undefined || typeof value.from === "string");
}

function isModeLike(value: unknown): boolean {
  if (value === undefined || typeof value === "string") return true;
  if (!isRecord(value)) return false;
  return isOptionalString(value.before) && isOptionalString(value.after);
}
