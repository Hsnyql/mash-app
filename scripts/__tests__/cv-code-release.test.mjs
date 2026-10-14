import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import test from "node:test";
import { applyRelease, digest, downloadDigest, location, planRelease, platforms,
  publishImmutable, recordRelease, tagRelease, validateReceipt, validateReports } from "../shared/cv-code-release.mjs";

function fixture(t) {
  const previous = process.cwd();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "cv code 中文 "));
  process.chdir(root);
  t.after(() => { process.chdir(previous); fs.rmSync(root, { recursive: true, force: true }); });
  const write = (file, text) => { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, text); };
  const git = (...args) => execFileSync("git", args, { encoding: "utf8", stdio: "pipe", windowsHide: true }).trim();
  const manifest = { mashCvCodeVersion: "1.0.0", platforms: Object.fromEntries(platforms.map(platform => [platform, {
    runtimeVersion: "0.1.0", runtimeUrl: `https://example.invalid/${platform}.zip`, runtimeSha256: "a".repeat(64),
    codeUrl: "https://example.invalid/old.zip", codeSha256: "b".repeat(64),
  }])) };
  write("src-tauri/resources/runtime-manifest.json", JSON.stringify(manifest));
  write("versions.toml", '[app]\nversion = "1.0.0"\n[mash_cv]\ncode = "1.0.0"\n[mash_cv.runtime]\nwindows-x86_64 = "0.1.0"\ndarwin-aarch64 = "0.1.0"\n');
  write("sidecar/mash_cv/mash_cv/__init__.py", "# code\n");
  git("init", "--quiet");
  git("config", "user.name", "Release Test");
  git("config", "user.email", "test@example.invalid");
  git("config", "commit.gpgsign", "false");
  git("config", "tag.gpgsign", "false");
  const commit = () => { git("add", "."); git("commit", "--quiet", "-m", "fixture"); };
  commit();
  git("update-ref", "refs/remotes/origin/main", "HEAD");
  const plan = () => planRelease("1.1.0", "HEAD", "main", "https://example.invalid", "mash");
  const candidate = () => {
    write("dist/code.zip", "candidate bytes");
    return recordRelease(plan(), "dist/code.zip", "candidate");
  };
  return { root, write, git, commit, plan, candidate, manifest };
}

test("plan pins reviewed source and runtime hashes without changing the worktree", t => {
  const f = fixture(t);
  const before = f.git("status", "--porcelain");
  const plan = f.plan();
  assert.equal(plan.sourceSha, f.git("rev-parse", "HEAD"));
  assert.deepEqual(Object.keys(plan.runtimes), platforms);
  assert.equal(plan.url, "https://example.invalid/mash/runtime/mash-cv/code/mash-cv-code-v1.1.0.zip");
  assert.equal(f.git("status", "--porcelain"), before);
  assert.throws(() => planRelease("1.0.0", "HEAD", "main", "https://example.invalid"), /must increase/);
  f.write("unreviewed.txt", "change");
  f.commit();
  assert.throws(() => f.plan(), /merge-base/);
});

test("plan refuses conflicting source tags and unsafe destinations", t => {
  const f = fixture(t);
  f.git("tag", "cv-code/1.1.0");
  assert.equal(f.plan().version, "1.1.0");
  f.write("reviewed.txt", "new source");
  f.commit();
  f.git("update-ref", "refs/remotes/origin/main", "HEAD");
  assert.throws(() => f.plan(), /different source/);
  for (const prefix of ["../mash", "/mash", "mash//code", "mash;echo"]) {
    assert.throws(() => location("1.1.0", "https://example.invalid", prefix), /prefix/);
  }
  assert.throws(() => location("1.1.0", "http://example.invalid"), /HTTPS/);
  assert.throws(() => location("1.1.0", "https://secret@example.invalid"), /HTTPS/);
});

test("candidate records the actual bytes and refuses a mismatched build checkout", t => {
  const f = fixture(t);
  const receipt = f.candidate();
  assert.equal(receipt.sha256, digest("candidate/mash-cv-code-v1.1.0.zip"));
  assert.equal(receipt.size, Buffer.byteLength("candidate bytes"));
  assert.deepEqual(JSON.parse(fs.readFileSync("candidate/release.json")), receipt);
  assert.throws(() => recordRelease({ ...receipt, sourceSha: "0".repeat(40) }, "dist/code.zip", "bad"), /checkout/);
  assert.throws(() => validateReceipt({ ...receipt, artifact: "../bad.zip" }), /record/);
});

test("both compatibility reports must match this code and its pinned runtimes", t => {
  const f = fixture(t);
  const receipt = f.candidate();
  assert.throws(() => validateReports(receipt, "reports"), /ENOENT/);
  for (const platform of platforms) f.write(`reports/${platform}.json`, JSON.stringify({ platform,
    status: "passed", sourceSha: receipt.sourceSha, codeSha256: receipt.sha256,
    runtimeSha256: receipt.runtimes[platform].runtimeSha256 }));
  validateReports(receipt, "reports");
  const report = JSON.parse(fs.readFileSync("reports/windows-x86_64.json"));
  f.write("reports/windows-x86_64.json", JSON.stringify({ ...report, codeSha256: "0".repeat(64) }));
  assert.throws(() => validateReports(receipt, "reports"), /mismatched/);
});

