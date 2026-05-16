import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { generatePatch, nativeBindingExists } from "../dist/src/index.js";

assert.equal(nativeBindingExists(), true);

const isWindows = process.platform === "win32";

function writeFixtureFile(root, path, content, mode) {
  const fullPath = join(root, path);
  mkdirSync(dirname(fullPath), { recursive: true });
  writeFileSync(fullPath, Buffer.from(content, "utf8"));
  if (mode && !isWindows) chmodSync(fullPath, Number.parseInt(mode, 8));
}

function readFixtureFile(root, path) {
  return readFileSync(join(root, path));
}

function git(root, args) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: "pipe" });
}

function generatedTextEditScenarios() {
  let seed = 0x5eed;
  const random = () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 2 ** 32;
  };
  const pick = (max) => Math.floor(random() * max);
  const makeText = (lineCount) => {
    const lines = Array.from({ length: lineCount }, (_, index) => `line-${index}-${pick(100)}\n`);
    if (lines.length > 0 && random() < 0.35) lines[lines.length - 1] = lines[lines.length - 1].trimEnd();
    return lines.join("");
  };
  const splitLines = (text) => text.match(/.*(?:\n|$)/g).filter((line) => line.length > 0);

  return Array.from({ length: 24 }, (_, index) => {
    const path = `generated/case-${index}.txt`;
    const before = makeText(1 + pick(14));
    const lines = splitLines(before);
    const op = index % 4;

    if (op === 0 && lines.length > 0) {
      lines[pick(lines.length)] = `replacement-${index}\n`;
    } else if (op === 1 && lines.length > 1) {
      lines.splice(pick(lines.length), 1);
    } else if (op === 2) {
      lines.splice(pick(lines.length + 1), 0, `inserted-${index}\n`);
    } else {
      lines.splice(0, lines.length, `whole-file-${index}\n`, `rewrite-${index}`);
    }

    const after = lines.join("");
    return {
      name: `generated text edit ${index}`,
      before: { [path]: before },
      changes: { [path]: { before, after } },
      after: { [path]: after },
      options: { contextLines: 1 + pick(4) },
    };
  });
}

