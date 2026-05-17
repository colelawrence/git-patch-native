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
  return loadNativeBinding().generatePatchJson(JSON.stringify(normalizeRequest(request)));
}

export function applyPatch(files: FileSnapshot, patch: string): ApplyPatchResult;
export function applyPatch(request: ApplyPatchRequest): ApplyPatchResult;
export function applyPatch(filesOrRequest: FileSnapshot | ApplyPatchRequest, patch?: string): ApplyPatchResult {
  const request = isApplyPatchRequest(filesOrRequest) ? filesOrRequest : { files: filesOrRequest, patch };
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
  return typeof value === "object" && value !== null && "changes" in value;
}

function isApplyPatchRequest(value: FileSnapshot | ApplyPatchRequest): value is ApplyPatchRequest {
  return typeof value === "object" && value !== null && "files" in value && "patch" in value;
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
