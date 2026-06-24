//! 진단 로깅 — 이벤트 링버퍼 + 롤링 파일(~2MB) + 리포트 조립.
//!
//! 원칙: 메시지 제목/본문/토큰 등 "내용"은 절대 기록하지 않는다.
//! 식별자(AUMID/앱이름)·길이·상태·카운트만 남긴다.

#[cfg(target_os = "windows")]
mod imp {
    use std::collections::{BTreeSet, VecDeque};
    use std::fs;
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    use tauri::{AppHandle, Manager};
    use windows::core::PWSTR;
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    use windows::Win32::UI::Shell::SHQueryUserNotificationState;

    const MAX_EVENTS: usize = 400;
    const MAX_LOG_BYTES: u64 = 1_000_000; // 1MB 초과 시 회전(.1 보관) → 총 ~2MB

    static EVENTS: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
    static LOG_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    static STARTED: OnceLock<String> = OnceLock::new();
    /// 이 세션에서 새 알림을 낸 모든 앱의 AUMID(슬랙 외 포함) — "다른 앱은 되는데 슬랙만?" 판별용.
    static AUMIDS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    static LAST_POLL: AtomicU64 = AtomicU64::new(0); // 마지막 폴링 epoch초(루프 생존 확인)
    static MAX_NOTIFS: AtomicU64 = AtomicU64::new(0); // 한 폴링에서 본 최대 알림 수

    static C_POLL: AtomicU64 = AtomicU64::new(0);
    static C_SEEN: AtomicU64 = AtomicU64::new(0);
    static C_SLACK: AtomicU64 = AtomicU64::new(0);
    static C_PUSH: AtomicU64 = AtomicU64::new(0);
    static C_SUPPRESS: AtomicU64 = AtomicU64::new(0);
    static C_EMPTY: AtomicU64 = AtomicU64::new(0);
    static C_ERR: AtomicU64 = AtomicU64::new(0);

