//! User-authored context belongs to a stored record, separate from CV evidence.
use serde_json::Value;
use std::{fs, io::Write, path::Path};

pub(super) fn save(dir: &Path, description: &str) -> Result<(), String> {
    if description.chars().count() > 10_000 {
        return Err("问题描述不能超过 10,000 字".into());
    }
    let path = dir.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let summary = manifest["summary"].as_object_mut().ok_or("诊断清单损坏")?;
    summary.insert("userDescription".into(), Value::String(description.into()));
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    temp.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recognition_diagnostics::commands::DiagnosticSummary;
    use serde_json::json;

    #[test]
    fn description_survives_reload_and_old_records_default_to_empty() {
        let dir = tempfile::tempdir().unwrap();
        let summary = json!({"id":"record", "createdAtMs": 1, "reason":"unknown", "imagePaths":[], "missing":[]});
        let old: DiagnosticSummary = serde_json::from_value(summary.clone()).unwrap();
        assert!(old.user_description.is_empty());
        let path = dir.path().join("manifest.json");
        fs::write(&path, json!({"summary": summary, "files": []}).to_string()).unwrap();
        save(dir.path(), "点击开始后停止\n每次都能复现").unwrap();
        let manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let loaded: DiagnosticSummary =
            serde_json::from_value(manifest["summary"].clone()).unwrap();
        assert_eq!(loaded.user_description, "点击开始后停止\n每次都能复现");
        let before = fs::read(&path).unwrap();
        assert!(save(dir.path(), &"字".repeat(10_001)).is_err());
        assert_eq!(fs::read(path).unwrap(), before);
    }
}
