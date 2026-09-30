use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow,
    WindowEvent,
};

mod calendar;
mod companion;
mod diag;
mod foreground;
mod fullscreen;
mod native_pet;
mod slack;
mod surfaces;

/// Logical overlay bounds reserve room for the HTML speech bubble.
const OVERLAY_WIDTH: f64 = 400.0;
const OVERLAY_HEIGHT: f64 = 450.0;

// ───────────────────── 표시 설정 (로컬 영속) ─────────────────────
fn default_speed() -> f64 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_monitor() -> String {
    "active".into()
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
    /// 항상 위에 표시
    #[serde(default = "default_true")]
    always_on_top: bool,
    /// 출력 화면: "active"(커서 있는 화면, 기본) | "primary"(주 디스플레이) | 모니터 name(고정, 제거 시 주 화면 폴백)
    #[serde(default = "default_monitor")]
    monitor: String,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            position: "bottom".into(), // 중간 아래
            scale: 1.7,
            margin: 24.0,
            custom_x: 0.5,
            custom_y: 0.92,
            speed: 1.0,
            sound: true,
            reduce_motion: false,
            always_on_top: true,
            monitor: "active".into(),
        }
    }
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    let _ = fs::create_dir_all(&dir);
    Some(dir.join("display.json"))
}

fn read_display(app: &AppHandle) -> DisplaySettings {
    settings_path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

struct RuntimeDisplay(std::sync::Mutex<DisplaySettings>);
fn load_display(app: &AppHandle) -> DisplaySettings {
    if let Some(state) = app.try_state::<RuntimeDisplay>() {
        state.0.lock().unwrap().clone()
    } else {
        read_display(app)
    }
}

fn save_display(app: &AppHandle, s: &DisplaySettings) -> Result<(), String> {
    let p = settings_path(app).ok_or("no config dir")?;
    let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    fs::write(p, json).map_err(|e| e.to_string())?;
    if let Some(state) = app.try_state::<RuntimeDisplay>() {
        *state.0.lock().unwrap() = s.clone();
    }
    Ok(())
}

#[tauri::command]
fn get_display_settings(app: AppHandle) -> DisplaySettings {
    effective_display(&app)
}
fn effective_display(app: &AppHandle) -> DisplaySettings {
    display_for_mode(load_display(app), companion::resident(app))
}
fn display_for_mode(mut s: DisplaySettings, resident: bool) -> DisplaySettings {
    if resident {
        // Autonomous pets start on the primary display; user placement is only
        // an alert-only preference and must not influence autonomous movement.
        s.position = "bottom".into();
        s.monitor = "primary".into();
        s.custom_x = 0.5;
        s.custom_y = 1.0;
    }
    s
}

#[derive(Serialize)]
struct MonitorInfo {
    /// 매칭/저장용 식별자(모니터 name). 설정의 monitor 값으로 쓰인다.
    id: String,
    /// 설정창에 보일 사람이 읽기 좋은 라벨
    label: String,
}

fn monitor_id(m: &Monitor) -> String {
    m.name().map(|n| n.to_string()).unwrap_or_else(|| {
        format!(
            "display:{}:{}:{}x{}",
            m.position().x,
            m.position().y,
            m.size().width,
            m.size().height
        )
    })
}

/// 연결된 모니터 목록(설정창의 '출력 화면' 드롭다운용).
#[tauri::command]
fn list_monitors(app: AppHandle) -> Vec<MonitorInfo> {
    let Some(win) = app.get_webview_window("overlay") else {
        return vec![];
    };
    let monitors = win.available_monitors().unwrap_or_default();
    let primary = win
        .primary_monitor()
        .ok()
        .flatten()
        .and_then(|m| m.name().map(|n| n.to_string()));
    monitors
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let id = monitor_id(m);
            let size = m.size();
            let is_primary = Some(&id) == primary.as_ref();
            let label = format!(
                "모니터 {}{} — {}×{}",
                i + 1,
                if is_primary { " (주)" } else { "" },
                size.width,
                size.height
            );
            MonitorInfo { id, label }
        })
        .collect()
}

