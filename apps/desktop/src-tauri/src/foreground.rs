//! Local foreground identity only; never persist or transmit window titles.
#[cfg(any(windows, test))]
fn windows_slack(process: &str, title: &str) -> bool {
    let name = process
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(process)
        .to_ascii_lowercase();
    if name == "slack.exe" {
        return true;
    }
    if !matches!(
        name.as_str(),
        "chrome.exe" | "msedge.exe" | "firefox.exe" | "brave.exe" | "opera.exe" | "vivaldi.exe"
    ) {
        return false;
    }
    // Preserve PWA/browser support, without matching arbitrary apps or "Slack API docs".
    title.split(['|', '-', '—', '–']).any(|part| {
        let part = part.trim();
        let part = if part.starts_with('(') {
            part.split_once(')')
                .filter(|(count, _)| count[1..].chars().all(|c| c.is_ascii_digit()))
                .map_or(part, |(_, rest)| rest.trim())
        } else {
            part
        };
        part.eq_ignore_ascii_case("slack") || part == "슬랙"
    })
}
#[cfg(target_os = "windows")]
pub fn slack_active() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn GetWindowThreadProcessId(w: isize, pid: *mut u32) -> u32;
        fn GetWindowTextW(w: isize, text: *mut u16, max: i32) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn QueryFullProcessImageNameW(
            process: isize,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }
    unsafe {
        let w = GetForegroundWindow();
        if w == 0 {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(w, &mut pid);
        let process = OpenProcess(0x1000, 0, pid);
        if process == 0 {
            return false;
        }
        let mut path = [0u16; 32768];
        let mut size = path.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut size);
        CloseHandle(process);
        if ok == 0 {
            return false;
        }
        let mut title = [0u16; 2048];
        let len = GetWindowTextW(w, title.as_mut_ptr(), title.len() as i32).max(0) as usize;
        windows_slack(
            &String::from_utf16_lossy(&path[..size as usize]),
            &String::from_utf16_lossy(&title[..len]),
        )
    }
}
#[cfg(target_os = "macos")]
pub fn slack_active() -> bool {
    use objc2::{class, msg_send, runtime::AnyObject};
    // NSWorkspace's running-app queries may be called from the notification worker.
    objc2::rc::autoreleasepool(|_| unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            return false;
        }
        let bundle: *mut AnyObject = msg_send![app, bundleIdentifier];
        if bundle.is_null() {
            return false;
        }
        let id: *const std::ffi::c_char = msg_send![bundle, UTF8String];
        !id.is_null() && std::ffi::CStr::from_ptr(id).to_bytes() == b"com.tinyspeck.slackmacgap"
    })
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn slack_active() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_store_pwa_and_browser_slack() {
        for (exe, title) in [
            (r"C:\Apps\Slack.exe", "general"),
            ("chrome.exe", "general (Channel) - team - Slack"),
            ("msedge.exe", "(2) Slack | team"),
            ("chrome.exe", "일반 - 회사 - 슬랙 - Google Chrome"),
        ] {
            assert!(windows_slack(exe, title), "{exe}: {title}");
        }
    }
    #[test]
    fn other_windows_and_slack_search_results_are_not_suppressed() {
        for (exe, title) in [
            ("notepad.exe", "Slack"),
            ("chrome.exe", "Slack API documentation - Google Chrome"),
            ("msedge.exe", "slack installation - Bing"),
            ("chrome.exe", "Slackware"),
            ("", "Slack"),
        ] {
            assert!(!windows_slack(exe, title), "{exe}: {title}");
        }
    }
}