function assertGitApplies({ name, before = {}, changes, after, options, beforeModes = {}, afterModes = {} }) {
  const root = mkdtempSync(join(tmpdir(), "git-patch-native-"));
  const patchPath = join(root, "generated.patch");

  try {
    git(root, ["init"]);
    git(root, ["config", "core.autocrlf", "false"]);
    git(root, ["config", "core.safecrlf", "false"]);
    git(root, ["config", "core.filemode", "true"]);

    for (const [path, content] of Object.entries(before)) {
      writeFixtureFile(root, path, content, beforeModes[path]);
    }

    const patch = generatePatch(changes, options);
    writeFileSync(patchPath, patch);

    git(root, ["apply", "--check", patchPath]);
    git(root, ["apply", patchPath]);

    for (const [path, content] of Object.entries(after)) {
      assert.equal(
        readFixtureFile(root, path).equals(Buffer.from(content, "utf8")),
        true,
        `${name}: ${path} bytes should match`,
      );
      if (afterModes[path] && !isWindows) {
        const actualMode = statSync(join(root, path)).mode & 0o777;
        const expectedMode = Number.parseInt(afterModes[path], 8);
        assert.equal(actualMode, expectedMode, `${name}: ${path} mode should match`);
      }
    }

    for (const path of Object.keys(before)) {
      if (!(path in after)) {
        assert.equal(existsSync(join(root, path)), false, `${name}: ${path} should be deleted`);
      }
    }

    return patch;
  } catch (error) {
    const patch = existsSync(patchPath) ? readFileSync(patchPath, "utf8") : "<not written>";
    throw new Error(`${name} failed\nPatch:\n${patch}\n${error.stack ?? error}`);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

const scenarios = [
  {
    name: "simple modify",
    before: { "src/main.ts": "const value = 1;\nconsole.log(value);\n" },
    changes: {
      "src/main.ts": {
        before: "const value = 1;\nconsole.log(value);\n",
        after: "const value = 2;\nconsole.log(value);\n",
      },
    },
    after: { "src/main.ts": "const value = 2;\nconsole.log(value);\n" },
  },
  {
    name: "change at start and end",
    before: { "file.txt": "one\ntwo\nthree\n" },
    changes: { "file.txt": { before: "one\ntwo\nthree\n", after: "ONE\ntwo\nTHREE\n" } },
    after: { "file.txt": "ONE\ntwo\nTHREE\n" },
  },
  {
    name: "two distant hunks with one context line",
    before: { "many.txt": "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n" },
    changes: {
      "many.txt": {
        before: "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n",
        after: "1\nTWO\n3\n4\n5\n6\n7\n8\nNINE\n10\n",
      },
    },
    options: { contextLines: 1 },
    after: { "many.txt": "1\nTWO\n3\n4\n5\n6\n7\n8\nNINE\n10\n" },
    inspect(patch) {
      assert.equal((patch.match(/^@@ /gm) ?? []).length, 2);
    },
  },
  {
    name: "add file",
    changes: { "src/added.ts": { after: "export const added = true;\n" } },
    after: { "src/added.ts": "export const added = true;\n" },
  },
  {
    name: "delete file",
    before: { "src/deleted.ts": "export const deleted = true;\n" },
    changes: { "src/deleted.ts": { before: "export const deleted = true;\n" } },
    after: {},
  },
  {
    name: "add empty file",
    changes: { "empty.txt": { after: "" } },
    after: { "empty.txt": "" },
  },
  {
    name: "delete empty file",
    before: { "empty.txt": "" },
    changes: { "empty.txt": { before: "" } },
    after: {},
  },
  {
    name: "empty to non-empty",
    before: { "empty.txt": "" },
    changes: { "empty.txt": { before: "", after: "now here\n" } },
    after: { "empty.txt": "now here\n" },
  },
  {
    name: "non-empty to empty",
    before: { "empty.txt": "gone\n" },
    changes: { "empty.txt": { before: "gone\n", after: "" } },
    after: { "empty.txt": "" },
  },
  {
    name: "rename only",
    before: { "src/old-name.ts": "export const name = 'same';\n" },
    changes: {
      "src/new-name.ts": {
        moved: "src/old-name.ts",
        before: "export const name = 'same';\n",
        after: "export const name = 'same';\n",
      },
    },
    after: { "src/new-name.ts": "export const name = 'same';\n" },
  },
  {
    name: "rename plus edit",
    before: { "src/old-name.ts": "export const name = 'old';\n" },
    changes: {
      "src/new-name.ts": {
        moved: { from: "src/old-name.ts", similarity: 92 },
        before: "export const name = 'old';\n",
        after: "export const name = 'new';\n",
      },
    },
    after: { "src/new-name.ts": "export const name = 'new';\n" },
  },
  {
    name: "paths with spaces unicode leading dash quotes and punctuation",
    changes: {
      "dir with space/file name.txt": { after: "space\n" },
      "unicodé/文件.txt": { after: "unicode\n" },
      "dash/-file.txt": { after: "dash\n" },
      "quote\"file.txt": { after: "quote\n" },
      "hash#[brackets].txt": { after: "punctuation\n" },
    },
    after: {
      "dir with space/file name.txt": "space\n",
      "unicodé/文件.txt": "unicode\n",
      "dash/-file.txt": "dash\n",
      "quote\"file.txt": "quote\n",
      "hash#[brackets].txt": "punctuation\n",
    },
    inspect(patch) {
      assert.match(patch, /diff --git "a\/quote\\"file\.txt" "b\/quote\\"file\.txt"/);
    },
  },
  {
    name: "backslash paths normalize to slash paths",
    changes: { "dir\\nested.txt": { after: "normalized\n" } },
    after: { "dir/nested.txt": "normalized\n" },
  },
  {
    name: "CRLF modification preserves bytes",
    before: { "crlf.txt": "a\r\nb\r\n" },
    changes: { "crlf.txt": { before: "a\r\nb\r\n", after: "a\r\nB\r\n" } },
    after: { "crlf.txt": "a\r\nB\r\n" },
  },
  {
    name: "LF to CRLF preserves bytes",
    before: { "line-endings.txt": "a\n" },
    changes: { "line-endings.txt": { before: "a\n", after: "a\r\n" } },
    after: { "line-endings.txt": "a\r\n" },
  },
  {
    name: "add executable file with scalar mode shorthand",
    changes: { "script.sh": { after: "#!/bin/sh\necho hi\n", mode: "100755" } },
    after: { "script.sh": "#!/bin/sh\necho hi\n" },
    afterModes: { "script.sh": "755" },
  },
  {
    name: "add executable file with mode.after",
    changes: { "bin/tool.sh": { after: "#!/bin/sh\necho tool\n", mode: { after: "100755" } } },
    after: { "bin/tool.sh": "#!/bin/sh\necho tool\n" },
    afterModes: { "bin/tool.sh": "755" },
  },
  {
    name: "delete executable file with mode.before",
    before: { "old-tool.sh": "#!/bin/sh\necho old\n" },
    beforeModes: { "old-tool.sh": "755" },
    changes: { "old-tool.sh": { before: "#!/bin/sh\necho old\n", mode: { before: "100755" } } },
    after: {},
  },
  {
    name: "chmod only to executable",
    before: { "script.sh": "#!/bin/sh\necho hi\n" },
    beforeModes: { "script.sh": "644" },
    changes: {
      "script.sh": {
        before: "#!/bin/sh\necho hi\n",
        after: "#!/bin/sh\necho hi\n",
        mode: { before: "100644", after: "100755" },
      },
    },
    after: { "script.sh": "#!/bin/sh\necho hi\n" },
    afterModes: { "script.sh": "755" },
    inspect(patch) {
      assert.equal(patch, "diff --git a/script.sh b/script.sh\nold mode 100644\nnew mode 100755\n");
    },
  },
  {
    name: "chmod only to regular",
    before: { "script.sh": "#!/bin/sh\necho hi\n" },
    beforeModes: { "script.sh": "755" },
    changes: {
      "script.sh": {
        before: "#!/bin/sh\necho hi\n",
        after: "#!/bin/sh\necho hi\n",
        mode: { before: "100755", after: "100644" },
      },
    },
    after: { "script.sh": "#!/bin/sh\necho hi\n" },
    afterModes: { "script.sh": "644" },
  },
  {
    name: "edit plus chmod",
    before: { "script.sh": "#!/bin/sh\necho hi\n" },
    beforeModes: { "script.sh": "644" },
    changes: {
      "script.sh": {
        before: "#!/bin/sh\necho hi\n",
        after: "#!/bin/sh\necho bye\n",
        mode: { before: "100644", after: "100755" },
      },
    },
    after: { "script.sh": "#!/bin/sh\necho bye\n" },
    afterModes: { "script.sh": "755" },
    inspect(patch) {
      assert.match(patch, /old mode 100644\nnew mode 100755\n--- a\/script\.sh\n\+\+\+ b\/script\.sh/);
    },
  },
  {
    name: "rename plus chmod",
    before: { "old.sh": "#!/bin/sh\necho same\n" },
    beforeModes: { "old.sh": "644" },
    changes: {
      "new.sh": {
        moved: "old.sh",
        before: "#!/bin/sh\necho same\n",
        after: "#!/bin/sh\necho same\n",
        mode: { before: "100644", after: "100755" },
      },
    },
    after: { "new.sh": "#!/bin/sh\necho same\n" },
    afterModes: { "new.sh": "755" },
    inspect(patch) {
      assert.match(patch, /old mode 100644\nnew mode 100755\nsimilarity index 100%\nrename from old\.sh\nrename to new\.sh/);
    },
  },
  {
    name: "rename edit plus chmod",
    before: { "old.sh": "#!/bin/sh\necho one\necho two\necho three\n" },
    beforeModes: { "old.sh": "644" },
    changes: {
      "new.sh": {
        moved: { from: "old.sh", similarity: 80 },
        before: "#!/bin/sh\necho one\necho two\necho three\n",
        after: "#!/bin/sh\necho one\necho TWO\necho three\n",
        mode: { before: "100644", after: "100755" },
      },
    },
    after: { "new.sh": "#!/bin/sh\necho one\necho TWO\necho three\n" },
    afterModes: { "new.sh": "755" },
  },
];

if (!isWindows) {
  scenarios.push({
    name: "quoted control-character paths apply",
    changes: {
      "tab\tfile.txt": { after: "tab\n" },
      "newline\nfile.txt": { after: "newline\n" },
      "carriage\rreturn.txt": { after: "cr\n" },
      "bell\u0007file.txt": { after: "bell\n" },
    },
    after: {
      "tab\tfile.txt": "tab\n",
      "newline\nfile.txt": "newline\n",
      "carriage\rreturn.txt": "cr\n",
      "bell\u0007file.txt": "bell\n",
    },
    inspect(patch) {
      assert.match(patch, /"a\/tab\\tfile\.txt"/);
      assert.match(patch, /"a\/newline\\nfile\.txt"/);
      assert.match(patch, /"a\/carriage\\rreturn\.txt"/);
      assert.match(patch, /"a\/bell\\007file\.txt"/);
    },
  });

  scenarios.push({
    name: "quoted rename paths apply",
    before: { "old\tname.txt": "same\n" },
    changes: {
      "new\nname.txt": { moved: "old\tname.txt", before: "same\n", after: "same\n" },
    },
    after: { "new\nname.txt": "same\n" },
    inspect(patch) {
      assert.match(patch, /rename from "old\\tname\.txt"\nrename to "new\\nname\.txt"/);
    },
  });
}

for (const scenario of scenarios) {
  const patch = assertGitApplies(scenario);
  scenario.inspect?.(patch);
}

for (const scenario of generatedTextEditScenarios()) {
  assertGitApplies(scenario);
}

for (const [name, before, after] of [
  ["final newline removed", "a\n", "a"],
  ["final newline added", "a", "a\n"],
  ["no final newline on both sides", "a", "b"],
  ["added file without final newline", undefined, "a"],
  ["deleted file without final newline", "a", undefined],
]) {
  const path = "newline.txt";
  const beforeFiles = before === undefined ? {} : { [path]: before };
  const change = {};
  if (before !== undefined) change.before = before;
  if (after !== undefined) change.after = after;
  const afterFiles = after === undefined ? {} : { [path]: after };
  const patch = assertGitApplies({ name, before: beforeFiles, changes: { [path]: change }, after: afterFiles });
  assert.match(patch, /\\ No newline at end of file/, `${name}: should emit final newline marker`);
}

const deterministicA = generatePatch({
  "z.txt": { after: "z\n" },
  "a.txt": { after: "a\n" },
  "m.txt": { after: "m\n" },
});
const deterministicB = generatePatch({
  "m.txt": { after: "m\n" },
  "z.txt": { after: "z\n" },
  "a.txt": { after: "a\n" },
});
assert.equal(deterministicA, deterministicB);
assert.deepEqual(
  [...deterministicA.matchAll(/^diff --git a\/(.*?) b\//gm)].map((match) => match[1]),
  ["a.txt", "m.txt", "z.txt"],
);

assert.equal(generatePatch({ "same.txt": { before: "same\n", after: "same\n" } }), "");

for (const [name, changes, message] of [
  ["missing before and after", { "bad.txt": {} }, /at least one/],
  ["NUL content", { "bad.txt": { after: "a\0b" } }, /text patches only/],
  ["NUL path", { "bad\0path.txt": { after: "x\n" } }, /path must not contain NUL/],
  ["absolute path", { "/bad.txt": { after: "x\n" } }, /absolute paths/],
  ["parent traversal", { "../bad.txt": { after: "x\n" } }, /path components/],
  ["duplicate normalized path", { "dir/file.txt": { after: "one\n" }, "dir\\file.txt": { after: "two\n" } }, /duplicate normalized path/],
  ["invalid similarity", { "new.txt": { moved: { from: "old.txt", similarity: 101 }, before: "x\n", after: "x\n" } }, /similarity/],
  ["invalid mode", { "bad.txt": { after: "x\n", mode: "100600" } }, /mode must/],
  ["scalar mode on modification", { "bad.txt": { before: "x\n", after: "y\n", mode: "100755" } }, /mode\.before and mode\.after/],
  ["add with mode.before", { "bad.txt": { after: "x\n", mode: { before: "100644" } } }, /mode\.before is not valid/],
  ["add with empty mode object", { "bad.txt": { after: "x\n", mode: {} } }, /mode\.after is required/],
  ["delete with mode.after", { "bad.txt": { before: "x\n", mode: { after: "100644" } } }, /mode\.after is not valid/],
  ["delete with empty mode object", { "bad.txt": { before: "x\n", mode: {} } }, /mode\.before is required/],
  ["modification with partial mode", { "bad.txt": { before: "x\n", after: "y\n", mode: { after: "100755" } } }, /require both mode\.before and mode\.after/],
  ["move without before", { "new.txt": { moved: "old.txt", after: "x\n" } }, /moved requires/],
]) {
  assert.throws(() => generatePatch(changes), message, name);
}

assert.throws(
  () => generatePatch({ "zero.txt": { before: "a\nb\n", after: "a\nB\n" } }, { contextLines: 0 }),
  /contextLines.*at least 1/,
);

console.log("e2e ok");
