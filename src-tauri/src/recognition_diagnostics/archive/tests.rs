use super::*;

#[test]
fn partial_bundle_survives_reload_and_exports_integrity_checked_files() {
    let root = tempfile::tempdir().unwrap();
    let summary = persist(
        root.path(),
        json!({"samples": [{"imageError": "capture_disabled"}],
        "configuration": {"screens": {}}, "templates": [{"key": "missing"}]}),
        json!({"appVersion": "test"}),
        "无法识别",
        Some("run-1".into()),
        "{\"message\":\"/Users/me/log\"}".into(),
        vec![],
        &[("/Users/me".into(), "<home>".into())],
    )
    .unwrap();
    assert_eq!(summary.missing.len(), 2);
    let dir = &records(root.path())[0];
    let manifest: Value =
        serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["runId"], "run-1");
    for file in manifest["files"].as_array().unwrap() {
        assert_eq!(
            file["sha256"],
            sha256(&fs::read(dir.join(file["path"].as_str().unwrap())).unwrap())
        );
    }
    let dest = root.path().join("bundle.zip");
    export(dir, &dest).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(dest).unwrap()).unwrap();
    let log = std::io::read_to_string(zip.by_name("logs.jsonl").unwrap()).unwrap();
    assert!(log.contains("<home>/log"));
    assert!(!log.contains("/Users/me"));
}

#[test]
fn retains_ten_newest_records_and_ignores_pending_directories() {
    let root = tempfile::tempdir().unwrap();
    for i in 0..12 {
        let dir = root.path().join(format!("{i:03}"));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("manifest.json"), "{}").unwrap();
    }
    fs::create_dir(root.path().join(".pending-abandoned")).unwrap();
    prune(root.path(), "011").unwrap();
    let dirs = records(root.path());
    assert_eq!(dirs.len(), 10);
    assert_eq!(dirs[0].file_name().unwrap(), "002");
}

#[test]
fn export_rejects_manifest_path_traversal() {
    for name in [
        "../outside",
        "/etc/passwd",
        "frames/../../outside",
        "C:\\secret",
    ] {
        assert!(!safe_relative(name));
    }
    assert!(safe_relative("frames/0.png"));
}

#[test]
fn png_and_frame_identity_survive_archive_and_no_local_root_leaks() {
    let root = tempfile::tempdir().unwrap();
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[1, 2, 3, 4, 5, 6])
            .unwrap();
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let summary = persist(root.path(), json!({"samples": [{"stage": "last", "frame": {"frameId": 7}, "pngB64": encoded}], "templates": []}),
        json!({}), "无法识别", None, String::new(), vec![], &[]).unwrap();
    assert_eq!(summary.image_paths, vec!["frames/0.png"]);
    let dir = root.path().join(&summary.id);
    let target = root.path().join("bundle.zip");
    export(&dir, &target).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(target).unwrap()).unwrap();
    use std::io::Read;
    let mut actual = Vec::new();
    zip.by_name("frames/0.png")
        .unwrap()
        .read_to_end(&mut actual)
        .unwrap();
    assert_eq!(actual, bytes);
    let report: Value = serde_json::from_reader(zip.by_name("recognition.json").unwrap()).unwrap();
    assert_eq!(report["samples"][0]["frame"]["frameId"], 7);
    assert!(report["samples"][0].get("pngB64").is_none());
    let manifest = std::io::read_to_string(zip.by_name("manifest.json").unwrap()).unwrap();
    assert!(!manifest.contains(root.path().to_str().unwrap()));
}

#[test]
fn changed_file_rejects_export_without_replacing_existing_target() {
    let root = tempfile::tempdir().unwrap();
    let summary = persist(
        root.path(),
        json!({"samples": []}),
        json!({}),
        "failure",
        None,
        String::new(),
        vec![],
        &[],
    )
    .unwrap();
    let dir = root.path().join(summary.id);
    fs::write(dir.join("recognition.json"), "tampered").unwrap();
    let target = root.path().join("existing.zip");
    fs::write(&target, "original").unwrap();
    assert!(export(&dir, &target).unwrap_err().contains("校验失败"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "original");
}

#[test]
fn total_size_limit_evicts_oldest_large_record() {
    let root = tempfile::tempdir().unwrap();
    for name in ["001", "002"] {
        let dir = root.path().join(name);
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("manifest.json"), "{}").unwrap();
        // Sparse files exercise metadata-based accounting without allocating 200 MiB.
        fs::File::create(dir.join("large.png"))
            .unwrap()
            .set_len(MAX_BYTES / 2 + 1)
            .unwrap();
    }
    prune(root.path(), "002").unwrap();
    assert_eq!(records(root.path()).len(), 1);
    assert!(!root.path().join("001").exists());
}

#[test]
fn exports_saved_user_description_with_integrity_and_supports_clearing() {
    let root = tempfile::tempdir().unwrap();
    let summary = persist(
        root.path(),
        json!({"samples": []}),
        json!({}),
        "failure",
        None,
        String::new(),
        vec![],
        &[],
    )
    .unwrap();
    let dir = root.path().join(summary.id);
    let description = "点击开始后卡在助战页面\n预期自动继续";
    crate::recognition_diagnostics::description::save(&dir, description).unwrap();
    let target = root.path().join("bundle.zip");
    export(&dir, &target).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(&target).unwrap()).unwrap();
    let report = std::io::read_to_string(zip.by_name("user-report.txt").unwrap()).unwrap();
    assert_eq!(report, description);
    let manifest: Value = serde_json::from_reader(zip.by_name("manifest.json").unwrap()).unwrap();
    assert_eq!(manifest["summary"]["userDescription"], description);
    let file = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "user-report.txt")
        .unwrap();
    assert_eq!(file["sha256"], sha256(description.as_bytes()));
    drop(zip);
    crate::recognition_diagnostics::description::save(&dir, "").unwrap();
    export(&dir, &target).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(target).unwrap()).unwrap();
    assert!(zip.by_name("user-report.txt").is_err());
}
