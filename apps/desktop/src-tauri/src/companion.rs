//! Local companion state. The webview is a renderer, not the timer clock.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub version: u8,
    pub onboarded: bool,
    pub resident: bool,
    pub hide_fullscreen: bool,
    /// Hide while a display is mirrored or a projector is connected.
    pub hide_presenting: bool,
    pub private_content: bool,
    pub stretch: bool,
    pub stretch_minutes: u64,
    pub quiet_until: u64,
    pub timer_during_quiet: bool,
    pub calendar_minutes: u64, // Legacy single reminder; preserved when upgrading.
    pub calendar_reminders: Vec<u64>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            onboarded: false,
            resident: false,
            hide_fullscreen: true,
            hide_presenting: true,
            private_content: false,
            stretch: false,
            stretch_minutes: 50,
            quiet_until: 0,
            timer_during_quiet: true,
            calendar_minutes: 5,
            calendar_reminders: Vec::new(),
        }
    }
}
impl Preferences {
    pub fn reminders(&self) -> Vec<u64> {
        let mut values = if self.calendar_reminders.is_empty() {
            vec![self.calendar_minutes]
        } else {
            self.calendar_reminders.clone()
        };
        values.sort_unstable_by(|a, b| b.cmp(a));
        values.dedup();
        values
    }
}
fn normalize_reminders(values: &mut Vec<u64>) -> Result<(), String> {
    if values.is_empty() || values.len() > 8 || values.iter().any(|n| *n > 1440) {
        return Err("일정 알림은 0~1440분 사이로 1~8개 설정해 주세요".into());
    }
    values.sort_unstable_by(|a, b| b.cmp(a));
    values.dedup();
    Ok(())
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Timer {
    pub duration: u64,
    pub deadline: Option<u64>,
    pub remaining: u64,
    pub completed: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Persisted {
    pub preferences: Preferences,
    pub timer: Timer,
}
#[derive(Default)]
pub struct State {
    pub saved: Persisted,
    pub alerts: Vec<Value>,
    pub ready: bool,
    pub active_seconds: u64,
    pub stretch_snooze: u64,
    pub snoozed: HashMap<String, (u64, Value)>,
    pub hit_regions: Vec<[f64; 4]>,
    pub interacting: bool,
    pub pet_anchor: Option<(f64, f64)>,
    pub pet_location: Option<(String, f64, f64)>,
    pub fullscreen: bool,
    pub presenting: bool,
}
impl State {
    /// The one place that decides when the character and its alerts stay off screen.
    pub fn hidden(&self) -> bool {
        (self.fullscreen && self.saved.preferences.hide_fullscreen)
            || (self.presenting && self.saved.preferences.hide_presenting)
    }
}
pub struct Companion(pub Mutex<State>);

fn write(app: &AppHandle, saved: &Persisted) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // All writes are serialized by Companion's lock; retain old data if writing fails.
    let tmp = dir.join("companion.next.json");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(saved).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(tmp, dir.join("companion.json")).map_err(|e| e.to_string())
}
pub fn init(app: &AppHandle) {
    let saved = app
        .path()
        .app_config_dir()
        .ok()
        .and_then(|p| std::fs::read(p.join("companion.json")).ok())
        .and_then(|s| serde_json::from_slice(&s).ok())
        .unwrap_or_default();
    app.manage(Companion(Mutex::new(State {
        saved,
        ..Default::default()
    })));
}
pub fn resident(app: &AppHandle) -> bool {
    let state = app.state::<Companion>();
    let s = state.0.lock().unwrap();
    s.saved.preferences.onboarded && s.saved.preferences.resident
}
pub fn emit(app: &AppHandle) {
    let _ = app.emit("companion-state", snapshot(app.clone()));
}
#[tauri::command]
pub fn snapshot(app: AppHandle) -> Value {
    let state = app.state::<Companion>();
    let s = state.0.lock().unwrap();
    json!({"preferences":s.saved.preferences,"timer":s.saved.timer,"alerts":s.alerts,"now":now(),"fullscreen":s.fullscreen,"presenting":s.presenting,"hidden":s.hidden()})
}
#[tauri::command]
pub fn set_preferences(app: AppHandle, patch: Value) -> Result<(), String> {
    let state = app.state::<Companion>();
    let was_resident = resident(&app);
    let mut deferred_calendar = Vec::new();
    {
        let mut s = state.0.lock().unwrap();
        let mut value = serde_json::to_value(&s.saved.preferences).unwrap();
        for (key, v) in patch.as_object().ok_or("설정 형식이 올바르지 않아요")? {
            if value.get(key).is_none() || key == "version" {
                return Err("알 수 없는 설정이에요".into());
            }
            value[key] = v.clone();
        }
        let mut p: Preferences =
            serde_json::from_value(value).map_err(|_| "설정 값이 올바르지 않아요")?;
        p.stretch_minutes = p.stretch_minutes.clamp(10, 180);
        if ![1, 5, 10, 15, 30].contains(&p.calendar_minutes) {
            return Err("일정 알림 시간을 확인해 주세요".into());
        }
        if patch.get("calendar_reminders").is_some() {
            normalize_reminders(&mut p.calendar_reminders)?;
        }
        p.quiet_until = p.quiet_until.min(now() + 24 * 3600);
        let mut saved = s.saved.clone();
        saved.preferences = p;
        write(&app, &saved)?;
        s.saved = saved;
        if !s.saved.preferences.stretch {
            s.alerts.retain(|a| a["source"] != "stretch");
            s.snoozed.retain(|_, v| v.1["source"] != "stretch");
            s.active_seconds = 0;
        }
        if s.saved.preferences.quiet_until > now() {
            deferred_calendar = s
                .alerts
                .iter()
                .filter(|a| a["source"] == "calendar")
                .filter_map(|a| a["id"].as_str().map(str::to_string))
                .collect();
            let keep_timer = s.saved.preferences.timer_during_quiet;
            s.alerts
                .retain(|a| a["source"] == "preview" || (keep_timer && a["source"] == "timer"));
        }
    }
    crate::calendar::defer_unread(&app, &deferred_calendar);
    if was_resident != resident(&app) {
        let _ = crate::apply_overlay_layout(&app);
    }
    emit(&app);
    Ok(())
}
pub fn push(app: &AppHandle, mut payload: Value) -> Result<(), String> {
    // Returning success still acknowledges relay delivery; do not replay after focus leaves.
    if payload["source"] == "slack" && crate::foreground::slack_active() {
        return Ok(());
    }
    {
        let state = app.state::<Companion>();
        let mut s = state.0.lock().unwrap();
        let p = &s.saved.preferences;
        if payload["source"] != "preview"
            && p.quiet_until > now()
            && !(payload["source"] == "timer" && p.timer_during_quiet)
        {
            return Ok(());
        }
        if payload
            .get("createdAt")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            == 0
        {
            payload["createdAt"] = json!(now() * 1000);
        }
        if payload["source"] == "slack" && payload["expiresAt"].as_u64().is_none() {
            payload["expiresAt"] = json!(now() + 90);
        }
        if !s.alerts.iter().any(|a| a["id"] == payload["id"]) {
            if s.alerts.len() >= 50 {
                s.alerts.remove(0);
            }
            s.alerts.push(payload);
        }
    }
    emit(app);
    Ok(())
}
#[tauri::command]
pub fn dismiss_alert(
    app: AppHandle,
    id: String,
    snooze_seconds: Option<u64>,
) -> Result<(), String> {
    {
        let state = app.state::<Companion>();
        let mut s = state.0.lock().unwrap();
        if let Some(i) = s.alerts.iter().position(|a| a["id"] == id) {
            let alert = s.alerts.remove(i);
            if let Some(seconds) = snooze_seconds.filter(|v| *v > 0) {
                if alert["source"] == "calendar" || alert["source"] == "stretch" {
                    s.snoozed
                        .insert(id, (now() + seconds.min(600), alert.clone()));
                }
            }
            if alert["source"] == "timer" {
                s.saved.timer.completed = false;
                write(&app, &s.saved)?;
            }
            if alert["source"] == "stretch" {
                s.active_seconds = 0;
            }
        }
    }
    emit(&app);
    Ok(())
}
#[tauri::command]
pub fn timer_action(app: AppHandle, action: String, minutes: Option<u64>) -> Result<(), String> {
    {
        let state = app.state::<Companion>();
        let mut s = state.0.lock().unwrap();
        let mut timer = s.saved.timer.clone();
        match action.as_str() {
            "start" => {
                let n = minutes
                    .filter(|n| *n >= 1 && *n <= 1440)
                    .ok_or("1~1440분을 입력해 주세요")?;
                timer = Timer {
                    duration: n * 60,
                    deadline: Some(now() + n * 60),
                    remaining: n * 60,
                    completed: false,
                };
            }
            "pause" => {
                if let Some(d) = timer.deadline.take() {
                    timer.remaining = d.saturating_sub(now());
                }
            }
            "resume" => {
                if timer.deadline.is_none() && timer.remaining > 0 && !timer.completed {
                    timer.deadline = Some(now() + timer.remaining);
                }
            }
            "cancel" => timer = Timer::default(),
            _ => return Err("알 수 없는 타이머 동작이에요".into()),
        }
        let mut saved = s.saved.clone();
        saved.timer = timer;
        write(&app, &saved)?;
        s.saved = saved;
        s.alerts.retain(|a| a["source"] != "timer");
    }
    if action == "start" {
        crate::native_pet::acknowledge_timer(&app);
    }
    emit(&app);
    Ok(())
}
#[tauri::command]
pub fn overlay_ready(app: AppHandle) -> Value {
    {
        let state = app.state::<Companion>();
        state.0.lock().unwrap().ready = true;
    }
    let _ = crate::apply_overlay_layout(&app);
    snapshot(app)
}
#[tauri::command]
pub fn overlay_regions(app: AppHandle, regions: Vec<[f64; 4]>, interacting: bool) {
    let state = app.state::<Companion>();
    let mut s = state.0.lock().unwrap();
    s.hit_regions = regions
        .into_iter()
        .take(12)
        .filter(|r| r.iter().all(|v| v.is_finite()))
        .collect();
    s.interacting = interacting;
}
#[tauri::command]
pub fn open_settings(app: AppHandle) {
    crate::show_settings(&app);
}

/// Seconds since any keyboard/mouse input, or `u64::MAX` when unknown. Needs no
/// permission and reveals nothing about which keys or where; shared by stretch
/// reminders and the character's dozing.
#[cfg(target_os = "windows")]
pub(crate) fn idle_seconds() -> u64 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut input = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        if GetLastInputInfo(&mut input).as_bool() {
            GetTickCount().wrapping_sub(input.dwTime) as u64 / 1000
        } else {
            u64::MAX
        }
    }
}
#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: i32, event: u32) -> f64;
}
#[cfg(target_os = "macos")]
pub(crate) fn idle_seconds() -> u64 {
    unsafe { CGEventSourceSecondsSinceLastEventType(1, u32::MAX).max(0.0) as u64 }
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn idle_seconds() -> u64 {
    u64::MAX
}
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last = now();
        let mut last_passthrough = None;
        let mut ticks = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(50));
            ticks += 1;
            // Poll outside the webview so click-through can be turned OFF again.
            if let Some(win) = app.get_webview_window("overlay") {
                if let (Ok(cursor), Ok(pos), Ok(sf)) = (
                    win.cursor_position(),
                    win.outer_position(),
                    win.scale_factor(),
                ) {
                    // Window queries may synchronously dispatch to the UI thread.
                    // Never hold Companion while making one: native tick needs this lock.
                    #[cfg(target_os = "macos")]
                    let cursor_sf = win
                        .primary_monitor()
                        .ok()
                        .flatten()
                        .map_or(sf, |m| m.scale_factor());
                    #[cfg(not(target_os = "macos"))]
                    let cursor_sf = sf;
                    let x = cursor.x / cursor_sf - pos.x as f64 / sf;
                    let y = cursor.y / cursor_sf - pos.y as f64 / sf;
                    let state = app.state::<Companion>();
                    let s = state.0.lock().unwrap();
                    let ignore = !s.interacting
                        && !s.hit_regions.iter().any(|r| {
                            x >= r[0] && y >= r[1] && x <= r[0] + r[2] && y <= r[1] + r[3]
                        });
                    drop(s);
                    if last_passthrough != Some(ignore) {
                        if win.set_ignore_cursor_events(ignore).is_ok() {
                            last_passthrough = Some(ignore);
                        }
                    }
                }
            }
            if ticks % 20 != 0 {
                continue;
            }
            let slack_active = crate::foreground::slack_active();
            // Query window APIs before locking Companion (UI-thread calls must never wait on this lock).
            let target = app
                .get_webview_window("overlay")
                .and_then(|w| w.current_monitor().ok().flatten())
                .map(|m| {
                    #[cfg(target_os = "macos")]
                    let unit = m.scale_factor();
                    #[cfg(not(target_os = "macos"))]
                    let unit = 1.;
                    [
                        m.position().x as f64 / unit,
                        m.position().y as f64 / unit,
                        m.size().width as f64 / unit,
                        m.size().height as f64 / unit,
                    ]
                });
            let fullscreen = crate::fullscreen::active(target);
            // Display probing runs on the UI thread and topology rarely changes:
            // refresh every five seconds, read last time's answer here.
            if ticks % 100 == 0 {
                crate::presenting::schedule(&app);
            }
            let presenting = crate::presenting::current();
            let full_changed = {
                let state = app.state::<Companion>();
                let mut s = state.0.lock().unwrap();
                let before = s.alerts.len();
                if slack_active {
                    s.alerts.retain(|a| a["source"] != "slack");
                }
                let changed = s.fullscreen != fullscreen
                    || s.presenting != presenting
                    || before != s.alerts.len();
                s.fullscreen = fullscreen;
                s.presenting = presenting;
                changed
            };
            if full_changed {
                emit(&app);
            }
            let current = now();
            let dt = current.saturating_sub(last);
            last = current;
            let mut alerts = Vec::new();
            let mut changed = false;
            let meeting = crate::calendar::in_meeting(&app, current);
            {
                let state = app.state::<Companion>();
                let mut s = state.0.lock().unwrap();
                if s.saved.timer.deadline.is_some_and(|d| d <= current) {
                    s.saved.timer.deadline = None;
                    s.saved.timer.remaining = 0;
                    s.saved.timer.completed = true;
                    let _ = write(&app, &s.saved);
                    changed = true;
                }
                if s.saved.timer.completed && !s.alerts.iter().any(|a| a["id"] == "timer:complete")
                {
                    alerts.push(json!({"id":"timer:complete","source":"timer","trigger":"timer","title":"약속한 시간이 됐어요!","body":format!("{}분 동안 수고했어요. 잠깐 쉬어볼까요?",s.saved.timer.duration/60)}));
                }
                let idle = idle_seconds();
                if idle >= 300 || dt > 15 {
                    s.active_seconds = 0;
                } else if s.saved.preferences.stretch {
                    s.active_seconds += dt;
                }
                if s.saved.preferences.stretch
                    && !meeting
                    && current >= s.stretch_snooze
                    && s.active_seconds >= s.saved.preferences.stretch_minutes * 60
                {
                    s.active_seconds = 0;
                    alerts.push(json!({"id":"stretch:break","source":"stretch","trigger":"stretch","title":"우리, 쭉— 펴볼까요?","body":"PC 사용 시간이 길어졌어요. 어깨를 내리고 팔을 위로 쭉 펴보세요.","expiresAt":current+30}));
                }
                let due: Vec<_> = s
                    .snoozed
                    .iter()
                    .filter(|(_, v)| v.0 <= current && s.saved.preferences.quiet_until <= current)
                    .map(|(k, _)| k.clone())
                    .collect();
                for id in due {
                    if let Some((_, mut alert)) = s.snoozed.remove(&id) {
                        if alert["source"] == "stretch" {
                            alert["expiresAt"] = json!(current + 30);
                        }
                        if !alert["endsAt"].as_u64().is_some_and(|end| end <= current) {
                            alerts.push(alert);
                        }
                    }
                }
                let n = s.alerts.len();
                s.alerts
                    .retain(|a| !a["expiresAt"].as_u64().is_some_and(|e| e <= current));
                changed |= n != s.alerts.len();
            }
            for alert in alerts {
                let _ = push(&app, alert);
            }
            if changed {
                emit(&app);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_empty_settings_are_safe() {
        let p: Persisted = serde_json::from_str("{}").unwrap();
        assert!(!p.preferences.resident);
        assert!(!p.preferences.onboarded);
        assert_eq!(p.preferences.stretch_minutes, 50);
    }
    #[test]
    fn paused_timer_roundtrips() {
        let t = Timer {
            duration: 300,
            deadline: None,
            remaining: 124,
            completed: false,
        };
        let decoded: Timer = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(decoded.remaining, 124);
        assert_eq!(decoded.deadline, None);
    }
}

#[cfg(test)]
mod reminder_settings_tests {
    use super::*;
    #[test]
    fn single_reminder_migrates_and_multi_reminders_are_unique() {
        let old: Preferences = serde_json::from_value(json!({"calendar_minutes":15})).unwrap();
        assert_eq!(old.reminders(), vec![15]);
        let mut values = vec![10, 30, 10, 0];
        normalize_reminders(&mut values).unwrap();
        assert_eq!(values, vec![30, 10, 0]);
        assert!(normalize_reminders(&mut vec![]).is_err());
        assert!(normalize_reminders(&mut vec![1441]).is_err());
    }
}
