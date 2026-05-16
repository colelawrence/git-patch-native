import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { currentPlatform, nativeFfiFilename, nativeNodeFilename } from "./platform-info.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const platform = currentPlatform();
const packageDir = join(root, "npm", platform.packageName);
const binDir = join(root, "bin");
const rootVersion = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;

if (!existsSync(packageDir)) {
  throw new Error(`Platform package directory not found: ${packageDir}`);
}

const packageJsonPath = join(packageDir, "package.json");
const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"));
packageJson.version = rootVersion;
writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`);

stage(nativeNodeFilename(platform));
stage(nativeFfiFilename(platform));
console.log(`Staged ${platform.packageName}@${rootVersion}`);

function stage(filename) {
  const source = join(binDir, filename);
  const destination = join(packageDir, filename);
  if (!existsSync(source)) {
    throw new Error(`Native artifact not found: ${source}`);
  }
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  console.log(`Copied ${source} -> ${destination}`);
}
