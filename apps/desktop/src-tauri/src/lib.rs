use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WindowEvent,
};

mod notifier;

/// 오버레이 한 변 기본 크기(논리 px). scale 을 곱한다.
const OVERLAY_BASE: f64 = 240.0;

// ───────────────────── 표시 설정 (로컬 영속) ─────────────────────
fn default_speed() -> f64 {
    1.0
}
fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone)]
struct DisplaySettings {
    /// "top-left".."bottom-right" | "center" | "custom"
    position: String,
    /// 이미지 크기 배율
    scale: f64,
    /// 가장자리 여백(논리 px) — 프리셋 위치에만 적용
    margin: f64,
    /// custom 위치: 모니터 가용 영역 대비 비율(0~1) — 해상도 무관 대응
    custom_x: f64,
    custom_y: f64,
    /// 애니메이션 속도 배율 (1.0 = 기본)
    #[serde(default = "default_speed")]
    speed: f64,
    /// 알림음 on/off
    #[serde(default = "default_true")]
    sound: bool,
    /// 모션 줄이기(접근성)
    #[serde(default)]
    reduce_motion: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            position: "bottom-right".into(),
            scale: 1.0,
            margin: 24.0,
            custom_x: 0.92,
            custom_y: 0.92,
            speed: 1.0,
            sound: true,
            reduce_motion: false,
        }
    }
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    let _ = fs::create_dir_all(&dir);
    Some(dir.join("display.json"))
}

