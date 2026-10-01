//! Window geometry only: no titles, screenshots, contents, or window manipulation.
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Monitor};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowRect {
    pub(crate) id: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct World {
    pub(crate) monitor: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) size: f64,
    pub(crate) windows: Vec<WindowRect>,
}
pub(crate) fn key(m: &Monitor) -> String {
    let w = m.work_area();
    format!(
        "{}:{}:{}:{}:{}",
        w.position.x,
        w.position.y,
        w.size.width,
        w.size.height,
        m.scale_factor()
    )
}
pub fn pet_world(app: AppHandle) -> Result<World, String> {
    let win = app.get_webview_window("overlay").ok_or("no overlay")?;
    let pos = win.outer_position().map_err(|e| e.to_string())?;
    let anchor = app
        .state::<crate::companion::Companion>()
        .0
        .lock()
        .unwrap()
        .pet_anchor
        .unwrap_or((200., 450.));
    // Use the requested foot position, not an asynchronous OS window move readback.
    let location = app
        .state::<crate::companion::Companion>()
        .0
        .lock()
        .unwrap()
        .pet_location
        .clone();
    let monitors = win.available_monitors().map_err(|e| e.to_string())?;
    let selected = location.as_ref().and_then(|(id, x, y)| {
        monitors
            .iter()
            .find(|m| key(m) == *id)
            .map(|m| (m.clone(), *x, *y))
    });
    let (m, x, y) = if let Some(selected) = selected {
        selected
    } else {
        let m = win
            .current_monitor()
            .ok()
            .flatten()
            .or_else(|| win.primary_monitor().ok().flatten())
            .ok_or("no monitor")?;
        (m, pos.x as f64 + anchor.0, pos.y as f64 + anchor.1)
    };
    world_at(&app, &m, x, y)
}

