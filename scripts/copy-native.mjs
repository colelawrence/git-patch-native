import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const profile = process.argv[2] === "debug" ? "debug" : "release";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = join(root, "target", profile, nativeLibraryFilename());
const destination = join(root, "bin", nativeNodeFilename());

if (!existsSync(source)) {
  throw new Error(`Native library not found at ${source}`);
}

mkdirSync(dirname(destination), { recursive: true });
copyFileSync(source, destination);
console.log(`Copied ${source} -> ${destination}`);

function nativeLibraryFilename() {
  if (process.platform === "win32") return "git_patch_native.dll";
  if (process.platform === "darwin") return "libgit_patch_native.dylib";
  return "libgit_patch_native.so";
}

function nativeNodeFilename() {
  return `git_patch_native.${platformTag()}.node`;
}

function platformTag() {
  if (process.platform === "darwin") {
    return process.arch === "arm64" ? "darwin-arm64" : "darwin-x64";
  }
  if (process.platform === "win32") {
    return process.arch === "arm64" ? "win32-arm64" : "win32-x64";
  }
  if (process.platform === "linux") {
    const arch = process.arch === "arm64" ? "arm64" : "x64";
    return `linux-${arch}-${isMusl() ? "musl" : "gnu"}`;
  }
  throw new Error(`Unsupported platform: ${process.platform}`);
}

function isMusl() {
  try {
    return process.report?.getReport?.().header?.glibcVersionRuntime === undefined;
  } catch {
    return false;
  }
}
