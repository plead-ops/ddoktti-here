use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow,
    WindowEvent,
};

mod diag;
mod notifier;

/// 오버레이 한 변 기본 크기(논리 px). scale 을 곱한다.
/// 창은 스프라이트(보이는 마스코트)보다 크다 — CSS `.overlay{padding:10%}` 로 스프라이트를
/// 창의 80% 안쪽에 그려 그림자(drop-shadow)가 창 밖으로 잘리지 않게 한다.
/// 보이는 마스코트 크기 = 300 × 0.8 = 240(이전 기본값과 동일).
const OVERLAY_BASE: f64 = 300.0;

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

#[derive(Serialize)]
struct MonitorInfo {
    /// 매칭/저장용 식별자(모니터 name). 설정의 monitor 값으로 쓰인다.
    id: String,
    /// 설정창에 보일 사람이 읽기 좋은 라벨
    label: String,
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
            let id = m
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("display-{i}"));
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
    save_display(&app, &settings)?;
    let _ = apply_overlay_layout(&app);
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
                if let Ok(Some(m)) = win.monitor_from_point(p.x, p.y) {
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
                if let Some(m) = monitors
                    .into_iter()
                    .find(|m| m.name().map(|n| n.to_string()) == Some(name.to_string()))
                {
                    return Some(m);
                }
            }
            win.primary_monitor().ok().flatten()
        }
    }
}

/// 오버레이 창 크기·위치를 현재 설정대로 적용 (대상 모니터의 작업영역=작업표시줄 제외 기준)
fn apply_overlay_layout(app: &AppHandle) -> tauri::Result<()> {
    let Some(win) = app.get_webview_window("overlay") else {
        return Ok(());
    };
    let s = load_display(app);
    let Some(monitor) = resolve_monitor(&win, &s) else {
        return Ok(());
    };
    let sf = monitor.scale_factor();

    let _ = win.set_always_on_top(s.always_on_top);

    let side = ((OVERLAY_BASE * s.scale) * sf).round().max(1.0) as u32;
    win.set_size(PhysicalSize::new(side, side))?;

    // 작업영역(work area): 작업표시줄을 제외한 사용 가능 화면 영역
    let wa = monitor.work_area();
    let (ox, oy) = (wa.position.x, wa.position.y);
    let (mw, mh) = (wa.size.width as i32, wa.size.height as i32);
    let (ww, wh) = (side as i32, side as i32);
    let gap = (s.margin * sf) as i32;

    let (x, y) = if s.position == "custom" {
        let aw = (mw - ww).max(0) as f64;
        let ah = (mh - wh).max(0) as f64;
        (ox + (s.custom_x * aw) as i32, oy + (s.custom_y * ah) as i32)
    } else {
        let x = match s.position.as_str() {
            "top-left" | "left" | "bottom-left" => ox + gap,
            "top-right" | "right" | "bottom-right" => ox + mw - ww - gap,
            _ => ox + (mw - ww) / 2,
        };
        let y = match s.position.as_str() {
            "top-left" | "top" | "top-right" => oy + gap,
            "bottom-left" | "bottom" | "bottom-right" => oy + mh - wh - gap,
            _ => oy + (mh - wh) / 2,
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
    apply_overlay_layout(&app).map_err(|e| e.to_string())?; // 항상위 설정도 여기서 적용
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
    let truncated: String = report.chars().take(35000).collect();
    let payload = serde_json::json!({ "text": format!("```\n{truncated}\n```") });
    let body = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    let mut tmp = std::env::temp_dir();
    tmp.push("ddoktti-diag.json");
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let cmd = format!(
            "try {{ Invoke-RestMethod -Uri '{webhook}' -Method Post -ContentType 'application/json' -InFile '{}'; exit 0 }} catch {{ exit 1 }}",
            tmp.display()
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &cmd])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW — 콘솔 창 숨김
            .status();
        let _ = std::fs::remove_file(&tmp);
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err("전송 실패(네트워크/웹훅 확인)".into()),
            Err(e) => Err(format!("전송 실행 실패: {e}")),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (webhook, tmp);
        Err("Windows 전용".into())
    }
}