/// Read geometry for an explicitly owned monitor/foot position. Scripted routes
/// must not select a monitor from an asynchronously moved overlay window.
pub(crate) fn world_at(app: &AppHandle, m: &Monitor, x: f64, y: f64) -> Result<World, String> {
    let wa = m.work_area();
    let sf = m.scale_factor();
    let ox = wa.position.x as f64 / sf;
    let oy = wa.position.y as f64 / sf;
    // If OS tracking is temporarily unavailable, continue safely on the desktop floor.
    let mut windows = platform::windows().unwrap_or_default();
    for w in &mut windows {
        // CoreGraphics is already in global logical points; Win32 is physical pixels.
        #[cfg(not(target_os = "macos"))]
        {
            w.x /= sf;
            w.y /= sf;
            w.width /= sf;
            w.height /= sf;
        }
        w.x -= ox;
        w.y -= oy;
    }
    let width = wa.size.width as f64 / sf;
    let height = wa.size.height as f64 / sf;
    windows.retain(|w| w.x < width && w.x + w.width > 0. && w.y < height && w.y + w.height > 0.);
    Ok(World {
        monitor: key(&m),
        x: x / sf - ox,
        y: y / sf - oy,
        width,
        height,
        size: crate::pet_size(&crate::effective_display(&app)),
        windows,
    })
}
#[cfg(target_os = "windows")]
mod platform {
    use super::WindowRect;
    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(cb: unsafe extern "system" fn(isize, isize) -> i32, param: isize) -> i32;
        fn IsWindowVisible(w: isize) -> i32;
        fn IsIconic(w: isize) -> i32;
        fn GetWindowRect(w: isize, r: *mut Rect) -> i32;
        fn GetWindowThreadProcessId(w: isize, p: *mut u32) -> u32;
        fn GetShellWindow() -> isize;
        fn GetDesktopWindow() -> isize;
        fn GetWindowLongW(w: isize, index: i32) -> i32;
    }
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmGetWindowAttribute(w: isize, attr: u32, out: *mut std::ffi::c_void, size: u32)
            -> i32;
    }
    unsafe extern "system" fn collect(w: isize, param: isize) -> i32 {
        let mut pid = 0;
        GetWindowThreadProcessId(w, &mut pid);
        if pid == std::process::id()
            || IsWindowVisible(w) == 0
            || IsIconic(w) != 0
            || w == GetShellWindow()
            || w == GetDesktopWindow()
        {
            return 1;
        }
        // Tooltips and click-through overlays should not become surfaces or occluders.
        if GetWindowLongW(w, -20) & (0x80 | 0x20) != 0 {
            return 1;
        }
        let mut cloaked = 0u32;
        DwmGetWindowAttribute(w, 14, (&mut cloaked as *mut u32).cast(), 4);
        if cloaked != 0 {
            return 1;
        }
        let mut r = Rect::default();
        if DwmGetWindowAttribute(
            w,
            9,
            (&mut r as *mut Rect).cast(),
            std::mem::size_of::<Rect>() as u32,
        ) != 0
            && GetWindowRect(w, &mut r) == 0
        {
            return 1;
        }
        if r.right - r.left < 40 || r.bottom - r.top < 30 {
            return 1;
        }
        let rows = &mut *(param as *mut Vec<WindowRect>);
        rows.push(WindowRect {
            id: format!("{pid}:{w}"),
            x: r.left as f64,
            y: r.top as f64,
            width: (r.right - r.left) as f64,
            height: (r.bottom - r.top) as f64,
        });
        1
    }
    pub fn windows() -> Result<Vec<WindowRect>, String> {
        let mut rows = Vec::new();
        if unsafe { EnumWindows(collect, (&mut rows as *mut Vec<WindowRect>) as isize) } == 0 {
            return Err("창 위치를 확인하지 못했어요".into());
        }
        Ok(rows)
    }
}
#[cfg(target_os = "macos")]
mod platform {
    use super::WindowRect;
    use std::ffi::{c_char, c_void};
    type Ref = *const c_void;
    #[repr(C)]
    #[derive(Default)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Size {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        origin: Point,
        size: Size,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCopyWindowInfo(options: u32, relative: u32) -> Ref;
        fn CGRectMakeWithDictionaryRepresentation(dict: Ref, rect: *mut Rect) -> bool;
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
    pub fn windows() -> Result<Vec<WindowRect>, String> {
        unsafe {
            let list = CGWindowListCopyWindowInfo(1 | 16, 0);
            if list.is_null() {
                return Err("창 위치를 확인하지 못했어요".into());
            }
            let keys = [
                c"kCGWindowLayer",
                c"kCGWindowOwnerPID",
                c"kCGWindowNumber",
                c"kCGWindowBounds",
                c"kCGWindowAlpha",
            ]
            .map(|k| CFStringCreateWithCString(std::ptr::null(), k.as_ptr(), 0x08000100));
            let mut rows = Vec::new();
            for i in 0..CFArrayGetCount(list) {
                let d = CFArrayGetValueAtIndex(list, i);
                let mut ns = [0i32; 3];
                let mut valid = true;
                for j in 0..3 {
                    let n = CFDictionaryGetValue(d, keys[j]);
                    if n.is_null() || !CFNumberGetValue(n, 3, (&mut ns[j] as *mut i32).cast()) {
                        valid = false;
                        break;
                    }
                }
                if !valid || ns[0] != 0 || ns[1] == std::process::id() as i32 {
                    continue;
                }
                let mut alpha = 1f64;
                let a = CFDictionaryGetValue(d, keys[4]);
                if !a.is_null() {
                    CFNumberGetValue(a, 6, (&mut alpha as *mut f64).cast());
                }
                if alpha < 0.05 {
                    continue;
                }
                let b = CFDictionaryGetValue(d, keys[3]);
                let mut r = Rect::default();
                if b.is_null()
                    || !CGRectMakeWithDictionaryRepresentation(b, &mut r)
                    || r.size.width < 40.
                    || r.size.height < 30.
                {
                    continue;
                }
                rows.push(WindowRect {
                    id: format!("{}:{}", ns[1], ns[2]),
                    x: r.origin.x,
                    y: r.origin.y,
                    width: r.size.width,
                    height: r.size.height,
                });
            }
            for k in keys {
                CFRelease(k);
            }
            CFRelease(list);
            Ok(rows)
        }
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub fn windows() -> Result<Vec<super::WindowRect>, String> {
        Ok(Vec::new())
    }
}