#[tauri::command]
fn set_display_settings(app: AppHandle, settings: DisplaySettings) -> Result<(), String> {
    let old = effective_display(&app);
    let mut settings = settings;
    if !settings.scale.is_finite()
        || !settings.speed.is_finite()
        || !settings.custom_x.is_finite()
        || !settings.custom_y.is_finite()
        || !settings.margin.is_finite()
    {
        return Err("설정 값이 올바르지 않아요".into());
    }
    settings.scale = settings.scale.clamp(0.5, 3.0);
    settings.speed = settings.speed.clamp(0.5, 3.0);
    settings.custom_x = settings.custom_x.clamp(0.0, 1.0);
    settings.custom_y = settings.custom_y.clamp(0.0, 1.0);
    settings.margin = settings.margin.clamp(0.0, 100.0);
    let mut stored = settings.clone();
    if companion::resident(&app) {
        let base = load_display(&app);
        stored.position = base.position;
        stored.monitor = base.monitor;
        stored.custom_x = base.custom_x;
        stored.custom_y = base.custom_y;
    }
    save_display(&app, &stored)?;
    if old.position != settings.position
        || old.monitor != settings.monitor
        || old.custom_x != settings.custom_x
        || old.custom_y != settings.custom_y
        || old.margin != settings.margin
    {
        apply_overlay_layout(&app).map_err(|e| e.to_string())?;
    }
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_always_on_top(settings.always_on_top);
    }
    let _ = app.emit("display-settings", &settings); // 오버레이/설정창 즉시 반영
    Ok(())
}

/// 오버레이를 띄울 모니터를 정한다.
/// - "active"(기본)/빈값 → 커서가 있는 모니터(핫플러그 안전). 못 찾으면 주 모니터.
/// - 그 외(특정 모니터 name) → 연결된 모니터 중 name 일치. 없으면(분리됨) 활성/주 모니터로 폴백.
fn resolve_monitor(win: &WebviewWindow, s: &DisplaySettings) -> Option<Monitor> {
    match s.monitor.as_str() {
        // 커서가 있는 모니터(없으면 주 디스플레이)
        "" | "active" => {
            if let Ok(p) = win.cursor_position() {
                #[cfg(target_os = "macos")]
                let unit = win.primary_monitor().ok().flatten()?.scale_factor();
                #[cfg(not(target_os = "macos"))]
                let unit = 1.;
                if let Ok(Some(m)) = win.monitor_from_point(p.x / unit, p.y / unit) {
                    return Some(m);
                }
            }
            win.primary_monitor().ok().flatten()
        }
        // 주 디스플레이(주 화면이 바뀌면 따라감)
        "primary" => win.primary_monitor().ok().flatten(),
        // 특정 모니터 고정 — 제거되면 주 디스플레이로 폴백
        name => {
            if let Ok(monitors) = win.available_monitors() {
                if let Some(m) = monitors.into_iter().find(|m| monitor_id(m) == name) {
                    return Some(m);
                }
            }
            win.primary_monitor().ok().flatten()
        }
    }
}

