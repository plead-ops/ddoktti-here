//! Native pet window + Rust scheduler. The `overlay` WebView is only a popup UI.
mod activity;
mod art;
mod crossing;
mod gait;
mod petting;
mod physics;
mod platform;
mod presence;
#[cfg(debug_assertions)]
pub mod smoke;
mod tickle;
use physics::{Motion, Nudge, Physics};
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
struct Press {
    cursor: (f64, f64),
    dragged: bool,
    // Pendulum while dangling from the antenna tip held at the pointer.
    last: (f64, f64),
    velocity: (f64, f64),
    angle: f64,
    spin: f64,
    shake: f64,
    dizzy: f64,
}
/// Advance the hanging pendulum: `angle` (radians, clockwise on screen) and `spin`
/// under gravity and the pointer's acceleration `(ax, ay)`; `length` is the
/// pivot-to-body distance in screen units. Returns the new (angle, spin).
fn swing(angle: f64, spin: f64, (ax, ay): (f64, f64), length: f64, dt: f64) -> (f64, f64) {
    const GRAVITY: f64 = 2600.;
    const DAMPING: f64 = 1.1;
    let length = length.max(20.);
    let steps = 4;
    let h = dt.clamp(0., 0.1) / steps as f64;
    let (mut a, mut w) = (angle, spin);
    for _ in 0..steps {
        let accel = (ax * a.cos() - (GRAVITY - ay) * a.sin()) / length - DAMPING * w;
        w = (w + accel * h).clamp(-30., 30.);
        a += w * h;
    }
    // Whirling the pointer can carry the body over the top; keep the angle wrapped.
    use std::f64::consts::PI;
    a = (a + PI).rem_euclid(2. * PI) - PI;
    (a, w)
}
pub struct Native {
    runtime: Mutex<Runtime>,
    reset: AtomicBool,
    pending: AtomicBool,
}
struct Runtime {
    art: art::Art,
    physics: Option<Physics>,
    activity: Option<activity::Activity>,
    crossing: Option<crossing::Crossing>,
    crossing_wait: f64,
    play_wait: f64,
    last_play: Option<activity::Kind>,
    play_side: f64,
    follow_age: f64,
    tickle: tickle::Tickle,
    petting: petting::Petting,
    connections: crate::connection::Notices,
    presence: presence::Presence,
    /// A welcome back (seconds away, seconds left to play it) waiting for a calm moment.
    pending_return: Option<(f64, f64)>,
    /// A lost-support landing waiting for a calm moment to sulk about.
    pending_sulk: bool,
    /// Seconds before another window-nudge reaction may play.
    nudge_wait: f64,
    world_logged: Instant,
    follow_rest: f64,
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
    pending_ack: bool,
    visible: bool,
    render_count: u64,
    tick_count: u64,
    started: Instant,
    last_error: Option<String>,
    retry_at: Instant,
    position: Option<(i32, i32)>,
    ignoring: Option<bool>,
    on_top: Option<bool>,
    // Overflow window: paints the part of the canvas lying on a neighbouring display.
    overflow_picture: String,
    overflow_position: Option<(i32, i32)>,
    overflow_visible: bool,
    overflow_on_top: Option<bool>,
}
impl Runtime {
    fn set_mode(&mut self, m: &str) {
        self.mode = m.into();
        self.age = 0.;
        self.gait_distance = 0.;
    }
    fn tickle(&mut self) {
        match self.tickle.request() {
            Some(tickle::Response::Laugh) => {
                self.set_mode("tickle");
                self.reaction = Some("간지러워요!");
            }
            Some(tickle::Response::Enough) => {
                self.set_mode("enough");
                self.reaction = Some("이제 그만!");
            }
            None => {}
        }
        self.menu = false;
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
        if matches!(m, "walk" | "run") {
            let sample = self.random();
            if let Some(p) = self.physics.as_mut() {
                p.begin_wander(sample);
            }
        }
        self.set_mode(m);
    }
}
pub fn reset(app: &AppHandle) {
    if let Some(s) = app.try_state::<Native>() {
        s.reset.store(true, Ordering::Release);
    }
}
/// Called only after the timer has been persisted, outside the Companion lock.
pub fn acknowledge_timer(app: &AppHandle) {
    if let Some(state) = app.try_state::<Native>() {
        if let Ok(mut r) = state.runtime.lock() {
            r.pending_ack = true;
            r.menu = false;
            r.pending_menu = false;
        }
    }
}
/// Service workers call this only after releasing their own state locks.
pub fn connection_status(
    app: &AppHandle,
    service: crate::connection::Service,
    problem: Option<crate::connection::Problem>,
) {
    if let Some(state) = app.try_state::<Native>() {
        if let Ok(mut r) = state.runtime.lock() {
            let now = r.started.elapsed().as_secs();
            r.connections.observe(service, problem, now);
        }
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
    json!({"renderer":"resvg-native","framesPresented":r.render_count,"uptimeSeconds":r.started.elapsed().as_secs_f64(),"spriteCacheLimit":16,"spriteCacheBytesLimit":art::CACHE_BYTES,"targetMovementFps":60,"schedulerTicks":r.tick_count,"physics":"rust","webviewAnimation":false})
}
pub fn init(app: &AppHandle) -> Result<(), String> {
    for label in ["pet-native", "pet-native-2"] {
        let win = tauri::WindowBuilder::new(app, label)
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
    }
    app.manage(Native {
        runtime: Mutex::new(Runtime {
            art: art::Art::new()?,
            physics: None,
            activity: None,
            crossing: None,
            crossing_wait: 20.,
            play_wait: 18.,
            last_play: None,
            play_side: 1.,
            follow_age: 0.,
            tickle: tickle::Tickle::default(),
            petting: petting::Petting::default(),
            connections: crate::connection::Notices::default(),
            presence: presence::Presence::default(),
            pending_return: None,
            pending_sulk: false,
            nudge_wait: 0.,
            world_logged: Instant::now() - Duration::from_secs(60),
            follow_rest: 0.,
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
            pending_ack: false,
            visible: false,
            render_count: 0,
            tick_count: 0,
            started: Instant::now(),
            last_error: None,
            retry_at: Instant::now(),
            position: None,
            ignoring: None,
            overflow_picture: String::new(),
            overflow_position: None,
            overflow_visible: false,
            overflow_on_top: None,
            on_top: None,
        }),
        reset: AtomicBool::new(true),
        pending: AtomicBool::new(false),
    });
    Ok(())
}
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let period = Duration::from_nanos(16_666_667);
        let mut next = Instant::now();
        loop {
            next += period;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now;
            }
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
    let tick_started = Instant::now();
    let mut r = state.runtime.lock().map_err(|_| "Native state poisoned")?;
    if Instant::now() < r.retry_at {
        return Ok(());
    }
    let dt = r.last.elapsed().as_secs_f64().min(0.1);
    r.last = Instant::now();
    r.tick_count += 1;
    let (preferences, alerts, ready, hidden, fullscreen, presenting) = {
        let state = app.state::<crate::companion::Companion>();
        let s = state.0.lock().unwrap();
        (
            s.saved.preferences.clone(),
            s.alerts.clone(),
            s.ready,
            s.hidden(),
            s.fullscreen,
            s.presenting,
        )
    };
    let notice_time = r.started.elapsed().as_secs();
    let notice_allowed =
        preferences.onboarded && preferences.quiet_until <= crate::companion::now();
    let visible = ready
        && !hidden
        && ((preferences.onboarded && preferences.resident)
            || !alerts.is_empty()
            || r.press.is_some()
            || r.pending_ack
            || (notice_allowed && r.connections.pending(notice_time))
            || r.reaction.is_some());
    let pet = app.get_window("pet-native").ok_or("No native window")?;
    let popup = app.get_webview_window("overlay").ok_or("No popup window")?;
    if !visible {
        if r.visible {
            trace(|| format!("hide fullscreen={fullscreen} presenting={presenting} ready={ready}"));
            let _ = pet.hide();
            let _ = popup.hide();
        }
        if r.overflow_visible {
            if let Some(w) = app.get_window("pet-native-2") {
                let _ = w.hide();
            }
            r.overflow_visible = false;
        }
        r.visible = false;
        r.crossing = None;
        if r.activity.take().is_some() {
            if let Some(p) = r.physics.as_mut() {
                p.reset(p.x, p.y);
            }
        }
        r.press = None;
        r.hover = false;
        r.menu = false;
        r.reaction = None;
        r.pending_menu = false;
        r.pending_tickle = false;
        r.pending_return = None;
        r.pending_sulk = false;
        (r.left, r.right) = platform::buttons();
        return Ok(());
    }
    let cfg = crate::effective_display(app);
    let resident = preferences.onboarded && preferences.resident;
    if state.reset.swap(false, Ordering::AcqRel) {
        r.physics = None;
        r.activity = None;
        r.crossing = None;
        r.last_picture.clear();
        r.overflow_picture.clear();
        r.last_event = Value::Null;
    }
    if resident && cfg.activity_scope == "primary" {
        if let Some(m) = popup.primary_monitor().map_err(|e| e.to_string())? {
            let mismatch = r
                .physics
                .as_ref()
                .is_some_and(|p| p.world.monitor != crate::surfaces::key(&m));
            if mismatch {
                let wa = m.work_area();
                crate::place_pet(
                    app,
                    &popup,
                    &m,
                    wa.position.x as f64 + wa.size.width as f64 / 2.,
                    wa.position.y as f64 + wa.size.height as f64,
                    &cfg,
                )
                .map_err(|e| e.to_string())?;
                r.physics = None;
                r.activity = None;
                r.crossing = None;
            }
        }
    }
    let mut nudge = Nudge::None;
    let mut presence_event = None;
    if r.physics.is_none() || r.sense.elapsed() >= Duration::from_millis(150) {
        let world = crate::surfaces::pet_world(app.clone())?;
        let guided = r.crossing.is_some() || r.activity.is_some();
        if let Some(p) = &mut r.physics {
            if guided {
                p.guide(world);
            } else {
                nudge = p.update(world);
            }
        } else {
            r.physics = Some(Physics::new(world));
        }
        let idle = crate::companion::idle_seconds();
        presence_event = r
            .presence
            .observe((idle != u64::MAX).then_some(idle as f64));
        r.sense = Instant::now();
        if trace_enabled() && r.world_logged.elapsed() >= Duration::from_secs(10) {
            r.world_logged = Instant::now();
            let p = r.physics.as_ref().unwrap();
            let windows: Vec<String> = p
                .world
                .windows
                .iter()
                .map(|w| format!("{}:{:.0},{:.0},{:.0},{:.0}", w.id, w.x, w.y, w.width, w.height))
                .collect();
            trace(|| {
                format!(
                    "world support={:?} perch={:.0} urge={:.2} choice={} windows=[{}]",
                    p.support_id(),
                    p.perch,
                    p.descent_urge(),
                    p.last_choice,
                    windows.join(" ")
                )
            });
        }
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
    let monitors = popup.available_monitors().map_err(|e| e.to_string())?;
    let current = r.physics.as_ref().unwrap().world.monitor.clone();
    let mut monitor = if let Some(m) = monitors.iter().find(|m| crate::surfaces::key(m) == current)
    {
        m.clone()
    } else {
        let m = popup
            .primary_monitor()
            .ok()
            .flatten()
            .or_else(|| monitors.first().cloned())
            .ok_or("No connected monitor")?;
        let wa = m.work_area();
        crate::place_pet(
            app,
            &popup,
            &m,
            wa.position.x as f64 + wa.size.width as f64 / 2.,
            wa.position.y as f64 + wa.size.height as f64,
            &cfg,
        )
        .map_err(|e| e.to_string())?;
        r.physics = Some(Physics::new(crate::surfaces::pet_world(app.clone())?));
        r.activity = None;
        r.crossing = None;
        r.press = None;
        m
    };
    let sf = monitor.scale_factor();
    if left && !r.left && hit && !r.hover {
        r.drag_age = 0.;
        r.presence.wake();
        r.press = Some(Press {
            cursor: (cursor.x, cursor.y),
            dragged: false,
            last: (cursor.x, cursor.y),
            velocity: (0., 0.),
            angle: 0.,
            spin: 0.,
            shake: 0.,
            dizzy: 0.,
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
    // Where the pointer holds the character: the antenna tip of the current drag frame.
    let grip = if r.press.is_some() {
        let dizzy = r.press.as_ref().is_some_and(|p| p.dizzy > 0.);
        let pose = r.art.pose(
            if dizzy { "dizzy" } else { "drag" },
            r.drag_age,
            cfg.reduce_motion,
        );
        let key = r.art.frame(&pose).key.clone();
        let tip = r.art.tip(&key);
        (tip.0 + pose.offset - 200., tip.1 - 250. - pose.lift)
    } else {
        (0., -200.)
    };
    let unit = crate::pet_size(&cfg) / 260. * sf / screen_unit(sf);
    let mut carry_to: Option<(f64, f64)> = None;
    if let Some(press) = &mut r.press {
        if left {
            if ((cursor.x - press.cursor.0).powi(2) + (cursor.y - press.cursor.1).powi(2)).sqrt()
                > 4.
            {
                press.dragged = true;
            }
            if press.dragged {
                // The pointer carries the antenna tip; the body swings below it.
                let step = dt.clamp(0.001, 0.1);
                let velocity = (
                    (cursor.x - press.last.0) / step,
                    (cursor.y - press.last.1) / step,
                );
                let accel = (
                    ((velocity.0 - press.velocity.0) / step).clamp(-60000., 60000.),
                    ((velocity.1 - press.velocity.1) / step).clamp(-60000., 60000.),
                );
                press.last = (cursor.x, cursor.y);
                press.velocity = velocity;
                let speed = velocity.0.hypot(velocity.1);
                press.shake =
                    (press.shake + ((speed - 1000.) * step).max(0.) - 500. * step).max(0.);
                if press.shake > 900. {
                    press.dizzy = 4.5;
                    press.shake = 450.;
                } else if press.dizzy > 0. && speed < 600. {
                    press.dizzy = (press.dizzy - step).max(0.);
                }
                let hang = (-grip.0 * unit, -grip.1 * unit);
                let length = hang.0.hypot(hang.1) * 0.6;
                let (angle, spin) = swing(press.angle, press.spin, accel, length, step);
                press.angle = angle;
                press.spin = spin;
                // The frame is drawn rotated about the pointer, so its unrotated antenna
                // tip must sit exactly on the pointer: no rotation here.
                carry_to = Some((cursor.x + hang.0, cursor.y + hang.1));
            }
        } else {
            dropped = press.dragged;
            if dropped && press.angle.abs() > 0.01 {
                // Land where the swinging body actually is, not at the unrotated foot.
                let (sin, cos) = press.angle.sin_cos();
                let (hx, hy) = (-grip.0 * unit, -grip.1 * unit);
                carry_to = Some((
                    press.last.0 + hx * cos - hy * sin,
                    press.last.1 + hx * sin + hy * cos,
                ));
            }
            if !dropped {
                if r.physics.as_ref().unwrap().busy() {
                    r.pending_tickle = true;
                } else {
                    let sleepy = matches!(r.mode.as_str(), "sleepy" | "asleep") && selected.is_none();
                    if sleepy {
                        r.set_mode("surprised");
                        r.reaction = Some("앗, 깼어요!");
                    } else {
                        r.tickle();
                    }
                    r.menu = false;
                }
            }
            r.press = None;
        }
    }
    if let Some((x, y)) = carry_to {
        let m = popup
            .monitor_from_point(x, y - 1.)
            .ok()
            .flatten()
            .unwrap_or_else(|| monitor.clone());
        let m = if resident && cfg.activity_scope == "primary" {
            popup.primary_monitor().ok().flatten().unwrap_or(m)
        } else {
            m
        };
        let work = m.work_area();
        let unit = screen_unit(m.scale_factor());
        let x = if resident && cfg.activity_scope == "primary" {
            x.clamp(
                work.position.x as f64 / unit
                    + (crate::pet_half(&cfg) * m.scale_factor()).min(work.size.width as f64 / 2.)
                        / unit,
                (work.position.x as f64 + work.size.width as f64) / unit
                    - (crate::pet_half(&cfg) * m.scale_factor()).min(work.size.width as f64 / 2.)
                        / unit,
            )
        } else {
            x
        };
        let y = if resident && cfg.activity_scope == "primary" {
            y.clamp(
                work.position.y as f64 / unit
                    + (crate::pet_height(&cfg) * m.scale_factor()).min(work.size.height as f64)
                        / unit,
                (work.position.y as f64 + work.size.height as f64) / unit,
            )
        } else {
            y
        };
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
    r.left = left;
    r.right = right;
    let dragging = r.press.as_ref().is_some_and(|p| p.dragged);
    let held = left && r.press.as_ref().is_some_and(|p| !p.dragged);
    if r.tickle.advance(dt, held) {
        if r.physics.as_ref().unwrap().busy() {
            r.pending_tickle = true;
        } else {
            r.tickle();
        }
    }
    let can_pet = hit
        && !left
        && !right
        && !r.hover
        && !r.menu
        && selected.is_none()
        && r.reaction.is_none()
        && !r.pending_ack
        && !r.pending_tickle
        && !r.pending_menu
        && r.activity.is_none()
        && r.crossing.is_none()
        && !r.physics.as_ref().unwrap().busy();
    if r.petting.sample(
        (cursor.x, cursor.y),
        dt,
        crate::pet_size(&cfg) * sf / screen_unit(sf),
        can_pet,
    ) {
        r.set_mode("petted");
        r.reaction = Some("헤헤, 좋아요.");
    }
    r.crossing_wait = (r.crossing_wait - dt).max(0.);
    let can_cross = resident
        && cfg.activity_scope == "all"
        && !cfg.reduce_motion
        && selected.is_none()
        && !dragging
        && !r.menu
        && !r.pending_menu
        && !r.pending_tickle
        && !r.pending_ack
        && r.reaction.is_none();
    if r.crossing.as_ref().is_some_and(|c| {
        !can_cross
            || [&c.from, &c.to]
                .iter()
                .any(|id| !monitors.iter().any(|m| crate::surfaces::key(m) == **id))
    }) {
        r.crossing = None;
        let p = r.physics.as_mut().unwrap();
        p.reset(p.x, p.y);
    }
    if can_cross
        && !hit
        && !r.hover
        && r.crossing.is_none()
        && r.activity.is_none()
        && r.crossing_wait == 0.
        && matches!(r.mode.as_str(), "walk" | "run")
    {
        let p = r.physics.as_ref().unwrap();
        // Walking into the edge wobbles and turns around within the same tick,
        // so departures are read from the physics state, not only when grounded.
        if let Some(direction) = p.edge_departure() {
            let screens = monitors
                .iter()
                .map(|m| {
                    let u = screen_unit(m.scale_factor());
                    let wa = m.work_area();
                    crossing::Screen {
                        id: crate::surfaces::key(m),
                        x: m.position().x as f64 / u,
                        y: m.position().y as f64 / u,
                        w: m.size().width as f64 / u,
                        h: m.size().height as f64 / u,
                        wx: wa.position.x as f64 / u,
                        wy: wa.position.y as f64 / u,
                        ww: wa.size.width as f64 / u,
                        wh: wa.size.height as f64 / u,
                        factor: m.scale_factor() / u,
                    }
                })
                .collect::<Vec<_>>();
            if let Some(from) = screens.iter().find(|s| s.id == p.world.monitor) {
                r.crossing = crossing::Crossing::new(
                    from,
                    &screens,
                    (from.wx + p.x * from.factor, from.wy + p.y * from.factor),
                    direction,
                    p.world.size,
                    gait::speed("walk", p.world.size, cfg.speed) * from.factor,
                );
            }
            // No neighbour on this side: retry soon at the other edge instead of
            // waiting out the full post-crossing rest.
            r.crossing_wait = if r.crossing.is_some() { 35. } else { 3. };
        }
    }
    let mut cross_frame = None;
    if let Some(c) = r.crossing.as_mut() {
        let f = c.step(dt);
        if let Some(m) = monitors
            .iter()
            .find(|m| crate::surfaces::key(m) == f.screen)
        {
            // Route coordinates are authoritative. Reading back the popup and
            // constructing normal physics here could acquire neighboring windows.
            let world = crate::surfaces::world_at(
                app,
                m,
                f.x * screen_unit(m.scale_factor()),
                f.y * screen_unit(m.scale_factor()),
            )?;
            let (x, y, size) = (world.x, world.y, world.size);
            let p = r.physics.as_mut().unwrap();
            p.guide(world);
            p.direction = f.direction;
            monitor = m.clone();
            cross_frame = Some(activity::Frame {
                x,
                y,
                mode: f.mode,
                age: if f.walk {
                    gait::elapsed(
                        "walk",
                        f.distance / (m.scale_factor() / screen_unit(m.scale_factor())),
                        size,
                    )
                } else if f.mode == "travel-jump" {
                    0.4
                } else {
                    f.age
                },
                direction: f.direction,
                anchor: f.anchor.map(|(ax, ay)| {
                    let factor = m.scale_factor() / screen_unit(m.scale_factor());
                    let wa = m.work_area();
                    (
                        ax / factor - wa.position.x as f64 / m.scale_factor(),
                        ay / factor - wa.position.y as f64 / m.scale_factor(),
                    )
                }),
            });
            if f.done {
                p.reset(x, y);
                p.direction = f.direction;
                r.crossing = None;
                cross_frame = None;
            }
        }
    }
    let sf = monitor.scale_factor();
    let wa = monitor.work_area();
    if dragging {
        r.drag_age += dt * cfg.speed;
    }
    if dropped && !resident {
        crate::persist_overlay_position(app.clone())?;
    }
    if r.activity.is_none() {
        r.play_wait = (r.play_wait - dt).max(0.);
    }
    let can_play = resident
        && cross_frame.is_none()
        && r.crossing.is_none()
        && selected.is_none()
        && !dragging
        && !r.menu
        && !r.pending_menu
        && !r.pending_tickle
        && !r.pending_ack
        && r.reaction.is_none()
        && !cfg.reduce_motion;
    let activity_invalid = r
        .activity
        .as_ref()
        .is_some_and(|a| !a.valid(&r.physics.as_ref().unwrap().world));
    if !can_play || activity_invalid {
        if r.activity.take().is_some() {
            let p = r.physics.as_mut().unwrap();
            p.reset(p.x, p.y);
            r.play_wait = 25.;
        }
    }
    if can_play
        && r.activity.is_none()
        && r.play_wait == 0.
        && r.physics.as_ref().unwrap().on_floor()
        && !hit
    {
        let split = activity::layout(&r.physics.as_ref().unwrap().world)
            .flatten()
            .is_some();
        let choices: Vec<_> = [
            activity::Kind::Tour,
            activity::Kind::Peek,
            activity::Kind::Divider,
        ]
        .into_iter()
        .filter(|k| Some(*k) != r.last_play && (*k != activity::Kind::Divider || split))
        .collect();
        let kind = choices[(r.random() * choices.len() as f64) as usize % choices.len()];
        let ceiling = (monitor.position().y as f64 - wa.position.y as f64) / sf;
        if let Some(mut play) =
            activity::Activity::new(&r.physics.as_ref().unwrap().world, kind, ceiling)
        {
            play.approach_from(r.play_side);
            r.play_side *= -1.;
            r.last_play = Some(kind);
            r.activity = Some(play);
        }
        r.play_wait = 60. + r.random() * 60.;
    }
    let local_cursor = cfg.follow_cursor.then_some((
        cursor.x * screen_unit(sf) / sf - wa.position.x as f64 / sf,
        cursor.y * screen_unit(sf) / sf - wa.position.y as f64 / sf,
    ));
    let activity_frame = r
        .activity
        .as_mut()
        .and_then(|a| a.step(dt, cfg.speed, local_cursor))
        .or(cross_frame);
    if let Some(f) = activity_frame {
        let p = r.physics.as_mut().unwrap();
        p.x = f.x;
        p.y = f.y;
    } else if r.activity.take().is_some() {
        let p = r.physics.as_mut().unwrap();
        p.reset(p.x, p.y);
        r.set_mode("relieved");
        r.play_wait = 60. + r.random() * 60.;
    }
    let mut landed = false;
    let mut cursor_following = false;
    let mut follow_edge = false;
    if !dragging {
        let reduced = cfg.reduce_motion;
        let mut walk = gait::speed(&r.mode, crate::pet_size(&cfg), cfg.speed);
        let attention = resident
            && selected.is_none()
            && !left
            && !r.hover
            && !r.menu
            && !r.pending_menu
            && !r.pending_tickle
            && !r.pending_ack
            && r.reaction.is_none();
        let auto = attention && !hit;
        // Follow only on the current support; stopping distance leaves the cursor clickable.
        r.follow_rest = (r.follow_rest - dt).max(0.);
        let cursor_local = (
            cursor.x * screen_unit(sf) / sf - wa.position.x as f64 / sf,
            cursor.y * screen_unit(sf) / sf - wa.position.y as f64 / sf,
        );
        let p = r.physics.as_ref().unwrap();
        let dx = cursor_local.0 - p.x;
        let dy = p.y - cursor_local.1;
        let near = dx.abs() < p.world.size * 2. && dy >= -10. && dy < p.height() * 1.5;
        let following = attention
            && cfg.follow_cursor
            && !reduced
            && !p.busy()
            && r.activity.is_none()
            && r.crossing.is_none()
            && activity_frame.is_none()
            && near
            && r.follow_rest == 0.;
        cursor_following = following;
        let stop_distance = p.world.size * if r.mode == "run" { 0.55 } else { 0.72 };
        if following && r.follow_rest == 0. && dx.abs() > stop_distance {
            r.follow_age += dt;
            if r.mode != "run" {
                r.set_mode("run");
            }
            walk = gait::speed("run", crate::pet_size(&cfg), cfg.speed)
                .min((dx.abs() - crate::pet_size(&cfg) * 0.55) / dt.max(0.001));
            r.physics.as_mut().unwrap().direction = dx.signum();
            if r.follow_age > 5. {
                r.follow_rest = 3.;
                r.follow_age = 0.;
            }
        } else {
            r.follow_age = 0.;
            if following {
                walk = 0.;
                if dx.abs() > 3. {
                    r.physics.as_mut().unwrap().direction = dx.signum();
                }
                if r.mode != "curious" {
                    r.set_mode("curious");
                }
            }
        }
        let p = r.physics.as_mut().unwrap();
        if resident && activity_frame.is_none() {
            let before = p.x;
            let previous_motion = p.motion;
            let grounded = p.motion == Motion::Grounded;
            p.step(dt, walk, auto && !following, reduced);
            if following && p.motion == Motion::Grounded {
                follow_edge = p.follow_step(dt, walk);
            }
            landed = matches!(previous_motion, Motion::Land | Motion::Hurt)
                && p.motion == Motion::Grounded;
            if grounded && (auto || following) && !reduced {
                let distance = (p.x - before).abs();
                r.gait_distance += distance;
            }
        } else if dropped {
            p.motion = Motion::Grounded;
        }
    }
    if follow_edge {
        // Give the edge animation and retreat time to finish before the pointer
        // can reverse our direction again at the same boundary.
        r.follow_rest = 3.;
        r.follow_age = 0.;
        r.set_mode("walk");
    }
    if landed && selected.is_none() && r.reaction.is_none() {
        r.set_mode("relieved");
    }
    if std::mem::take(&mut r.physics.as_mut().unwrap().climbed)
        && selected.is_none()
        && r.reaction.is_none()
        && !dragging
    {
        r.set_mode("proud");
    }
    // Reactions to what the person does around us: dragging our window, brushing
    // past with another one, closing the one under our feet, leaving and returning.
    r.nudge_wait = (r.nudge_wait - dt).max(0.);
    let calm = resident
        && !r.physics.as_ref().unwrap().busy()
        && !dragging
        && !r.menu
        && selected.is_none()
        && r.reaction.is_none()
        && activity_frame.is_none()
        && r.crossing.is_none()
        && !cursor_following;
    if landed && std::mem::take(&mut r.physics.as_mut().unwrap().lost_support) {
        r.pending_sulk = true;
    }
    if calm && r.pending_sulk && r.nudge_wait == 0. {
        r.pending_sulk = false;
        r.set_mode("sulking");
        if notice_allowed {
            r.reaction = Some("앗, 발판이 없어졌어요…");
        }
        r.nudge_wait = 20.;
    } else if calm && !r.presence.asleep() && r.nudge_wait == 0. {
        match nudge {
            Nudge::Ride => {
                r.set_mode("surprised");
                r.nudge_wait = 15.;
            }
            Nudge::Brush => {
                r.set_mode("curious");
                r.nudge_wait = 10.;
            }
            _ => {}
        }
    }
    match presence_event {
        Some(presence::Event::Doze) if calm => r.set_mode("asleep"),
        Some(presence::Event::Return(away)) => r.pending_return = Some((away, 20.)),
        _ => {}
    }
    // The welcome waits for a calm moment (an alert may be up when the person
    // returns) but not forever: a reunion is only convincing right after it.
    if let Some((away, left_s)) = r.pending_return {
        if calm {
            r.pending_return = None;
            r.set_mode(if away >= presence::LONG_AWAY {
                "excited"
            } else {
                "greeting"
            });
            if notice_allowed {
                r.reaction = Some(presence::welcome(away));
            }
        } else if left_s <= dt || r.presence.asleep() {
            r.pending_return = None;
            r.pending_sulk = false;
        } else {
            r.pending_return = Some((away, left_s - dt));
        }
    }
    if (!resident || !r.physics.as_ref().unwrap().busy()) && !dragging && activity_frame.is_none() {
        if r.pending_ack {
            r.pending_ack = false;
            r.set_mode("ack");
            r.reaction = Some("알겠어요! 시간이 되면 알려드릴게요.");
        }
        if r.pending_menu {
            r.menu = true;
            r.pending_menu = false;
        }
        if r.pending_tickle {
            r.pending_tickle = false;
            r.menu = false;
            r.tickle();
        }
    }
    if notice_allowed
        && selected.is_none()
        && r.reaction.is_none()
        && !r.menu
        && !dragging
        && !left
        && !right
        && activity_frame.is_none()
        && (!resident || !r.physics.as_ref().unwrap().busy())
    {
        if let Some(message) = r.connections.take(notice_time) {
            r.set_mode("connection");
            r.reaction = Some(message);
        }
    }
    let busy = resident && (r.physics.as_ref().unwrap().busy() || activity_frame.is_some());
    if !busy && !dragging && !r.menu {
        if r.reaction.is_some() {
            r.age += dt
                * if matches!(
                    r.mode.as_str(),
                    "ack" | "enough" | "tickle" | "petted" | "connection"
                ) {
                    1.
                } else {
                    cfg.speed
                };
            if r.age >= art::duration(&r.mode) && !(held && r.mode == "tickle") {
                let wake = r.mode == "surprised";

                r.reaction = None;
                r.set_mode(if wake { "curious" } else { "relieved" });
            }
        } else if selected.is_none() && !cfg.reduce_motion && !cursor_following {
            r.age += dt * cfg.speed;
            let asleep = r.presence.asleep();
            if asleep {
                // Nobody is here (a parked pointer over us is not a person): stay
                // in bed under the blanket instead of picking new behaviours.
                if r.mode != "asleep" && (r.age >= art::duration(&r.mode) || r.mode == "walk") {
                    r.set_mode("asleep");
                }
            } else if r.physics.as_ref().unwrap().approaching() {
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
    let mode = if let Some(f) = activity_frame {
        f.mode.to_string()
    } else if dragging {
        if r.press.as_ref().is_some_and(|p| p.dizzy > 0.) {
            "dizzy".to_string()
        } else {
            "drag".to_string()
        }
    } else if busy {
        physics_mode.pose().into()
    } else if r.menu {
        "idle".into()
    } else if reaction.is_some() {
        r.mode.clone()
    } else if let Some(a) = selected {
        a["source"].as_str().unwrap_or("slack").into()
    } else if cursor_following && r.mode == "run" {
        "chase".into()
    } else {
        r.mode.clone()
    };
    let age = if let Some(f) = activity_frame {
        f.age
    } else if mode == "chase" {
        gait::elapsed("run", r.gait_distance, crate::pet_size(&cfg))
    } else if matches!(mode.as_str(), "walk" | "run") {
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
    let facing = if let Some(f) = activity_frame {
        f.direction
    } else if dragging {
        // The antenna grip is measured on the unmirrored drag frames.
        1.
    } else if busy || matches!(mode.as_str(), "walk" | "run" | "chase" | "curious") {
        direction
    } else {
        1.
    };
    let pose = r.art.pose(&mode, age, cfg.reduce_motion);
    let key = r.art.frame(&pose).key.clone();
    let hit_rects = r.art.frame(&pose).hit.clone();
    let size = crate::pet_size(&cfg);
    let u = screen_unit(sf);
    // Foot and rope anchor in global screen units (points on macOS, pixels on Windows).
    let gx = (wa.position.x as f64 + x * sf) / u;
    let gy = (wa.position.y as f64 + y * sf) / u;
    let grope = if cfg.reduce_motion || dragging {
        None
    } else {
        activity_frame
            .and_then(|f| f.anchor)
            .or(rope_anchor)
            .map(|(ax, ay)| {
                (
                    (wa.position.x as f64 + ax * sf) / u,
                    (wa.position.y as f64 + ay * sf) / u,
                )
            })
    };
    let spec = FrameSpec {
        key: &key,
        pose: &pose,
        facing,
        size,
        age,
        mode: &mode,
        activity: r.activity.is_some(),
        swing: r
            .press
            .as_ref()
            .filter(|p| p.dragged && p.angle.abs() > 0.002)
            .map(|p| (p.angle, (cursor.x, cursor.y))),
    };
    // On macOS a canvas window spanning two displays of different backing scale is
    // re-backed on every frame and visibly blinks, so each window stays within one
    // display: the part beyond this one is painted by the overflow window instead.
    let split = cfg!(target_os = "macos");
    let geo = geometry(&monitor, gx, gy, grope, &spec, split || spec.activity);
    if geo.width == 0 || geo.height == 0 || geo.width > 8192 || geo.height > 16384 {
        return Err("Native overlay bounds invalid".into());
    }
    let (k, fx, fy) = (geo.k, geo.fx, geo.fy);
    // Geometry and new pixels are submitted together. Tauri's macOS geometry
    // setters enqueue work, which otherwise lets a new rope frame appear at the
    // preceding canvas origin/size for one presentation.
    let position = (geo.left.round() as i32, geo.top.round() as i32);
    if geo.stamp != r.last_picture || !r.visible {
        let t_paint = Instant::now();
        let canvas = paint(&mut r.art, &geo, &spec, &monitor)?;
        let d_paint = t_paint.elapsed();
        let t_present = Instant::now();
        platform::present(&pet, &canvas, (geo.left, geo.top), sf)?;
        let d_present = t_present.elapsed();
        r.last_picture = geo.stamp.clone();
        r.render_count += 1;
        trace(|| {
            format!(
                "present tick={:.1}ms bitmap=0.0ms compose={:.1}ms present={:.1}ms mon={} sf={sf} mode={mode} age={age:.2} facing={facing} x={x:.1} y={y:.1} left={:.0} top={:.0} w={} h={} rope={:?} activity={} crossing={} key={key}",
                tick_started.elapsed().as_secs_f64() * 1000.,
                d_paint.as_secs_f64() * 1000.,
                d_present.as_secs_f64() * 1000.,
                crate::surfaces::key(&monitor),
                geo.left,
                geo.top,
                geo.width,
                geo.height,
                geo.rope.map(|a| (a.0.round(), a.1.round())),
                r.activity.is_some(),
                r.crossing.is_some(),
            )
        });
    } else if r.position != Some(position) {
        platform::move_to(&pet, (geo.left, geo.top), sf)?;
        trace(|| format!("move left={:.0} top={:.0}", geo.left, geo.top));
    }
    r.position = Some(position);
    // Overflow onto one neighbouring display. With the primary-only activity scope
    // the character never shows on other displays, not even a peeking head.
    let mut overflow = None;
    if split && !(resident && cfg.activity_scope == "primary") {
        let full = geometry(&monitor, gx, gy, grope, &spec, false);
        let (gl, gt) = (full.left / u, full.top / u);
        let (gr, gb) = (
            (full.left + full.width as f64) / u,
            (full.top + full.height as f64) / u,
        );
        for n in monitors.iter() {
            if crate::surfaces::key(n) == crate::surfaces::key(&monitor) {
                continue;
            }
            let un = screen_unit(n.scale_factor());
            let (np, ns) = (n.position(), n.size());
            let (nl, nt) = (np.x as f64 / un, np.y as f64 / un);
            let (nr, nb) = (
                (np.x as f64 + ns.width as f64) / un,
                (np.y as f64 + ns.height as f64) / un,
            );
            if gl < nr && gr > nl && gt < nb && gb > nt {
                let g = geometry(n, gx, gy, grope, &spec, true);
                if g.width > 0 && g.height > 0 && g.width <= 8192 && g.height <= 16384 {
                    overflow = Some((n.clone(), g));
                    break;
                }
            }
        }
    }
    if let Some(pet2) = app.get_window("pet-native-2") {
        if let Some((n, g)) = overflow {
            let sfn = n.scale_factor();
            let position = (g.left.round() as i32, g.top.round() as i32);
            if g.stamp != r.overflow_picture || !r.overflow_visible {
                let canvas = paint(&mut r.art, &g, &spec, &n)?;
                platform::present(&pet2, &canvas, (g.left, g.top), sfn)?;
                r.overflow_picture = g.stamp;
                trace(|| {
                    format!(
                        "overflow mon={} sf={sfn} left={:.0} top={:.0} w={} h={}",
                        crate::surfaces::key(&n),
                        g.left,
                        g.top,
                        g.width,
                        g.height
                    )
                });
            } else if r.overflow_position != Some(position) {
                platform::move_to(&pet2, (g.left, g.top), sfn)?;
            }
            r.overflow_position = Some(position);
            if r.overflow_on_top != Some(cfg.always_on_top) {
                platform::on_top(&pet2, cfg.always_on_top)?;
                r.overflow_on_top = Some(cfg.always_on_top);
            }
            if !r.overflow_visible {
                pet2.show().map_err(|e| e.to_string())?;
                r.overflow_visible = true;
            }
        } else if r.overflow_visible {
            let _ = pet2.hide();
            r.overflow_visible = false;
        }
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
    if r.activity.is_some() {
        let u = screen_unit(sf);
        let ax = wa.position.x as f64 / u;
        let ay = wa.position.y as f64 / u;
        r.hits = r
            .hits
            .iter()
            .filter_map(|h| {
                let x = h[0].max(ax);
                let y = h[1].max(ay);
                let right = (h[0] + h[2]).min(ax + wa.size.width as f64 / u);
                let bottom = (h[1] + h[3]).min(ay + wa.size.height as f64 / u);
                (right > x && bottom > y).then_some([x, y, right - x, bottom - y])
            })
            .collect();
    }
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
        trace(|| "show".to_string());
        pet.show().map_err(|e| e.to_string())?;
    }
    r.visible = true;
    if r.last_error.take().is_some() {
        let _ = app.emit("native-pet-error", "");
    }
    Ok(())
}

/// Per-frame drawing inputs shared by the main and overflow canvases.
struct FrameSpec<'a> {
    key: &'a str,
    pose: &'a art::Pose,
    facing: f64,
    size: f64,
    age: f64,
    mode: &'a str,
    activity: bool,
    /// Rotation while dangling: (angle in radians, pivot in global screen units).
    swing: Option<(f64, (f64, f64))>,
}
/// Canvas placement on one display, in that display's physical pixels.
struct Geo {
    k: f64,
    fx: f64,
    fy: f64,
    pet_left: f64,
    pet_top: f64,
    rope: Option<(f64, f64)>,
    left: f64,
    top: f64,
    width: u32,
    height: u32,
    stamp: String,
    /// (degrees, pivot x, pivot y) relative to the canvas origin.
    rotation: Option<(f32, f32, f32)>,
}
fn geometry(
    m: &tauri::Monitor,
    gx: f64,
    gy: f64,
    grope: Option<(f64, f64)>,
    f: &FrameSpec,
    crop: bool,
) -> Geo {
    let sf = m.scale_factor();
    let u = screen_unit(sf);
    let k = (f.size * sf).ceil() / 260.;
    let (fx, fy) = (gx * u, gy * u);
    let pet_left = fx - 200. * k + f.facing * f.pose.offset * k;
    let pet_top = fy - (250. + f.pose.lift) * k;
    let rope = grope.map(|(ax, ay)| (ax * u, ay * u));
    let left = pet_left
        .min(rope.map_or(pet_left, |a| a.0 - 15. * sf))
        .floor()
        - 2.;
    let top = pet_top
        .min(rope.map_or(pet_top, |a| a.1 - 12. * sf))
        .floor()
        - 2.;
    let mut width = (pet_left + 400. * k - left + 4.).ceil();
    let mut height = (pet_top + 260. * k - top + 4.).ceil();
    let (mut left, mut top) = (left, top);
    let mut rotation = None;
    if let Some((angle, pivot)) = f.swing {
        // A square around the pivot (the pointer) that holds the frame at every
        // angle, so a fast swing never changes the canvas size between frames:
        // on Windows a per-frame resize lagged the pixels and clipped the body.
        let (px, py) = (pivot.0 * u, pivot.1 * u);
        let reach = [
            (pet_left, pet_top),
            (pet_left + 400. * k, pet_top),
            (pet_left, pet_top + 260. * k),
            (pet_left + 400. * k, pet_top + 260. * k),
        ]
        .iter()
        .map(|(cx, cy)| (cx - px).hypot(cy - py))
        .fold(0., f64::max)
        .ceil()
            + 2.;
        left = (px - reach).floor();
        top = (py - reach).floor();
        width = (reach * 2. + 2.).ceil();
        height = width;
        rotation = Some((angle.to_degrees() as f32, px, py));
    }
    let (left, top, width, height) = if crop {
        let (mp, ms) = (m.position(), m.size());
        crop_to_display(
            (left, top, width, height),
            (mp.x as f64, mp.y as f64, ms.width as f64, ms.height as f64),
        )
    } else {
        (left, top, width, height)
    };
    let rotation = rotation.map(|(deg, px, py)| (deg, (px - left) as f32, (py - top) as f32));
    let stamp = format!(
        "{}:{:.2}:{:.2}:{:.2}:{:.2}:{:.2}:{width}:{height}:{}:{:?}:{:.2}:{:?}",
        f.key,
        k,
        f.pose.offset,
        f.pose.lift,
        pet_top - top,
        pet_left - left,
        f.facing,
        rope.map(|a| (a.0 - left, a.1 - top)),
        if rope.is_some() { f.age } else { 0. },
        rotation.map(|(d, x, y)| ((d * 100.).round(), x.round(), y.round()))
    );
    Geo {
        k,
        fx,
        fy,
        pet_left,
        pet_top,
        rope,
        left,
        top,
        width: width as u32,
        height: height as u32,
        stamp,
        rotation,
    }
}
fn paint(art: &mut art::Art, g: &Geo, f: &FrameSpec, m: &tauri::Monitor) -> Result<Pixmap, String> {
    let frame = art.bitmap(f.key, (260. * g.k).ceil() as u32)?;
    let mut canvas = Pixmap::new(g.width, g.height).ok_or("Native canvas allocation failed")?;
    if let Some(anchor) = g.rope {
        let hands = art
            .hands(f.pose)
            .into_iter()
            .map(|h| {
                (
                    (g.fx + f.facing * (h[0] - 200. + f.pose.offset) * g.k - g.left) as f32,
                    (g.fy + (h[1] - 250. - f.pose.lift) * g.k - g.top) as f32,
                )
            })
            .collect::<Vec<_>>();
        art::rope(
            &mut canvas,
            ((anchor.0 - g.left) as f32, (anchor.1 - g.top) as f32),
            &hands,
            f.age,
            f.mode,
            g.k as f32,
            f.facing as f32,
        );
    }
    art::composite_rotated(
        &mut canvas,
        &frame,
        (g.pet_left - g.left) as f32,
        (g.pet_top - g.top) as f32,
        f.facing < 0.,
        g.rotation,
    );
    if f.activity {
        // Peek may intentionally cross this display edge; never paint on a forbidden neighbour.
        let (mp, ms) = (m.position(), m.size());
        clip_to_display(
            &mut canvas,
            (g.left, g.top),
            (mp.x as f64, mp.y as f64, ms.width as f64, ms.height as f64),
        );
    }
    Ok(canvas)
}

// Opt-in frame trace for field diagnosis: DDOKTTI_TRACE=1. Geometry only, never content.
fn trace_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("DDOKTTI_TRACE").is_some())
}
fn trace(line: impl FnOnce() -> String) {
    if trace_enabled() {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0., |d| d.as_secs_f64());
        eprintln!("[trace {t:.3}] {}", line());
    }
}

// Intersect a canvas rectangle with a display rectangle (physical pixels). The
// result keeps whole pixels and is empty (zero size) when they do not overlap.
fn crop_to_display(
    rect: (f64, f64, f64, f64),
    display: (f64, f64, f64, f64),
) -> (f64, f64, f64, f64) {
    let left = rect.0.max(display.0).floor();
    let top = rect.1.max(display.1).floor();
    let right = (rect.0 + rect.2).min(display.0 + display.2).ceil();
    let bottom = (rect.1 + rect.3).min(display.1 + display.3).ceil();
    (left, top, (right - left).max(0.), (bottom - top).max(0.))
}

// Zero every canvas pixel outside the display rectangle (physical pixels).
fn clip_to_display(canvas: &mut Pixmap, origin: (f64, f64), display: (f64, f64, f64, f64)) {
    let (width, height) = (canvas.width() as f64, canvas.height() as f64);
    let bound = |v: f64, limit: f64| v.max(0.).min(limit) as usize;
    let x0 = bound((display.0 - origin.0).ceil(), width);
    let x1 = bound((display.0 + display.2 - origin.0).floor(), width);
    let y0 = bound((display.1 - origin.1).ceil(), height);
    let y1 = bound((display.1 + display.3 - origin.1).floor(), height);
    let stride = canvas.width() as usize * 4;
    for (row, pixels) in canvas.data_mut().chunks_exact_mut(stride).enumerate() {
        if row < y0 || row >= y1 {
            pixels.fill(0);
        } else {
            pixels[..x0 * 4].fill(0);
            pixels[x1 * 4..].fill(0);
        }
    }
}

// Global screen positions use points on macOS and physical pixels on Windows.
fn screen_unit(scale: f64) -> f64 {
    if cfg!(target_os = "macos") {
        scale
    } else {
        1.
    }
}
#[cfg(test)]
mod clip_tests {
    use super::clip_to_display;
    use resvg::tiny_skia::Pixmap;
    fn filled(w: u32, h: u32) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        p.data_mut().fill(255);
        p
    }
    fn alive(p: &Pixmap, x: u32, y: u32) -> bool {
        p.pixel(x, y).unwrap().alpha() != 0
    }
    #[test]
    fn keeps_pixels_between_physical_screen_top_and_work_area_on_macos_layout() {
        // Display 0..200 x 0..100; menu bar occupies rows 0..30 above the work area.
        // Canvas top sits 10px above the screen so the rope hook has room.
        let mut c = filled(100, 80);
        clip_to_display(&mut c, (50., -10.), (0., 0., 200., 100.));
        assert!(!alive(&c, 10, 9), "above the screen is cleared");
        assert!(alive(&c, 10, 10), "screen top row survives");
        assert!(alive(&c, 10, 25), "menu bar strip (work area y<0) survives");
        assert!(alive(&c, 10, 79));
    }
    #[test]
    fn crop_keeps_canvas_inside_the_display_and_preserves_inner_rects() {
        use super::crop_to_display;
        // Peek window straddling the right edge of a 2x display (-3456..0, 654..4990).
        let d = (-3456., 654., 3456., 4336.);
        assert_eq!(
            crop_to_display((-334., 628., 627., 1153.), d),
            (-334., 654., 334., 1127.)
        );
        // Fully inside: unchanged.
        assert_eq!(
            crop_to_display((-1000., 700., 627., 410.), d),
            (-1000., 700., 627., 410.)
        );
        // Fully outside: empty.
        assert_eq!(crop_to_display((10., 700., 100., 100.), d).2, 0.);
    }
    #[test]
    fn clears_only_the_neighbouring_display_side() {
        let mut c = filled(100, 40);
        clip_to_display(&mut c, (150., 20.), (0., 0., 200., 100.));
        assert!(alive(&c, 49, 0));
        assert!(!alive(&c, 50, 0), "right of the display is cleared");
        assert!(!alive(&c, 99, 39));
        let mut c = filled(100, 40);
        clip_to_display(&mut c, (-30., 20.), (0., 0., 200., 100.));
        assert!(!alive(&c, 29, 0));
        assert!(alive(&c, 30, 0));
    }
}
