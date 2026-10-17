import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

function fixture(t) {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "mash-linux-build-")));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const directory of ["bin", "scripts/linux", "sidecar/cv-code", "sidecar/cv-runtime"]) {
    fs.mkdirSync(path.join(root, directory), { recursive: true });
  }
  const script = path.join(root, "scripts/linux/build-cv.sh");
  const calls = path.join(root, "calls.log");
  fs.copyFileSync(new URL("../linux/build-cv.sh", import.meta.url), script);
  for (const command of ["poetry", "python3"]) {
    fs.writeFileSync(path.join(root, "bin", command),
      '#!/usr/bin/env bash\nprintf \'%s|%s|%s\\n\' "${0##*/}" "$PWD" "$*" >> "$MASH_TEST_CALLS"\n',
      { mode: 0o755 });
  }
  const run = args => spawnSync("bash", [script, ...args], {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, PATH: `${path.join(root, "bin")}${path.delimiter}${process.env.PATH}`,
      MASH_TEST_CALLS: calls },
  });
  const readCalls = () => fs.existsSync(calls) ? fs.readFileSync(calls, "utf8").trim().split("\n") : [];
  return { root, run, readCalls };
}

for (const mode of ["--all", "--runtime-only", "--code-only"]) {
  test(`Linux CV build ${mode} uses the split runtime and code directories`,
    { skip: process.platform === "win32" }, t => {
      const f = fixture(t);
      const result = f.run([mode]);
      assert.ifError(result.error);
      assert.equal(result.status, 0, result.stderr);
      const cwd = fs.realpathSync(path.join(f.root, "sidecar/cv-runtime"));
      const expected = [];
      if (mode !== "--code-only") {
        expected.push(`poetry|${cwd}|install --no-root`, `poetry|${cwd}|run python build_runtime.py`);
      }
      if (mode !== "--runtime-only") {
        expected.push(`python3|${cwd}|../cv-code/build_code.py`);
      }
      assert.deepEqual(f.readCalls(), expected);
    });
}

test("Linux CV build defaults to both artifacts and rejects invalid modes before building",
  { skip: process.platform === "win32" }, t => {
    const f = fixture(t);
    for (const args of [["--unknown"], ["--all", "extra"]]) {
      const result = f.run(args);
      assert.ifError(result.error);
      assert.equal(result.status, 2, result.stderr);
      assert.deepEqual(f.readCalls(), []);
    }
    const result = f.run([]);
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(f.readCalls().length, 3);
  });
