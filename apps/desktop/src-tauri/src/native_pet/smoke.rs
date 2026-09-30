//! Debug-only, isolated native integration check. Never reads production account storage.
use super::*;
pub fn requested(app: &AppHandle) -> bool {
    app.config().identifier.ends_with(".native-smoke")
        && std::env::args().any(|a| a == "--native-smoke")
}
pub fn start(app: &AppHandle) {
    {
        let state = app.state::<crate::companion::Companion>();
        let mut s = state.0.lock().unwrap();
        s.saved.preferences.onboarded = true;
        s.saved.preferences.resident = true;
        s.saved.preferences.hide_fullscreen = false;
        s.ready = true;
    }
    let _ = crate::apply_overlay_layout(app);
    crate::companion::emit(app);
    // Run the production input/foreground polling too; animation-only checks miss
    // lock inversions between the background poller and the main-thread renderer.
    crate::companion::start(app.clone());
    let app = app.clone();
    let completed = std::sync::Arc::new(AtomicBool::new(false));
    let watchdog = completed.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        if !watchdog.load(Ordering::Acquire) {
            eprintln!("SMOKE FAILED: main thread/poller stopped responding");
            std::process::exit(2);
        }
    });
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(1));
        // Exercise the actual window APIs and delayed readback on every attached display.
        let monitors = app.available_monitors().unwrap_or_default();
        let mut monitor_checks = true;
        for monitor in monitors {
            let a = app.clone();
            let expected = crate::surfaces::key(&monitor);
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = app.run_on_main_thread(move || {
                let result = (|| -> Result<(), String> {
                    let popup = a.get_webview_window("overlay").ok_or("No popup")?;
                    let wa = monitor.work_area();
                    crate::place_pet(
                        &a,
                        &popup,
                        &monitor,
                        wa.position.x as f64 + wa.size.width as f64 * 0.5,
                        wa.position.y as f64 + wa.size.height as f64,
                        &crate::effective_display(&a),
                    )
                    .map_err(|e| e.to_string())?;
                    let world = crate::surfaces::pet_world(a.clone())?;
                    if world.monitor != crate::surfaces::key(&monitor) {
                        return Err("Move readback selected another display".into());
                    }
                    let state = a.state::<Native>();
                    state.reset.store(false, Ordering::Release);
                    let mut r = state.runtime.lock().unwrap();
                    r.physics = Some(Physics::new(world));
                    r.set_mode("walk");
                    Ok(())
                })();
                let _ = tx.send(result);
            });
            let moved = matches!(rx.recv_timeout(Duration::from_secs(5)), Ok(Ok(())));
            std::thread::sleep(Duration::from_millis(1200));
            let state = app.state::<Native>();
            let r = state.runtime.lock().unwrap();
            let retained = r
                .physics
                .as_ref()
                .is_some_and(|p| p.world.monitor == expected);
            let walking = r
                .physics
                .as_ref()
                .is_some_and(|p| (p.x - p.world.width * 0.5).abs() > 1.);
            let passed = moved && retained && walking && r.last_error.is_none();
            eprintln!("SMOKE monitor {expected}: moved={moved} retained={retained} walking={walking} passed={passed} mode={} hover={} foot={:?}", r.mode, r.hover, r.physics.as_ref().map(|p| (p.x,p.y)));
            monitor_checks &= passed;
        }
        for mode in [
            "idle", "walk", "run", "sleepy", "tickle", "slack", "calendar", "timer", "stretch",
        ] {
            let a = app.clone();
            let _ = app.run_on_main_thread(move || {
                {
                    let state = a.state::<crate::companion::Companion>();
                    let mut s = state.0.lock().unwrap();
                    s.alerts.clear();
                    if ["slack", "calendar", "timer", "stretch"].contains(&mode) {
                        s.alerts.push(json!({"id":mode,"source":mode,"title":"네이티브 알림 확인","body":"캐릭터와 HTML 말풍선","createdAt":crate::companion::now()*1000}));
                    }
                }
                {
                    let state = a.state::<Native>();
                    let mut r = state.runtime.lock().unwrap();
                    r.set_mode(mode);
                    r.reaction = if mode == "tickle" { Some("간지러워요!") } else { None };
                }
                crate::companion::emit(&a);
            });
            std::thread::sleep(Duration::from_secs(2));
            let state = app.state::<Native>();
            let r = state.runtime.lock().unwrap();
            eprintln!(
                "SMOKE {mode}: visible={} frames={} error={:?} mode={} menu={} dragging={}",
                r.visible,
                r.render_count,
                r.last_error,
                r.mode,
                r.menu,
                r.press.as_ref().is_some_and(|p| p.dragged)
            );
        }
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            let state = a.state::<Native>();
            let r = state.runtime.lock().unwrap();
            let passed = monitor_checks
                && r.render_count > 10
                && r.last_error.is_none()
                && r.visible
                && a.get_webview_window("pet-native").is_none();
            eprintln!(
                "SMOKE RESULT passed={passed} native_window={} webviews={}",
                a.get_window("pet-native").is_some(),
                a.webview_windows().len()
            );
            drop(r);
            completed.store(true, Ordering::Release);
            a.exit(if passed { 0 } else { 1 });
        });
    });
}
