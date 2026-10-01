//! Native presentation only. Pixels are premultiplied RGBA from resvg, never a WebView.
#[cfg(target_os = "macos")]
mod implementation {
    use objc2::{class, msg_send, runtime::AnyObject, Encode, Encoding};
    use std::ffi::c_void;
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Size {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Rect {
        origin: Point,
        size: Size,
    }
    unsafe impl Encode for Point {
        const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for Size {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for Rect {
        const ENCODING: Encoding = Encoding::Struct("CGRect", &[Point::ENCODING, Size::ENCODING]);
    }
    fn appkit_frame(origin: (f64, f64), pixels: (u32, u32), scale: f64, desktop_top: f64) -> Rect {
        Rect {
            origin: Point {
                x: origin.0 / scale,
                y: desktop_top - (origin.1 + pixels.1 as f64) / scale,
            },
            size: Size {
                width: pixels.0 as f64 / scale,
                height: pixels.1 as f64 / scale,
            },
        }
    }
    unsafe fn desktop_top() -> f64 {
        // First NSScreen is the menu-bar display, matching the desktop coordinate
        // conversion used by tao. A secondary display may have a negative origin.
        let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
        let primary: *mut AnyObject = msg_send![screens, objectAtIndex: 0usize];
        let frame: Rect = msg_send![primary, frame];
        frame.origin.y + frame.size.height
    }
    pub fn move_to(win: &tauri::Window, origin: (f64, f64), scale: f64) -> Result<(), String> {
        unsafe {
            let window = win.ns_window().map_err(|e| e.to_string())? as *mut AnyObject;
            let current: Rect = msg_send![window, frame];
            let point = Point {
                x: origin.0 / scale,
                y: desktop_top() - origin.1 / scale - current.size.height,
            };
            let _: () = msg_send![window, setFrameOrigin: point];
        }
        Ok(())
    }
    #[link(name = "QuartzCore", kind = "framework")]
    extern "C" {}
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
        fn CGColorSpaceRelease(p: *mut c_void);
        fn CGDataProviderCreateWithCFData(p: *const c_void) -> *mut c_void;
        fn CGDataProviderRelease(p: *mut c_void);
        fn CGImageCreate(
            w: usize,
            h: usize,
            bpc: usize,
            bpp: usize,
            bpr: usize,
            space: *mut c_void,
            info: u32,
            provider: *mut c_void,
            decode: *const f64,
            interpolate: bool,
            intent: u32,
        ) -> *mut c_void;
        fn CGImageRelease(p: *mut c_void);
        fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDataCreate(allocator: *const c_void, bytes: *const u8, len: isize) -> *mut c_void;
        fn CFRelease(p: *const c_void);
    }
    pub fn buttons() -> (bool, bool) {
        unsafe {
            (
                CGEventSourceButtonState(1, 0),
                CGEventSourceButtonState(1, 1),
            )
        }
    }
    thread_local! {static HOSTED:std::cell::Cell<bool>=const {std::cell::Cell::new(false)};}
    pub fn present(
        win: &tauri::Window,
        pix: &resvg::tiny_skia::Pixmap,
        origin: (f64, f64),
        scale: f64,
    ) -> Result<(), String> {
        unsafe {
            let window = win.ns_window().map_err(|e| e.to_string())? as *mut AnyObject;
            let view: *mut AnyObject = msg_send![window, contentView];
            HOSTED.with(|hosted| {
                if !hosted.get() {
                    // Layer-hosting view: AppKit must not redraw over the sprite contents.
                    let layer: *mut AnyObject = msg_send![class!(CALayer), layer];
                    let _: () = msg_send![view,setLayer:layer];
                    let _: () = msg_send![view,setWantsLayer:true];
                    hosted.set(true);
                }
            });
            let layer: *mut AnyObject = msg_send![view, layer];
            if layer.is_null() {
                return Err("Native layer unavailable".into());
            }
            let data = CFDataCreate(
                std::ptr::null(),
                pix.data().as_ptr(),
                pix.data().len() as isize,
            );
            if data.is_null() {
                return Err("Pixel allocation failed".into());
            }
            let provider = CGDataProviderCreateWithCFData(data);
            let space = CGColorSpaceCreateDeviceRGB();
            let image = CGImageCreate(
                pix.width() as usize,
                pix.height() as usize,
                8,
                32,
                pix.width() as usize * 4,
                space,
                (4 << 12) | 1,
                provider,
                std::ptr::null(),
                true,
                0,
            );
            if !image.is_null() {
                let _: () = msg_send![class!(CATransaction), begin];
                let _: () = msg_send![class!(CATransaction),setDisableActions:true];
                // Keep the old image from being resized/presented separately from
                // its replacement. These calls run synchronously on the main thread.
                let _: () = msg_send![window, disableScreenUpdatesUntilFlush];
                let frame = appkit_frame(origin, (pix.width(), pix.height()), scale, desktop_top());
                let _: () = msg_send![window, setFrame: frame, display: false];
                let _: () = msg_send![layer, setContentsScale: scale];
                let _: () = msg_send![layer,setContents:image as *mut AnyObject];
                let _: () = msg_send![class!(CATransaction), commit];
                CGImageRelease(image);
            }
            CGColorSpaceRelease(space);
            CGDataProviderRelease(provider);
            CFRelease(data);
            if image.is_null() {
                Err("Native image creation failed".into())
            } else {
                Ok(())
            }
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn canvas_resize_keeps_screen_anchor_fixed_on_standard_and_retina_displays() {
            let art = crate::native_pet::art::Art::new().unwrap();
            for scale in [1., 2.] {
                for facing in [-1., 1.] {
                    let anchor = (-250. * scale, 380. * scale);
                    let mut previous_left: Option<f64> = None;
                    let mut old_order_would_drift = false;
                    for mode in ["grab", "climb", "pull"] {
                        for index in 0..8 {
                            let pose = art.pose(mode, index as f64 * 0.1, false);
                            let k = 180. * scale / 260.;
                            let fx = anchor.0 - facing * 180. * 0.46 * scale;
                            let pet_left = fx - 200. * k + facing * pose.offset * k;
                            let pet_top = anchor.1 + 320. * scale - (250. + pose.lift) * k;
                            let left = pet_left.min(anchor.0 - 15. * scale).floor() - 2.;
                            let top = pet_top.min(anchor.1 - 12. * scale).floor() - 2.;
                            let pixels = (
                                (pet_left + 400. * k - left + 4.).ceil() as u32,
                                (pet_top + 260. * k - top + 4.).ceil() as u32,
                            );
                            let frame = appkit_frame((left, top), pixels, scale, 2160.);
                            let local_anchor =
                                ((anchor.0 - left) / scale, (anchor.1 - top) / scale);
                            let screen_anchor = (
                                frame.origin.x + local_anchor.0,
                                2160. - frame.origin.y - frame.size.height + local_anchor.1,
                            );
                            assert!((screen_anchor.0 - anchor.0 / scale).abs() < 1e-8);
                            assert!((screen_anchor.1 - anchor.1 / scale).abs() < 1e-8);
                            if let Some(old_left) = previous_left {
                                // Old code displayed this local anchor with the old
                                // queued window origin; actual animation exposes it.
                                old_order_would_drift |= ((old_left - left) / scale).abs() > 1.;
                            }
                            previous_left = Some(left);
                        }
                    }
                    assert!(old_order_would_drift);
                }
            }
        }
    }
}
#[cfg(target_os = "windows")]
mod implementation {
    use std::ffi::c_void;
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct Size {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct Blend {
        op: u8,
        flags: u8,
        alpha: u8,
        format: u8,
    }
    #[repr(C)]
    struct Header {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bits: u16,
        compression: u32,
        image: u32,
        x: i32,
        y: i32,
        used: u32,
        important: u32,
    }
    #[repr(C)]
    struct Info {
        header: Header,
        colours: [u32; 1],
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetDC(w: isize) -> isize;
        fn ReleaseDC(w: isize, dc: isize) -> i32;
        fn GetAsyncKeyState(k: i32) -> i16;
        fn GetWindowLongPtrW(w: isize, i: i32) -> isize;
        fn SetWindowLongPtrW(w: isize, i: i32, v: isize) -> isize;
        fn UpdateLayeredWindow(
            w: isize,
            dst: isize,
            point: *const Point,
            size: *const Size,
            src: isize,
            origin: *const Point,
            key: u32,
            blend: *const Blend,
            flags: u32,
        ) -> i32;
    }
    type SubclassProc = unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize;
    #[link(name = "comctl32")]
    extern "system" {
        fn SetWindowSubclass(hwnd: isize, proc: SubclassProc, id: usize, data: usize) -> i32;
        fn RemoveWindowSubclass(hwnd: isize, proc: SubclassProc, id: usize) -> i32;
        fn DefSubclassProc(hwnd: isize, msg: u32, w: usize, l: isize) -> isize;
    }
    unsafe extern "system" fn canvas_messages(
        hwnd: isize,
        msg: u32,
        w: usize,
        l: isize,
        id: usize,
        _: usize,
    ) -> isize {
        if msg == 0x02E0 {
            // WM_DPICHANGED
            // This canvas already uses destination-monitor physical pixels in
            // UpdateLayeredWindow. Tao's logical-size preservation would scale
            // it a second time (and crop the cached bitmap on a lower-DPI screen).
            // Only the native canvas is subclassed; settings/popups retain Tao DPI handling.
            return 0;
        }
        if msg == 0x0082 {
            // WM_NCDESTROY
            RemoveWindowSubclass(hwnd, canvas_messages, id);
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
    unsafe fn own_canvas_dpi(hwnd: isize) -> Result<(), String> {
        // Native presentation runs on the HWND's owning UI thread. Reinstalling
        // the same callback/id updates it without stacking handlers.
        if SetWindowSubclass(hwnd, canvas_messages, 0xDD01, 0) == 0 {
            Err("Unable to install native canvas DPI handler".into())
        } else {
            Ok(())
        }
    }
    pub fn ignore_cursor(win: &tauri::Window, ignore: bool) -> Result<(), String> {
        unsafe {
            let hwnd = win.hwnd().map_err(|e| e.to_string())?.0 as isize;
            let ex = GetWindowLongPtrW(hwnd, -20) | 0x80000 | 0x08000000 | 0x80;
            SetWindowLongPtrW(hwnd, -20, if ignore { ex | 0x20 } else { ex & !0x20 });
            Ok(())
        }
    }
    pub fn on_top(win: &tauri::Window, top: bool) -> Result<(), String> {
        #[link(name = "user32")]
        extern "system" {
            fn SetWindowPos(
                w: isize,
                after: isize,
                x: i32,
                y: i32,
                cx: i32,
                cy: i32,
                flags: u32,
            ) -> i32;
        }
        let hwnd = win.hwnd().map_err(|e| e.to_string())?.0 as isize;
        if unsafe { SetWindowPos(hwnd, if top { -1 } else { -2 }, 0, 0, 0, 0, 0x13) } == 0 {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(())
        }
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(dc: isize) -> isize;
        fn DeleteDC(dc: isize) -> i32;
        fn CreateDIBSection(
            dc: isize,
            info: *const Info,
            usage: u32,
            bits: *mut *mut c_void,
            section: isize,
            offset: u32,
        ) -> isize;
        fn SelectObject(dc: isize, o: isize) -> isize;
        fn DeleteObject(o: isize) -> i32;
    }
    pub fn buttons() -> (bool, bool) {
        unsafe { (GetAsyncKeyState(1) < 0, GetAsyncKeyState(2) < 0) }
    }
    pub fn move_to(win: &tauri::Window, origin: (f64, f64), _: f64) -> Result<(), String> {
        win.set_position(tauri::PhysicalPosition::new(
            origin.0.round() as i32,
            origin.1.round() as i32,
        ))
        .map_err(|e| e.to_string())
    }
    pub fn present(
        win: &tauri::Window,
        pix: &resvg::tiny_skia::Pixmap,
        position: (f64, f64),
        _: f64,
    ) -> Result<(), String> {
        unsafe {
            let hwnd = win.hwnd().map_err(|e| e.to_string())?.0 as isize;
            own_canvas_dpi(hwnd)?;
            let ex = GetWindowLongPtrW(hwnd, -20);
            SetWindowLongPtrW(hwnd, -20, ex | 0x80000 | 0x08000000 | 0x80);
            let screen = GetDC(0);
            let dc = CreateCompatibleDC(screen);
            if dc == 0 {
                ReleaseDC(0, screen);
                return Err("CreateCompatibleDC failed".into());
            }
            let info = Info {
                header: Header {
                    size: 40,
                    width: pix.width() as i32,
                    height: -(pix.height() as i32),
                    planes: 1,
                    bits: 32,
                    compression: 0,
                    image: 0,
                    x: 0,
                    y: 0,
                    used: 0,
                    important: 0,
                },
                colours: [0],
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, 0, &mut bits, 0, 0);
            if bitmap == 0 || bits.is_null() {
                DeleteDC(dc);
                ReleaseDC(0, screen);
                return Err("CreateDIBSection failed".into());
            }
            let dst = std::slice::from_raw_parts_mut(bits as *mut u8, pix.data().len());
            for (out, rgba) in dst.chunks_exact_mut(4).zip(pix.data().chunks_exact(4)) {
                out.copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
            }
            let old = SelectObject(dc, bitmap);
            let size = Size {
                x: pix.width() as i32,
                y: pix.height() as i32,
            };
            let origin = Point { x: 0, y: 0 };
            let blend = Blend {
                op: 0,
                flags: 0,
                alpha: 255,
                format: 1,
            };
            let destination = Point {
                x: position.0.round() as i32,
                y: position.1.round() as i32,
            };
            let ok =
                UpdateLayeredWindow(hwnd, screen, &destination, &size, dc, &origin, 0, &blend, 2);
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            ReleaseDC(0, screen);
            if ok == 0 {
                Err(std::io::Error::last_os_error().to_string())
            } else {
                Ok(())
            }
        }
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod implementation {
    pub fn buttons() -> (bool, bool) {
        (false, false)
    }
    pub fn move_to(_: &tauri::Window, _: (f64, f64), _: f64) -> Result<(), String> {
        Err("Native pets support macOS and Windows".into())
    }
    pub fn present(
        _: &tauri::Window,
        _: &resvg::tiny_skia::Pixmap,
        _: (f64, f64),
        _: f64,
    ) -> Result<(), String> {
        Err("Native pets support macOS and Windows".into())
    }
}
pub use implementation::*;

#[cfg(not(target_os = "windows"))]
pub fn ignore_cursor(win: &tauri::Window, ignore: bool) -> Result<(), String> {
    win.set_ignore_cursor_events(ignore)
        .map_err(|e| e.to_string())
}
#[cfg(not(target_os = "windows"))]
pub fn on_top(win: &tauri::Window, top: bool) -> Result<(), String> {
    win.set_always_on_top(top).map_err(|e| e.to_string())
}
