import { execSync } from "node:child_process";

export function getTriple(): string {
  const archName = normalizeArch(process.arch);
  const osName = normalizeOs(process.platform);
  return `${archName}-${osName}`;
}

export function getNpmPackageName(): string {
  const triple = getTriple();
  const packageName = TRIPLE_TO_NPM_PACKAGE[triple];
  if (!packageName) {
    throw new Error(`No npm package available for platform: ${triple}`);
  }
  return packageName;
}

export function getNativeFilename(): string {
  return `git_patch_native.${npmPlatformTag()}.node`;
}

export function getFfiFilename(): string {
  return `git_patch_ffi.${npmPlatformTag()}.${getFfiExtension()}`;
}

function getFfiExtension(): "dylib" | "so" | "dll" {
  switch (process.platform) {
    case "darwin":
      return "dylib";
    case "win32":
      return "dll";
    default:
      return "so";
  }
}

function normalizeOs(platform: NodeJS.Platform): string {
  switch (platform) {
    case "darwin":
      return "apple-darwin";
    case "linux":
      return detectLinuxLibc();
    case "win32":
      return "pc-windows-msvc";
    default:
      throw new Error(`Unsupported platform: ${platform}`);
  }
}

function detectLinuxLibc(): string {
  try {
    const lddOutput = execSync("ldd --version 2>&1", {
      encoding: "utf-8",
      timeout: 5000,
    });
    if (lddOutput.toLowerCase().includes("musl")) {
      return "unknown-linux-musl";
    }
  } catch {
    // Assume glibc if ldd is unavailable.
  }
  return "unknown-linux-gnu";
}

function normalizeArch(arch: string): string {
  switch (arch) {
    case "x64":
    case "amd64":
      return "x86_64";
    case "arm64":
      return "aarch64";
    default:
      throw new Error(`Unsupported architecture: ${arch}`);
  }
}

function npmPlatformTag(): string {
  switch (getTriple()) {
    case "aarch64-apple-darwin":
      return "darwin-arm64";
    case "x86_64-unknown-linux-gnu":
      return "linux-x64-gnu";
    case "aarch64-unknown-linux-gnu":
      return "linux-arm64-gnu";
    case "x86_64-pc-windows-msvc":
      return "win32-x64";
    case "aarch64-pc-windows-msvc":
      return "win32-arm64";
    default:
      throw new Error(`Unsupported platform: ${getTriple()}`);
  }
}

const TRIPLE_TO_NPM_PACKAGE: Record<string, string> = {
  "aarch64-apple-darwin": "git-patch-native-darwin-arm64",
  "x86_64-unknown-linux-gnu": "git-patch-native-linux-x64-gnu",
  "aarch64-unknown-linux-gnu": "git-patch-native-linux-arm64-gnu",
  "x86_64-pc-windows-msvc": "git-patch-native-win32-x64",
  "aarch64-pc-windows-msvc": "git-patch-native-win32-arm64",
};
