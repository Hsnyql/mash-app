//! Delete only owned diagnostic record directories, never frontend-provided paths.
use std::{
    fs,
    path::{Path, PathBuf},
};

fn records(root: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("读取诊断记录失败: {error}")),
    };
    let mut records = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        let metadata = path.symlink_metadata().map_err(|e| e.to_string())?;
        if metadata.file_type().is_dir() && path.join("manifest.json").is_file() {
            records.push(path);
        }
    }
    Ok(records)
}

pub(super) fn delete(root: &Path, id: &str) -> Result<(), String> {
    let path = records(root)?
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(id))
        .ok_or("诊断记录不存在或已清理")?;
    fs::remove_dir_all(path).map_err(|e| format!("删除诊断记录失败: {e}"))
}

pub(super) fn clear(root: &Path) -> Result<(), String> {
    let mut errors = Vec::new();
    for path in records(root)? {
        if let Err(error) = fs::remove_dir_all(path) {
            errors.push(error.to_string());
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("部分诊断记录未能清除: {}", errors.join("；")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(root: &Path, id: &str) {
        let dir = root.join(id);
        fs::create_dir_all(dir.join("frames")).unwrap();
        fs::write(dir.join("manifest.json"), "{}").unwrap();
        fs::write(dir.join("frames/0.png"), "frame").unwrap();
    }

    #[test]
    fn deletion_is_scoped_to_one_record_and_rejects_paths() {
        let root = tempfile::tempdir().unwrap();
        record(root.path(), "first");
        record(root.path(), "second");
        for id in ["../first", "/first", "first/frames", "missing"] {
            assert!(delete(root.path(), id).is_err());
        }
        delete(root.path(), "first").unwrap();
        assert!(!root.path().join("first").exists());
        assert!(root.path().join("second/frames/0.png").is_file());
    }

    #[test]
    fn clear_removes_records_and_keeps_other_files_and_exported_bundles() {
        let root = tempfile::tempdir().unwrap();
        record(root.path(), "first");
        record(root.path(), "second");
        fs::write(root.path().join("export.zip"), "exported").unwrap();
        fs::create_dir(root.path().join("unrelated")).unwrap();
        clear(root.path()).unwrap();
        assert!(records(root.path()).unwrap().is_empty());
        assert!(root.path().join("export.zip").is_file());
        assert!(root.path().join("unrelated").is_dir());
        clear(root.path()).unwrap();
        clear(&root.path().join("missing-root")).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn deletion_does_not_follow_record_directory_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("manifest.json"), "{}").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
        assert!(delete(root.path(), "linked").is_err());
        clear(root.path()).unwrap();
        assert!(outside.path().join("manifest.json").exists());
    }
}