/// 오버레이 창 크기·위치를 현재 설정대로 적용 (대상 모니터의 작업영역=작업표시줄 제외 기준)
pub(crate) fn apply_overlay_layout(app: &AppHandle) -> tauri::Result<()> {
    let Some(win) = app.get_webview_window("overlay") else {
        return Ok(());
    };
    let s = effective_display(app);
    win.set_always_on_top(s.always_on_top)?;
    let Some(monitor) = resolve_monitor(&win, &s) else {
        return Ok(());
    };
    let wa = monitor.work_area();
    let sf = monitor.scale_factor();
    let margin = s.margin * sf;
    let half = pet_half(&s) * sf;
    let height = pet_height(&s) * sf;
    let left = wa.position.x as f64 + half + margin;
    let right = (wa.position.x as f64 + wa.size.width as f64 - half - margin).max(left);
    let top = wa.position.y as f64 + height + margin;
    let bottom = (wa.position.y as f64 + wa.size.height as f64 - margin).max(top);
    let x = if s.position == "custom" {
        left + (right - left) * s.custom_x
    } else if s.position.contains("left") {
        left
    } else if s.position.contains("right") {
        right
    } else {
        (left + right) / 2.0
    };
    let y = if s.position == "custom" {
        top + (bottom - top) * s.custom_y
    } else if s.position.contains("top") {
        top
    } else if s.position.contains("bottom") {
        bottom
    } else {
        (top + bottom) / 2.0
    };
    let result = place_pet(app, &win, &monitor, x, y, &s);
    native_pet::reset(app);
    result
}
fn pet_size(s: &DisplaySettings) -> f64 {
    (180.0 * s.scale / 1.7).clamp(110.0, 245.0)
}
fn pet_half(s: &DisplaySettings) -> f64 {
    pet_size(s) * 0.46
}
fn pet_height(s: &DisplaySettings) -> f64 {
    pet_size(s) * 0.83
}
fn place_pet(
    app: &AppHandle,
    win: &WebviewWindow,
    m: &Monitor,
    x: f64,
    y: f64,
    _s: &DisplaySettings,
) -> tauri::Result<()> {
    let wa = m.work_area();
    let sf = m.scale_factor();
    let ww = (OVERLAY_WIDTH * sf).min(wa.size.width as f64);
    let wh = (OVERLAY_HEIGHT * sf).min(wa.size.height as f64);
    let wx = (x - ww / 2.0).clamp(
        wa.position.x as f64,
        wa.position.x as f64 + wa.size.width as f64 - ww,
    );
    let wy = (y - wh).clamp(
        wa.position.y as f64,
        wa.position.y as f64 + wa.size.height as f64 - wh,
    );
    let size = PhysicalSize::new(ww as u32, wh as u32);
    let position = PhysicalPosition::new(wx.round() as i32, wy.round() as i32);
    let resized = win.inner_size()? != size;
    let moved = win.outer_position()? != position;
    let anchor = Some((x - wx.round(), y - wy.round()));
    let changed = app
        .state::<companion::Companion>()
        .0
        .lock()
        .unwrap()
        .pet_anchor
        != anchor;
    if resized {
        #[cfg(target_os = "macos")]
        win.set_size(tauri::LogicalSize::new(ww / sf, wh / sf))?;
        #[cfg(not(target_os = "macos"))]
        win.set_size(size)?;
    }
    if moved {
        #[cfg(target_os = "macos")]
        win.set_position(tauri::LogicalPosition::new(
            wx.round() / sf,
            wy.round() / sf,
        ))?;
        #[cfg(not(target_os = "macos"))]
        win.set_position(position)?;
    }
    app.state::<companion::Companion>()
        .0
        .lock()
        .unwrap()
        .pet_location = Some((surfaces::key(m), x, y));
    if !changed && !resized && !moved {
        return Ok(());
    }

    let _ = app.emit(
        "pet-layout",
        serde_json::json!({"x":(x-wx.round())/sf,"y":(y-wy.round())/sf}),
    );
    {
        let state = app.state::<companion::Companion>();
        state.0.lock().unwrap().pet_anchor = Some((x - wx.round(), y - wy.round()));
    }
    Ok(())
}
/// 오버레이를 띄우고 페이로드를 전달(서비스 이벤트/미리보기 공용).
pub(crate) fn push_overlay(app: &AppHandle, payload: serde_json::Value) -> Result<(), String> {
    companion::push(app, payload)
}

#[tauri::command]
fn display_notification(app: AppHandle, payload: serde_json::Value) -> Result<(), String> {
    push_overlay(&app, payload)
}

/// 진단 리포트 텍스트(조회 모달/복사용). 메시지 내용은 포함하지 않음.
/// async = 메인 스레드를 막지 않음(수집에 수 초 걸려도 UI 안 멈춤).
#[tauri::command]
async fn collect_diagnostics(app: AppHandle) -> String {
    diag::collect(&app)
}

/// 진단 리포트를 Slack 웹훅으로 전송(개발자에게). 웹훅은 빌드시 시크릿으로 주입.
#[tauri::command]
async fn send_diagnostics(app: AppHandle) -> Result<(), String> {
    let webhook = option_env!("DIAG_SLACK_WEBHOOK").unwrap_or("");
    if webhook.is_empty() {
        return Err("전송이 설정되지 않았어요(웹훅 미설정 빌드)".into());
    }
    let report = diag::collect(&app);
    tauri::async_runtime::spawn_blocking(move || {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|_| "진단 전송 준비 실패".to_string())?
            .post(webhook)
            .json(&serde_json::json!({"text": report}))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|_| "진단 전송 실패".to_string())?;
        Ok(())
    })
    .await
    .map_err(|_| "진단 전송 작업 실패".to_string())?
}

// ───────────────────── 자동 시작 (HKCU Run 키) ─────────────────────
// Windows 구현. macOS 로그인 항목 연동은 별도 구현한다.
// 설치 프로그램(nsis-hooks)이 기본 등록, 토글은 여기서 등록/해제.
#[cfg(target_os = "windows")]
const RUN_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const RUN_VALUE: &str = "DdoktiHere";

