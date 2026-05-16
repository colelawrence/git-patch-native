import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { currentPlatform } from "./platform-info.mjs";

const root = resolve(new URL("..", import.meta.url).pathname);
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const platform = currentPlatform();
const temp = mkdtempSync(join(tmpdir(), "git-patch-native-node-api-"));
const packDir = join(temp, "pack");
const consumerDir = join(temp, "consumer");

mkdirSync(packDir, { recursive: true });
mkdirSync(consumerDir, { recursive: true });

execFileSync(npm, ["run", "stage:platform-package"], { cwd: root, stdio: "inherit" });
const platformPack = npmPack(join(root, "npm", platform.packageName));
const rootPack = npmPack(root);

writeFileSync(join(consumerDir, "package.json"), JSON.stringify({ type: "module", private: true }));
execFileSync(npm, ["install", "--ignore-scripts", platformPack, rootPack], {
  cwd: consumerDir,
  stdio: "inherit",
});

const smoke = `
  import assert from "node:assert/strict";
  import { generatePatch, nativeBindingExists } from "git-patch-native";

  assert.equal(nativeBindingExists(), true);
  const patch = generatePatch({ "a.txt": { before: "one\\n", after: "two\\n" } });
  assert.match(patch, /diff --git a\\/a\\.txt b\\/a\\.txt/);
  assert.match(patch, /-one\\n\\+two/);
  console.log("node-api package smoke ok");
`;
writeFileSync(join(consumerDir, "smoke.mjs"), smoke);
execFileSync(process.execPath, ["smoke.mjs"], { cwd: consumerDir, stdio: "inherit" });

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
