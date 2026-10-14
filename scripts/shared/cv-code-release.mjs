import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { applyCvArtifact, readVersions, updateVersion, validateVersion } from "./release-state.mjs";
import { compareVersions } from "./updater-manifest.mjs";

export const platforms = ["windows-x86_64", "darwin-aarch64"];
const manifestPath = "src-tauri/resources/runtime-manifest.json";
const json = file => JSON.parse(fs.readFileSync(file, "utf8"));
const write = (file, value) => fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
const run = (command, args) => execFileSync(command, args, { encoding: "utf8", windowsHide: true, stdio: "pipe" }).trim();
const git = (...args) => run("git", args);
export const digest = file => createHash("sha256").update(fs.readFileSync(file)).digest("hex");

function https(value) {
  const url = new URL(value);
  if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash) throw new Error("Expected a public HTTPS URL");
  return value;
}
function runtimeEntries(manifest) {
  return Object.fromEntries(platforms.map(platform => {
    const artifact = manifest.platforms?.[platform];
    if (!artifact || !/^[\w.-]+$/.test(artifact.runtimeVersion) ||
        !/^[a-f0-9]{64}$/.test(artifact.runtimeSha256) || /^0+$/.test(artifact.runtimeSha256)) {
      throw new Error(`Missing published runtime for ${platform}`);
    }
    https(artifact.runtimeUrl);
    return [platform, { runtimeVersion: artifact.runtimeVersion,
      runtimeUrl: artifact.runtimeUrl, runtimeSha256: artifact.runtimeSha256 }];
  }));
}
export function location(version, baseUrl, prefix = "mash") {
  validateVersion(version);
  baseUrl = baseUrl.replace(/\/+$/, "");
  https(baseUrl);
  if (!/^[A-Za-z0-9_-]+(?:\/[A-Za-z0-9_-]+)*$/.test(prefix)) throw new Error("Invalid R2 prefix");
  const artifact = `mash-cv-code-v${version}.zip`;
  const objectKey = `${prefix}/runtime/mash-cv/code/${artifact}`;
  return { artifact, objectKey, url: `${baseUrl}/${objectKey}` };
}
export function planRelease(version, ref, branch, baseUrl, prefix) {
  validateVersion(version);
  const sourceSha = git("rev-parse", "--verify", "--end-of-options", `${ref}^{commit}`);
  // Publication tools come from the default branch; only reviewed source is eligible.
  git("merge-base", "--is-ancestor", sourceSha, `refs/remotes/origin/${branch}`);
  const manifest = JSON.parse(git("show", `${sourceSha}:${manifestPath}`));
  if (compareVersions(version, manifest.mashCvCodeVersion) <= 0) throw new Error("CV code version must increase relative to the selected source");
  const tag = git("tag", "--list", `cv-code/${version}`);
  if (tag && git("rev-parse", `${tag}^{commit}`) !== sourceSha) throw new Error("CV code tag belongs to different source");
  return { schemaVersion: 1, version, sourceSha, ...location(version, baseUrl, prefix),
    runtimes: runtimeEntries(manifest) };
}
export function validateReceipt(receipt) {
  validateVersion(receipt.version);
  if (receipt.schemaVersion !== 1 || !/^[a-f0-9]{40}$/.test(receipt.sourceSha) ||
      receipt.artifact !== `mash-cv-code-v${receipt.version}.zip` ||
      !/^[a-f0-9]{64}$/.test(receipt.sha256) || !Number.isSafeInteger(receipt.size) || receipt.size <= 0) {
    throw new Error("Invalid CV code release record");
  }
  https(receipt.url);
  const suffix = `/runtime/mash-cv/code/${receipt.artifact}`;
  if (!receipt.objectKey?.endsWith(suffix) || !receipt.url.endsWith(`/${receipt.objectKey}`)) throw new Error("Invalid artifact location");
  runtimeEntries({ platforms: receipt.runtimes });
  return receipt;
}
export function recordRelease(plan, sourceFile, directory) {
  if (git("rev-parse", "HEAD") !== plan.sourceSha) throw new Error("Build checkout differs from release source");
  fs.mkdirSync(directory, { recursive: true });
  const receipt = validateReceipt({ ...plan, sha256: digest(sourceFile), size: fs.statSync(sourceFile).size });
  fs.copyFileSync(sourceFile, path.join(directory, receipt.artifact));
  write(path.join(directory, "release.json"), receipt);
  return receipt;
}
export function validateReports(receipt, directory) {
  for (const platform of platforms) {
    const report = json(path.join(directory, `${platform}.json`));
    if (report.platform !== platform || report.sourceSha !== receipt.sourceSha || report.codeSha256 !== receipt.sha256 ||
        report.runtimeSha256 !== receipt.runtimes[platform].runtimeSha256 || report.status !== "passed") {
      throw new Error(`Missing or mismatched compatibility result: ${platform}`);
    }
  }
}
export async function downloadDigest(url, fetcher = fetch) {
  const response = await fetcher(url, { signal: AbortSignal.timeout(300000), cache: "no-store" });
  if (!response.ok) throw new Error(`Download failed: ${response.status} ${url}`);
  const hash = createHash("sha256");
  for await (const chunk of response.body) hash.update(chunk);
  return hash.digest("hex");
}