test("manifest application updates shared code, preserves every runtime and is retryable", t => {
  const f = fixture(t);
  const receipt = f.candidate();
  applyRelease(receipt);
  const manifest = JSON.parse(fs.readFileSync("src-tauri/resources/runtime-manifest.json"));
  assert.equal(manifest.mashCvCodeVersion, "1.1.0");
  for (const platform of platforms) assert.deepEqual(manifest.platforms[platform], {
    ...f.manifest.platforms[platform], codeUrl: receipt.url, codeSha256: receipt.sha256,
  });
  assert.match(fs.readFileSync("versions.toml", "utf8"), /code = "1.1.0"/);
  assert.doesNotThrow(() => applyRelease(receipt));
  assert.throws(() => applyRelease({ ...receipt, sha256: "c".repeat(64) }), /another artifact/);
});

test("manifest application rejects changed source, changed runtime and downgrades", t => {
  const f = fixture(t);
  const receipt = f.candidate();
  f.write("sidecar/mash_cv/mash_cv/__init__.py", "# newer code\n");
  f.commit();
  assert.throws(() => applyRelease(receipt), /source changed/);
  f.git("checkout", receipt.sourceSha);
  const changed = structuredClone(f.manifest);
  changed.platforms["windows-x86_64"].runtimeSha256 = "c".repeat(64);
  f.write("src-tauri/resources/runtime-manifest.json", JSON.stringify(changed));
  assert.throws(() => applyRelease(receipt), /Runtime changed/);
  f.write("src-tauri/resources/runtime-manifest.json", JSON.stringify(f.manifest));
  assert.throws(() => applyRelease({ ...receipt, version: "0.9.0", ...location("0.9.0", "https://example.invalid") }), /stale/);
});

function fakeStore(file) {
  const objects = new Map();
  const calls = [];
  const aws = args => {
    calls.push([...args]);
    const key = args[args.indexOf("--key") + 1];
    if (args[1] === "put-object") {
      assert.equal(args[args.indexOf("--if-none-match") + 1], "*");
      if (objects.has(key)) throw new Error("An error occurred (412): PreconditionFailed");
      objects.set(key, fs.readFileSync(args[args.indexOf("--body") + 1]));
    } else fs.writeFileSync(args.at(-1), objects.get(key));
  };
  return { objects, calls, options: { aws, remoteDigest: async () => digest(file) } };
}

test("conditional upload reuses identical objects and refuses conflicting bytes", async t => {
  const f = fixture(t);
  f.write("code.zip", "tested ZIP");
  const store = fakeStore("code.zip");
  await publishImmutable("code.zip", "code.zip", "https://example.invalid/code.zip", store.options);
  await publishImmutable("code.zip", "code.zip", "https://example.invalid/code.zip", store.options);
  assert.equal(store.calls.filter(args => args[1] === "get-object").length, 1);
  store.objects.set("code.zip", Buffer.from("another release"));
  await assert.rejects(publishImmutable("code.zip", "code.zip", "https://example.invalid/code.zip", store.options), /differs/);
  assert.equal(store.objects.get("code.zip").toString(), "another release");
});

test("CDN failure can retry the original artifact without overwriting it", async t => {
  const f = fixture(t);
  f.write("code.zip", "tested ZIP");
  const store = fakeStore("code.zip");
  await assert.rejects(publishImmutable("code.zip", "code.zip", "https://example.invalid/code.zip", {
    ...store.options, remoteDigest: async () => "wrong",
  }), /CDN SHA-256/);
  await publishImmutable("code.zip", "code.zip", "https://example.invalid/code.zip", store.options);
  await assert.rejects(publishImmutable("code.zip", "new.zip", "https://example.invalid/new.zip", {
    ...store.options, aws: () => { throw new Error("AccessDenied"); },
  }), /AccessDenied/);
});

test("CDN verification hashes the complete body and propagates HTTP failures", async t => {
  const f = fixture(t);
  f.write("code.zip", "full response");
  assert.equal(await downloadDigest("https://example.invalid/code.zip", async () => new Response("full response")), digest("code.zip"));
  await assert.rejects(downloadDigest("https://example.invalid/code.zip", async () => new Response("", { status: 404 })), /404/);
});

test("tag creation is retryable and never moves an existing tag", t => {
  const f = fixture(t);
  const receipt = f.candidate();
  const remote = path.join(f.root, "remote.git");
  f.git("init", "--bare", "--quiet", remote);
  f.git("remote", "add", "origin", remote);
  tagRelease(receipt);
  tagRelease(receipt);
  assert.equal(f.git("rev-parse", "cv-code/1.1.0^{commit}"), receipt.sourceSha);
  f.write("new.txt", "new source");
  f.commit();
  assert.throws(() => tagRelease({ ...receipt, sourceSha: f.git("rev-parse", "HEAD") }), /different source/);
});
