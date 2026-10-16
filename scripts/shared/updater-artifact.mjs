import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Select the output of the native build, without scanning retained old bundles.
export function selectUpdaterArtifact(bundleDir, config, target, requireSignature = true) {
  const architectures = { "windows-x86_64": "x64", "windows-aarch64": "arm64" };
  let relative;
  if (Object.hasOwn(architectures, target)) {
    relative = path.join("nsis", `${config.productName}_${config.version}_${architectures[target]}-setup.exe`);
  } else if (target === "darwin-aarch64" || target === "darwin-x86_64") {
    // Tauri overwrites this native app archive on each successful macOS build.
    relative = path.join("macos", `${config.productName}.app.tar.gz`);
  } else if (target === "linux-x86_64") {
    // Tauri signs the AppImage itself; the signature is stored next to it.
    relative = path.join("appimage", `${config.productName}_${config.version}_amd64.AppImage`);
  } else {
    throw new Error(`Unsupported updater target: ${target}`);
  }
  const artifact = path.resolve(bundleDir, relative);
  const requiredFiles = requireSignature ? [artifact, `${artifact}.sig`] : [artifact];
  for (const file of requiredFiles) {
    if (!fs.existsSync(file) || !fs.statSync(file).isFile()) {
      throw new Error(`Expected updater file not found: ${file}`);
    }
  }
  return artifact;
}

if (process.argv[1] && fs.realpathSync.native(fileURLToPath(import.meta.url)) === fs.realpathSync.native(process.argv[1])) {
  const [bundleDir, configPath, target] = process.argv.slice(2);
  const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
  // Forward slashes also keep basename and file reads working under Git Bash.
  process.stdout.write(selectUpdaterArtifact(bundleDir, config, target, process.argv[5] !== "--unsigned").split(path.sep).join("/"));
}