// Conditional writes also protect against another publisher racing our release.
export async function publishImmutable(file, key, url, { aws, remoteDigest = downloadDigest }) {
  const sha256 = digest(file);
  try {
    aws(["s3api", "put-object", "--key", key, "--body", path.resolve(file), "--if-none-match", "*",
      "--cache-control", "public, max-age=31536000, immutable",
      "--content-type", key.endsWith(".json") ? "application/json" : "application/zip"]);
  } catch (error) {
    const detail = `${error.message}\n${error.stderr ?? ""}`;
    if (!/PreconditionFailed|\(412\)/.test(detail)) throw error;
    const work = fs.mkdtempSync(path.join(os.tmpdir(), "mash-cv-existing-"));
    try {
      const existing = path.join(work, "object");
      aws(["s3api", "get-object", "--key", key, existing]);
      if (digest(existing) !== sha256) throw new Error(`Immutable object differs: ${key}`);
    } finally { fs.rmSync(work, { recursive: true, force: true }); }
  }
  if (await remoteDigest(url) !== sha256) throw new Error(`CDN SHA-256 mismatch: ${url}`);
}
export async function publishRelease(receiptFile, reports, env = process.env) {
  const receipt = validateReceipt(json(receiptFile));
  validateReports(receipt, reports);
  const expected = location(receipt.version, env.RELEASE_BASE_URL, env.R2_PREFIX || "mash");
  for (const key of Object.keys(expected)) if (receipt[key] !== expected[key]) throw new Error("Release destination changed after build");
  for (const name of ["R2_BUCKET", "R2_ENDPOINT"]) if (!env[name]) throw new Error(`${name} is required`);
  https(env.R2_ENDPOINT);
  const file = path.join(path.dirname(receiptFile), receipt.artifact);
  if (digest(file) !== receipt.sha256 || fs.statSync(file).size !== receipt.size) throw new Error("Candidate artifact changed");
  const aws = args => {
    const tail = args[1] === "get-object" ? [args.pop()] : [];
    return run("aws", [...args, "--bucket", env.R2_BUCKET, "--endpoint-url", env.R2_ENDPOINT,
      "--cli-connect-timeout", "30", "--cli-read-timeout", "300", ...tail]);
  };
  await publishImmutable(file, receipt.objectKey, receipt.url, { aws });
  await publishImmutable(receiptFile, `${receipt.objectKey}.release.json`, `${receipt.url}.release.json`, { aws });
}
export function validateApplication(receipt) {
  validateReceipt(receipt);
  const current = json(manifestPath);
  const versions = readVersions();
  if (versions.code !== current.mashCvCodeVersion || compareVersions(versions.code, receipt.version) > 0) throw new Error("Refusing stale or inconsistent CV code version");
  if (JSON.stringify(runtimeEntries(current)) !== JSON.stringify(receipt.runtimes)) throw new Error("Runtime changed since compatibility testing; run a new release candidate");
  if (git("diff", "--name-only", receipt.sourceSha, "HEAD", "--", "sidecar/mash_cv/mash_cv")) {
    throw new Error("CV source changed since candidate build; select the current source");
  }
  if (versions.code === receipt.version && Object.values(current.platforms).some(p => p.codeUrl !== receipt.url || p.codeSha256 !== receipt.sha256)) {
    throw new Error("Existing code version refers to another artifact");
  }
}
export function applyRelease(receipt) {
  validateApplication(receipt);
  updateVersion("code", receipt.version);
  applyCvArtifact("code", receipt.version, "windows-x86_64", receipt.url, receipt.sha256);
}
export function tagRelease(receipt) {
  validateReceipt(receipt);
  const tag = `cv-code/${receipt.version}`;
  if (git("tag", "--list", tag)) {
    if (git("rev-parse", `${tag}^{commit}`) !== receipt.sourceSha) throw new Error("Release tag points to different source");
  } else git("tag", tag, receipt.sourceSha);
  git("push", "origin", `refs/tags/${tag}`);
}

if (process.argv[1] && fs.realpathSync.native(fileURLToPath(import.meta.url)) === fs.realpathSync.native(process.argv[1])) {
  const [command, ...args] = process.argv.slice(2);
  switch (command) {
    case "plan": {
      const plan = planRelease(args[0], args[1], args[2], process.env.RELEASE_BASE_URL, process.env.R2_PREFIX || "mash");
      write(args[3], plan);
      if (process.env.GITHUB_OUTPUT) fs.appendFileSync(process.env.GITHUB_OUTPUT, `source_sha=${plan.sourceSha}\nversion=${plan.version}\n`);
      break;
    }
    case "record": recordRelease(json(args[0]), args[1], args[2]); break;
    case "publish": await publishRelease(args[0], args[1]); break;
    case "validate-apply": validateApplication(json(args[0])); break;
    case "apply": applyRelease(json(args[0])); break;
    case "tag": tagRelease(json(args[0])); break;
    case "pr-body": {
      const receipt = validateReceipt(json(args[0]));
      fs.writeFileSync(args[1], `Use CV code ${receipt.version} on all runtime platforms. Runtime versions and artifacts are unchanged.\n\n` +
        `Source: ${receipt.sourceSha}\n\nArtifact: ${receipt.url}\n\nSHA-256: ${receipt.sha256}\n\n` +
        `Validation: source pytest and published-runtime compatibility checks on Windows x64 and macOS Apple Silicon; CDN download SHA-256 verified.\n`);
      break;
    }
    default: throw new Error(`Unknown command: ${command}`);
  }
}
