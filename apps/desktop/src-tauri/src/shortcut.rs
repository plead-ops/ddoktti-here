//! A global shortcut (Control+Option/Alt+D) confirms the bubble on screen, like
//! its "확인" button. Registered only while the preference is on, so the keys
//! stay free for other apps otherwise.
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

fn shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyD)
}
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _, event| {
            if event.state() == ShortcutState::Pressed {
                let _ = app.emit_to("overlay", "shortcut-ack", ());
            }
        })
        .build()
}
/// Registers or releases the shortcut to match the preference.
pub fn sync(app: &AppHandle) {
    let enabled = app
        .state::<crate::companion::Companion>()
        .0
        .lock()
        .unwrap()
        .saved
        .preferences
        .shortcut;
    let keys = app.global_shortcut();
    let registered = keys.is_registered(shortcut());
    if enabled && !registered {
        // Another app may own the combination already; the app works without it.
        if let Err(e) = keys.register(shortcut()) {
            eprintln!("shortcut: {e}");
        }
    } else if !enabled && registered {
        let _ = keys.unregister(shortcut());
    }
}
