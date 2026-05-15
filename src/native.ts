import { existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { getNativeFilename, getNpmPackageName } from "./platform.js";
import type { NativeBinding } from "./types.js";

let cached: NativeBinding | undefined;

export function loadNativeBinding(): NativeBinding {
  if (cached) return cached;

  const candidates = nativeCandidates();
  const require = createRequire(import.meta.url);
  const errors: string[] = [];

  for (const candidate of candidates) {
    try {
      if (existsSync(candidate)) {
        cached = require(candidate) as NativeBinding;
        return cached;
      }
    } catch (error) {
      errors.push(`${candidate}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  throw new Error(
    [
      "git-patch-native: native binding not found.",
      "Run `npm run build:native` during local development.",
      `Checked: ${candidates.join(", ")}`,
      errors.length ? `Load errors: ${errors.join("; ")}` : "",
    ]
      .filter(Boolean)
      .join("\n"),
  );
}

export function nativeBindingExists(): boolean {
  return nativeCandidates().some((candidate) => existsSync(candidate));
}

function nativeCandidates(): string[] {
  const packageDir = getPackageDir();
  const candidates = [join(packageDir, "bin", getNativeFilename())];

  try {
    const packageName = getNpmPackageName();
    const require = createRequire(join(packageDir, "package.json"));
    const packageJsonPath = require.resolve(`${packageName}/package.json`);
    candidates.push(join(dirname(packageJsonPath), getNativeFilename()));
  } catch {
    // Platform packages are only expected after release packaging exists.
  }

  return candidates;
}

function getPackageDir(): string {
  let dir = dirname(fileURLToPath(import.meta.url));

  for (let i = 0; i < 5; i++) {
    const packageJson = join(dir, "package.json");
    if (existsSync(packageJson)) {
      try {
        const pkg = JSON.parse(readFileSync(packageJson, "utf8")) as { name?: string };
        if (pkg.name === "git-patch-native") return dir;
      } catch {
        // Keep walking upward.
      }
    }
    dir = dirname(dir);
  }

  return dirname(dirname(fileURLToPath(import.meta.url)));
}
