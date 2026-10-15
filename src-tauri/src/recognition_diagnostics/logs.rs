//! Backend-owned, bounded logs independent of the frontend debug toggle.
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::Manager;

const MAX_ENTRIES: usize = 2_000;
const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct DiagnosticLogs(Mutex<Buffer>);

#[derive(Default)]
struct Buffer {
    entries: VecDeque<String>,
    bytes: usize,
    run_id: Option<String>,
}

impl Buffer {
    fn push(&mut self, line: String) {
        if line.len() > MAX_BYTES {
            return;
        }
        self.bytes += line.len();
        self.entries.push_back(line);
        while self.entries.len() > MAX_ENTRIES || self.bytes > MAX_BYTES {
            if let Some(line) = self.entries.pop_front() {
                self.bytes -= line.len();
            }
        }
    }
}

pub(crate) fn begin(app: &tauri::AppHandle) {
    if let Some(logs) = app.try_state::<DiagnosticLogs>() {
        *logs.0.lock().unwrap() = Buffer {
            run_id: Some(uuid::Uuid::new_v4().to_string()),
            ..Buffer::default()
        };
    }
}

pub(crate) fn record(app: &tauri::AppHandle, source: &str, message: &str) {
    if let Some(logs) = app.try_state::<DiagnosticLogs>() {
        let mut buffer = logs.0.lock().unwrap();
        let line = json!({"timeMs": super::now_ms(), "runId": buffer.run_id,
                          "source": source, "message": message})
        .to_string();
        buffer.push(line);
    }
}

pub(super) fn snapshot(app: &tauri::AppHandle) -> (Option<String>, String) {
    let Some(logs) = app.try_state::<DiagnosticLogs>() else {
        return (None, String::new());
    };
    let buffer = logs.0.lock().unwrap();
    (
        buffer.run_id.clone(),
        buffer
            .entries
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub(super) fn redact_text(text: &str, replacements: &[(String, String)]) -> String {
    let mut result = text.to_string();
    for (source, replacement) in replacements {
        if !source.is_empty() {
            result = result.replace(source, replacement);
        }
    }
    result
}

pub(super) fn redact(value: &mut Value, replacements: &[(String, String)]) {
    match value {
        Value::String(s) => *s = redact_text(s, replacements),
        Value::Array(a) => a.iter_mut().for_each(|v| redact(v, replacements)),
        Value::Object(o) => o.values_mut().for_each(|v| redact(v, replacements)),
        _ => {}
    }
}

/// Known source paths are removed from structured fields and embedded errors.
pub(super) fn collect_path_replacements(value: &Value, replacements: &mut Vec<(String, String)>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(
                    key.as_str(),
                    "runtimeExecutable"
                        | "codeDirectory"
                        | "path"
                        | "sourcePath"
                        | "templatePath"
                        | "imagePath"
                ) {
                    if let Some(path) = value.as_str() {
                        let normalized = path.replace('\\', "/");
                        if normalized.starts_with('/')
                            || normalized.as_bytes().get(1) == Some(&b':')
                        {
                            replacements.push((path.to_string(), "<local-file>".into()));
                        }
                    }
                }
                collect_path_replacements(value, replacements);
            }
        }
        Value::Array(array) => {
            for item in array {
                collect_path_replacements(item, replacements);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evicts_old_logs_by_count_and_bytes() {
        let mut b = Buffer::default();
        for _ in 0..MAX_ENTRIES + 1 {
            b.push("a".into());
        }
        assert_eq!(b.entries.len(), MAX_ENTRIES);
        b.push("b".repeat(MAX_BYTES));
        assert_eq!(b.entries.len(), 1);
        assert_eq!(b.bytes, MAX_BYTES);
    }
    #[test]
    fn redacts_nested_values_and_json_logs() {
        let replacements = vec![
            ("/Users/person".into(), "<home>".into()),
            ("127.0.0.1:5555".into(), "<device>".into()),
        ];
        let mut value = json!({"error": ["/Users/person/data 127.0.0.1:5555"]});
        redact(&mut value, &replacements);
        assert_eq!(value["error"][0], "<home>/data <device>");
    }
}
