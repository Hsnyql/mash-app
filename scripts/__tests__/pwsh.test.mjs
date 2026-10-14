import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import test from "node:test";
import { pwshArgs, psLiteral } from "./helpers/pwsh.mjs";

const pwshAvailable = !spawnSync("pwsh", ["-Version"], { windowsHide: true }).error;

test("pwsh preserves Unicode script paths, arguments and working directories", {
  skip: process.platform !== "win32" && !pwshAvailable,
}, t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mash 中文 O'Brien "));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const file = path.join(root, "路径 ' probe.ps1");
  fs.writeFileSync(path.join(root, "标记.txt"), "fixture marker");
  fs.writeFileSync(file, `param([string]$Value)
@{
  value = $Value
  cwd = (Get-Location).Path
  script = $PSCommandPath
  marker = Get-Content -LiteralPath './标记.txt' -Raw -Encoding utf8
} | ConvertTo-Json -Compress
`);
  const value = "中文 アルトリア O'Brien $env:PATH `tick";
  const args = pwshArgs(`& ${psLiteral(file)} ${psLiteral(value)}`, root);
  // Neither the script path nor cwd crosses the native argv encoding boundary.
  assert.ok(args.every(arg => /^[\x00-\x7f]*$/.test(arg)));
  const output = execFileSync("pwsh", args, {
    cwd: os.tmpdir(), encoding: "utf8", windowsHide: true,
  });
  const actual = JSON.parse(output);
  assert.equal(actual.value, value);
  assert.equal(fs.realpathSync.native(actual.cwd), fs.realpathSync.native(root));
  assert.equal(fs.realpathSync.native(actual.script), fs.realpathSync.native(file));
  assert.equal(actual.marker, "fixture marker");
});
