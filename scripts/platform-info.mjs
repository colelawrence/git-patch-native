export const PLATFORMS = [
  {
    tag: "darwin-arm64",
    packageName: "git-patch-native-darwin-arm64",
    os: "darwin",
    cpu: "arm64",
    rustTarget: "aarch64-apple-darwin",
    ffiExtension: "dylib",
  },
  {
    tag: "darwin-x64",
    packageName: "git-patch-native-darwin-x64",
    os: "darwin",
    cpu: "x64",
    rustTarget: "x86_64-apple-darwin",
    ffiExtension: "dylib",
  },
  {
    tag: "linux-x64-gnu",
    packageName: "git-patch-native-linux-x64-gnu",
    os: "linux",
    cpu: "x64",
    libc: "glibc",
    rustTarget: "x86_64-unknown-linux-gnu",
    ffiExtension: "so",
  },
  {
    tag: "linux-arm64-gnu",
    packageName: "git-patch-native-linux-arm64-gnu",
    os: "linux",
    cpu: "arm64",
    libc: "glibc",
    rustTarget: "aarch64-unknown-linux-gnu",
    ffiExtension: "so",
  },
  {
    tag: "win32-x64",
    packageName: "git-patch-native-win32-x64",
    os: "win32",
    cpu: "x64",
    rustTarget: "x86_64-pc-windows-msvc",
    ffiExtension: "dll",
  },
  {
    tag: "win32-arm64",
    packageName: "git-patch-native-win32-arm64",
    os: "win32",
    cpu: "arm64",
    rustTarget: "aarch64-pc-windows-msvc",
    ffiExtension: "dll",
  },
];

export function platformFromTag(tag) {
  const platform = PLATFORMS.find((entry) => entry.tag === tag);
  if (!platform) throw new Error(`Unknown platform tag: ${tag}`);
  return platform;
}

export function currentPlatform() {
  return platformFromTag(currentPlatformTag());
}

export function currentPlatformTag() {
  if (process.env.GIT_PATCH_PLATFORM_TAG) return process.env.GIT_PATCH_PLATFORM_TAG;

  if (process.platform === "darwin") return process.arch === "arm64" ? "darwin-arm64" : "darwin-x64";
  if (process.platform === "win32") return process.arch === "arm64" ? "win32-arm64" : "win32-x64";
  if (process.platform === "linux") {
    const arch = process.arch === "arm64" ? "arm64" : "x64";
    return `linux-${arch}-${isMusl() ? "musl" : "gnu"}`;
  }

  throw new Error(`Unsupported platform: ${process.platform}`);
}

export function nativeNodeFilename(platform = currentPlatform()) {
  return `git_patch_native.${platform.tag}.node`;
}

export function nativeFfiFilename(platform = currentPlatform()) {
  return `git_patch_ffi.${platform.tag}.${platform.ffiExtension}`;
}

export function sourceLibraryFilename(libraryName, platform = currentPlatform()) {
  if (platform.os === "win32") return `${libraryName}.dll`;
  if (platform.os === "darwin") return `lib${libraryName}.dylib`;
  return `lib${libraryName}.so`;
}

function isMusl() {
  try {
    return process.report?.getReport?.().header?.glibcVersionRuntime === undefined;
  } catch {
    return false;
  }
}
