//! Narrow Tauri API: select stored records by id, never accept source paths.
use super::{archive, root};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticSummary {
    pub id: String,
    pub created_at_ms: u128,
    pub reason: String,
    pub image_paths: Vec<String>,
    pub missing: Vec<String>,
    #[serde(default)]
    pub user_description: String,
}

#[tauri::command]
pub(crate) async fn list_recognition_diagnostics(
    app: tauri::AppHandle,
) -> Result<Vec<DiagnosticSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = root(&app);
        let _lock = app.state::<super::DiagnosticStore>();
        let _guard = _lock.0.lock().unwrap();
        if root.is_dir() {
            app.asset_protocol_scope()
                .allow_directory(&root, true)
                .map_err(|e| e.to_string())?;
        }
        let mut records = Vec::new();
        for path in archive::records(&root).into_iter().rev() {
            let Some(value) = std::fs::read(path.join("manifest.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            else {
                continue;
            };
            let Ok(mut summary) =
                serde_json::from_value::<DiagnosticSummary>(value["summary"].clone())
            else {
                continue;
            };
            // Rebuild paths from the owned directory, ignoring persisted absolute paths.
            summary.image_paths = value["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f["path"].as_str())
                .filter(|name| matches!(*name, "frames/0.png" | "frames/1.png" | "frames/2.png"))
                .map(|name| path.join(name).to_string_lossy().into_owned())
                .collect();
            records.push(summary);
        }
        Ok(records)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn export_recognition_diagnostic(
    app: tauri::AppHandle,
    diagnostic_id: String,
    user_description: Option<String>,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = root(&app);
        // The id must resolve to an existing owned record; no arbitrary file access.
        let dir = archive::records(&root)
            .into_iter()
            .find(|p| p.file_name().and_then(|p| p.to_str()) == Some(&diagnostic_id))
            .ok_or("诊断记录不存在或已清理")?;
        if let Some(description) = user_description {
            let store = app.state::<super::DiagnosticStore>();
            let _guard = store.0.lock().unwrap();
            super::description::save(&dir, &description)?;
        }
        let target = app
            .dialog()
            .file()
            .add_filter("ZIP", &["zip"])
            .set_title("导出识别诊断包（包含游戏截图）")
            .set_file_name(format!("mash-diagnostic-{diagnostic_id}.zip"))
            .blocking_save_file();
        let Some(target) = target else {
            return Ok(None);
        };
        let mut target = target.into_path().map_err(|e| e.to_string())?;
        if !target
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        {
            target.set_extension("zip");
        }
        let store = app.state::<super::DiagnosticStore>();
        let _guard = store.0.lock().unwrap();
        archive::export(&dir, &target)?;
        Ok(Some(target.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn delete_recognition_diagnostic(
    app: tauri::AppHandle,
    diagnostic_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<super::DiagnosticStore>();
        let _guard = store.0.lock().unwrap();
        let result = super::maintenance::delete(&root(&app), &diagnostic_id);
        let _ = app.emit("recognition-diagnostics-changed", ());
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn clear_recognition_diagnostics(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<super::DiagnosticStore>();
        let _guard = store.0.lock().unwrap();
        let result = super::maintenance::clear(&root(&app));
        let _ = app.emit("recognition-diagnostics-changed", ());
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
