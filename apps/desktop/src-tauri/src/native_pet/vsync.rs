//! Frame timing locked to the display refresh. A sleeping timer drifts against
//! the refresh, so two updates sometimes land in one refresh and none in the
//! next, which reads as judder while walking. macOS waits on a CVDisplayLink,
//! Windows on DwmFlush (the next composition); elsewhere, or when the display
//! stops refreshing (asleep), a plain timer takes over.
use std::time::{Duration, Instant};
/// Blocks until the first display refresh at least ~¾ of `period` after `since`:
/// one tick per refresh on a 60 Hz display, every other one at 120 Hz.
pub fn wait(since: Instant, period: Duration) {
    let enough = period * 3 / 4;
    while since.elapsed() < enough {
        if !imp::refresh() {
            // No refresh signal: fall back to the timer for this frame.
            if let Some(rest) = period.checked_sub(since.elapsed()) {
                std::thread::sleep(rest);
            }
            return;
        }
    }
}
#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;
    use std::sync::{Condvar, Mutex, OnceLock};
    use std::time::Duration;
    #[link(name = "CoreVideo", kind = "framework")]
    extern "C" {
        fn CVDisplayLinkCreateWithActiveCGDisplays(link: *mut *mut c_void) -> i32;
        fn CVDisplayLinkSetOutputCallback(
            link: *mut c_void,
            callback: extern "C" fn(
                *mut c_void,
                *const c_void,
                *const c_void,
                u64,
                *mut u64,
                *mut c_void,
            ) -> i32,
            context: *mut c_void,
        ) -> i32;
        fn CVDisplayLinkSetCurrentCGDisplay(link: *mut c_void, display: u32) -> i32;
        fn CVDisplayLinkStart(link: *mut c_void) -> i32;
    }
    struct Link {
        link: usize,
        display: Mutex<u32>,
        count: Mutex<u64>,
        tick: Condvar,
    }
    extern "C" fn on_refresh(
        _: *mut c_void,
        _: *const c_void,
        _: *const c_void,
        _: u64,
        _: *mut u64,
        _: *mut c_void,
    ) -> i32 {
        if let Some(l) = LINK.get().and_then(Option::as_ref) {
            *l.count.lock().unwrap() += 1;
            l.tick.notify_all();
        }
        0
    }
    static LINK: OnceLock<Option<Link>> = OnceLock::new();
    fn link() -> Option<&'static Link> {
        LINK.get_or_init(|| unsafe {
            let mut link = std::ptr::null_mut();
            if CVDisplayLinkCreateWithActiveCGDisplays(&mut link) != 0 || link.is_null() {
                return None;
            }
            Some(Link {
                link: link as usize,
                display: Mutex::new(0),
                count: Mutex::new(0),
                tick: Condvar::new(),
            })
        })
        .as_ref()
        .inspect(|l| {
            static STARTED: OnceLock<bool> = OnceLock::new();
            STARTED.get_or_init(|| unsafe {
                let link = l.link as *mut c_void;
                CVDisplayLinkSetOutputCallback(link, on_refresh, std::ptr::null_mut()) == 0
                    && CVDisplayLinkStart(link) == 0
            });
        })
    }
    /// Waits for the next refresh; false when none arrives (display asleep, no link).
    pub fn refresh() -> bool {
        let Some(l) = link() else { return false };
        let count = l.count.lock().unwrap();
        let seen = *count;
        let (count, timeout) = l
            .tick
            .wait_timeout_while(count, Duration::from_millis(50), |c| *c == seen)
            .unwrap();
        drop(count);
        !timeout.timed_out()
    }
    /// Follow the display showing the character (refresh rates and phases differ).
    pub fn follow(display: u32) {
        let Some(l) = link() else { return };
        let mut current = l.display.lock().unwrap();
        if *current != display {
            *current = display;
            unsafe { CVDisplayLinkSetCurrentCGDisplay(l.link as *mut c_void, display) };
        }
    }
}
#[cfg(target_os = "windows")]
mod imp {
    use std::time::{Duration, Instant};
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmFlush() -> i32;
    }
    /// Waits for the next desktop composition (one per refresh).
    pub fn refresh() -> bool {
        let start = Instant::now();
        if unsafe { DwmFlush() } < 0 {
            return false;
        }
        // An immediate return means no composition to wait for: do not spin.
        if start.elapsed() < Duration::from_micros(500) {
            std::thread::sleep(Duration::from_millis(1));
        }
        true
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    pub fn refresh() -> bool {
        false
    }
}
#[cfg(target_os = "macos")]
pub use imp::follow;