// ───────────────────── 자동 시작 (StartupTask) ─────────────────────
// 패키지 ID 앱은 레지스트리 Run 대신 매니페스트 StartupTask 가 정공법.
// COM 미초기화 스레드에서 불릴 수 있어 MTA 전용 스레드에서 수행 후 join.

/// 로그인 자동시작이 켜져 있는지(StartupTask 상태).
#[tauri::command]
fn autostart_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(|| {
            use windows::core::HSTRING;
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
            use windows::ApplicationModel::StartupTask;
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            (|| -> windows::core::Result<bool> {
                let task = StartupTask::GetAsync(&HSTRING::from("DdoktiHereStartup"))?.get()?;
                let s = task.State()?.0;
                Ok(s == 2 || s == 4) // Enabled | EnabledByPolicy
            })()
            .unwrap_or(false)
        })
        .join()
        .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// 로그인 자동시작 켜기/끄기.
#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || -> Result<(), String> {
            use windows::core::HSTRING;
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
            use windows::ApplicationModel::StartupTask;
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let task = StartupTask::GetAsync(&HSTRING::from("DdoktiHereStartup"))
                .and_then(|op| op.get())
                .map_err(|e| format!("StartupTask 조회 실패: {e:?}"))?;
            if enabled {
                let s = task
                    .RequestEnableAsync()
                    .and_then(|op| op.get())
                    .map_err(|e| format!("{e:?}"))?
                    .0;
                match s {
                    2 | 4 => Ok(()),
                    1 => Err("Windows 작업관리자/설정에서 꺼져 있어요. 거기서 켜주세요.".into()),
                    3 => Err("정책으로 차단되어 있어요.".into()),
                    _ => Err("자동시작을 켜지 못했어요.".into()),
                }
            } else {
                task.Disable().map_err(|e| format!("{e:?}"))?;
                Ok(())
            }
        })
        .join()
        .map_err(|_| "스레드 패닉".to_string())?
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
        Err("Windows 전용".into())
    }
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
    let pos = win.outer_position().map_err(|e| e.to_string())?;
    let size = win.outer_size().map_err(|e| e.to_string())?;
    // 창이 실제로 떠 있는 모니터 기준(없으면 주 모니터) — 배치 시 대상 모니터와 일치
    let monitor = win
        .current_monitor()
        .map_err(|e| e.to_string())?
        .or_else(|| win.primary_monitor().ok().flatten())
        .ok_or("no monitor")?;
    let wa = monitor.work_area(); // 작업표시줄 제외 영역 기준(배치와 일치)
    let aw = (wa.size.width as f64 - size.width as f64).max(1.0);
    let ah = (wa.size.height as f64 - size.height as f64).max(1.0);
    let cx = (((pos.x - wa.position.x) as f64) / aw).clamp(0.0, 1.0);
    let cy = (((pos.y - wa.position.y) as f64) / ah).clamp(0.0, 1.0);

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            show_overlay,
            hide_overlay,
            preview_overlay,
            display_notification,
            persist_overlay_position,
            get_display_settings,
            set_display_settings,
            list_monitors,
            open_slack,
            notification_access,
            request_notification_access,
            open_notification_settings,
            collect_diagnostics,
            send_diagnostics,
            autostart_enabled,
            set_autostart
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

            // 자동 시작은 패키지 매니페스트의 StartupTask(Enabled=true)가 기본 켜짐으로 처리.
            // 로그인 자동시작은 인자를 못 넘기므로 "첫 실행에만 설정창"으로 판별한다(이후/로그인 = 트레이만).
            let handle = app.handle().clone();
            let first_run = handle
                .path()
                .app_config_dir()
                .ok()
                .map(|dir| {
                    let _ = fs::create_dir_all(&dir);
                    let marker = dir.join(".first-run-done");
                    let first = !marker.exists();
                    if first {
                        let _ = fs::write(&marker, "1");
                    }
                    first
                })
                .unwrap_or(true);
            if first_run {
                show_settings(&handle);
            }

            // 진단 로깅 초기화(notifier 가 기록하므로 먼저). 그 외 플랫폼은 no-op.
            diag::init(&handle);
            // Windows OS 알림(슬랙) 폴링 시작. 그 외 플랫폼은 no-op.
            notifier::start(handle);
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
