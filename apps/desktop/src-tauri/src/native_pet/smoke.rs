//! Debug-only, isolated native integration check. Never reads production account storage.
use super::*;
pub fn requested(app: &AppHandle) -> bool {
    app.config().identifier.ends_with(".native-smoke")
        && std::env::args().any(|a| a == "--native-smoke")
}
pub fn start(app: &AppHandle) {
    {
        let state = app.state::<crate::companion::Companion>();
        let mut s = state.0.lock().unwrap();
        s.saved.preferences.onboarded = true;
        s.saved.preferences.resident = true;
        s.saved.preferences.hide_fullscreen = false;
        s.ready = true;
    }
    let _ = crate::apply_overlay_layout(app);
    crate::companion::emit(app);
    let app = app.clone();
    std::thread::spawn(move || {
        for mode in [
            "idle", "walk", "run", "sleepy", "tickle", "slack", "calendar", "timer", "stretch",
        ] {
            let a = app.clone();
            let _ = app.run_on_main_thread(move || {
                {
                    let state = a.state::<crate::companion::Companion>();
                    let mut s = state.0.lock().unwrap();
                    s.alerts.clear();
                    if ["slack", "calendar", "timer", "stretch"].contains(&mode) {
                        s.alerts.push(json!({"id":mode,"source":mode,"title":"네이티브 알림 확인","body":"캐릭터와 HTML 말풍선","createdAt":crate::companion::now()*1000}));
                    }
                }
                {
                    let state = a.state::<Native>();
                    let mut r = state.runtime.lock().unwrap();
                    r.set_mode(mode);
                    r.reaction = if mode == "tickle" { Some("간지러워요!") } else { None };
                }
                crate::companion::emit(&a);
            });
            std::thread::sleep(Duration::from_secs(2));
            let state = app.state::<Native>();
            let r = state.runtime.lock().unwrap();
            eprintln!(
                "SMOKE {mode}: visible={} frames={} error={:?} mode={} menu={} dragging={}",
                r.visible,
                r.render_count,
                r.last_error,
                r.mode,
                r.menu,
                r.press.as_ref().is_some_and(|p| p.dragged)
            );
        }
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            let state = a.state::<Native>();
            let r = state.runtime.lock().unwrap();
            let passed = r.render_count > 10
                && r.last_error.is_none()
                && r.visible
                && a.get_webview_window("pet-native").is_none();
            eprintln!(
                "SMOKE RESULT passed={passed} native_window={} webviews={}",
                a.get_window("pet-native").is_some(),
                a.webview_windows().len()
            );
            drop(r);
            a.exit(if passed { 0 } else { 1 });
        });
    });
}
