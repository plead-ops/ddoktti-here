//! Is the desktop being shown to other people? Two local signals, no permission:
//! display mirroring (Win+P duplicate, macOS mirror sets) and a connected display
//! that identifies itself as a projector. Only EDID names, vendor codes and sizes
//! are read, and nothing is stored.
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Display {
    /// EDID monitor name as the OS shows it ("EPSON PJ", "LG ULTRAWIDE").
    pub name: String,
    /// Three-letter PNP vendor code from the EDID ("BNQ", "OTM").
    pub vendor: String,
    /// EDID reports no physical size; common for projectors, also TVs and AV receivers.
    pub sizeless: bool,
    pub builtin: bool,
}
/// Vendors that make projectors and (practically) no monitors: Epson, Optoma,
/// XGIMI, Hitachi's projector line, Casio, Vivitek, InFocus, Christie.
const PROJECTOR_VENDORS: [&str; 10] = [
    "EPS", "EHJ", "OTM", "OPT", "GMI", "HTC", "CAS", "VIT", "IFS", "CHR",
];
/// Brand or series words that only appear on projectors.
const PROJECTOR_WORDS: [&str; 16] = [
    "PROJECTOR", "EPSON", "OPTOMA", "XGIMI", "NEBULA", "JMGO", "DANGBEI", "FORMOVIE", "YABER",
    "WANBO", "VIVITEK", "INFOCUS", "CINEBEAM", "FREESTYLE", "VPL", "PJ",
];
/// Model prefixes: NEC NP-, Panasonic PT-, Sony VPL-, Epson EB-/EH-/EF-.
const MODEL_PREFIXES: [&str; 6] = ["NP-", "PT-", "VPL", "EB-", "EH-", "EF-"];
/// Projectors that name themselves only by resolution (Optoma, Grundig).
const RESOLUTION_NAMES: [&str; 10] = [
    "XGA", "WXGA", "WUXGA", "SXGA", "UXGA", "1080P", "720P", "4K", "FHD", "UHD",
];
/// Decode an EDID manufacturer id (big-endian 16-bit) into its PNP letters.
/// Unknown or virtual displays carry ids outside A-Z and decode to "".
pub fn pnp(id: u16) -> String {
    let fields = [(id >> 10) & 31, (id >> 5) & 31, id & 31];
    if fields.iter().any(|c| !(1..=26).contains(c)) {
        return String::new();
    }
    fields.iter().map(|c| char::from(64 + *c as u8)).collect()
}
pub fn projector(d: &Display) -> bool {
    if d.builtin {
        return false;
    }
    let name = d.name.trim().to_ascii_uppercase();
    let vendor = d.vendor.to_ascii_uppercase();
    if PROJECTOR_VENDORS.contains(&vendor.as_str()) {
        return true;
    }
    // Whole words only: "22MP55PJ" is an LG monitor, "PJ" and "XGA PJ" are projectors.
    let words: Vec<&str> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    if words.iter().any(|w| PROJECTOR_WORDS.contains(w)) {
        return true;
    }
    if MODEL_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return true;
    }
    // LG projector series (HU/PF/HF/BU) under LG's monitor vendor code.
    if vendor == "GSM"
        && ["HU", "PF", "HF", "BU"]
            .iter()
            .any(|p| name.starts_with(p) && name[p.len()..].starts_with(|c: char| c.is_ascii_digit()))
    {
        return true;
    }
    words.len() == 1
        && RESOLUTION_NAMES.contains(&words[0])
        && (d.sizeless || vendor == "GRU")
}
pub fn active(mirrored: bool, displays: &[Display]) -> bool {
    mirrored || displays.iter().any(projector)
}
static PRESENTING: AtomicBool = AtomicBool::new(false);
/// Latest probe result; `schedule` refreshes it on the UI thread.
pub fn current() -> bool {
    PRESENTING.load(Ordering::Relaxed)
}
/// Display APIs (NSScreen on macOS) belong to the UI thread; the worker only
/// asks for a refresh and reads the cached answer next time round.
pub fn schedule(app: &tauri::AppHandle) {
    let _ = app.run_on_main_thread(|| {
        let (mirrored, displays) = platform::probe();
        PRESENTING.store(active(mirrored, &displays), Ordering::Relaxed);
    });
}
#[cfg(target_os = "macos")]
mod platform {
    use super::Display;
    use objc2::{class, msg_send, runtime::AnyObject, sel};
    use std::ffi::{c_char, CStr};
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Size {
        width: f64,
        height: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
        fn CGDisplayIsInMirrorSet(display: u32) -> u32;
        fn CGDisplayIsBuiltin(display: u32) -> u32;
        fn CGDisplayVendorNumber(display: u32) -> u32;
        fn CGDisplayScreenSize(display: u32) -> Size;
    }
    unsafe fn name_of(display: u32) -> String {
        let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
        if screens.is_null() {
            return String::new();
        }
        let key: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: c"NSScreenNumber".as_ptr()];
        let count: usize = msg_send![screens, count];
        for i in 0..count {
            let screen: *mut AnyObject = msg_send![screens, objectAtIndex: i];
            let description: *mut AnyObject = msg_send![screen, deviceDescription];
            let number: *mut AnyObject = msg_send![description, objectForKey: key];
            if number.is_null() {
                continue;
            }
            let id: u32 = msg_send![number, unsignedIntValue];
            if id != display {
                continue;
            }
            let responds: bool = msg_send![screen, respondsToSelector: sel!(localizedName)];
            if !responds {
                return String::new();
            }
            let name: *mut AnyObject = msg_send![screen, localizedName];
            if name.is_null() {
                return String::new();
            }
            let utf8: *const c_char = msg_send![name, UTF8String];
            if utf8.is_null() {
                return String::new();
            }
            return CStr::from_ptr(utf8).to_string_lossy().into_owned();
        }
        String::new()
    }
    pub fn probe() -> (bool, Vec<Display>) {
        objc2::rc::autoreleasepool(|_| unsafe {
            let mut ids = [0u32; 16];
            let mut count = 0;
            if CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut count) != 0 {
                return (false, Vec::new());
            }
            let mut mirrored = false;
            let mut out = Vec::new();
            for id in ids.iter().take(count as usize) {
                mirrored |= CGDisplayIsInMirrorSet(*id) != 0;
                let size = CGDisplayScreenSize(*id);
                out.push(Display {
                    name: name_of(*id),
                    vendor: super::pnp(CGDisplayVendorNumber(*id) as u16),
                    sizeless: size.width <= 0. || size.height <= 0.,
                    builtin: CGDisplayIsBuiltin(*id) != 0,
                });
            }
            (mirrored, out)
        })
    }
}
#[cfg(target_os = "windows")]
mod platform {
    use super::Display;
    use windows::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
        DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_MODE_INFO,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_TARGET_DEVICE_NAME, DISPLAYCONFIG_TOPOLOGY_CLONE, DISPLAYCONFIG_TOPOLOGY_ID,
        QDC_DATABASE_CURRENT,
    };
    const PATH_ACTIVE: u32 = 1;
    fn wide(s: &[u16]) -> String {
        let end = s.iter().position(|c| *c == 0).unwrap_or(s.len());
        String::from_utf16_lossy(&s[..end])
    }
    /// "\\?\DISPLAY#BNQ7F3D#..." carries the PNP code; the EDID id word is the fallback.
    fn vendor(path: &str, id: u16) -> String {
        let code: String = path
            .split('#')
            .nth(1)
            .unwrap_or("")
            .chars()
            .take(3)
            .collect();
        if code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic()) {
            code.to_ascii_uppercase()
        } else {
            super::pnp(id.swap_bytes())
        }
    }
    /// No UI-thread requirement on Windows, but the shared schedule keeps one path.
    pub fn probe() -> (bool, Vec<Display>) {
        unsafe {
            let (mut paths, mut modes) = (0u32, 0u32);
            if GetDisplayConfigBufferSizes(QDC_DATABASE_CURRENT, &mut paths, &mut modes).is_err() {
                return (false, Vec::new());
            }
            let mut path_array = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
            let mut mode_array = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
            let mut topology = DISPLAYCONFIG_TOPOLOGY_ID::default();
            if QueryDisplayConfig(
                QDC_DATABASE_CURRENT,
                &mut paths,
                path_array.as_mut_ptr(),
                &mut modes,
                mode_array.as_mut_ptr(),
                Some(&mut topology),
            )
            .is_err()
            {
                return (false, Vec::new());
            }
            let mut out = Vec::new();
            let mut sources = Vec::new();
            for p in path_array.iter().take(paths as usize) {
                if p.flags & PATH_ACTIVE == 0 {
                    continue;
                }
                let a = p.sourceInfo.adapterId;
                sources.push((a.LowPart, a.HighPart, p.sourceInfo.id));
                let mut name = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
                name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
                name.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
                name.header.adapterId = p.targetInfo.adapterId;
                name.header.id = p.targetInfo.id;
                if DisplayConfigGetDeviceInfo(&mut name.header) != 0 {
                    continue;
                }
                out.push(Display {
                    name: wide(&name.monitorFriendlyDeviceName),
                    vendor: vendor(&wide(&name.monitorDevicePath), name.edidManufactureId),
                    sizeless: false,
                    builtin: p.targetInfo.outputTechnology
                        == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL,
                });
            }
            // Two active paths fed by one source is a clone even outside the database topology.
            let total = sources.len();
            sources.sort();
            sources.dedup();
            (topology == DISPLAYCONFIG_TOPOLOGY_CLONE || sources.len() < total, out)
        }
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub fn probe() -> (bool, Vec<super::Display>) {
        (false, Vec::new())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn d(name: &str, vendor: &str, sizeless: bool) -> Display {
        Display {
            name: name.into(),
            vendor: vendor.into(),
            sizeless,
            builtin: false,
        }
    }
    #[test]
    fn pnp_decodes_edid_vendor_ids() {
        assert_eq!(pnp(0x10AC), "DEL");
        assert_eq!(pnp(1552), "APP");
        assert_eq!(pnp(0x09D1), "BNQ");
        assert_eq!(pnp(0), "");
        assert_eq!(pnp(0x6b6e), "", "kDisplayVendorIDUnknown low word");
    }
    #[test]
    fn projectors_seen_in_the_wild_are_recognised() {
        for (name, vendor, sizeless) in [
            ("EPSON PJ", "SEC", true),
            ("EPSON", "SEC", true),
            ("PJ", "BNQ", true),
            ("PJ", "SNY", false),
            ("PJ", "VSC", true),
            ("PJ", "CAS", true),
            ("XGA PJ", "ACR", true),
            ("1080P PJ", "ACR", true),
            ("LG PROJECTOR", "GSM", true),
            ("Projector", "HTC", false),
            ("LCD PROJECTOR", "HTC", true),
            ("NP-M311X", "NEC", true),
            ("PT-LW333D", "MEI", false),
            ("VPL-VW295ES", "SNY", false),
            ("1080P", "OTM", true),
            ("WXGA", "OTM", false),
            ("WUXGA", "GRU", false),
            ("", "OTM", true),
            ("XGIMI Horizon", "GMI", false),
            ("Nebula Capsule", "ANK", false),
            ("HU715Q", "GSM", false),
            ("The Freestyle", "SAM", false),
            ("EB-L200F", "SEC", true),
        ] {
            assert!(projector(&d(name, vendor, sizeless)), "{vendor} {name:?}");
        }
    }
    #[test]
    fn monitors_tvs_receivers_and_built_in_panels_are_not_projectors() {
        for (name, vendor, sizeless) in [
            ("22MP55PJ", "GSM", false),
            ("LG ULTRAWIDE", "GSM", false),
            ("LG HDR 4K", "GSM", false),
            ("LG TV", "GSM", true),
            ("TV", "SNY", true),
            ("SONY TV", "SNY", true),
            ("AVR", "DEN", true),
            ("SyncMaster", "SAM", true),
            ("", "SAM", true),
            ("DELL U2723QE", "DEL", false),
            ("PA278CV", "AUS", false),
            ("BenQ PD2700U", "BNQ", false),
            ("HF225", "HSD", false),
            ("PF71N", "LGD", false),
            ("1080P", "SAM", false),
            ("Color LCD", "APP", false),
            ("VS2447", "VSC", false),
            ("Odyssey G7", "SAM", false),
        ] {
            assert!(!projector(&d(name, vendor, sizeless)), "{vendor} {name:?}");
        }
        let mut built_in = d("EPSON PJ", "SEC", true);
        built_in.builtin = true;
        assert!(!projector(&built_in));
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn probe_reads_this_machines_displays_without_panicking() {
        let (mirrored, displays) = platform::probe();
        for d in &displays {
            eprintln!("display {d:?}");
            assert!(d.vendor.is_empty() || d.vendor.len() == 3);
            assert!(d.vendor.chars().all(|c| c.is_ascii_uppercase()));
        }
        eprintln!("mirrored={mirrored} presenting={}", active(mirrored, &displays));
    }
    #[test]
    fn mirroring_alone_counts_as_presenting() {
        let monitors = [d("LG ULTRAWIDE", "GSM", false), d("Color LCD", "APP", false)];
        assert!(!active(false, &monitors));
        assert!(active(true, &monitors));
        assert!(active(false, &[d("Color LCD", "APP", false), d("PJ", "BNQ", true)]));
        assert!(!active(false, &[]));
    }
}
