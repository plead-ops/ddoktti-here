use keyring::Entry;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, WindowEvent,
};

const KR_SERVICE: &str = "kr.co.plead.ddoktti-here";
const KR_USER: &str = "session";

fn session_entry() -> Result<Entry, String> {
    Entry::new(KR_SERVICE, KR_USER).map_err(|e| e.to_string())
}

/// 세션 토큰을 OS 보안 저장소에 보관 (PRD §13.5)
#[tauri::command]
fn save_session(token: String) -> Result<(), String> {
    session_entry()?.set_password(&token).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_session() -> Result<Option<String>, String> {
    match session_entry()?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn clear_session() -> Result<(), String> {
    match session_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// 서버에서 받은 알림을 오버레이로 표시 (배치 후 notify emit)
#[tauri::command]
fn display_notification(
    app: AppHandle,
    payload: serde_json::Value,
    position: Option<String>,
    margin: Option<f64>,
) -> Result<(), String> {
    let position = position.unwrap_or_else(|| "bottom-right".into());
    let margin = margin.unwrap_or(24.0);
    show_overlay(app.clone(), position, margin)?;
    if let Some(overlay) = app.get_webview_window("overlay") {
        overlay.emit("notify", payload).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 설정 창을 앞으로 (PRD §5.6 진입점)
fn show_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// 오버레이 창을 메인 디스플레이의 9방향 위치로 배치 (PRD §5.4)
fn position_overlay_window(app: &AppHandle, position: &str, margin: f64) -> tauri::Result<()> {
    let Some(win) = app.get_webview_window("overlay") else {
        return Ok(());
    };
    let Some(monitor) = win.primary_monitor()? else {
        return Ok(());
    };
    let m_pos = monitor.position(); // PhysicalPosition<i32>
    let m_size = monitor.size(); // PhysicalSize<u32>
    let w_size = win.outer_size()?; // PhysicalSize<u32>
    let gap = (margin * monitor.scale_factor()) as i32;

    let (mw, mh) = (m_size.width as i32, m_size.height as i32);
    let (ww, wh) = (w_size.width as i32, w_size.height as i32);

    let x = match position {
        "top-left" | "left" | "bottom-left" => m_pos.x + gap,
        "top-right" | "right" | "bottom-right" => m_pos.x + mw - ww - gap,
        _ => m_pos.x + (mw - ww) / 2,
    };
    let y = match position {
        "top-left" | "top" | "top-right" => m_pos.y + gap,
        "bottom-left" | "bottom" | "bottom-right" => m_pos.y + mh - wh - gap,
        _ => m_pos.y + (mh - wh) / 2,
    };
    win.set_position(PhysicalPosition::new(x, y))?;
    Ok(())
}

/// 오버레이를 지정 위치에 표시 (지속 노출 — 자동으로 사라지지 않음)
#[tauri::command]
fn show_overlay(app: AppHandle, position: String, margin: f64) -> Result<(), String> {
    let win = app
        .get_webview_window("overlay")
        .ok_or("no overlay window")?;
    position_overlay_window(&app, &position, margin).map_err(|e| e.to_string())?;
    win.set_always_on_top(true).map_err(|e| e.to_string())?;
    win.show().map_err(|e| e.to_string())?;
    Ok(())
}

/// 오버레이 숨김 (클릭 닫힘 / dismiss)
#[tauri::command]
fn hide_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("overlay") {
        win.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// M1: 설정 창의 '오버레이 미리보기' — 배치 후 데모 알림 emit
#[tauri::command]
fn preview_overlay(
    app: AppHandle,
    position: Option<String>,
    margin: Option<f64>,
) -> Result<(), String> {
    let position = position.unwrap_or_else(|| "bottom-right".into());
    let margin = margin.unwrap_or(24.0);
    show_overlay(app.clone(), position, margin)?;
    if let Some(overlay) = app.get_webview_window("overlay") {
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
        .invoke_handler(tauri::generate_handler![
            show_overlay,
            hide_overlay,
            preview_overlay,
            display_notification,
            save_session,
            get_session,
            clear_session
        ])
        .setup(|app| {
            // 트레이 메뉴 (PRD §5.7)
            let settings_i = MenuItem::with_id(app, "settings", "설정…", true, None::<&str>)?;
            let preview_i = MenuItem::with_id(app, "preview", "오버레이 미리보기", true, None::<&str>)?;
            let pause_i = MenuItem::with_id(app, "pause", "일시중지", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_i, &preview_i, &pause_i, &sep, &quit_i])?;

            TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_settings(app),
                    "preview" => {
                        let _ = preview_overlay(app.clone(), None, None);
                    }
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