    pub fn inc_poll() { C_POLL.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_seen() { C_SEEN.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_slack() { C_SLACK.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_push() { C_PUSH.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_suppress() { C_SUPPRESS.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_empty() { C_EMPTY.fetch_add(1, Ordering::Relaxed); }
    pub fn inc_err() { C_ERR.fetch_add(1, Ordering::Relaxed); }

    /// 새 알림을 낸 앱 AUMID 기록(슬랙 외 포함).
    pub fn note_aumid(a: &str) {
        if !a.is_empty() {
            AUMIDS.lock().unwrap().insert(a.to_string());
        }
    }
    /// 폴링 1회 — 마지막 폴링 시각 + 본 알림 최대치 갱신.
    pub fn note_poll(count: usize) {
        LAST_POLL.store(now_secs(), Ordering::Relaxed);
        MAX_NOTIFS.fetch_max(count as u64, Ordering::Relaxed);
    }

    fn now_secs() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }
    fn now_hms() -> String {
        let t = now_secs() % 86_400;
        format!("{:02}:{:02}:{:02}", t / 3600, (t % 3600) / 60, t % 60)
    }

    pub fn init(app: &AppHandle) {
        let path = app.path().app_local_data_dir().ok().map(|d| {
            let logs = d.join("logs");
            let _ = fs::create_dir_all(&logs);
            logs.join("diag.log")
        });
        if let Some(p) = &path {
            rotate_if_big(p);
        }
        let _ = LOG_PATH.set(path);
        let _ = STARTED.set(now_hms());
    }

    fn rotate_if_big(p: &PathBuf) {
        if let Ok(m) = fs::metadata(p) {
            if m.len() > MAX_LOG_BYTES {
                let _ = fs::rename(p, p.with_extension("1.log"));
            }
        }
    }

    pub fn log(msg: &str) {
        let line = format!("{} UTC  {}", now_hms(), msg);
        {
            let mut ev = EVENTS.lock().unwrap();
            ev.push_back(line.clone());
            while ev.len() > MAX_EVENTS {
                ev.pop_front();
            }
        }
        if let Some(Some(p)) = LOG_PATH.get() {
            rotate_if_big(p);
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(f, "{line}");
            }
        }
    }

    /// 런타임 패키지 ID(= Sparse Package 등록이 실제로 먹었는지).
    pub fn package_status() -> String {
        unsafe {
            let mut len: u32 = 0;
            let _ = GetCurrentPackageFullName(&mut len, PWSTR::null());
            if len == 0 {
                return "❌ 없음 (NO_PACKAGE — 신원 미적용)".into();
            }
            let mut buf = vec![0u16; len as usize];
            // 반환형(WIN32_ERROR/u32)이 버전마다 달라 무시하고, 버퍼가 채워졌는지로 판정.
            let _ = GetCurrentPackageFullName(&mut len, PWSTR(buf.as_mut_ptr()));
            if buf.first().copied().unwrap_or(0) != 0 {
                let n = String::from_utf16_lossy(&buf);
                let n = n.trim_end_matches('\0');
                format!("✅ 있음 ({n})")
            } else {
                "❌ 조회실패".into()
            }
        }
    }

    /// 집중 지원/방해금지 등 알림 억제 상태.
    pub fn focus_status() -> String {
        unsafe {
            match SHQueryUserNotificationState() {
                Ok(s) => match s.0 {
                    1 => "NOT_PRESENT (잠금화면 등)".into(),
                    2 => "BUSY".into(),
                    3 => "⚠️ FULLSCREEN_D3D (전체화면 앱/게임 → 알림 억제)".into(),
                    4 => "⚠️ PRESENTATION (발표 모드 → 억제)".into(),
                    5 => "✅ ACCEPTS (정상 수신)".into(),
                    6 => "⚠️ QUIET_TIME (집중 지원/방해 금지 → 억제)".into(),
                    7 => "⚠️ APP_FULLSCREEN (전체화면 앱 → 억제)".into(),
                    n => format!("기타({n})"),
                },
                Err(e) => format!("조회실패({e:?})"),
            }
        }
    }

    /// 레지스트리/프로세스/버전/인증서 등은 PowerShell 한 번으로 수집(블라인드 Rust 위험 축소).
    fn system_section() -> String {
        let ps = r#"$ErrorActionPreference='SilentlyContinue'
$cv = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
'os: ' + $cv.ProductName + ' ' + $cv.DisplayVersion + ' build ' + $cv.CurrentBuild + '.' + $cv.UBR
'locale: ' + (Get-Culture).Name + ' / sys ' + (Get-WinSystemLocale).Name
'elevated: ' + ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
'slack_proc_count: ' + (@(Get-Process slack -ErrorAction SilentlyContinue).Count)

# Windows 푸시 알림 사용자 서비스(멈춰있으면 알림 시스템 자체가 깨짐)
$wpn = Get-Service -Name 'WpnUserService*' -ErrorAction SilentlyContinue | Select-Object -First 1
'WpnUserService: ' + $(if ($wpn) { $wpn.Status } else { 'NOT_FOUND' })

# 알림 전역 마스터 + 그룹정책 차단
$pn = 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\PushNotifications'
'push_ToastEnabled_master: ' + (Get-ItemProperty $pn -ErrorAction SilentlyContinue).ToastEnabled
'policy_NoToast_HKCU: ' + (Get-ItemProperty 'HKCU:\SOFTWARE\Policies\Microsoft\Windows\CurrentVersion\PushNotifications' -ErrorAction SilentlyContinue).NoToastApplicationNotification
'policy_NoToast_HKLM: ' + (Get-ItemProperty 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\CurrentVersion\PushNotifications' -ErrorAction SilentlyContinue).NoToastApplicationNotification
'policy_NoCloud_HKLM: ' + (Get-ItemProperty 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\CurrentVersion\PushNotifications' -ErrorAction SilentlyContinue).NoCloudApplicationNotification

# 자동시작 — 레지스트리 Run(패키지 ID 앱에선 무시될 수 있음) + Task Manager 사용여부 + 첫실행 마커
$run = Get-ItemProperty 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue
$found = $false
if ($run) { $run.PSObject.Properties | Where-Object { $_.Name -notmatch '^PS' -and ($_.Value -match 'ddoktti' -or $_.Name -match 'ddoktti|똑띠') } | ForEach-Object { $script:found = $true; 'autostart_run: ' + $_.Name + ' = ' + $_.Value } }
if (-not $found) { 'autostart_run: (없음 — Run 키 미등록)' }
$sa = Get-ItemProperty 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run' -ErrorAction SilentlyContinue
if ($sa) { $sa.PSObject.Properties | Where-Object { $_.Name -match 'ddoktti|똑띠' } | ForEach-Object { 'autostart_approved_firstbyte(2/3=사용안함): ' + $_.Name + ' = ' + $_.Value[0] } }
'autostart_startup_lnk: ' + (Test-Path (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Startup\똑띠왔어요.lnk'))
'autostart_disabled_pref: ' + (Test-Path (Join-Path $env:APPDATA 'kr.co.plead.ddoktti-here\.autostart-disabled'))

# 슬랙 앱별 알림 설정 — 값 전체 덤프(ShowInActionCenter=0 이면 리스너가 못 봄)
$ns = 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Notifications\Settings'
'toast_ToastEnabled: ' + (Get-ItemProperty $ns -ErrorAction SilentlyContinue).ToastEnabled
Get-ChildItem $ns -ErrorAction SilentlyContinue | Where-Object { $_.PSChildName -match 'slack' } | ForEach-Object {
  $k = $_.PSChildName
  (Get-ItemProperty $_.PSPath).PSObject.Properties | Where-Object { $_.Name -notmatch '^PS' } | ForEach-Object { 'notif_setting: ' + $k + ' ' + $_.Name + '=' + $_.Value }
}

# 시스템에 등록된 슬랙 앱 AUMID(실제 슬랙이 쓰는 식별자 확인)
Get-StartApps -ErrorAction SilentlyContinue | Where-Object { $_.Name -match 'slack' } | ForEach-Object { 'startapp: ' + $_.Name + ' = ' + $_.AppID }
Get-AppxPackage *slack* -ErrorAction SilentlyContinue | ForEach-Object { 'appx_slack: ' + $_.PackageFamilyName }

# WebView2 / 인증서 신뢰
$g = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
$wv = (Get-ItemProperty ('HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\' + $g) -ErrorAction SilentlyContinue).pv
if (-not $wv) { $wv = (Get-ItemProperty ('HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\' + $g) -ErrorAction SilentlyContinue).pv }
'webview2: ' + $wv
'cert_LocalMachine_Root_CN_Plead: ' + (@(Get-ChildItem Cert:\LocalMachine\Root -ErrorAction SilentlyContinue | Where-Object { $_.Subject -eq 'CN=Plead' }).Count)
'cert_LocalMachine_TrustedPeople_CN_Plead: ' + (@(Get-ChildItem Cert:\LocalMachine\TrustedPeople -ErrorAction SilentlyContinue | Where-Object { $_.Subject -eq 'CN=Plead' }).Count)"#;
        match std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", ps])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW — 콘솔 창 숨김
            .output()
        {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout);
                let t = s.trim();
                if t.is_empty() {
                    "(시스템 정보 없음)".into()
                } else {
                    t.to_string()
                }
            }
            Err(e) => format!("(시스템 정보 조회 실패: {e})"),
        }
    }

    fn read_install_log(app: &AppHandle) -> String {
        let Ok(dir) = app.path().app_local_data_dir() else {
            return "(없음)".into();
        };
        let p = dir.join("logs").join("install.log");
        match fs::read_to_string(&p) {
            Ok(s) => {
                let lines: Vec<&str> = s.lines().collect();
                let tail = if lines.len() > 30 { &lines[lines.len() - 30..] } else { &lines[..] };
                tail.join("\n")
            }
            Err(_) => "(설치 로그 없음)".into(),
        }
    }

    pub fn collect(app: &AppHandle) -> String {
        let pc = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "(unknown)".into());
        let started = STARTED.get().cloned().unwrap_or_default();
        let events: Vec<String> = EVENTS.lock().unwrap().iter().cloned().collect();
        let ev_txt = if events.is_empty() {
            "(아직 기록된 이벤트 없음)".to_string()
        } else {
            events.join("\n")
        };
        let aumids: Vec<String> = AUMIDS.lock().unwrap().iter().cloned().collect();
        let aumids_txt = if aumids.is_empty() {
            "(이 세션에서 관찰된 새 알림 없음)".to_string()
        } else {
            aumids.join("\n  ")
        };
        let last_poll = LAST_POLL.load(Ordering::Relaxed);
        let poll_age = if last_poll == 0 {
            "(폴링 기록 없음 — 리스너 미동작?)".to_string()
        } else {
            format!("{}초 전", now_secs().saturating_sub(last_poll))
        };
        const GUIDE: &str = "슬랙 AUMID 가 [스냅샷]/[관찰된 AUMID]에 전혀 없고 다른 앱은 보이면:\n\
             - 슬랙 인앱 '환경설정 > 알림 > 알림 방식'이 'Windows 기본 알림'이 아닐 수 있음(슬랙 자체 알림 → Windows 토스트 미생성 → 감지 불가)\n\
             - 또는 [시스템] notif_setting 의 ShowInActionCenter=0(배너만, 알림센터 미포함 → 리스너가 못 봄)";
        format!(
            "똑띠왔어요 진단 리포트\n\
             ※ 메시지 제목/본문 등 '내용'은 기록하지 않습니다 (식별자/길이/상태/카운트만).\n\n\
             [앱] v{ver}  PC={pc}  세션시작={started} UTC\n\
             [패키지ID] {pkg}\n\
             [알림접근] {access}\n\
             [집중지원] {focus}\n\
             [폴링] 마지막 {poll_age} · 한 폴링 최대 알림수 {maxn}\n\n\
             [시스템]\n{sys}\n\n\
             [요약] poll={poll} seen={seen} slack={slack} push={push} suppress={suppress} empty={empty} err={err}\n\n\
             [관찰된 AUMID] (이 세션에서 새 알림을 낸 앱들)\n  {aumids}\n\n\
             [현재 알림함 스냅샷]\n{snap}\n\n\
             [설치 로그(최근)]\n{install}\n\n\
             [최근 이벤트] ({nev}건, 내용 없이 메타데이터만)\n{events}\n\n\
             [해석 힌트]\n{guide}\n",
            ver = env!("CARGO_PKG_VERSION"),
            pkg = package_status(),
            access = crate::notifier::access_status(),
            focus = focus_status(),
            maxn = MAX_NOTIFS.load(Ordering::Relaxed),
            sys = system_section(),
            poll = C_POLL.load(Ordering::Relaxed),
            seen = C_SEEN.load(Ordering::Relaxed),
            slack = C_SLACK.load(Ordering::Relaxed),
            push = C_PUSH.load(Ordering::Relaxed),
            suppress = C_SUPPRESS.load(Ordering::Relaxed),
            empty = C_EMPTY.load(Ordering::Relaxed),
            err = C_ERR.load(Ordering::Relaxed),
            snap = crate::notifier::snapshot(),
            install = read_install_log(app),
            nev = events.len(),
            events = ev_txt,
            aumids = aumids_txt,
            guide = GUIDE,
        )
    }
}

#[cfg(target_os = "windows")]
pub use imp::{
    collect, focus_status, inc_empty, inc_err, inc_poll, inc_push, inc_seen, inc_slack,
    inc_suppress, init, log, note_aumid, note_poll, package_status,
};

// 비-Windows: no-op
#[cfg(not(target_os = "windows"))]
mod stub {
    use tauri::AppHandle;
    pub fn init(_app: &AppHandle) {}
    pub fn log(_msg: &str) {}
    pub fn collect(_app: &AppHandle) -> String {
        "(진단은 Windows 전용입니다)".into()
    }
    pub fn inc_poll() {}
    pub fn inc_seen() {}
    pub fn inc_slack() {}
    pub fn inc_push() {}
    pub fn inc_suppress() {}
    pub fn inc_empty() {}
    pub fn inc_err() {}
    pub fn note_aumid(_a: &str) {}
    pub fn note_poll(_count: usize) {}
}
#[cfg(not(target_os = "windows"))]
pub use stub::{
    collect, inc_empty, inc_err, inc_poll, inc_push, inc_seen, inc_slack, inc_suppress, init, log,
    note_aumid, note_poll,
};
