import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { PLATFORMS } from "./platform-info.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const version = packageJson.version;
const tag = `v${version}`;
const repoUrl = repositoryUrl();
const npmUrl = (packageName) => `https://www.npmjs.com/package/${packageName}/v/${version}`;

const previousTag = findPreviousTag(tag);
const compareUrl = previousTag ? `${repoUrl}/compare/${previousTag}...${tag}` : undefined;

const lines = [
  `## git-patch-native ${version}`,
  "",
  "Rust-backed Node/Bun package for generating Git-compatible patch strings from in-memory change records.",
  "",
  "### npm packages",
  "",
  `- Root SDK: [\`${packageJson.name}@${version}\`](${npmUrl(packageJson.name)})`,
  ...PLATFORMS.map((platform) => `- ${platform.tag}: [\`${platform.packageName}@${version}\`](${npmUrl(platform.packageName)})`),
  "",
  "### Install",
  "",
  "```sh",
  `npm install ${packageJson.name}@${version}`,
  "```",
  "",
  "The root package installs the matching optional native package for supported platforms.",
  "",
  "### Supported prebuilt platforms",
  "",
  "| Platform | npm package |",
  "| --- | --- |",
  ...PLATFORMS.map((platform) => `| ${platform.tag} | [\`${platform.packageName}\`](${npmUrl(platform.packageName)}) |`),
  "",
  "Not currently published: Intel macOS (`darwin-x64`) and musl Linux builds.",
  "",
  "### Verification",
  "",
  "Published by GitHub Actions with npm Trusted Publishing / provenance. The release workflow verifies npm registry visibility, package signatures, and root-package tarball shape before creating this GitHub Release.",
  "",
  compareUrl ? `Changes since ${previousTag}: ${compareUrl}` : "Initial published release.",
  "",
];

process.stdout.write(lines.join("\n"));

function repositoryUrl() {
  if (process.env.GITHUB_REPOSITORY) return `https://github.com/${process.env.GITHUB_REPOSITORY}`;

  const raw = packageJson.repository?.url ?? "";
  const match = raw.match(/^git\+https:\/\/github\.com\/(.+?)\.git$/);
  return match ? `https://github.com/${match[1]}` : "https://github.com/colelawrence/git-patch-native";
}

function findPreviousTag(currentTag) {
  try {
    const tags = execFileSync("git", ["tag", "--list", "v*.*.*", "--sort=version:refname"], {
      cwd: root,
      encoding: "utf8",
    })
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    const index = tags.indexOf(currentTag);
    return index > 0 ? tags[index - 1] : undefined;
  } catch {
    return undefined;
  }
}
