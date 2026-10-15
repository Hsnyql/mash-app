//! Full template validation is run only by the user-requested self-check.

use crate::screen::TemplateLoadSpec;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn shipped_template_keys(server: &str) -> Vec<String> {
    include_str!(concat!(env!("OUT_DIR"), "/template-keys.txt"))
        .lines()
        .filter_map(|line| {
            let (source, key) = line.split_once('/')?;
            if source == "shared" {
                Some(format!("shared/{key}"))
            } else if source.eq_ignore_ascii_case(server) {
                Some(key.to_string())
            } else {
                None
            }
        })
        .collect()
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TemplateCheckStatus {
    server: String,
    checked: u64,
    errors: Vec<String>,
}

fn collect_files(
    root: &Path,
    dir: &Path,
    prefix: &str,
    files: &mut BTreeMap<String, PathBuf>,
    errors: &mut Vec<String>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!("无法读取模板目录 {}: {error}", dir.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(format!("无法读取模板目录 {}: {error}", dir.display()));
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, prefix, files, errors);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        {
            let key = path
                .strip_prefix(root)
                .unwrap()
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(format!("{prefix}{key}"), path);
        }
    }
}

fn merge_config(base: &mut Value, overlay: Value) {
    if let (Some(base), Some(overlay)) = (base.as_object_mut(), overlay.as_object()) {
        for (key, value) in overlay {
            merge_config(
                base.entry(key.clone()).or_insert(Value::Null),
                value.clone(),
            );
        }
    } else {
        *base = overlay;
    }
}

fn collect_references(value: &Value, keys: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) => {
            for (name, value) in fields {
                if name == "template" {
                    if let Some(key) = value.as_str().filter(|key| !key.is_empty()) {
                        keys.insert(key.to_string());
                    }
                } else if name == "templates" {
                    if let Some(items) = value.as_array() {
                        keys.extend(
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .filter(|key| !key.is_empty())
                                .map(str::to_string),
                        );
                    }
                }
                collect_references(value, keys);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_references(item, keys);
            }
        }
        _ => {}
    }
}

fn validate_png(path: &Path) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut reader = png::Decoder::new(file)
        .read_info()
        .map_err(|error| error.to_string())?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    reader
        .next_frame(&mut pixels)
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn check_templates(
    server: &str,
    directories: &[TemplateLoadSpec],
    configs: &[PathBuf],
    expected_keys: &[String],
) -> TemplateCheckStatus {
    let mut errors = Vec::new();
    let mut files = BTreeMap::new();
    if directories.is_empty() {
        errors.push("未找到模板目录".into());
    }
    for spec in directories {
        let prefix = spec.key_prefix.as_deref().unwrap_or("").trim_matches('/');
        let prefix = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}/")
        };
        collect_files(&spec.dir, &spec.dir, &prefix, &mut files, &mut errors);
    }
    for (key, path) in &files {
        if let Err(error) = validate_png(path) {
            errors.push(format!("模板损坏: {key} ({}): {error}", path.display()));
        }
    }
    let mut config = Value::Null;
    if configs.is_empty() {
        errors.push("未找到识别配置".into());
    }
    for path in configs {
        match fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|data| serde_json::from_slice(&data).map_err(|error| error.to_string()))
        {
            Ok(overlay) => merge_config(&mut config, overlay),
            Err(error) => errors.push(format!("无法读取识别配置 {}: {error}", path.display())),
        }
    }
    let mut references = BTreeSet::new();
    references.extend(expected_keys.iter().cloned());
    collect_references(&config, &mut references);
    for key in references {
        if !files.contains_key(&key) {
            errors.push(format!("模板缺失: {key}"));
        }
    }
    TemplateCheckStatus {
        server: server.to_string(),
        checked: files.len() as u64,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_check_reports_missing_and_corrupt_templates_including_variants_and_options() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("broken.png"), b"not a png").unwrap();
        let valid = temp.path().join("valid.png");
        let mut encoder = png::Encoder::new(fs::File::create(&valid).unwrap(), 1, 1);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[127])
            .unwrap();
        let config = temp.path().join("cv.json");
        fs::write(&config, serde_json::json!({"screens": {"Test": {
            "detect": {"templateOptions": [{"template": "shared/valid"}, {"template": "shared/missing_option"}]},
            "variants": {"main": {"elements": {"button": {"template": "shared/missing_element"}}}},
            "elements": {"alternatives": {"templates": ["shared/broken", "shared/missing_variant"]}}
        }}}).to_string()).unwrap();
        let status = check_templates(
            "CN",
            &[TemplateLoadSpec {
                dir: temp.path().into(),
                key_prefix: Some("shared".into()),
            }],
            &[config],
            &["shared/missing_internal".into()],
        );
        assert_eq!(status.checked, 2);
        assert_eq!(status.errors.len(), 5);
        for key in [
            "shared/broken",
            "shared/missing_option",
            "shared/missing_element",
            "shared/missing_variant",
            "shared/missing_internal",
        ] {
            assert!(
                status.errors.iter().any(|error| error.contains(key)),
                "{status:?}"
            );
        }
        assert_eq!(serde_json::to_value(status).unwrap()["server"], "CN");
    }

    #[test]
    fn self_check_uses_merged_config_and_reports_unreadable_directories() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("shared.json");
        let overlay = temp.path().join("server.json");
        fs::write(
            &base,
            r#"{"screens":{"Test":{"detect":{"template":"overridden"}}}}"#,
        )
        .unwrap();
        fs::write(
            &overlay,
            r#"{"screens":{"Test":{"detect":{"template":"missing"}}}}"#,
        )
        .unwrap();
        let status = check_templates(
            "JP",
            &[TemplateLoadSpec {
                dir: temp.path().join("absent"),
                key_prefix: None,
            }],
            &[base, overlay],
            &[],
        );
        assert_eq!(status.errors.len(), 2);
        assert!(status.errors[0].contains("absent"));
        assert_eq!(status.errors[1], "模板缺失: missing");
    }
}
