import { loadNativeBinding, nativeBindingExists } from "./native.js";
import type { Changes, GeneratePatchOptions, GeneratePatchRequest } from "./types.js";

export type {
  Changes,
  FileChange,
  FileContent,
  FileModeChange,
  GeneratePatchOptions,
  GitFileMode,
  GeneratePatchRequest,
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

function isGeneratePatchRequest(value: Changes | GeneratePatchRequest): value is GeneratePatchRequest {
  return typeof value === "object" && value !== null && "changes" in value;
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
