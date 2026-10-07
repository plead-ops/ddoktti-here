//! Developer-only alert injection. With `DDOKTTI_DEV_INBOX=<dir>`, every `*.json`
//! file dropped there is read and deleted: an alert object (or an array of them)
//! is pushed like a real one, and `{"preferences": {...}}` patches preferences.
//! Without the variable nothing here runs.
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tauri::AppHandle;

fn inbox() -> Option<PathBuf> {
    std::env::var_os("DDOKTTI_DEV_INBOX").map(PathBuf::from)
}
/// Injected alerts carry ":dev:" in their id; sync loops must not prune them.
pub fn is_fake(alert: &Value) -> bool {
    inbox().is_some() && alert["id"].as_str().is_some_and(|id| id.contains(":dev:"))
}
pub fn start(app: AppHandle) {
    let Some(dir) = inbox() else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        for path in files {
            let data = std::fs::read(&path);
            let _ = std::fs::remove_file(&path);
            let Some(value) = data.ok().and_then(|d| serde_json::from_slice::<Value>(&d).ok())
            else {
                eprintln!("dev inbox: unreadable {}", path.display());
                continue;
            };
            if let Some(patch) = value.get("preferences") {
                let r = crate::companion::set_preferences(app.clone(), patch.clone());
                eprintln!("dev inbox: preferences {patch} -> {r:?}");
                continue;
            }
            let alerts = match value {
                Value::Array(list) => list,
                one => vec![one],
            };
            for alert in alerts {
                let r = crate::companion::push(&app, alert);
                eprintln!("dev inbox: push -> {r:?}");
            }
        }
    });
}
