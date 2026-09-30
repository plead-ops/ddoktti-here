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
        // Prior smoke runs save a real timer; never let it expire into this run.
        s.saved = crate::companion::Persisted::default();
        s.alerts.clear();
        s.snoozed.clear();
        s.saved.preferences.onboarded = true;
        s.saved.preferences.resident = true;
        s.saved.preferences.hide_fullscreen = false;
        s.ready = true;
    }
    let mut cfg = crate::effective_display(app);
    cfg.scale = 1.7;
    cfg.speed = 1.;
    cfg.follow_cursor = false;
    cfg.activity_scope = "all".into();
    cfg.reduce_motion = false;
    let _ = crate::set_display_settings(app.clone(), cfg);
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
        // Exercise the real native handoff between touching attached monitors.
        let a = app.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            let result = (|| -> Result<Option<String>, String> {
                let popup = a.get_webview_window("overlay").ok_or("No popup")?;
                let monitors = popup.available_monitors().map_err(|e| e.to_string())?;
                let screens = monitors
                    .iter()
                    .map(|m| {
                        let u = screen_unit(m.scale_factor());
                        let w = m.work_area();
                        crossing::Screen {
                            id: crate::surfaces::key(m),
                            x: m.position().x as f64 / u,
                            y: m.position().y as f64 / u,
                            w: m.size().width as f64 / u,
                            h: m.size().height as f64 / u,
                            wx: w.position.x as f64 / u,
                            wy: w.position.y as f64 / u,
                            ww: w.size.width as f64 / u,
                            wh: w.size.height as f64 / u,
                            factor: m.scale_factor() / u,
                        }
                    })
                    .collect::<Vec<_>>();
                let cfg = crate::effective_display(&a);
                let size = crate::pet_size(&cfg);
                for from in &screens {
                    for direction in [1., -1.] {
                        let x = if direction > 0. {
                            from.wx + from.ww - size * 0.46 * from.factor
                        } else {
                            from.wx + size * 0.46 * from.factor
                        };
                        let y = from.wy + from.wh;
                        if let Some(c) = crossing::Crossing::new(
                            from,
                            &screens,
                            (x, y),
                            direction,
                            size,
                            gait::speed("walk", size, cfg.speed) * from.factor,
                        ) {
                            let target = c.to.clone();
                            let m = monitors
                                .iter()
                                .find(|m| crate::surfaces::key(m) == from.id)
                                .unwrap();
                            crate::place_pet(
                                &a,
                                &popup,
                                m,
                                x * screen_unit(m.scale_factor()),
                                y * screen_unit(m.scale_factor()),
                                &cfg,
                            )
                            .map_err(|e| e.to_string())?;
                            let p = Physics::new(crate::surfaces::pet_world(a.clone())?);
                            let state = a.state::<Native>();
                            let mut r = state.runtime.lock().unwrap();
                            r.physics = Some(p);
                            r.activity = None;
                            r.crossing = Some(c);
                            r.set_mode("walk");
                            r.reaction = None;
                            return Ok(Some(target));
                        }
                    }
                }
                Ok(None)
            })();
            let _ = tx.send(result);
        });
        let crossing_check = match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(Some(target))) => {
                let started = Instant::now();
                let mut passed = false;
                while started.elapsed() < Duration::from_secs(25) {
                    std::thread::sleep(Duration::from_millis(200));
                    let state = app.state::<Native>();
                    let r = state.runtime.lock().unwrap();
                    if r.last_error.is_some() {
                        break;
                    }
                    if r.crossing.is_none() {
                        passed = r.physics.as_ref().is_some_and(|p| {
                            p.world.monitor == target && (p.y - p.world.height).abs() < 2.
                        });
                        break;
                    }
                }
                eprintln!("SMOKE autonomous display handoff: passed={passed}");
                passed
            }
            Ok(Ok(None)) => {
                eprintln!("SMOKE autonomous display handoff: skipped (no touching display pair)");
                true
            }
            _ => false,
        };
        let ticks_start = app.state::<Native>().runtime.lock().unwrap().tick_count;
        let measurement = Instant::now();
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
                    // Each input sample starts grounded; a rope approach from the
                    // preceding run must not override the requested sleepy pose.
                    if let Some(previous)=r.physics.take(){let mut world=previous.world;world.x=world.width/2.;world.y=world.height;r.physics=Some(Physics::new(world));}
                    r.activity=None;r.crossing=None;r.play_wait=100.;r.crossing_wait=100.;
                    r.menu=false;r.pending_menu=false;r.pending_tickle=false;r.press=None;
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
        let rate = (app.state::<Native>().runtime.lock().unwrap().tick_count - ticks_start) as f64
            / measurement.elapsed().as_secs_f64();
        eprintln!("SMOKE scheduler observed rate: {rate:.1} ticks/s (target 60)");
        // Successful timer save reveals the acknowledgement even in notification-only mode.
        let a = app.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            {
                let state = a.state::<crate::companion::Companion>();
                let mut s = state.0.lock().unwrap();
                s.saved.preferences.resident = false;
                s.alerts.clear();
            }
            {
                let state = a.state::<Native>();
                let mut r = state.runtime.lock().unwrap();
                r.reaction = None;
                r.pending_ack = false;
                r.activity = None;
                r.crossing = None;
                r.menu = false;
                // Notification-only custom placement can be above the floor.
                if let Some(p) = r.physics.as_mut() {
                    p.reset(p.world.width / 2., p.world.height / 2.);
                }
            }
            let invalid =
                crate::companion::timer_action(a.clone(), "start".into(), Some(0)).is_err();
            let no_false_ack = !a.state::<Native>().runtime.lock().unwrap().pending_ack;
            let saved = crate::companion::timer_action(a.clone(), "start".into(), Some(10)).is_ok();
            let _ = tx.send(invalid && no_false_ack && saved);
        });
        let timer_saved = rx.recv_timeout(Duration::from_secs(5)).unwrap_or(false);
        std::thread::sleep(Duration::from_millis(400));
        let timer_ack = {
            let state = app.state::<Native>();
            let r = state.runtime.lock().unwrap();
            timer_saved && r.visible && r.mode == "ack" && r.reaction.is_some() && !r.pending_ack
        };
        eprintln!("SMOKE timer acknowledgement: passed={timer_ack}");
        std::thread::sleep(Duration::from_secs(3));
        let timer_hides = {
            let state = app.state::<Native>();
            let r = state.runtime.lock().unwrap();
            !r.visible && r.reaction.is_none()
        };
        eprintln!("SMOKE notification-only acknowledgement hides: passed={timer_hides}");
        let a = app.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            {
                let state = a.state::<crate::companion::Companion>();
                let mut s = state.0.lock().unwrap();
                s.saved.preferences.resident = true;
            }
            let mut cfg = crate::effective_display(&a);
            cfg.scale = 5.;
            let result = crate::set_display_settings(a.clone(), cfg);
            let _ = tx.send(result.is_ok());
        });
        let large_saved = rx.recv_timeout(Duration::from_secs(5)).unwrap_or(false);
        std::thread::sleep(Duration::from_secs(1));
        let large = {
            let state = app.state::<Native>();
            let r = state.runtime.lock().unwrap();
            large_saved
                && r.visible
                && r.physics.as_ref().is_some_and(|p| p.world.size > 500.)
                && r.last_error.is_none()
        };
        eprintln!("SMOKE 5x native size: passed={large}");
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            let state = a.state::<Native>();
            let r = state.runtime.lock().unwrap();
            let passed = monitor_checks
                && crossing_check
                && timer_ack
                && timer_hides
                && large
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
