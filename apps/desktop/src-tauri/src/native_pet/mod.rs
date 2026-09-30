//! Native pet window + Rust scheduler. The `overlay` WebView is only a popup UI.
mod art;
mod gait;
mod physics;
mod platform;
#[cfg(debug_assertions)]
pub mod smoke;
use physics::{Motion, Physics};
use resvg::tiny_skia::Pixmap;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
#[cfg(not(target_os = "macos"))]
use tauri::{PhysicalPosition, PhysicalSize};
struct Press {
    cursor: (f64, f64),
    foot: (f64, f64),
    dragged: bool,
}
pub struct Native {
    runtime: Mutex<Runtime>,
    reset: AtomicBool,
    pending: AtomicBool,
}
struct Runtime {
    art: art::Art,
    physics: Option<Physics>,
    last: Instant,
    sense: Instant,
    mode: String,
    age: f64,
    drag_age: f64,
    gait_distance: f64,
    reaction: Option<&'static str>,
    last_source: String,
    left: bool,
    right: bool,
    press: Option<Press>,
    hits: Vec<[f64; 4]>,
    menu: bool,
    hover: bool,
    choice: Option<String>,
    selected: Option<String>,
    alert_age: f64,
    seed: u64,
    last_event: Value,
    last_picture: String,
    pending_menu: bool,
    pending_tickle: bool,
    visible: bool,
    render_count: u64,
    started: Instant,
    last_error: Option<String>,
    retry_at: Instant,
    position: Option<(i32, i32)>,
    ignoring: Option<bool>,
    on_top: Option<bool>,
}
impl Runtime {
    fn set_mode(&mut self, m: &str) {
        self.mode = m.into();
        self.age = 0.;
        self.gait_distance = 0.;
    }
    fn random(&mut self) -> f64 {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.seed >> 11) as f64 / (1u64 << 53) as f64
    }
    fn choose(&mut self) {
        let calm = ["idle", "idle", "idle", "bored", "sleepy", "curious"];
        let active = [
            "walk",
            "walk",
            "run",
            "jump",
            "excited",
            "greeting",
            "proud",
            "shy",
            "surprised",
            "playful",
            "sulking",
            "cheering",
        ];
        let pool: &[&str] = if calm.contains(&self.mode.as_str()) {
            &active
        } else {
            &calm
        };
        let m = pool[(self.random() * pool.len() as f64) as usize];
        self.set_mode(m);
    }
}
pub fn reset(app: &AppHandle) {
    if let Some(s) = app.try_state::<Native>() {
        s.reset.store(true, Ordering::Release);
    }
}
#[tauri::command]
pub fn native_pet_ui(
    app: AppHandle,
    menu: Option<bool>,
    hover: Option<bool>,
    choice: Option<String>,
) -> Result<(), String> {
    let s = app.state::<Native>();
    let mut r = s.runtime.lock().map_err(|_| "Native state unavailable")?;
    if let Some(v) = menu {
        r.menu = v;
    }
    if let Some(v) = hover {
        r.hover = v;
    }
    r.choice = choice;
    r.last_event = Value::Null;
    Ok(())
}
#[tauri::command]
pub fn native_pet_metrics(app: AppHandle) -> Value {
    let s = app.state::<Native>();
    let r = s.runtime.lock().unwrap();
    json!({"renderer":"resvg-native","framesPresented":r.render_count,"uptimeSeconds":r.started.elapsed().as_secs_f64(),"spriteCacheLimit":16,"physics":"rust","webviewAnimation":false})
}
pub fn init(app: &AppHandle) -> Result<(), String> {
    let win = tauri::WindowBuilder::new(app, "pet-native")
        .title("똑띠")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focusable(false)
        .visible(false)
        .inner_size(300., 260.)
        .build()
        .map_err(|e| e.to_string())?;
    platform::ignore_cursor(&win, true)?;
    app.manage(Native {
        runtime: Mutex::new(Runtime {
            art: art::Art::new()?,
            physics: None,
            last: Instant::now(),
            sense: Instant::now() - Duration::from_secs(1),
            mode: "greeting".into(),
            age: 0.,
            drag_age: 0.,
            gait_distance: 0.,
            reaction: None,
            last_source: String::new(),
            left: false,
            right: false,
            press: None,
            hits: vec![],
            menu: false,
            hover: false,
            choice: None,
            selected: None,
            alert_age: 0.,
            seed: crate::companion::now(),
            last_event: Value::Null,
            last_picture: String::new(),
            pending_menu: false,
            pending_tickle: false,
            visible: false,
            render_count: 0,
            started: Instant::now(),
            last_error: None,
            retry_at: Instant::now(),
            position: None,
            ignoring: None,
            on_top: None,
        }),
        reset: AtomicBool::new(true),
        pending: AtomicBool::new(false),
    });
    Ok(())
}
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(33));
        let state = handle.state::<Native>();
        if state.pending.swap(true, Ordering::AcqRel) {
            continue;
        }
        let app = handle.clone();
        if handle
            .run_on_main_thread(move || {
                let state = app.state::<Native>();
                if let Err(error) = tick(&app, &state) {
                    let mut r = state.runtime.lock().unwrap();
                    if r.last_error.as_ref() != Some(&error) {
                        eprintln!("Native pet: {error}");
                        let _ = app.emit("native-pet-error", &error);
                        r.last_error = Some(error);
                    }
                    r.retry_at = Instant::now() + Duration::from_secs(2);
                    if let Some(w) = app.get_webview_window("overlay") {
                        let _ = w.show();
                    }
                }
                state.pending.store(false, Ordering::Release);
            })
            .is_err()
        {
            state.pending.store(false, Ordering::Release);
            break;
        }
    });
}
fn priority(a: &Value) -> u8 {
    match a["source"].as_str().unwrap_or("") {
        "preview" => 0,
        "calendar"
            if a["startsAt"].as_f64().unwrap_or(f64::INFINITY)
                < crate::companion::now() as f64 + 300. =>
        {
            1
        }
        "timer" => 2,
        "calendar" => 3,
        "stretch" => 5,
        _ => 4,
    }
}
fn tick(app: &AppHandle, state: &Native) -> Result<(), String> {
    let mut r = state.runtime.lock().map_err(|_| "Native state poisoned")?;
    if Instant::now() < r.retry_at {
        return Ok(());
    }
    let dt = r.last.elapsed().as_secs_f64().min(0.1);
    r.last = Instant::now();
    let (preferences, alerts, ready, fullscreen) = {
        let state = app.state::<crate::companion::Companion>();
        let s = state.0.lock().unwrap();
        (
            s.saved.preferences.clone(),
            s.alerts.clone(),
            s.ready,
            s.fullscreen,
        )
    };
    let visible = ready
        && !(fullscreen && preferences.hide_fullscreen)
        && ((preferences.onboarded && preferences.resident)
            || !alerts.is_empty()
            || r.press.is_some());
    let pet = app.get_window("pet-native").ok_or("No native window")?;
    let popup = app.get_webview_window("overlay").ok_or("No popup window")?;
    if !visible {
        if r.visible {
            let _ = pet.hide();
            let _ = popup.hide();
        }
        r.visible = false;
        r.press = None;
        r.hover = false;
        r.menu = false;
        r.reaction = None;
        r.pending_menu = false;
        r.pending_tickle = false;
        (r.left, r.right) = platform::buttons();
        return Ok(());
    }
    let cfg = crate::effective_display(app);
    let resident = preferences.onboarded && preferences.resident;
    if state.reset.swap(false, Ordering::AcqRel) {
        r.physics = None;
        r.last_picture.clear();
        r.last_event = Value::Null;
    }
    if r.physics.is_none() || r.sense.elapsed() >= Duration::from_millis(150) {
        let world = crate::surfaces::pet_world(app.clone())?;
        if let Some(p) = &mut r.physics {
            p.update(world);
        } else {
            r.physics = Some(Physics::new(world));
        }
        r.sense = Instant::now();
    }
    let mut alerts = alerts;
    alerts.sort_by(|a, b| {
        priority(a)
            .cmp(&priority(b))
            .then(a["createdAt"].as_u64().cmp(&b["createdAt"].as_u64()))
    });
    let first = alerts.first();
    let selected = alerts
        .iter()
        .find(|a| a["id"].as_str() == r.choice.as_deref())
        .or(first);
    let id = selected.and_then(|a| a["id"].as_str()).map(str::to_string);
    if id != r.selected {
        if r.selected.is_some() && id.is_none() && r.last_source == "timer" {
            r.set_mode("proud");
        }
        r.last_source = selected
            .and_then(|a| a["source"].as_str())
            .unwrap_or("")
            .into();
        r.selected = id;
        r.alert_age = 0.;
    } else {
        r.alert_age += dt * cfg.speed;
    }
    #[allow(unused_mut)]
    let mut cursor = pet.cursor_position().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        // Tao returns cursor pixels using the PRIMARY scale, even on another display.
        let primary = pet
            .primary_monitor()
            .map_err(|e| e.to_string())?
            .ok_or("No primary monitor")?;
        cursor.x /= primary.scale_factor();
        cursor.y /= primary.scale_factor();
    }
    let (left, right) = platform::buttons();
    let hit = r.hits.iter().any(|p| {
        cursor.x >= p[0] && cursor.x <= p[0] + p[2] && cursor.y >= p[1] && cursor.y <= p[1] + p[3]
    });
    let p = r.physics.as_ref().unwrap();
    let mut monitor = popup
        .available_monitors()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|m| crate::surfaces::key(m) == p.world.monitor)
        .ok_or("Pet monitor disconnected")?;
    let sf = monitor.scale_factor();
    let wa = monitor.work_area();
    let foot = (
        (wa.position.x as f64 + p.x * sf) / screen_unit(sf),
        (wa.position.y as f64 + p.y * sf) / screen_unit(sf),
    );
    if left && !r.left && hit && !r.hover {
        r.drag_age = 0.;
        r.press = Some(Press {
            cursor: (cursor.x, cursor.y),
            foot,
            dragged: false,
        });
    }
    if right && !r.right && hit && !r.hover {
        if r.physics.as_ref().unwrap().busy() {
            r.pending_menu = true;
        } else {
            r.menu = !r.menu;
        }
    }
    let mut dropped = false;
    if let Some(press) = &mut r.press {
        if left {
            if ((cursor.x - press.cursor.0).powi(2) + (cursor.y - press.cursor.1).powi(2)).sqrt()
                > 4.
            {
                press.dragged = true;
            }
            if press.dragged {
                let x = press.foot.0 + cursor.x - press.cursor.0;
                let y = press.foot.1 + cursor.y - press.cursor.1;
                let m = popup
                    .monitor_from_point(x, y - 1.)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| monitor.clone());
                crate::place_pet(
                    app,
                    &popup,
                    &m,
                    x * screen_unit(m.scale_factor()),
                    y * screen_unit(m.scale_factor()),
                    &cfg,
                )
                .map_err(|e| e.to_string())?;
                let w = crate::surfaces::pet_world(app.clone())?;
                r.physics = Some(Physics::new(w));
                monitor = m;
                r.menu = false;
                r.reaction = None;
                r.pending_menu = false;
                r.pending_tickle = false;
            }
        } else {
            dropped = press.dragged;
            if !dropped {
                if r.physics.as_ref().unwrap().busy() {
                    r.pending_tickle = true;
                } else {
                    let sleepy = r.mode == "sleepy" && selected.is_none();
                    r.set_mode(if sleepy { "surprised" } else { "tickle" });
                    r.reaction = Some(if sleepy {
                        "앗, 깼어요!"
                    } else {
                        "간지러워요!"
                    });
                    r.menu = false;
                }
            }
            r.press = None;
        }
    }
    r.left = left;
    r.right = right;
    let dragging = r.press.as_ref().is_some_and(|p| p.dragged);
    let sf = monitor.scale_factor();
    let wa = monitor.work_area();
    if dragging {
        r.drag_age += dt * cfg.speed;
    }
    if dropped && !resident {
        crate::persist_overlay_position(app.clone())?;
    }
    let mut landed = false;
    if !dragging {
        let reduced = cfg.reduce_motion;
        let walk = gait::speed(&r.mode, crate::pet_size(&cfg), cfg.speed);
        let auto = resident
            && selected.is_none()
            && !hit
            && !r.hover
            && !r.menu
            && !r.pending_menu
            && !r.pending_tickle;
        let p = r.physics.as_mut().unwrap();
        if resident {
            let before = p.x;
            let previous_motion = p.motion;
            let grounded = p.motion == Motion::Grounded;
            p.step(dt, walk, auto, reduced);
            landed = previous_motion == Motion::Land && p.motion == Motion::Grounded;
            if grounded && auto && !reduced {
                let distance = (p.x - before).abs();
                r.gait_distance += distance;
            }
        } else if dropped {
            p.motion = Motion::Grounded;
        }
    }
    if landed && selected.is_none() && r.reaction.is_none() {
        r.set_mode("relieved");
    }
    if !r.physics.as_ref().unwrap().busy() {
        if r.pending_menu {
            r.menu = true;
            r.pending_menu = false;
        }
        if r.pending_tickle {
            r.pending_tickle = false;
            r.menu = false;
            r.set_mode("tickle");
            r.reaction = Some("간지러워요!");
        }
    }
    let busy = resident && r.physics.as_ref().unwrap().busy();
    if !busy && !dragging && !r.menu {
        if r.reaction.is_some() {
            r.age += dt * cfg.speed;
            if r.age >= art::duration(&r.mode) {
                let wake = r.mode == "surprised";
                r.reaction = None;
                r.set_mode(if wake { "curious" } else { "shy" });
            }
        } else if selected.is_none() && !cfg.reduce_motion {
            r.age += dt * cfg.speed;
            if r.physics.as_ref().unwrap().approaching() {
                if !matches!(r.mode.as_str(), "walk" | "run") {
                    r.set_mode("walk");
                }
            } else if r.age >= art::duration(&r.mode) {
                r.choose();
            }
            if hit && matches!(r.mode.as_str(), "walk" | "run") {
                r.set_mode("curious");
            }
        }
    }
    let reaction = r.reaction;
    let event =
        json!({"busy":busy,"dragging":dragging,"menu":r.menu,"reaction":reaction,"mode":r.mode});
    if event != r.last_event {
        app.emit("native-pet", &event).map_err(|e| e.to_string())?;
        r.last_event = event;
    }
    let show_popup = !dragging && !busy && (r.menu || selected.is_some() || reaction.is_some());
    if !show_popup {
        r.hover = false;
    }
    let (x, y, direction, rope_anchor, physics_mode, physics_age) = {
        let p = r.physics.as_ref().unwrap();
        (p.x, p.y, p.direction, p.rope_anchor(), p.motion, p.age)
    };
    // Keep the HTML popup geometry as the shared screen/monitor anchor; no JS physics runs.
    if !dragging {
        crate::place_pet(
            app,
            &popup,
            &monitor,
            wa.position.x as f64 + x * sf,
            wa.position.y as f64 + y * sf,
            &cfg,
        )
        .map_err(|e| e.to_string())?;
    }
    if show_popup {
        if !popup.is_visible().unwrap_or(false) {
            popup.show().map_err(|e| e.to_string())?;
        }
    } else if popup.is_visible().unwrap_or(false) {
        popup.hide().map_err(|e| e.to_string())?;
    }
    let mode = if dragging {
        "drag".to_string()
    } else if busy {
        physics_mode.pose().into()
    } else if r.menu {
        "idle".into()
    } else if reaction.is_some() {
        r.mode.clone()
    } else if let Some(a) = selected {
        a["source"].as_str().unwrap_or("slack").into()
    } else {
        r.mode.clone()
    };
    let age = if matches!(mode.as_str(), "walk" | "run") {
        gait::elapsed(&mode, r.gait_distance, crate::pet_size(&cfg))
    } else if dragging {
        r.drag_age
    } else if busy {
        physics_age
    } else if selected.is_some() && reaction.is_none() {
        r.alert_age
    } else {
        r.age
    };
    let facing = if busy || matches!(mode.as_str(), "walk" | "run") {
        direction
    } else {
        1.
    };
    let pose = r.art.pose(&mode, age, cfg.reduce_motion);
    let key = r.art.frame(&pose).key.clone();
    let hit_rects = r.art.frame(&pose).hit.clone();
    let size = crate::pet_size(&cfg);
    let k = (size * sf).ceil() / 260.;
    let (fx, fy) = (wa.position.x as f64 + x * sf, wa.position.y as f64 + y * sf);
    let pet_left = fx - 200. * k + facing * pose.offset * k;
    let pet_top = fy - (250. + pose.lift) * k;
    let rope = if cfg.reduce_motion || dragging {
        None
    } else {
        rope_anchor.map(|(x, y)| (wa.position.x as f64 + x * sf, wa.position.y as f64 + y * sf))
    };
    let left = pet_left
        .min(rope.map_or(pet_left, |a| a.0 - 15. * sf))
        .floor()
        - 2.;
    let top = pet_top
        .min(rope.map_or(pet_top, |a| a.1 - 12. * sf))
        .floor()
        - 2.;
    let width = (pet_left + 400. * k - left + 4.).ceil() as u32;
    let height = (pet_top + 260. * k - top + 4.).ceil() as u32;
    if width == 0 || height == 0 || width > 8192 || height > 16384 {
        return Err("Native overlay bounds invalid".into());
    }
    let stamp = format!(
        "{key}:{:.2}:{:.2}:{:.2}:{:.2}:{:.2}:{width}:{height}:{facing}:{:?}:{:.2}",
        k,
        pose.offset,
        pose.lift,
        pet_top - top,
        pet_left - left,
        rope.map(|a| (a.0 - left, a.1 - top)),
        if rope.is_some() { age } else { 0. }
    );
    let position = (
        (left / screen_unit(sf)).round() as i32,
        (top / screen_unit(sf)).round() as i32,
    );
    if r.position != Some(position) {
        #[cfg(target_os = "macos")]
        pet.set_position(tauri::LogicalPosition::new(left / sf, top / sf))
            .map_err(|e| e.to_string())?;
        #[cfg(not(target_os = "macos"))]
        pet.set_position(PhysicalPosition::new(position.0, position.1))
            .map_err(|e| e.to_string())?;
        r.position = Some(position);
    }
    if stamp != r.last_picture || !r.visible {
        #[cfg(target_os = "macos")]
        pet.set_size(tauri::LogicalSize::new(
            width as f64 / sf,
            height as f64 / sf,
        ))
        .map_err(|e| e.to_string())?;
        #[cfg(not(target_os = "macos"))]
        pet.set_size(PhysicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
        let frame = r.art.bitmap(&key, (260. * k).ceil() as u32)?;
        let mut canvas = Pixmap::new(width, height).ok_or("Native canvas allocation failed")?;
        if let Some(anchor) = rope {
            let hands = r
                .art
                .hands(&pose)
                .into_iter()
                .map(|h| {
                    (
                        (fx + facing * (h[0] - 200. + pose.offset) * k - left) as f32,
                        (fy + (h[1] - 250. - pose.lift) * k - top) as f32,
                    )
                })
                .collect::<Vec<_>>();
            art::rope(
                &mut canvas,
                ((anchor.0 - left) as f32, (anchor.1 - top) as f32),
                &hands,
                age,
                &mode,
                k as f32,
                facing as f32,
            );
        }
        art::composite(
            &mut canvas,
            &frame,
            (pet_left - left) as f32,
            (pet_top - top) as f32,
            facing < 0.,
        );
        platform::present(&pet, &canvas)?;
        r.last_picture = stamp;
        r.render_count += 1;
    }
    r.hits = hit_rects
        .into_iter()
        .map(|h| {
            let a = fx + facing * (h[0] - 200. + pose.offset) * k;
            let b = a + facing * h[2] * k;
            [
                a.min(b) / screen_unit(sf),
                (fy + (h[1] - 250. - pose.lift) * k) / screen_unit(sf),
                h[2] * k / screen_unit(sf),
                h[3] * k / screen_unit(sf),
            ]
        })
        .collect();
    let ignore = !hit && r.press.is_none();
    if r.ignoring != Some(ignore) {
        platform::ignore_cursor(&pet, ignore)?;
        r.ignoring = Some(ignore);
    }
    if r.on_top != Some(cfg.always_on_top) {
        platform::on_top(&pet, cfg.always_on_top)?;
        r.on_top = Some(cfg.always_on_top);
    }
    if !r.visible {
        pet.show().map_err(|e| e.to_string())?;
    }
    r.visible = true;
    if r.last_error.take().is_some() {
        let _ = app.emit("native-pet-error", "");
    }
    Ok(())
}

// Global screen positions use points on macOS and physical pixels on Windows.
fn screen_unit(scale: f64) -> f64 {
    if cfg!(target_os = "macos") {
        scale
    } else {
        1.
    }
}
