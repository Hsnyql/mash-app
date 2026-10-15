//! File format, partial evidence handling, integrity and bounded retention.
use super::commands::DiagnosticSummary;
use super::logs::{redact, redact_text};
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const MAX_RECORDS: usize = 10;
const MAX_BYTES: u64 = 200 * 1024 * 1024;
const MAX_RECORD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RESOURCE_BYTES: u64 = 8 * 1024 * 1024;

pub(super) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_file(
    dir: &Path,
    name: &str,
    bytes: &[u8],
    files: &mut Vec<Value>,
    missing: &mut Vec<String>,
) {
    if files
        .iter()
        .filter_map(|f| f["bytes"].as_u64())
        .sum::<u64>()
        + bytes.len() as u64
        > MAX_RECORD_BYTES
    {
        missing.push(format!("{name}: record_size_limit"));
        return;
    }
    match fs::write(dir.join(name), bytes) {
        Ok(()) => files.push(json!({"path": name, "bytes": bytes.len(), "sha256": sha256(bytes)})),
        Err(e) => missing.push(format!("{name}: {e}")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn persist(
    root: &Path,
    mut report: Value,
    mut environment: Value,
    reason: &str,
    run_id: Option<String>,
    logs: String,
    mut missing: Vec<String>,
    replacements: &[(String, String)],
) -> Result<DiagnosticSummary, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let id = format!("{:013}-{}", super::now_ms(), uuid::Uuid::new_v4());
    let staging = tempfile::Builder::new()
        .prefix(".pending-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    let dir = staging.path();
    fs::create_dir(dir.join("frames")).map_err(|e| e.to_string())?;
    fs::create_dir(dir.join("resources")).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    let mut previews = Vec::new();
    if let Some(samples) = report["samples"].as_array_mut() {
        for (index, sample) in samples.iter_mut().enumerate() {
            let png = sample.as_object_mut().and_then(|s| s.remove("pngB64"));
            if let Some(encoded) = png.and_then(|v| v.as_str().map(str::to_string)) {
                let name = format!("frames/{index}.png");
                match base64::engine::general_purpose::STANDARD.decode(encoded) {
                    Ok(bytes) if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => {
                        write_file(dir, &name, &bytes, &mut files, &mut missing);
                        if dir.join(&name).is_file() {
                            sample["image"] = json!(name);
                            previews.push(name);
                        }
                    }
                    _ => missing.push(format!("{name}: invalid PNG")),
                }
            } else {
                let reason = match sample["imageError"].as_str() {
                    Some("capture_disabled") => "未开启现场截图",
                    Some("frame_unavailable") => "没有取得视频帧",
                    _ => "截图编码失败",
                };
                missing.push(format!("现场截图 {}: {reason}", index + 1));
            }
        }
    }
    // Copy only template files named by the internal sidecar, never arbitrary frontend paths.
    if let Some(templates) = report["templates"].as_array_mut() {
        for (index, template) in templates.iter_mut().enumerate() {
            let source = template
                .as_object_mut()
                .and_then(|t| t.remove("sourcePath"));
            if let Some(path) = source.as_ref().and_then(Value::as_str) {
                let name = format!("resources/template-{index}.png");
                match fs::metadata(path).and_then(|m| {
                    if m.len() <= MAX_RESOURCE_BYTES {
                        fs::read(path)
                    } else {
                        Err(std::io::Error::other("template too large"))
                    }
                }) {
                    Ok(bytes) => {
                        write_file(dir, &name, &bytes, &mut files, &mut missing);
                        template["file"] = json!(name);
                    }
                    Err(e) => missing.push(format!("template {}: {e}", template["key"])),
                }
            } else {
                missing.push(format!("template {}: source unavailable", template["key"]));
            }
        }
    }
    if let Some(mut configuration) = report
        .as_object_mut()
        .and_then(|r| r.remove("configuration"))
    {
        redact(&mut configuration, replacements);
        write_file(
            dir,
            "resources/configuration.json",
            &serde_json::to_vec_pretty(&configuration).map_err(|e| e.to_string())?,
            &mut files,
            &mut missing,
        );
    }
    redact(&mut report, replacements);
    redact(&mut environment, replacements);
    write_file(
        dir,
        "recognition.json",
        &serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        &mut files,
        &mut missing,
    );
    write_file(
        dir,
        "environment.json",
        &serde_json::to_vec_pretty(&environment).map_err(|e| e.to_string())?,
        &mut files,
        &mut missing,
    );
    // Redact each decoded JSON line, preserving valid JSON even for Windows paths.
    let logs = logs
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|mut v| {
            redact(&mut v, replacements);
            v.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    write_file(dir, "logs.jsonl", logs.as_bytes(), &mut files, &mut missing);
    let summary = DiagnosticSummary {
        id: id.clone(),
        created_at_ms: super::now_ms(),
        reason: redact_text(reason, replacements),
        image_paths: previews,
        user_description: String::new(),
        missing: missing
            .into_iter()
            .map(|m| redact_text(&m, replacements))
            .collect(),
    };
    let manifest =
        json!({"schemaVersion": 1, "id": id, "runId": run_id, "summary": summary, "files": files});
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(dir, root.join(&id)).map_err(|e| e.to_string())?;
    // Protect the record just created. Oversized evidence is still available for export.
    if let Err(error) = prune(root, &id) {
        eprintln!("[recognition-diagnostics] retention: {error}");
    }
    Ok(summary)
}

pub(super) fn records(root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("manifest.json").is_file())
        .collect();
    paths.sort();
    paths
}

fn size(path: &Path) -> u64 {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                size(&p)
            } else {
                e.metadata().map(|m| m.len()).unwrap_or(0)
            }
        })
        .sum()
}

