import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";
import { PLATFORMS } from "./platform-info.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const stage = readArg("--stage") ?? "all";
const rootPackage = readJson("package.json");
const version = rootPackage.version;
const expectedPackages = PLATFORMS.map((platform) => platform.packageName).sort();
const failures = [];

if (!["platforms", "all"].includes(stage)) {
  fail(`Unknown --stage=${stage}; expected platforms or all`);
}

checkRootManifest();
checkPlatformPackageDirs();
checkLockfile();
checkReleaseMatrix();
checkRootPack();
await checkPublishedPlatforms();
if (stage === "all") await checkPublishedRoot();

if (failures.length) {
  console.error(`release postcheck failed with ${failures.length} issue(s):`);
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`release postcheck ok (${stage}, ${version})`);

function checkRootManifest() {
  if (rootPackage.name !== "git-patch-native") fail(`Unexpected root package name: ${rootPackage.name}`);
  expectExactSet(Object.keys(rootPackage.optionalDependencies ?? {}).sort(), expectedPackages, "root optionalDependencies");

  for (const packageName of expectedPackages) {
    const actual = rootPackage.optionalDependencies?.[packageName];
    if (actual !== version) fail(`root optional dependency ${packageName} is ${actual}, expected ${version}`);
  }
}

function checkPlatformPackageDirs() {
  const npmDir = join(root, "npm");
  const dirs = readdirSync(npmDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();

  expectExactSet(dirs, expectedPackages, "npm platform package dirs");

  for (const platform of PLATFORMS) {
    const packageJson = readJson(join("npm", platform.packageName, "package.json"));
    if (packageJson.name !== platform.packageName) fail(`${platform.packageName}: package name mismatch`);
    if (packageJson.version !== version) fail(`${platform.packageName}: version ${packageJson.version}, expected ${version}`);
    expectArray(packageJson.os, [platform.os], `${platform.packageName}.os`);
    expectArray(packageJson.cpu, [platform.cpu], `${platform.packageName}.cpu`);
    if (platform.libc) expectArray(packageJson.libc, [platform.libc], `${platform.packageName}.libc`);

    const expectedFiles = [
      `git_patch_native.${platform.tag}.node`,
      `git_patch_ffi.${platform.tag}.${platform.ffiExtension}`,
    ].sort();
    expectArray((packageJson.files ?? []).sort(), expectedFiles, `${platform.packageName}.files`);
  }
}

function checkLockfile() {
  const lock = readJson("package-lock.json");
  if (lock.name !== rootPackage.name) fail(`lockfile root name ${lock.name}, expected ${rootPackage.name}`);
  if (lock.version !== version) fail(`lockfile root version ${lock.version}, expected ${version}`);

  const rootLock = lock.packages?.[""];
  expectExactSet(Object.keys(rootLock?.optionalDependencies ?? {}).sort(), expectedPackages, "lockfile root optionalDependencies");

  const platformLockEntries = Object.keys(lock.packages ?? {})
    .filter((key) => key.startsWith("node_modules/git-patch-native-") && key !== "node_modules/git-patch-native")
    .map((key) => key.replace("node_modules/", ""))
    .sort();
  const extraLockEntries = platformLockEntries.filter((packageName) => !expectedPackages.includes(packageName));
  if (extraLockEntries.length) fail(`lockfile has stale platform entries: ${extraLockEntries.join(", ")}`);

  for (const platform of PLATFORMS) {
    const packageName = platform.packageName;
    const entry = lock.packages?.[`node_modules/${packageName}`];
    if (!entry?.version) continue;
    if (entry.version !== version) fail(`lockfile ${packageName} version ${entry.version}, expected ${version}`);
    if (entry.optional !== true) fail(`lockfile ${packageName} must be optional`);
    expectArray(entry.os, [platform.os], `lockfile ${packageName}.os`);
    expectArray(entry.cpu, [platform.cpu], `lockfile ${packageName}.cpu`);
    if (platform.libc) expectArray(entry.libc, [platform.libc], `lockfile ${packageName}.libc`);
  }
}

function checkReleaseMatrix() {
  const workflow = readFileSync(join(root, ".github", "workflows", "release.yml"), "utf8");
  const publishBlock = workflow.slice(workflow.indexOf("  publish-platform:"), workflow.indexOf("  publish-main:"));
  const matrixPackages = [...publishBlock.matchAll(/^\s+package:\s+(\S+)\s*$/gm)].map((match) => match[1]).sort();
  expectExactSet(matrixPackages, expectedPackages, "release publish-platform matrix packages");
}

function checkRootPack() {
  const output = execFileSync("npm", ["pack", "--dry-run", "--ignore-scripts", "--json"], {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
  });
  const [pack] = JSON.parse(output);
  const forbidden = pack.files
    .map((file) => file.path)
    .filter((path) => path.startsWith("bin/") || path.startsWith("npm/") || /\.(node|so|dll|dylib)$/.test(path));
  if (forbidden.length) fail(`root tarball includes native/platform payload: ${forbidden.join(", ")}`);
}

async function checkPublishedPlatforms() {
  for (const platform of PLATFORMS) {
    const metadata = await npmView(`${platform.packageName}@${version}`);
    checkPublishedMetadata(metadata, platform.packageName);
    expectArray(metadata.os, [platform.os], `published ${platform.packageName}.os`);
    expectArray(metadata.cpu, [platform.cpu], `published ${platform.packageName}.cpu`);
    if (platform.libc) expectArray(metadata.libc, [platform.libc], `published ${platform.packageName}.libc`);
  }
}

async function checkPublishedRoot() {
  const metadata = await npmView(`${rootPackage.name}@${version}`);
  checkPublishedMetadata(metadata, rootPackage.name);
  expectExactSet(Object.keys(metadata.optionalDependencies ?? {}).sort(), expectedPackages, "published root optionalDependencies");
}

function checkPublishedMetadata(metadata, packageName) {
  if (metadata.name !== packageName) fail(`published ${packageName}: name mismatch ${metadata.name}`);
  if (metadata.version !== version) fail(`published ${packageName}: version ${metadata.version}, expected ${version}`);
  if (!metadata.dist?.integrity) fail(`published ${packageName}: missing dist.integrity`);
  if (!Array.isArray(metadata.dist?.signatures) || metadata.dist.signatures.length === 0) {
    fail(`published ${packageName}: missing dist.signatures`);
  }
}

async function npmView(spec) {
  const attempts = Number(process.env.RELEASE_POSTCHECK_ATTEMPTS ?? 24);
  const delayMs = Number(process.env.RELEASE_POSTCHECK_DELAY_MS ?? 5000);
  let lastError = "";

  for (let attempt = 1; attempt <= attempts; attempt++) {
    try {
      return JSON.parse(execFileSync("npm", ["view", spec, "--json"], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }));
    } catch (error) {
      lastError = error.stderr?.toString?.().trim() || error.message;
      if (attempt < attempts) await sleep(delayMs);
    }
  }

  fail(`npm view ${spec} failed after ${attempts} attempts: ${lastError}`);
  return undefined;
}

function readJson(path) {
  return JSON.parse(readFileSync(join(root, path), "utf8"));
}

function expectArray(actual, expected, label) {
  if (JSON.stringify(actual ?? []) !== JSON.stringify(expected)) {
    fail(`${label} is ${JSON.stringify(actual ?? [])}, expected ${JSON.stringify(expected)}`);
  }
}

function expectExactSet(actual, expected, label) {
  const missing = expected.filter((value) => !actual.includes(value));
  const extra = actual.filter((value) => !expected.includes(value));
  if (missing.length || extra.length) {
    fail(`${label} mismatch${missing.length ? `; missing: ${missing.join(", ")}` : ""}${extra.length ? `; extra: ${extra.join(", ")}` : ""}`);
  }
}

function readArg(name) {
  const prefix = `${name}=`;
  const found = process.argv.slice(2).find((arg) => arg.startsWith(prefix));
  return found?.slice(prefix.length);
}

function fail(message) {
  failures.push(message);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
