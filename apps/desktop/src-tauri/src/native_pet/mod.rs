//! Native pet window + Rust scheduler. The `overlay` WebView is only a popup UI.
mod activity;
mod art;
mod crossing;
mod gait;
mod physics;
mod platform;
#[cfg(debug_assertions)]
pub mod smoke;
mod tickle;
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
    activity: Option<activity::Activity>,
    crossing: Option<crossing::Crossing>,
    crossing_wait: f64,
    play_wait: f64,
    last_play: Option<activity::Kind>,
    play_side: f64,
    follow_age: f64,
    tickle: tickle::Tickle,
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
            activity: None,
            crossing: None,
            crossing_wait: 20.,
            play_wait: 18.,
            last_play: None,
            play_side: 1.,
            follow_age: 0.,
            tickle: tickle::Tickle::default(),
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
    let mut r = state.runtime.lock().map_err(|_| "Native state poisoned")?;
    if Instant::now() < r.retry_at {
        return Ok(());
    }
    let dt = r.last.elapsed().as_secs_f64().min(0.1);
    r.last = Instant::now();
    r.tick_count += 1;
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
            || r.press.is_some()
            || r.pending_ack
            || r.reaction.is_some());
    let pet = app.get_window("pet-native").ok_or("No native window")?;
    let popup = app.get_webview_window("overlay").ok_or("No popup window")?;
    if !visible {
        if r.visible {
            let _ = pet.hide();
            let _ = popup.hide();
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
    let p = r.physics.as_ref().unwrap();
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
                            + (crate::pet_half(&cfg) * m.scale_factor())
                                .min(work.size.width as f64 / 2.)
                                / unit,
                        (work.position.x as f64 + work.size.width as f64) / unit
                            - (crate::pet_half(&cfg) * m.scale_factor())
                                .min(work.size.width as f64 / 2.)
                                / unit,
                    )
                } else {
                    x
                };
                let y = if resident && cfg.activity_scope == "primary" {
                    y.clamp(
                        work.position.y as f64 / unit
                            + (crate::pet_height(&cfg) * m.scale_factor())
                                .min(work.size.height as f64)
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
        } else {
            dropped = press.dragged;
            if !dropped {
                if r.physics.as_ref().unwrap().busy() {
                    r.pending_tickle = true;
                } else {
                    let sleepy = r.mode == "sleepy" && selected.is_none();
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
    r.crossing_wait = (r.crossing_wait - dt).max(0.);
    let can_cross = resident
        && !hit
        && !r.hover
        && cfg.activity_scope == "all"
        && !cfg.reduce_motion
        && selected.is_none()
        && !dragging
        && !r.menu
        && !r.pending_menu
        && !r.pending_tickle
        && !r.pending_ack
        && r.reaction.is_none();
    if r.crossing
        .as_ref()
        .is_some_and(|c| !can_cross || !monitors.iter().any(|m| crate::surfaces::key(m) == c.to))
    {
        r.crossing = None;
        let p = r.physics.as_mut().unwrap();
        p.reset(p.x, p.y);
    }
    if can_cross
        && r.crossing.is_none()
        && r.activity.is_none()
        && r.crossing_wait == 0.
        && matches!(r.mode.as_str(), "walk" | "run")
    {
        let p = r.physics.as_ref().unwrap();
        let at_floor = (p.y - p.world.height).abs() < 2.;
        let at_edge = if p.direction > 0. {
            p.x >= p.world.width - p.half() - 2.
        } else {
            p.x <= p.half() + 2.
        };
        if at_floor && at_edge {
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
                    p.direction,
                    p.world.size,
                    gait::speed("walk", p.world.size, cfg.speed) * from.factor,
                );
            }
            r.crossing_wait = 35.;
        }
    }
    let mut cross_frame = None;
    if let Some(c) = r.crossing.as_mut() {
        let f = c.step(dt);
        if let Some(m) = monitors
            .iter()
            .find(|m| crate::surfaces::key(m) == f.screen)
        {
            crate::place_pet(
                app,
                &popup,
                m,
                f.x * screen_unit(m.scale_factor()),
                f.y * screen_unit(m.scale_factor()),
                &cfg,
            )
            .map_err(|e| e.to_string())?;
            let world = crate::surfaces::pet_world(app.clone())?;
            let (x, y, size) = (world.x, world.y, world.size);
            r.physics = Some(Physics::new(world));
            let p = r.physics.as_mut().unwrap();
            p.x = x;
            p.y = y;
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
    let busy = resident && (r.physics.as_ref().unwrap().busy() || activity_frame.is_some());
    if !busy && !dragging && !r.menu {
        if r.reaction.is_some() {
            r.age += dt
                * if matches!(r.mode.as_str(), "ack" | "enough" | "tickle") {
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
    let mode = if let Some(f) = activity_frame {
        f.mode.to_string()
    } else if dragging {
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
    let age = if let Some(f) = activity_frame {
        f.age
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
    } else if busy || matches!(mode.as_str(), "walk" | "run" | "curious") {
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
        activity_frame
            .and_then(|f| f.anchor)
            .or(rope_anchor)
            .map(|(x, y)| (wa.position.x as f64 + x * sf, wa.position.y as f64 + y * sf))
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
    // Geometry and new pixels are submitted together. Tauri's macOS geometry
    // setters enqueue work, which otherwise lets a new rope frame appear at the
    // preceding canvas origin/size for one presentation.
    let position = (left.round() as i32, top.round() as i32);
    if stamp != r.last_picture || !r.visible {
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
        if r.activity.is_some() {
            // Peek may intentionally cross this display edge; never paint on a forbidden neighbor.
            let x0 = (wa.position.x as f64 - left)
                .ceil()
                .max(0.)
                .min(width as f64) as usize;
            let x1 = (wa.position.x as f64 + wa.size.width as f64 - left)
                .floor()
                .max(0.)
                .min(width as f64) as usize;
            let y0 = (wa.position.y as f64 - top)
                .ceil()
                .max(0.)
                .min(height as f64) as usize;
            let y1 = (wa.position.y as f64 + wa.size.height as f64 - top)
                .floor()
                .max(0.)
                .min(height as f64) as usize;
            for (row, pixels) in canvas
                .data_mut()
                .chunks_exact_mut(width as usize * 4)
                .enumerate()
            {
                if row < y0 || row >= y1 {
                    pixels.fill(0);
                } else {
                    pixels[..x0 * 4].fill(0);
                    pixels[x1 * 4..].fill(0);
                }
            }
        }
        platform::present(&pet, &canvas, (left, top), sf)?;
        r.last_picture = stamp;
        r.render_count += 1;
    } else if r.position != Some(position) {
        platform::move_to(&pet, (left, top), sf)?;
    }
    r.position = Some(position);
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