fn prune(root: &Path, keep: &str) -> Result<(), String> {
    let paths = records(root);
    let mut total: u64 = paths.iter().map(|p| size(p)).sum();
    let mut count = paths.len();
    for path in paths {
        if count <= MAX_RECORDS && total <= MAX_BYTES {
            break;
        }
        if path.file_name().and_then(|p| p.to_str()) == Some(keep) {
            continue;
        }
        let bytes = size(&path);
        fs::remove_dir_all(path).map_err(|e| e.to_string())?;
        count -= 1;
        total = total.saturating_sub(bytes);
    }
    Ok(())
}

pub(super) fn export(dir: &Path, target: &Path) -> Result<(), String> {
    let parent = target.parent().ok_or("无效保存位置")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    {
        let mut zip = zip::ZipWriter::new(temp.as_file_mut());
        let mut manifest: Value = serde_json::from_slice(
            &fs::read(dir.join("manifest.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let description = manifest["summary"]["userDescription"]
            .as_str()
            .unwrap_or_default()
            .as_bytes()
            .to_vec();
        if !description.is_empty() {
            manifest["files"].as_array_mut().ok_or("诊断清单损坏")?.push(json!({
                "path": "user-report.txt", "bytes": description.len(), "sha256": sha256(&description)
            }));
        }
        let paths = manifest["files"].as_array().ok_or("诊断清单损坏")?;
        let mut names = vec!["manifest.json".to_string()];
        for file in paths {
            names.push(file["path"].as_str().ok_or("诊断清单损坏")?.to_string());
        }
        for name in names {
            if !safe_relative(&name) {
                return Err("诊断文件路径无效".into());
            }
            let bytes = if name == "manifest.json" {
                serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?
            } else if name == "user-report.txt" && !description.is_empty() {
                description.clone()
            } else {
                fs::read(dir.join(&name)).map_err(|e| e.to_string())?
            };
            if let Some(expected) = paths.iter().find(|f| f["path"].as_str() == Some(&name)) {
                if expected["sha256"].as_str() != Some(&sha256(&bytes)) {
                    return Err(format!("诊断文件校验失败: {name}"));
                }
            }
            zip.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    temp.persist(target).map_err(|e| e.to_string())?;
    Ok(())
}

fn safe_relative(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\\')
        && !name.contains(':')
        && Path::new(name)
            .components()
            .all(|p| matches!(p, std::path::Component::Normal(_)))
}

#[cfg(test)]
mod tests;
