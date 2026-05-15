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
type Changes = Record<string, {
  before?: string | null; // omitted for additions
  after?: string | null;  // omitted for deletions
  moved?: string | { from: string; similarity?: number };
  mode?: string;          // defaults to 100644 for add/delete headers
}>;
```

Guarantees for the initial contract:

- deterministic path ordering
- git-style `diff --git`, `---`, `+++`, hunk, add/delete, and rename headers
- path separator normalization to `/`
- Git-compatible C-style quoting for paths containing quotes or control characters
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
```

The native package is currently local-build first. `scripts/copy-native.mjs` copies the Rust cdylib into `bin/*.node`, following the same platform-tag idea used by `.references/fff-package/packages/fff-node/package.json`.

## Reference architecture

The fff repo is vendored as a submodule at `.references/fff-package`. The package skeleton mirrors its useful Node-package decisions:

- ESM package with `dist/src/index.js` + generated declarations
- explicit `os` / `cpu` support metadata
- platform detection helpers
- local dev binary lookup under `bin`
- future-compatible platform package names for prebuilt artifacts

This project intentionally uses N-API instead of `ffi-rs` for the first native binding because N-API is the cleaner shared seam for Node and Bun.
