//! Detect an on-screen full-display window without reading window titles/content.
use std::sync::atomic::{AtomicBool, Ordering};
static FULLSCREEN: AtomicBool = AtomicBool::new(false);
/// Answer for a background poller. On macOS the window list is read on the UI
/// thread and the previous answer returned: off that thread,
/// CGWindowListCopyWindowInfo waits for the UI thread's pending Core Animation
/// commit while holding the WindowServer lock that commit needs, and both stall
/// for about half a second (a visible hitch in the character's motion).
pub fn check(app: &tauri::AppHandle, target: Option<[f64; 4]>) -> bool {
    if cfg!(target_os = "macos") {
        let _ = app.run_on_main_thread(move || {
            FULLSCREEN.store(active(target), Ordering::Relaxed);
        });
        FULLSCREEN.load(Ordering::Relaxed)
    } else {
        active(target)
    }
}
#[cfg(target_os = "windows")]
pub fn active(target: Option<[f64; 4]>) -> bool {
    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct Info {
        size: u32,
        monitor: Rect,
        work: Rect,
        flags: u32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn GetDesktopWindow() -> isize;
        fn GetShellWindow() -> isize;
        fn GetWindowRect(w: isize, r: *mut Rect) -> i32;
        fn MonitorFromWindow(w: isize, flags: u32) -> isize;
        fn GetMonitorInfoW(m: isize, i: *mut Info) -> i32;
    }
    unsafe {
        let w = GetForegroundWindow();
        if w == 0 || w == GetDesktopWindow() || w == GetShellWindow() {
            return false;
        }
        let mut r = Rect::default();
        let mut i = Info {
            size: std::mem::size_of::<Info>() as u32,
            monitor: Rect::default(),
            work: Rect::default(),
            flags: 0,
        };
        if GetWindowRect(w, &mut r) == 0 || GetMonitorInfoW(MonitorFromWindow(w, 2), &mut i) == 0 {
            return false;
        }
        if target.is_some_and(|t| {
            (t[0] - i.monitor.left as f64).abs() > 3. || (t[1] - i.monitor.top as f64).abs() > 3.
        }) {
            return false;
        }
        r.left <= i.monitor.left
            && r.top <= i.monitor.top
            && r.right >= i.monitor.right
            && r.bottom >= i.monitor.bottom
    }
}
#[cfg(target_os = "macos")]
pub fn active(target: Option<[f64; 4]>) -> bool {
    use std::ffi::{c_char, c_void};
    type Ref = *const c_void;
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Size {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Rect {
        origin: Point,
        size: Size,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCopyWindowInfo(options: u32, relative: u32) -> Ref;
        fn CGRectMakeWithDictionaryRepresentation(dict: Ref, rect: *mut Rect) -> bool;
        fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
        fn CGDisplayBounds(display: u32) -> Rect;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(a: Ref) -> isize;
        fn CFArrayGetValueAtIndex(a: Ref, i: isize) -> Ref;
        fn CFDictionaryGetValue(d: Ref, key: Ref) -> Ref;
        fn CFStringCreateWithCString(a: Ref, s: *const c_char, encoding: u32) -> Ref;
        fn CFNumberGetValue(n: Ref, t: i32, out: *mut c_void) -> bool;
        fn CFRelease(r: Ref);
    }
    unsafe {
        let list = CGWindowListCopyWindowInfo(1 | 16, 0);
        if list.is_null() {
            return false;
        }
        let keys = [c"kCGWindowLayer", c"kCGWindowOwnerPID", c"kCGWindowBounds"]
            .map(|k| CFStringCreateWithCString(std::ptr::null(), k.as_ptr(), 0x08000100));
        let mut displays = [0u32; 16];
        let mut count = 0;
        let mut found = false;
        if CGGetActiveDisplayList(16, displays.as_mut_ptr(), &mut count) == 0 {
            for i in 0..CFArrayGetCount(list) {
                let d = CFArrayGetValueAtIndex(list, i);
                let mut layer = 0i32;
                let mut pid = 0i32;
                let l = CFDictionaryGetValue(d, keys[0]);
                let p = CFDictionaryGetValue(d, keys[1]);
                if l.is_null() || p.is_null() {
                    continue;
                }
                CFNumberGetValue(l, 3, (&mut layer as *mut i32).cast());
                CFNumberGetValue(p, 3, (&mut pid as *mut i32).cast());
                if layer != 0 || pid == std::process::id() as i32 {
                    continue;
                }
                let b = CFDictionaryGetValue(d, keys[2]);
                let mut r = Rect::default();
                if b.is_null() || !CGRectMakeWithDictionaryRepresentation(b, &mut r) {
                    continue;
                }
                for id in displays.iter().take(count as usize) {
                    let screen = CGDisplayBounds(*id);
                    if target.is_some_and(|t| {
                        (t[0] - screen.origin.x).abs() > 3. || (t[1] - screen.origin.y).abs() > 3.
                    }) {
                        continue;
                    }
                    if (r.origin.x - screen.origin.x).abs() < 3.0
                        && (r.origin.y - screen.origin.y).abs() < 3.0
                        && (r.size.width - screen.size.width).abs() < 3.0
                        && (r.size.height - screen.size.height).abs() < 3.0
                    {
                        found = true;
                        break;
                    }
                }
                if found {
                    break;
                }
            }
        }
        for k in keys {
            CFRelease(k)
        }
        CFRelease(list);
        found
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn active(_target: Option<[f64; 4]>) -> bool {
    false
}
