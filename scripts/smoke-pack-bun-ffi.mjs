import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { currentPlatform, nativeFfiFilename } from "./platform-info.mjs";

const root = resolve(new URL("..", import.meta.url).pathname);
const bun = process.platform === "win32" ? "bun.exe" : "bun";
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const platform = currentPlatform();
const temp = mkdtempSync(join(tmpdir(), "git-patch-native-bun-ffi-"));
const packDir = join(temp, "pack");
const consumerDir = join(temp, "consumer");

mkdirSync(packDir, { recursive: true });
mkdirSync(consumerDir, { recursive: true });

execFileSync(npm, ["run", "stage:platform-package"], { cwd: root, stdio: "inherit" });
const platformPack = npmPack(join(root, "npm", platform.packageName));
const rootPack = npmPack(root);

writeFileSync(join(consumerDir, "package.json"), JSON.stringify({ type: "module", private: true }));
execFileSync(bun, ["install", platformPack, rootPack], { cwd: consumerDir, stdio: "inherit" });

const smoke = `
  import assert from "node:assert/strict";
  import { CString, dlopen, FFIType } from "bun:ffi";
  import { existsSync } from "node:fs";
  import { dirname, join } from "node:path";

  const packageJson = Bun.resolveSync("${platform.packageName}/package.json", import.meta.dir);
  const libPath = join(dirname(packageJson), "${nativeFfiFilename(platform)}");
  assert.equal(existsSync(libPath), true, libPath);

  const lib = dlopen(libPath, {
    git_patch_generate_patch_json_result: {
      args: [FFIType.cstring],
      returns: FFIType.ptr,
    },
    git_patch_free_string: {
      args: [FFIType.ptr],
      returns: FFIType.void,
    },
  });

  const input = new TextEncoder().encode(JSON.stringify({
    changes: { "a.txt": { before: "one\\n", after: "two\\n" } },
  }) + "\\0");
  const resultPtr = lib.symbols.git_patch_generate_patch_json_result(input);
  assert.notEqual(resultPtr, null);

  try {
    const result = JSON.parse(new CString(resultPtr).toString());
    assert.equal(result.ok, true, result.error);
    assert.match(result.value, /diff --git a\\/a\\.txt b\\/a\\.txt/);
    assert.match(result.value, /-one\\n\\+two/);
  } finally {
    lib.symbols.git_patch_free_string(resultPtr);
  }

  console.log("bun-ffi package smoke ok");
`;
writeFileSync(join(consumerDir, "smoke.mjs"), smoke);
execFileSync(bun, ["smoke.mjs"], { cwd: consumerDir, stdio: "inherit" });

function npmPack(cwd) {
  const packOutput = execFileSync(npm, ["pack", "--json", "--pack-destination", packDir], {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
  });
  const jsonStart = packOutput.lastIndexOf("\n[");
  const [pack] = JSON.parse(packOutput.slice(jsonStart >= 0 ? jsonStart + 1 : packOutput.indexOf("[")));
  return join(packDir, pack.filename);
}