fn load_display(app: &AppHandle) -> DisplaySettings {
    settings_path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_display(app: &AppHandle, s: &DisplaySettings) -> Result<(), String> {
    let p = settings_path(app).ok_or("no config dir")?;
    let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    fs::write(p, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_display_settings(app: AppHandle) -> DisplaySettings {
    load_display(&app)
}

#[tauri::command]
fn set_display_settings(app: AppHandle, settings: DisplaySettings) -> Result<(), String> {
    save_display(&app, &settings)?;
    let _ = apply_overlay_layout(&app);
    Ok(())
}

/// 오버레이 창 크기·위치를 현재 설정대로 적용 (메인 디스플레이 기준)
fn apply_overlay_layout(app: &AppHandle) -> tauri::Result<()> {
    let Some(win) = app.get_webview_window("overlay") else {
        return Ok(());
    };
    let Some(monitor) = win.primary_monitor()? else {
        return Ok(());
    };
    let sf = monitor.scale_factor();
    let s = load_display(app);

    let side = ((OVERLAY_BASE * s.scale) * sf).round().max(1.0) as u32;
    win.set_size(PhysicalSize::new(side, side))?;

    let m_pos = monitor.position();
    let m_size = monitor.size();
    let (mw, mh) = (m_size.width as i32, m_size.height as i32);
    let (ww, wh) = (side as i32, side as i32);
    let gap = (s.margin * sf) as i32;

    let (x, y) = if s.position == "custom" {
        let aw = (mw - ww).max(0) as f64;
        let ah = (mh - wh).max(0) as f64;
        (
            m_pos.x + (s.custom_x * aw) as i32,
            m_pos.y + (s.custom_y * ah) as i32,
        )
    } else {
        let x = match s.position.as_str() {
            "top-left" | "left" | "bottom-left" => m_pos.x + gap,
            "top-right" | "right" | "bottom-right" => m_pos.x + mw - ww - gap,
            _ => m_pos.x + (mw - ww) / 2,
        };
        let y = match s.position.as_str() {
            "top-left" | "top" | "top-right" => m_pos.y + gap,
            "bottom-left" | "bottom" | "bottom-right" => m_pos.y + mh - wh - gap,
            _ => m_pos.y + (mh - wh) / 2,
        };
        (x, y)
    };
    win.set_position(PhysicalPosition::new(x, y))?;
    Ok(())
}

#[tauri::command]
fn show_overlay(app: AppHandle) -> Result<(), String> {
    let win = app
        .get_webview_window("overlay")
        .ok_or("no overlay window")?;
    apply_overlay_layout(&app).map_err(|e| e.to_string())?;
    win.set_always_on_top(true).map_err(|e| e.to_string())?;
    win.show().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn hide_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("overlay") {
        win.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 오버레이를 띄우고 페이로드를 전달(커맨드/네이티브 알림 폴러 공용).
pub(crate) fn push_overlay(app: &AppHandle, payload: serde_json::Value) -> Result<(), String> {
    show_overlay(app.clone())?;
    if let Some(overlay) = app.get_webview_window("overlay") {
        overlay.emit("notify", payload).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 해당 알림(id)을 닫도록 오버레이에 통지(슬랙에서 읽힘 → 자동 닫기).
/// 오버레이는 "dismiss-one" 이벤트의 {id} 를 듣는다(overlay.ts).
pub(crate) fn dismiss_overlay(app: &AppHandle, id: &str) -> Result<(), String> {
    if let Some(overlay) = app.get_webview_window("overlay") {
        overlay
            .emit("dismiss-one", serde_json::json!({ "id": id }))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn display_notification(app: AppHandle, payload: serde_json::Value) -> Result<(), String> {
    push_overlay(&app, payload)
}

/// 알림 접근 권한 상태: "allowed" | "denied" | "unspecified" | "unsupported".
#[tauri::command]
fn notification_access() -> String {
    notifier::access_status().to_string()
}

/// 알림 접근 권한 요청(동의창 시도). 결과 상태 문자열 반환.
#[tauri::command]
fn request_notification_access() -> String {
    notifier::request_access().to_string()
}

/// Windows 알림(개인정보) 설정 페이지 열기 — 동의창이 안 뜰 때 수동 허용 유도.
#[tauri::command]
fn open_notification_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg("ms-settings:privacy-notifications")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 오버레이 클릭 → 슬랙 데스크톱 앱 열기(AUMID). OS 알림엔 정밀 딥링크가 없어 앱만 연다.
#[tauri::command]
fn open_slack(aumid: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(format!("shell:AppsFolder\\{aumid}"))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = aumid;
    }
    Ok(())
}

#[tauri::command]
fn preview_overlay(app: AppHandle) -> Result<(), String> {
    let payload = serde_json::json!({
        "id": "preview:1",
        "trigger": "dm",
        "channelId": "D0",
        "channelType": "im",
        "ts": "1700000000.0001",
        "deepLink": "slack://open",
        "createdAt": 0
    });
    display_notification(app, payload)
}

/// 드래그 종료 후, 현재 오버레이 위치를 모니터 대비 비율(custom)로 저장
#[tauri::command]
fn persist_overlay_position(app: AppHandle) -> Result<(), String> {
    let win = app.get_webview_window("overlay").ok_or("no overlay")?;
    let pos = win.outer_position().map_err(|e| e.to_string())?;
    let size = win.outer_size().map_err(|e| e.to_string())?;
    let monitor = win
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("no monitor")?;
    let m_pos = monitor.position();
    let m_size = monitor.size();
    let aw = (m_size.width as f64 - size.width as f64).max(1.0);
    let ah = (m_size.height as f64 - size.height as f64).max(1.0);
    let cx = (((pos.x - m_pos.x) as f64) / aw).clamp(0.0, 1.0);
    let cy = (((pos.y - m_pos.y) as f64) / ah).clamp(0.0, 1.0);

    let mut s = load_display(&app);
    s.position = "custom".into();
    s.custom_x = cx;
    s.custom_y = cy;
    save_display(&app, &s)?;
    let _ = app.emit("display-settings", &s); // 설정창 동기화
    Ok(())
}

// ───────────────────── 창/트레이 ─────────────────────
fn show_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            show_overlay,
            hide_overlay,
            preview_overlay,
            display_notification,
            persist_overlay_position,
            get_display_settings,
            set_display_settings,
            open_slack,
            notification_access,
            request_notification_access,
            open_notification_settings
        ])
        .setup(|app| {
            let settings_i = MenuItem::with_id(app, "settings", "설정…", true, None::<&str>)?;
            let preview_i =
                MenuItem::with_id(app, "preview", "알림화면 미리보기", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &settings_i,
                    &preview_i,
                    &PredefinedMenuItem::separator(app)?,
                    &quit_i,
                ],
            )?;

            // 트레이 아이콘(Windows 컬러)
            let tray_icon = tauri::include_image!("icons/tray/icon-win.png");

            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(false)
                .tooltip("똑띠왔어요")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_settings(app),
                    "preview" => {
                        let _ = preview_overlay(app.clone());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            if let Some(overlay) = app.get_webview_window("overlay") {
                let _ = overlay.hide();
            }
            show_settings(&app.handle().clone());

            // Windows OS 알림(슬랙) 폴링 시작. 그 외 플랫폼은 no-op.
            notifier::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
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
