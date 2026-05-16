# git-patch-native

Rust-backed Node/Bun-oriented package for generating git-style unified patch strings from in-memory file changes.

```ts
import { generatePatch } from "git-patch-native";

const patch = generatePatch({
  "src/main.ts": {
    before: "const value = 1;\n",
    after: "const value = 2;\n",
  },
  "src/added.ts": {
    after: "export const added = true;\n",
  },
  "src/new-name.ts": {
    moved: "src/old-name.ts",
    before: "same\n",
    after: "same\n",
  },
});
```

## API shape

`generatePatch(changes, options?)` accepts a record whose key is the new path, except deletions where the key is the deleted path.

```ts
type GitFileMode = "100644" | "100755";

type Changes = Record<string, {
  before?: string | null; // omitted for additions
  after?: string | null;  // omitted for deletions
  moved?: string | { from: string; similarity?: number };
  mode?: GitFileMode | { before?: GitFileMode | null; after?: GitFileMode | null };
}>;
```

Guarantees for the initial contract:

- deterministic path ordering
- git-style `diff --git`, `---`, `+++`, hunk, add/delete, and rename headers
- path separator normalization to `/`
- Git-compatible C-style quoting for paths containing quotes or control characters
- `new file mode`, `deleted file mode`, `old mode`, and `new mode` headers for `100644`/`100755` mode metadata
- NUL paths and NUL content are rejected
- final-newline markers when needed
- `contextLines >= 1` so output applies with default `git apply`
- text-only input: NUL-containing content is rejected
- all diff formatting owned by Rust core; JS only serializes inputs and loads the native binding

## Development

```sh
npm install
npm run build
npm test
cargo test
npm run smoke:pack
```

The native package is currently local-build first. `scripts/copy-native.mjs` copies the Rust Node-API cdylib into `bin/*.node` and the Bun FFI cdylib into `bin/*.{dylib,so,dll}`, following the same platform-tag idea used by `.references/fff-package/packages/fff-node/package.json`.

`npm run smoke:pack` verifies publish shape by packing the package, installing the tarball into clean temporary consumers, then proving both:

- Node can import the package and load the Node-API addon.
- Bun can `dlopen` the packaged FFI library through `bun:ffi`.

The public SDK entrypoint remains `generatePatch`; the FFI surface is a low-level packaging/runtime artifact.

## Publishing

This package uses a two-tier npm publish shape:

- `git-patch-native` publishes the JS/TypeScript SDK once.
- `git-patch-native-<platform>` packages publish native Node-API and Bun FFI artifacts from a GitHub Actions matrix.

Release tags are `v<package.json version>`, for example `v0.1.0`. The release workflow publishes with npm provenance:

```sh
git tag v0.1.0
git push origin v0.1.0
```

Before the first release, configure npm Trusted Publishing for the main package and each platform package. Use repository `colelawrence/git-patch-native` and workflow `.github/workflows/release.yml`.

After publishing, verify package signatures/provenance metadata with npm:

```sh
npm view git-patch-native@0.1.0 dist.integrity dist.signatures
mkdir /tmp/git-patch-native-verify && cd /tmp/git-patch-native-verify
npm init -y
npm install git-patch-native@0.1.0
npm audit signatures
```

The npm package page should also show provenance for packages published by the release workflow.

## Reference architecture

The fff repo is vendored as a submodule at `.references/fff-package`. The package skeleton mirrors its useful Node-package decisions:

- ESM package with `dist/src/index.js` + generated declarations
- explicit `os` / `cpu` support metadata
- platform detection helpers
- local dev binary lookup under `bin`
- future-compatible platform package names for prebuilt artifacts

This project intentionally uses N-API instead of `ffi-rs` for the first native binding because N-API is the cleaner shared seam for Node and Bun.
