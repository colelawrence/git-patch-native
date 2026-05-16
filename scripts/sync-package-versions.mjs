import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";
import { PLATFORMS } from "./platform-info.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const rootPackagePath = join(root, "package.json");
const rootPackage = JSON.parse(readFileSync(rootPackagePath, "utf8"));

for (const platform of PLATFORMS) {
  rootPackage.optionalDependencies[platform.packageName] = rootPackage.version;

  const packagePath = join(root, "npm", platform.packageName, "package.json");
  const packageJson = JSON.parse(readFileSync(packagePath, "utf8"));
  packageJson.version = rootPackage.version;
  writeFileSync(packagePath, `${JSON.stringify(packageJson, null, 2)}\n`);
}

writeFileSync(rootPackagePath, `${JSON.stringify(rootPackage, null, 2)}\n`);
