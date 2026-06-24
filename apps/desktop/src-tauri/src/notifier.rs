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

    /// 슬랙 알림 식별. 설치 형태별로 AUMID 가 다르다:
    ///  - 공식 .exe / 구 스토어 : com.tinyspeck.slackdesktop_8yrtsj140pw4g!Slack
    ///  - 신 Microsoft Store    : 91750D7E.Slack_8she8kybcnzg4!App
    /// 식별/사용 형태별로 신호가 다르다:
    ///  - 공식 .exe / 구 스토어 : AUMID com.tinyspeck.slackdesktop_...!Slack
    ///  - 신 Microsoft Store    : AUMID 91750D7E.Slack_...!App
    ///  - 브라우저(Chrome PWA)  : AUMID 는 Chrome._crx_... 라 "slack" 이 없고, 표시이름이 "Slack"/"슬랙"
    /// → AUMID 에 "slack" 포함 OR 표시이름이 Slack/슬랙(로캘) 이면 슬랙으로 본다.
    fn is_slack(aumid: &str, app_name: &str) -> bool {
        let name = app_name.trim();
        aumid.to_ascii_lowercase().contains("slack")
            || name.eq_ignore_ascii_case("slack")
            || name == "슬랙"
    }

    pub fn start(app: AppHandle) {
        thread::spawn(move || {
            // WinRT 아파트(MTA). 폴링은 .get() 블로킹이라 MTA 로 충분.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let listener = match UserNotificationListener::Current() {
                Ok(l) => l,
                Err(e) => {
                    crate::diag::log(&format!("listener Current() 실패: {e:?}"));
                    return;
                }
            };
            // 권한이 없으면 1회 요청(이미 Allowed 면 통과). 실패해도 폴링은 시도.
            if listener.GetAccessStatus() != Ok(UserNotificationListenerAccessStatus::Allowed) {
                if let Ok(op) = listener.RequestAccessAsync() {
                    let _ = op.get();
                }
            }
            crate::diag::log(&format!(
                "listener 시작: access={} pkg={} focus={}",
                access_status(),
                crate::diag::package_status(),
                crate::diag::focus_status()
            ));

            let mut seen: HashSet<u32> = HashSet::new(); // 직전 폴링에 존재하던 모든 id
            let mut shown: HashSet<u32> = HashSet::new(); // 우리가 오버레이를 띄운 슬랙 메시지 id

            // 시작 시점에 이미 떠 있던 알림은 baseline 으로 기록(오버레이 안 띄움).
            // 앱을 켤 때 기존 알림들이 한꺼번에 뜨는 것을 방지 — 이후 새로 도착하는 것만 반응.
            if let Ok(op) = listener.GetNotificationsAsync(NotificationKinds::Toast) {
                if let Ok(notifs) = op.get() {
                    if let Ok(size) = notifs.Size() {
                        for i in 0..size {
                            if let Ok(n) = notifs.GetAt(i) {
                                if let Ok(id) = n.Id() {
                                    seen.insert(id);
                                }
                            }
                        }
                    }
                }
            }

            let mut last_err = String::new();
            loop {
                if let Err(e) = poll(&app, &listener, &mut seen, &mut shown) {
                    crate::diag::inc_err();
                    let es = format!("{e:?}");
                    if es != last_err {
                        crate::diag::log(&format!("poll 에러: {es}"));
                        last_err = es;
                    }
                }
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
        crate::diag::inc_poll();
        let notifs: IVectorView<UserNotification> =
            listener.GetNotificationsAsync(NotificationKinds::Toast)?.get()?;

        let mut current: Vec<(u32, UserNotification)> = Vec::new();
        for i in 0..notifs.Size()? {
            let n = notifs.GetAt(i)?;
            current.push((n.Id()?, n));
        }
        let current_ids: HashSet<u32> = current.iter().map(|(id, _)| *id).collect();
        crate::diag::note_poll(current.len());

        // ── Added: 이번에 새로 나타난 알림 ─────────────────────────────────
        for (id, n) in &current {
            if seen.contains(id) {
                continue;
            }
            let aumid = aumid(n).unwrap_or_default();
            let app_label = app_name(n).unwrap_or_default();
            crate::diag::inc_seen();
            crate::diag::note_aumid(&aumid); // 슬랙 외 포함 — 어떤 앱이 알림 내는지
            if !is_slack(&aumid, &app_label) {
                crate::diag::log(&format!("notif aumid={aumid} app={app_label:?} isSlack=N → skip(슬랙아님)"));
                continue; // 슬랙 외 앱(설정 등) 무시
            }
            crate::diag::inc_slack();
            let (title, body) = text(n);
            // 제목/본문이 모두 있어야 실제 메시지로 본다.
            // (슬랙 시스템 공지가 드물게 통과할 순 있으나, "놓침"을 막는 게 우선 — false negative 회피)
            if title.trim().is_empty() || body.trim().is_empty() {
                crate::diag::inc_empty();
                crate::diag::log(&format!(
                    "notif aumid={aumid} app={app_label:?} tLen={} bLen={} isSlack=Y → skip(제목/본문 빔)",
                    title.trim().len(),
                    body.trim().len()
                ));
                continue;
            }
            // 사용자가 슬랙을 보고 있으면 억제(오버레이 안 띄움).
            // seen 은 루프 끝에서 일괄 갱신되므로 재평가되지 않음.
            if foreground_is_slack() {
                crate::diag::inc_suppress();
                crate::diag::log(&format!("notif aumid={aumid} app={app_label:?} isSlack=Y → skip(슬랙 포커스 억제)"));
                continue;
            }
            let payload = serde_json::json!({
                "id": format!("win:{id}"),
                "trigger": "mention",
                "title": title,
                "body": body,
                "app": app_label,
                "aumid": aumid,
                // OS 알림엔 정밀 딥링크가 없음 → 클릭 시 aumid 로 슬랙 열기(open_slack)
                "deepLink": "",
                "createdAt": 0,
                "source": "os"
            });
            let _ = crate::push_overlay(app, payload);
            crate::diag::inc_push();
            crate::diag::log(&format!(
                "notif aumid={aumid} app={app_label:?} tLen={} bLen={} → PUSH(오버레이)",
                title.trim().len(),
                body.trim().len()
            ));
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

    /// 진단용: 현재 액션센터의 알림을 메타데이터만으로 덤프(내용 없음).
    /// COM 미초기화 스레드에서 호출될 수 있어, MTA 전용 스레드에서 수행 후 join.
    pub fn snapshot() -> String {
        let h = thread::spawn(|| {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let work = || -> windows::core::Result<String> {
                let listener = UserNotificationListener::Current()?;
                let notifs = listener.GetNotificationsAsync(NotificationKinds::Toast)?.get()?;
                let size = notifs.Size()?;
                let mut out = format!("총 {size}개\n");
                for i in 0..size {
                    let n = notifs.GetAt(i)?;
                    let aumid = aumid(&n).unwrap_or_default();
                    let app = app_name(&n).unwrap_or_default();
                    crate::diag::note_aumid(&aumid);
                    let (t, b) = text(&n);
                    out.push_str(&format!(
                        "  aumid={aumid} app={app:?} tLen={} bLen={} isSlack={}\n",
                        t.trim().len(),
                        b.trim().len(),
                        if is_slack(&aumid, &app) { "Y" } else { "N" }
                    ));
                }
                Ok(out)
            };
            work().unwrap_or_else(|e| format!("(스냅샷 실패: {e:?})"))
        });
        h.join().unwrap_or_else(|_| "(스냅샷 스레드 패닉)".into())
    }
}

#[cfg(target_os = "windows")]
pub use imp::{access_status, request_access, snapshot, start};

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
#[cfg(not(target_os = "windows"))]
pub fn snapshot() -> String {
    "(Windows 전용)".into()
}
