//! Windows OS 알림(UserNotificationListener) 폴링 → 슬랙 메시지 감지 → 오버레이 트리거.
//!
//! 검증 완료(spike/rust-notif-poc): package identity 불필요, 1초 폴링, .get() 블로킹.
//! NotificationChanged 이벤트는 데스크톱에서 불안정해 폴링 방식 사용.
//! Windows 외 플랫폼에선 no-op.

#[cfg(target_os = "windows")]
mod imp {
    use std::collections::HashSet;
    use std::thread;
    use std::time::Duration;

    use tauri::AppHandle;
    use windows::core::PWSTR;
    use windows::Foundation::Collections::IVectorView;
    use windows::UI::Notifications::Management::{
        UserNotificationListener, UserNotificationListenerAccessStatus,
    };
    use windows::UI::Notifications::{KnownNotificationBindings, NotificationKinds, UserNotification};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    /// 슬랙 데스크톱 AUMID 접두사 (com.tinyspeck.slackdesktop_xxx!Slack)
    const SLACK_AUMID: &str = "com.tinyspeck.slackdesktop";

    pub fn start(app: AppHandle) {
        thread::spawn(move || {
            // WinRT 아파트(MTA). 폴링은 .get() 블로킹이라 MTA 로 충분.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let listener = match UserNotificationListener::Current() {
                Ok(l) => l,
                Err(_) => return,
            };
            // 권한이 없으면 1회 요청(이미 Allowed 면 통과). 실패해도 폴링은 시도.
            if listener.GetAccessStatus() != Ok(UserNotificationListenerAccessStatus::Allowed) {
                if let Ok(op) = listener.RequestAccessAsync() {
                    let _ = op.get();
                }
            }

            let mut seen: HashSet<u32> = HashSet::new(); // 직전 폴링에 존재하던 모든 id
            let mut shown: HashSet<u32> = HashSet::new(); // 우리가 오버레이를 띄운 슬랙 메시지 id
            loop {
                let _ = poll(&app, &listener, &mut seen, &mut shown);
                thread::sleep(Duration::from_secs(1));
            }
        });
    }

    fn poll(
        app: &AppHandle,
        listener: &UserNotificationListener,
        seen: &mut HashSet<u32>,
        shown: &mut HashSet<u32>,
    ) -> windows::core::Result<()> {
        let notifs: IVectorView<UserNotification> =
            listener.GetNotificationsAsync(NotificationKinds::Toast)?.get()?;

        let mut current: Vec<(u32, UserNotification)> = Vec::new();
        for i in 0..notifs.Size()? {
            let n = notifs.GetAt(i)?;
            current.push((n.Id()?, n));
        }
        let current_ids: HashSet<u32> = current.iter().map(|(id, _)| *id).collect();

        // ── Added: 이번에 새로 나타난 알림 ─────────────────────────────────
        for (id, n) in &current {
            if seen.contains(id) {
                continue;
            }
            let aumid = aumid(n).unwrap_or_default();
            if !aumid.starts_with(SLACK_AUMID) {
                continue; // 슬랙 외 앱(설정 등) 무시
            }
            let (title, body) = text(n);
            // 제목/본문이 모두 있어야 실제 메시지로 본다.
            // (슬랙 시스템 공지가 드물게 통과할 순 있으나, "놓침"을 막는 게 우선 — false negative 회피)
            if title.trim().is_empty() || body.trim().is_empty() {
                continue;
            }
            // 사용자가 슬랙을 보고 있으면 억제(오버레이 안 띄움).
            // seen 은 루프 끝에서 일괄 갱신되므로 재평가되지 않음.
            if foreground_is_slack() {
                continue;
            }
            let payload = serde_json::json!({
                "id": format!("win:{id}"),
                "trigger": "mention",
                "title": title,
                "body": body,
                "app": app_name(n).unwrap_or_default(),
                "aumid": aumid,
                // OS 알림엔 정밀 딥링크가 없음 → 클릭 시 aumid 로 슬랙 열기(open_slack)
                "deepLink": "",
                "createdAt": 0,
                "source": "os"
            });
            let _ = crate::push_overlay(app, payload);
            shown.insert(*id);
        }

        // ── 슬랙을 포커스하면, 알림을 안 읽었어도(Removed 없어도) 떠 있는 오버레이를 닫는다 ──
        if foreground_is_slack() && !shown.is_empty() {
            for id in shown.drain() {
                let _ = crate::dismiss_overlay(app, &format!("win:{id}"));
            }
        }

        // ── Removed: 사라진 알림(슬랙에서 읽음/해제) → 오버레이 닫기 ──────────
        let removed: Vec<u32> = seen.iter().filter(|id| !current_ids.contains(id)).copied().collect();
        for id in removed {
            if shown.remove(&id) {
                let _ = crate::dismiss_overlay(app, &format!("win:{id}"));
            }
        }

        *seen = current_ids;
        Ok(())
    }

    fn aumid(n: &UserNotification) -> Option<String> {
        Some(n.AppInfo().ok()?.AppUserModelId().ok()?.to_string())
    }

    fn app_name(n: &UserNotification) -> Option<String> {
        Some(n.AppInfo().ok()?.DisplayInfo().ok()?.DisplayName().ok()?.to_string())
    }

    /// 토스트 바인딩 텍스트 추출(첫 요소=제목, 나머지=본문).
    fn text(n: &UserNotification) -> (String, String) {
        let extract = || -> windows::core::Result<(String, String)> {
            let binding = n
                .Notification()?
                .Visual()?
                .GetBinding(&KnownNotificationBindings::ToastGeneric()?)?;
            let texts = binding.GetTextElements()?;
            let mut title = String::new();
            let mut body = Vec::new();
            for i in 0..texts.Size()? {
                let t = texts.GetAt(i)?.Text()?.to_string();
                if i == 0 {
                    title = t;
                } else {
                    body.push(t);
                }
            }
            Ok((title, body.join("\n")))
        };
        extract().unwrap_or_default()
    }

    fn foreground_is_slack() -> bool {
        foreground_process_name().eq_ignore_ascii_case("slack")
    }

    fn foreground_process_name() -> String {
        unsafe {
            let hwnd = GetForegroundWindow();
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return String::new();
            }
            let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(h) => h,
                Err(_) => return String::new(),
            };
            let mut buf = [0u16; 260];
            let mut len = buf.len() as u32;
            let r =
                QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
            let _ = CloseHandle(handle);
            if r.is_err() {
                return String::new();
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let file = path.rsplit(['\\', '/']).next().unwrap_or(&path);
            file.strip_suffix(".exe")
                .or_else(|| file.strip_suffix(".EXE"))
                .unwrap_or(file)
                .to_string()
        }
    }

    /// 알림 접근 권한 상태: "allowed" | "denied" | "unspecified".
    pub fn access_status() -> &'static str {
        match UserNotificationListener::Current().and_then(|l| l.GetAccessStatus()) {
            Ok(s) => map_status(s),
            Err(_) => "unspecified",
        }
    }

    /// 권한 요청(동의창). 결과 상태 문자열 반환.
    /// 동의창은 UI 컨텍스트가 필요해 안 뜰 수 있으며, 그 경우 설정 페이지로 유도한다.
    pub fn request_access() -> &'static str {
        let r = (|| -> windows::core::Result<UserNotificationListenerAccessStatus> {
            UserNotificationListener::Current()?.RequestAccessAsync()?.get()
        })();
        match r {
            Ok(s) => map_status(s),
            Err(_) => "unspecified",
        }
    }

    fn map_status(s: UserNotificationListenerAccessStatus) -> &'static str {
        if s == UserNotificationListenerAccessStatus::Allowed {
            "allowed"
        } else if s == UserNotificationListenerAccessStatus::Denied {
            "denied"
        } else {
            "unspecified"
        }
    }
}

#[cfg(target_os = "windows")]
pub use imp::{access_status, request_access, start};

// Windows 외 플랫폼: no-op / 미지원 (개발 빌드용).
#[cfg(not(target_os = "windows"))]
pub fn start(_app: tauri::AppHandle) {}
#[cfg(not(target_os = "windows"))]
pub fn access_status() -> &'static str {
    "unsupported"
}
#[cfg(not(target_os = "windows"))]
pub fn request_access() -> &'static str {
    "unsupported"
}