/// 로그인 자동시작이 켜져 있는지(Run 값 존재 여부).
#[tauri::command]
fn autostart_enabled(app: AppHandle) -> bool {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_autostart::ManagerExt;
        return app.autolaunch().is_enabled().unwrap_or(false);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(RUN_PATH)
            .and_then(|k| k.get_value::<String, _>(RUN_VALUE))
            .is_ok()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

/// 로그인 자동시작 켜기/끄기 + 사용자 선호 기록(.autostart-disabled).
#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if !cfg!(any(target_os = "windows", target_os = "macos")) {
        return Err("이 운영체제의 자동 시작은 아직 지원하지 않아요".into());
    }
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_autostart::ManagerExt;
        if enabled {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        }
        .map_err(|e| e.to_string())?;
    }
    if let Ok(dir) = app.path().app_config_dir() {
        let _ = fs::create_dir_all(&dir);
        let marker = dir.join(".autostart-disabled");
        if enabled {
            let _ = fs::remove_file(&marker);
        } else {
            let _ = fs::write(&marker, "1");
        }
    }
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
        use winreg::RegKey;
        let run = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_PATH, KEY_WRITE)
            .map_err(|e| e.to_string())?;
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let val = format!("\"{}\" --autostart", exe.display());
            run.set_value(RUN_VALUE, &val).map_err(|e| e.to_string())?;
        } else {
            let _ = run.delete_value(RUN_VALUE);
        }
    }
    Ok(())
}

#[tauri::command]
fn preview_overlay(app: AppHandle) -> Result<(), String> {
    let payload = serde_json::json!({
        "id": "preview:1",
        "trigger": "dm",
        "title": "똑띠 미리보기",
        "body": "이렇게 알림이 떠요!",
        "deepLink": "slack://open",
        "source": "preview",
        "createdAt": 0
    });
    display_notification(app, payload)
}

