import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { currentPlatform, nativeFfiFilename, nativeNodeFilename, sourceLibraryFilename } from "./platform-info.mjs";

const profile = process.argv[2] === "debug" ? "debug" : "release";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const platform = currentPlatform();
const targetDir = process.env.CARGO_BUILD_TARGET
  ? join(root, "target", process.env.CARGO_BUILD_TARGET, profile)
  : join(root, "target", profile);
const binDir = join(root, "bin");

copyNativeArtifact(sourceLibraryFilename("git_patch_native", platform), nativeNodeFilename(platform));
copyNativeArtifact(sourceLibraryFilename("git_patch_ffi", platform), nativeFfiFilename(platform));

function copyNativeArtifact(sourceName, destinationName) {
  const source = join(targetDir, sourceName);
  const destination = join(binDir, destinationName);

  if (!existsSync(source)) {
    throw new Error(`Native library not found at ${source}`);
  }

  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  console.log(`Copied ${source} -> ${destination}`);
}
