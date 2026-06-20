use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WindowEvent,
};

/// 설정 창을 앞으로 (PRD §5.6 진입점)
fn show_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// M1: 오버레이 데모 알림 표시 (설정 창의 '오버레이 미리보기')
#[tauri::command]
fn preview_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.show();
        // TODO(M1): 9방향 위치/메인 디스플레이 배치
        let payload = serde_json::json!({
            "id": "preview:1",
            "trigger": "dm",
            "channelId": "D0",
            "channelType": "im",
            "ts": "1700000000.0001",
            "deepLink": "slack://open",
            "createdAt": 0
        });
        overlay.emit("notify", payload).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 중복 실행 시 새 인스턴스 대신 기존 설정 창을 앞으로 (PRD §5.6)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![preview_overlay])
        .setup(|app| {
            // 트레이 메뉴 (PRD §5.7)
            let settings_i = MenuItem::with_id(app, "settings", "설정…", true, None::<&str>)?;
            let pause_i = MenuItem::with_id(app, "pause", "일시중지", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_i, &pause_i, &sep, &quit_i])?;

            TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_settings(app),
                    "pause" => { /* TODO(M5): 알림 일시중지 토글 */ }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // 오버레이 창은 숨긴 채 준비
            if let Some(overlay) = app.get_webview_window("overlay") {
                let _ = overlay.hide();
            }
            // 첫 실행: 설정 창 자동 오픈 (TODO(M2): 세션 있으면 숨김 유지)
            show_settings(&app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // 설정 창 닫기 = 종료가 아니라 트레이로 숨김
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "settings" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running 똑띠왔어요");
}