/// 드래그 종료 후, 현재 오버레이 위치를 모니터 대비 비율(custom)로 저장
#[tauri::command]
fn persist_overlay_position(app: AppHandle) -> Result<(), String> {
    let win = app.get_webview_window("overlay").ok_or("no overlay")?;
    if companion::resident(&app) {
        return Ok(());
    }
    let world = surfaces::pet_world(app.clone())?;
    let monitor = win
        .available_monitors()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|m| surfaces::key(m) == world.monitor)
        .ok_or("no monitor")?;
    let x = monitor.work_area().position.x as f64 + world.x * monitor.scale_factor();
    let y = monitor.work_area().position.y as f64 + world.y * monitor.scale_factor();
    let wa = monitor.work_area();
    let sf = monitor.scale_factor();
    let mut s = load_display(&app);
    let half = pet_half(&s) * sf;
    let h = pet_height(&s) * sf;
    let margin = s.margin * sf;
    s.position = "custom".into();
    s.monitor = monitor_id(&monitor);
    s.custom_x = ((x - wa.position.x as f64 - half - margin)
        / (wa.size.width as f64 - 2.0 * (half + margin)).max(1.0))
    .clamp(0.0, 1.0);
    s.custom_y = ((y - wa.position.y as f64 - h - margin)
        / (wa.size.height as f64 - h - 2.0 * margin).max(1.0))
    .clamp(0.0, 1.0);
    save_display(&app, &s)?;
    let _ = app.emit("display-settings", &s);
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--autostart"])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            preview_overlay,
            display_notification,
            persist_overlay_position,
            get_display_settings,
            set_display_settings,
            list_monitors,
            collect_diagnostics,
            send_diagnostics,
            autostart_enabled,
            set_autostart,
            companion::snapshot,
            companion::set_preferences,
            companion::timer_action,
            companion::dismiss_alert,
            companion::overlay_ready,
            companion::overlay_regions,
            companion::open_settings,
            slack::slack_status,
            slack::slack_connect,
            slack::slack_disconnect,
            slack::slack_filters,
            calendar::calendar_status,
            calendar::calendar_connect,
            calendar::calendar_disconnect,
            calendar::calendar_refresh,
            native_pet::native_pet_ui,
            native_pet::native_pet_metrics,
        ])
        .setup(|app| {
            app.manage(RuntimeDisplay(std::sync::Mutex::new(read_display(
                app.handle(),
            ))));
            companion::init(app.handle());
            calendar::init(app.handle());
            slack::init(app.handle());
            native_pet::init(app.handle()).map_err(std::io::Error::other)?;
            // WebView2 can dispatch IPC while another window is being created.
            // Register every command's state before any HTML starts loading.
            for config in &app.config().app.windows {
                tauri::WebviewWindowBuilder::from_config(app, config)?.build()?;
            }
            let _ = apply_overlay_layout(app.handle());
            native_pet::start(app.handle());
            #[cfg(debug_assertions)]
            if native_pet::smoke::requested(app.handle()) {
                native_pet::smoke::start(app.handle());
                return Ok(());
            }
            let settings_i = MenuItem::with_id(app, "settings", "설정…", true, None::<&str>)?;
            let preview_i =
                MenuItem::with_id(app, "preview", "알림화면 미리보기", true, None::<&str>)?;
            let timer_i =
                MenuItem::with_id(app, "timer-start", "25분 타이머 시작", true, None::<&str>)?;
            let pause_i = MenuItem::with_id(
                app,
                "timer-toggle",
                "타이머 일시정지 / 이어서",
                true,
                None::<&str>,
            )?;
            let cancel_i =
                MenuItem::with_id(app, "timer-cancel", "타이머 취소", true, None::<&str>)?;
            let quiet_i = MenuItem::with_id(app, "quiet", "30분 알림 중지", true, None::<&str>)?;
            let resume_i = MenuItem::with_id(app, "resume", "알림 재개", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &timer_i,
                    &pause_i,
                    &cancel_i,
                    &PredefinedMenuItem::separator(app)?,
                    &quiet_i,
                    &resume_i,
                    &PredefinedMenuItem::separator(app)?,
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
                    "timer-start" => {
                        let _ = companion::timer_action(app.clone(), "start".into(), Some(25));
                    }
                    "timer-toggle" => {
                        let running =
                            companion::snapshot(app.clone())["timer"]["deadline"].is_number();
                        let _ = companion::timer_action(
                            app.clone(),
                            if running { "pause" } else { "resume" }.into(),
                            None,
                        );
                    }
                    "timer-cancel" => {
                        let _ = companion::timer_action(app.clone(), "cancel".into(), None);
                    }
                    "quiet" => {
                        let _ = companion::set_preferences(
                            app.clone(),
                            serde_json::json!({"quiet_until":companion::now()+1800}),
                        );
                    }
                    "resume" => {
                        let _ = companion::set_preferences(
                            app.clone(),
                            serde_json::json!({"quiet_until":0}),
                        );
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            if let Some(overlay) = app.get_webview_window("overlay") {
                let _ = overlay.hide();
            }

            // 로그인 자동시작(Startup 바로가기)은 --autostart 인자로 실행 → 설정창 숨김(트레이만).
            // 그 외(수동 실행)는 설정창을 띄운다.
            let handle = app.handle().clone();
            let autostarted = std::env::args().any(|a| a == "--autostart");
            let onboarded = {
                let state = app.state::<companion::Companion>();
                let value = state.0.lock().unwrap().saved.preferences.onboarded;
                value
            };
            if !autostarted || !onboarded {
                show_settings(&handle);
            }

            // 서비스 스케줄러는 운영체제와 독립적으로 실행한다.
            companion::start(handle.clone());
            calendar::start(handle.clone());
            slack::start(handle.clone());
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

#[cfg(test)]
mod placement_tests {
    use super::*;
    #[test]
    fn autonomous_start_does_not_use_alert_placement() {
        let mut s = DisplaySettings::default();
        s.position = "top-right".into();
        s.monitor = "second".into();
        let resident = display_for_mode(s.clone(), true);
        let alerts = display_for_mode(s, false);
        assert_eq!(resident.position, "bottom");
        assert_eq!(resident.monitor, "primary");
        assert_eq!(alerts.position, "top-right");
        assert_eq!(alerts.monitor, "second");
    }
    #[test]
    fn obsolete_resident_placement_is_ignored() {
        let mut value = serde_json::to_value(DisplaySettings::default()).unwrap();
        value["resident_placement"] = serde_json::json!({"position":"top-left","monitor":"removed","custom_x":0.0,"custom_y":0.0});
        let s: DisplaySettings = serde_json::from_value(value).unwrap();
        let resident = display_for_mode(s, true);
        assert_eq!(resident.position, "bottom");
        assert_eq!(resident.monitor, "primary");
    }
}
