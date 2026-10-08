use super::physics::{climb_frame, descend_frame};
use resvg::{
    tiny_skia::{
        LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform,
    },
    usvg,
};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
#[derive(Deserialize)]
pub struct Frame {
    pub key: String,
    pub right: f64,
    pub hit: Vec<[f64; 4]>,
}
#[derive(Deserialize)]
pub struct Sheet {
    pub frames: Vec<Frame>,
}
pub const CACHE_BYTES: usize = 96 * 1024 * 1024;
pub struct Art {
    paths: HashMap<String, String>,
    pub sheets: HashMap<String, Sheet>,
    hands: HashMap<String, Vec<[f64; 2]>>,
    cache: VecDeque<(String, u32, Pixmap)>,
    tips: HashMap<String, (f64, f64)>,
}
pub struct Pose {
    pub sheet: &'static str,
    pub index: usize,
    pub lift: f64,
    pub offset: f64,
}
/// Distance from the foot anchor to the gripping hand in the first hang pose.
/// Keep the screen attachment tied to the reviewed hand landmarks, not SVG bounds.
pub fn hanging_reach() -> f64 {
    static REACH: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *REACH.get_or_init(|| {
        let hands: HashMap<String, Vec<[f64; 2]>> =
            serde_json::from_str(include_str!("../../../src/pet-climb-hands.json"))
                .expect("reviewed hand landmarks");
        250. - hands["13"]
            .iter()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min)
    })
}
/// Alert poses after the two-second introduction: keep showing the news by
/// alternating the row's two closing drawings (Slack: sign held up / pointing;
/// calendar, timer, stretch: the last two) at a per-row pace. Frame changes are
/// the whole motion: no vertical bob on top. Keep in sync with `alertLoop` in pet-behaviors.ts.
pub fn alert_loop(row: usize, t: f64) -> usize {
    let (a, b, period) = match row {
        0 => (3, 1, 1.0),
        1 => (3, 2, 0.9),
        2 => (3, 2, 0.8),
        _ => (3, 2, 1.3),
    };
    let loop_t = t - 2.;
    if ((loop_t / period).floor() as usize).is_multiple_of(2) { a } else { b }
}
/// Idle loop on the head-tilt sheet (front, tilt left, tilt right, blink):
/// front → left → front → right → front over 7.2 s, a 0.15 s blink while facing
/// front every 4.1 s. Keep in sync with `idlePose` in pet-behaviors.ts.
pub fn idle_pose(t: f64) -> usize {
    let cycle = t % 7.2;
    let frame = if (1.4..2.8).contains(&cycle) {
        1
    } else if (4.0..5.4).contains(&cycle) {
        2
    } else {
        0
    };
    let blink = t % 4.1;
    if frame == 0 && (3.55..3.7).contains(&blink) {
        3
    } else {
        frame
    }
}
/// Dozing loop shared by the end of `sleepy` and the fallback of `asleep`: nod
/// between the two eyes-closed drawings (offset within the row).
/// Keep in sync with `napPose` in pet-behaviors.ts.
pub fn nap_pose(nap: f64) -> usize {
    if ((nap / 1.4).floor() as usize).is_multiple_of(2) { 2 } else { 3 }
}
pub fn duration(mode: &str) -> f64 {
    match mode {
        "run" => 2.5,
        "bored" => 5.,
        "sleepy" => 9.,
        "jump" => 1.,
        "excited" => 2.4,
        "greeting" | "playful" | "sulking" | "cheering" => 2.6,
        "proud" => 2.8,
        "shy" => 3.5,
        "curious" => 4.,
        "surprised" => 1.8,
        "relieved" => 2.3,
        "tickle" => 2.2,
        "petted" => 2.4,
        "connection" => 4.,
        "enough" => 3.0,
        "ack" => 2.2,
        "asleep" => 60.,
        _ => 7.,
    }
}
impl Art {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            paths: serde_json::from_str(include_str!("../../../src/pet-vector-paths.json"))
                .map_err(|e| e.to_string())?,
            sheets: serde_json::from_str(include_str!("../../../src/pet-vector-frames.json"))
                .map_err(|e| e.to_string())?,
            hands: serde_json::from_str(include_str!("../../../src/pet-climb-hands.json"))
                .map_err(|e| e.to_string())?,
            cache: VecDeque::new(),
            tips: HashMap::new(),
        })
    }
    /// Topmost drawn point of a frame in 400×260 art units: the antenna ball top,
    /// which the pointer holds while dragging. Read from the reviewed path data.
    pub fn tip(&mut self, key: &str) -> (f64, f64) {
        if let Some(t) = self.tips.get(key) {
            return *t;
        }
        let body = self.paths.get(key).map(String::as_str).unwrap_or("");
        let mut best = (200., 50.);
        let mut found = false;
        for segment in body.split(" d=\"").skip(1) {
            let d = segment.split('"').next().unwrap_or("");
            let numbers: Vec<f64> = d
                .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .filter(|t| !t.is_empty())
                .filter_map(|t| t.parse().ok())
                .collect();
            for point in numbers.chunks_exact(2) {
                if !found || point[1] < best.1 {
                    best = (point[0], point[1]);
                    found = true;
                }
            }
        }
        self.tips.insert(key.into(), best);
        best
    }
    pub fn pose(&self, mode: &str, time: f64, reduced: bool) -> Pose {
        let t = if reduced { 0. } else { time.max(0.) };
        let mut p = Pose {
            sheet: "edge",
            index: 3,
            lift: 0.,
            offset: 0.,
        };
        let behavior = match mode {
            "bored" => Some(("behaviors-v2", 1, 1.1, false)),
            "sleepy" => Some(("behaviors-v2", 2, 1.7, false)),
            "jump" => Some(("behaviors-v2", 3, 0.25, false)),
            "excited" => Some(("emotions-v1", 0, 0.2, true)),
            "greeting" => Some(("emotions-v1", 1, 0.4, false)),
            "proud" => Some(("emotions-v1", 2, 0.6, false)),
            "shy" => Some(("emotions-v1", 3, 0.75, false)),
            "curious" => Some(("emotions-v1", 4, 0.9, false)),
            "surprised" => Some(("emotions-v2", 0, 0.4, false)),
            "playful" => Some(("emotions-v2", 1, 0.55, false)),
            "ack" => Some(("emotions-v1", 2, 0.4, false)),
            "sulking" => Some(("emotions-v2", 2, 0.55, false)),
            "cheering" => Some(("emotions-v2", 3, 0.55, false)),
            "relieved" => Some(("emotions-v2", 4, 0.45, false)),
            _ => None,
        };
        if mode == "run" || mode == "chase" {
            p.sheet = if mode == "chase" {
                "chase-v1"
            } else {
                "run-v3"
            };
            p.index = super::gait::frame("run", t, reduced);
            return p;
        }
        if let Some((sheet, row, ms, looped)) = behavior {
            let phase = if looped {
                t % (ms * 4.)
            } else {
                t.min(ms * 4. - 0.0001)
            } / (ms * 4.);
            p.sheet = sheet;
            p.index = row * 4
                + if reduced {
                    3
                } else {
                    (phase * 4.).floor() as usize
                };
            if !reduced && matches!(mode, "jump" | "excited") && phase > 0.25 && phase < 0.75 {
                p.lift = ((phase - 0.25) * std::f64::consts::TAU).sin()
                    * if mode == "excited" { 38. } else { 25. };
            }
            if mode == "sleepy" && !reduced && t >= ms * 4. {
                // After the yawn, keep dozing: nod between the two eyes-closed
                // drawings for as long as the nap lasts.
                p.index = row * 4 + nap_pose(t - ms * 4.);
            }
            return p;
        }
        match mode {
            "idle" | "connection" => {
                // Standing still: a slow head tilt left and right and a blink every
                // few seconds. The relaxed relieved drawing stands in until the tilt
                // artwork is part of the atlas.
                if self.sheets.contains_key("idle-v1") {
                    p.sheet = "idle-v1";
                    p.index = if reduced { 0 } else { idle_pose(t) };
                } else {
                    p.sheet = "emotions-v2";
                    p.index = if !reduced && idle_pose(t) == 3 { 17 } else { 18 };
                }
            }
            "walk" => {
                p.sheet = "walk";
                p.index = super::gait::frame(mode, t, reduced);
            }
            "tickle" | "drag" => {
                p.sheet = "interactions-v2";
                p.index = (t / if mode == "drag" { 0.23 } else { 0.16 }).floor() as usize % 4
                    + if mode == "drag" { 4 } else { 0 };
                if mode == "drag" {
                    p.lift = 12.;
                }
            }
            "dizzy" => {
                // Shaken while dangling from the antenna; falls back to the drag frames
                // until the dizzy artwork is part of the atlas.
                if self.sheets.contains_key("dizzy-v1") {
                    p.sheet = "dizzy-v1";
                    p.index = ((t / 0.3).floor() as usize) % 4;
                } else {
                    p.sheet = "interactions-v2";
                    p.index = 4 + ((t / 0.23).floor() as usize) % 4;
                }
                p.lift = 12.;
            }
            "asleep" => {
                // In bed under the blanket after a long absence; the dozing drawings
                // stand in until the sleep artwork is part of the atlas.
                if self.sheets.contains_key("sleep-v1") {
                    p.sheet = "sleep-v1";
                    p.index = if reduced { 0 } else { ((t / 1.3).floor() as usize) % 4 };
                } else {
                    p.sheet = "behaviors-v2";
                    p.index = if reduced { 10 } else { 8 + nap_pose(t) };
                }
            }
            "petted" => {
                // Blushing, eyes-closed loop; the older hands-on-hips drawings remain the
                // fallback until the petted artwork is part of the atlas.
                if self.sheets.contains_key("petted-v1") {
                    p.sheet = "petted-v1";
                    p.index = ((t / 0.45).floor() as usize) % 4;
                } else {
                    p.sheet = "emotions-v1";
                    p.index = if t < 0.25 || t >= 2.1 { 8 } else { 9 };
                }
                if !reduced {
                    p.offset = (t * std::f64::consts::TAU).sin() * 2.;
                }
            }
            "enough" => {
                p.sheet = "emotions-v2";
                p.index = 8 + ((t / 0.2).floor() as usize).min(2);
            }
            "hang" => {
                p.sheet = "surfaces-v2";
                p.index = if ((t / 0.6).floor() as usize) % 2 == 0 {
                    13
                } else {
                    15
                };
                p.lift = if p.index == 15 { 3.8 } else { 0. };
            }
            "peek" => {
                p.sheet = "surfaces-v2";
                p.index = 12;
            }
            "hurt" => {
                p.sheet = "hurt-v1";
                p.index = if t < 0.35 {
                    0
                } else if t < 1.15 {
                    1
                } else if t < 1.75 {
                    2
                } else if t < 2.3 {
                    3
                } else {
                    4
                };
            }
            "fall" | "land" => {
                p.sheet = "edge-v2";
                p.index = if mode == "fall" { 5 } else { 6 };
            }
            "wobble" => p.index = ((t / 0.25).floor() as usize).min(3),
            "grab" | "climb" | "pull" | "lower" | "descend" | "prepare" | "travel-jump" => {
                p.sheet = "surfaces-v2";
                p.index = match mode {
                    "grab" => 12 + ((t / 0.1).floor() as usize).min(3),
                    "climb" => climb_frame(t),
                    "descend" => descend_frame(t),
                    "pull" => 4 + ((t / 0.175).floor() as usize).min(3),
                    // Pull played backwards: swing from the ledge corner out onto the rope.
                    "lower" => 7 - ((t / 0.175).floor() as usize).min(3),
                    "prepare" => 8,
                    _ => {
                        if t < 0.15 {
                            9
                        } else {
                            10
                        }
                    }
                };
                if matches!(mode, "grab" | "climb" | "pull" | "lower" | "descend") {
                    p.offset = (119.6 - (self.sheets[p.sheet].frames[p.index].right - 200.))
                        * match mode {
                            "pull" => 1. - (t / 0.7).min(1.),
                            "lower" => (t / 0.7).min(1.),
                            _ => 1.,
                        };
                }
            }
            "slack" | "calendar" | "timer" | "stretch" | "preview" => {
                p.sheet = "alerts";
                let row = match mode {
                    "calendar" => 1,
                    "timer" => 2,
                    "stretch" => 3,
                    _ => 0,
                };
                p.index = row * 4
                    + if reduced {
                        3
                    } else if t >= 2. {
                        alert_loop(row, t)
                    } else {
                        ((t / 0.5).floor() as usize).min(3)
                    };
                if !reduced && t > 0.8 && t < 1.4 {
                    p.lift = ((t - 0.8) / 0.6 * std::f64::consts::PI).sin() * 18.;
                }
            }
            _ => {}
        }
        p
    }
    pub fn frame(&self, p: &Pose) -> &Frame {
        &self.sheets[p.sheet].frames[p.index]
    }
    pub fn bitmap(&mut self, key: &str, pixel_height: u32) -> Result<Pixmap, String> {
        if let Some(i) = self
            .cache
            .iter()
            .position(|(k, h, _)| k == key && *h == pixel_height)
        {
            let e = self.cache.remove(i).unwrap();
            let p = e.2.clone();
            self.cache.push_back(e);
            return Ok(p);
        }
        let tree = self.tree(key)?;
        let scale = pixel_height as f32 / 260.;
        let mut pix =
            Pixmap::new((400. * scale).ceil() as u32, pixel_height).ok_or("Invalid frame size")?;
        resvg::render(
            &tree,
            Transform::from_scale(scale, scale),
            &mut pix.as_mut(),
        );
        self.cache
            .push_back((key.into(), pixel_height, pix.clone()));
        while self.cache.len() > 16
            || (self.cache.len() > 1
                && self.cache.iter().map(|e| e.2.data().len()).sum::<usize>() > CACHE_BYTES)
        {
            self.cache.pop_front();
        }
        Ok(pix)
    }
    fn tree(&self, key: &str) -> Result<usvg::Tree, String> {
        let body = self.paths.get(key).ok_or("Missing native frame")?;
        let text = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"400\" height=\"260\">{body}</svg>"
        );
        usvg::Tree::from_str(&text, &usvg::Options::default()).map_err(|e| e.to_string())
    }
    /// Draws a frame turned by `rotation` = (degrees, pivot x, pivot y) in target
    /// pixels, clockwise on screen, straight from its paths: about 1 ms and sharp,
    /// where rotating the cached bitmap with bicubic filtering takes ~15 ms at
    /// Retina size. Placement matches `composite` of `bitmap(key, pixel_height)`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_rotated(
        &self,
        target: &mut Pixmap,
        key: &str,
        pixel_height: u32,
        x: f32,
        y: f32,
        flip: bool,
        rotation: (f32, f32, f32),
    ) -> Result<(), String> {
        let tree = self.tree(key)?;
        let scale = pixel_height as f32 / 260.;
        let width = (400. * scale).ceil();
        let ts = if flip {
            Transform::from_row(-scale, 0., 0., scale, x + width, y)
        } else {
            Transform::from_row(scale, 0., 0., scale, x, y)
        };
        let (degrees, px, py) = rotation;
        resvg::render(&tree, ts.post_rotate_at(degrees, px, py), &mut target.as_mut());
        Ok(())
    }
    pub fn hands(&self, p: &Pose) -> Vec<[f64; 2]> {
        self.hands
            .get(&p.index.to_string())
            .cloned()
            .unwrap_or_default()
    }
}
pub fn rope(
    pix: &mut Pixmap,
    anchor: (f32, f32),
    hands: &[(f32, f32)],
    phase: f64,
    mode: &str,
    scale: f32,
    direction: f32,
) {
    if hands.len() < 2 {
        return;
    }
    let retract = match mode {
        "pull" => ((phase - 0.3) / 0.4).clamp(0., 1.) as f32,
        // The rope pays out from the anchor while the body swings over the corner.
        "lower" => (1. - phase / 0.4).clamp(0., 1.) as f32,
        _ => 0.,
    };
    let opacity = (1. - retract)
        * if mode == "grab" {
            (phase / 0.15).min(1.) as f32
        } else {
            1.
        };
    if opacity <= 0. {
        return;
    }
    let a = anchor;
    let blend = |p: (f32, f32)| (p.0 + (a.0 - p.0) * retract, p.1 + (a.1 - p.1) * retract);
    let h = blend(hands[0]);
    let l = blend(hands[1]);
    let tail = (
        l.0 - direction * 8. * scale,
        l.1 + 26. * scale * (1. - retract),
    );
    let mut pb = PathBuilder::new();
    pb.move_to(a.0, a.1);
    pb.line_to(h.0, h.1);
    pb.quad_to(h.0, l.1, l.0, l.1);
    pb.quad_to(tail.0 + 8. * scale, tail.1 - 10. * scale, tail.0, tail.1);
    if let Some(path) = pb.finish() {
        for (width, colour) in [(5., (35, 48, 59)), (3., (221, 174, 91))] {
            let mut paint = Paint::default();
            paint.set_color_rgba8(colour.0, colour.1, colour.2, (255. * opacity) as u8);
            pix.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: width * scale,
                    line_cap: LineCap::Round,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
    }
    let mut hook = PathBuilder::new();
    hook.move_to(a.0 + direction * 10. * scale, a.1 - 3. * scale);
    hook.line_to(a.0, a.1 - 3. * scale);
    hook.quad_to(
        a.0 - direction * 5. * scale,
        a.1 - 3. * scale,
        a.0 - direction * 5. * scale,
        a.1 + 6. * scale,
    );
    if let Some(path) = hook.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(88, 103, 119, (255. * opacity) as u8);
        pix.stroke_path(
            &path,
            &paint,
            &Stroke {
                width: 4. * scale,
                line_cap: LineCap::Round,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}
/// Source-over of premultiplied RGBA at an integer offset, optionally mirrored.
fn blit(target: &mut Pixmap, pet: &Pixmap, x: i32, y: i32, flip: bool) {
    let (tw, th) = (target.width() as i32, target.height() as i32);
    let (pw, ph) = (pet.width() as i32, pet.height() as i32);
    let (x0, x1) = (x.max(0), (x + pw).min(tw));
    let (y0, y1) = (y.max(0), (y + ph).min(th));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let src = pet.data();
    let dst = target.data_mut();
    for ty in y0..y1 {
        let srow = ((ty - y) * pw) as usize * 4;
        let drow = (ty * tw) as usize * 4;
        for tx in x0..x1 {
            let sx = if flip { pw - 1 - (tx - x) } else { tx - x };
            let s = &src[srow + sx as usize * 4..][..4];
            let a = s[3];
            if a == 0 {
                continue;
            }
            let d = &mut dst[drow + tx as usize * 4..][..4];
            if a == 255 {
                d.copy_from_slice(s);
            } else {
                let inv = 255 - a as u32;
                for c in 0..4 {
                    d[c] = s[c] + ((d[c] as u32 * inv + 127) / 255) as u8;
                }
            }
        }
    }
}
/// Draws a cached frame bitmap at (x, y), mirrored when `flip`. Positions round
/// to whole pixels, as the former nearest-neighbour draw_pixmap did.
pub fn composite(target: &mut Pixmap, pet: &Pixmap, x: f32, y: f32, flip: bool) {
    blit(target, pet, x.round() as i32, y.round() as i32, flip);
}

#[cfg(test)]
mod tests {
    use super::*;
    use resvg::tiny_skia::{FilterQuality, PixmapPaint};
    #[test]
    fn antenna_tip_is_the_topmost_point_of_each_drag_frame() {
        let mut art = Art::new().unwrap();
        for index in 4..8 {
            let key = format!("interactions-v2-{index}");
            let (x, y) = art.tip(&key);
            assert!(
                (185. ..215.).contains(&x) && (40. ..60.).contains(&y),
                "{key}: {x},{y}"
            );
            assert_eq!(art.tip(&key), (x, y), "cached");
        }
    }
    #[test]
    fn every_original_frame_rasterizes_transparently_and_cache_is_bounded() {
        let mut art = Art::new().unwrap();
        let keys: Vec<_> = art.paths.keys().cloned().collect();
        assert!(keys.len() >= 133);
        for key in keys {
            let bitmap = art.bitmap(&key, 260).unwrap();
            assert!(
                bitmap.data().chunks_exact(4).any(|p| p[3] > 0),
                "empty {key}"
            );
            assert_eq!(&bitmap.data()[0..4], &[0, 0, 0, 0]);
            assert!(art.cache.len() <= 16);
        }
        let p = art.bitmap("walk-0", 260).unwrap();
        let before = art.cache.len();
        assert_eq!(art.bitmap("walk-0", 260).unwrap().data(), p.data());
        assert_eq!(art.cache.len(), before);
    }
    /// Per-frame rendering cost at 1x and 2x (Retina) pet sizes. Run with
    /// `cargo test --release --lib render_benchmark -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn render_benchmark() {
        use std::time::Instant;
        let avg = |runs: u32, mut f: Box<dyn FnMut() + '_>| {
            let t = Instant::now();
            for _ in 0..runs {
                f();
            }
            t.elapsed().as_secs_f64() * 1000. / runs as f64
        };
        for h in [230u32, 460] {
            let mut art = Art::new().unwrap();
            let keys: Vec<_> = art.paths.keys().cloned().collect();
            let mut worst = 0f64;
            let t = Instant::now();
            for k in &keys {
                let s = Instant::now();
                art.bitmap(k, h).unwrap();
                worst = worst.max(s.elapsed().as_secs_f64() * 1000.);
            }
            let raster = t.elapsed().as_secs_f64() * 1000. / keys.len() as f64;
            let key = keys.last().unwrap().clone();
            let hit = avg(500, Box::new(|| drop(art.bitmap(&key, h).unwrap())));
            let pet = art.bitmap(&key, h).unwrap();
            let mut canvas = Pixmap::new(pet.width() + 40, pet.height() + 40).unwrap();
            let blit_ms = avg(
                500,
                Box::new(|| composite(&mut canvas, &pet, 20.4, 20., true)),
            );
            let rotated = avg(
                100,
                Box::new(|| {
                    art.draw_rotated(&mut canvas, &key, h, 20., 20., false, (8., 200., 40.))
                        .unwrap()
                }),
            );
            eprintln!(
                "{}x{} rasterize avg={raster:.2}ms worst={worst:.2}ms cached={hit:.3}ms blit={blit_ms:.3}ms rotated={rotated:.3}ms",
                pet.width(),
                pet.height()
            );
        }
    }
    #[test]
    fn rotated_vector_frame_lands_where_the_rotated_bitmap_did() {
        let mut art = Art::new().unwrap();
        for flip in [false, true] {
            let pet = art.bitmap("interactions-v2-5", 460).unwrap();
            let rotation = (12f32, 330f32, 60f32);
            let mut old = Pixmap::new(900, 700).unwrap();
            let ts = if flip {
                Transform::from_row(-1., 0., 0., 1., 30. + pet.width() as f32, 40.)
            } else {
                Transform::from_translate(30., 40.)
            };
            let paint = PixmapPaint {
                quality: FilterQuality::Bicubic,
                ..PixmapPaint::default()
            };
            old.draw_pixmap(0, 0, pet.as_ref(), &paint, ts.post_rotate_at(12., 330., 60.), None);
            let mut new = Pixmap::new(900, 700).unwrap();
            art.draw_rotated(&mut new, "interactions-v2-5", 460, 30., 40., flip, rotation)
                .unwrap();
            // Same coverage, differing only along anti-aliased edges.
            let covered = |p: &Pixmap| p.data().chunks_exact(4).filter(|c| c[3] > 127).count();
            let (a, b) = (covered(&old), covered(&new));
            assert!(a.abs_diff(b) * 100 < a, "flip={flip}: {a} vs {b}");
            let mean = old
                .data()
                .iter()
                .zip(new.data())
                .map(|(x, y)| x.abs_diff(*y) as u64)
                .sum::<u64>() as f64
                / old.data().len() as f64;
            assert!(mean < 2., "flip={flip}: mean difference {mean}");
        }
    }
    #[test]
    fn blit_matches_draw_pixmap_over_rope_and_at_edges() {
        let mut art = Art::new().unwrap();
        let pet = art.bitmap("walk-3", 300).unwrap();
        for flip in [false, true] {
            for (x, y) in [(13., 7.), (-40., -25.), (200., 120.)] {
                let mut base = Pixmap::new(520, 360).unwrap();
                // A half-transparent stroke under the sprite, like the rope.
                rope(&mut base, (260., 0.), &[(250., 200.), (270., 200.)], 1., "climb", 1.2, 1.);
                assert!(base.data().chunks_exact(4).any(|p| p[3] > 0), "rope drawn");
                let mut fast = base.clone();
                blit(&mut fast, &pet, x as i32, y as i32, flip);
                let ts = if flip {
                    Transform::from_row(-1., 0., 0., 1., x + pet.width() as f32, y)
                } else {
                    Transform::from_translate(x, y)
                };
                base.draw_pixmap(0, 0, pet.as_ref(), &PixmapPaint::default(), ts, None);
                let worst = fast
                    .data()
                    .iter()
                    .zip(base.data())
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(worst <= 1, "flip={flip} at {x},{y}: off by {worst}");
            }
        }
    }
    #[test]
    fn mirror_and_hidpi_preserve_dimensions_and_alpha() {
        let mut art = Art::new().unwrap();
        let p = art.bitmap("walk-0", 260).unwrap();
        let hi = art.bitmap("walk-0", 520).unwrap();
        assert_eq!((hi.width(), hi.height()), (p.width() * 2, p.height() * 2));
        let mut out = Pixmap::new(400, 260).unwrap();
        composite(&mut out, &p, 0., 0., true);
        for y in 0..260 {
            for x in 0..400 {
                assert_eq!(out.pixel(x, y), p.pixel(399 - x, y));
            }
        }
    }
    #[test]
    fn alerts_introduce_then_keep_alternating_instead_of_freezing() {
        let art = Art::new().unwrap();
        assert_eq!(art.pose("slack", 0.2, false).index, 0);
        assert_eq!(art.pose("slack", 1.9, false).index, 3);
        let (a, b) = (art.pose("slack", 2.5, false), art.pose("slack", 3.5, false));
        assert_eq!((a.index, b.index), (3, 1));
        assert_eq!(a.lift, 0., "frame changes carry the motion; no bob on top");
        assert_eq!(art.pose("calendar", 60.5, false).index, 4 + 2);
        assert_eq!(art.pose("calendar", 61.4, false).index, 4 + 3);
        assert_eq!(art.pose("stretch", 2.0, false).index, 12 + 3);
        assert_eq!(art.pose("stretch", 3.4, false).index, 12 + 2);
        assert_eq!(art.pose("timer", 300., true).index, 8 + 3, "reduced motion holds the closing frame");
    }
    #[test]
    fn idle_breathes_tilts_and_blinks_while_dozing_keeps_moving() {
        assert_eq!([0.5, 2.0, 3.0, 4.5, 6.0, 3.6, 4.1 + 3.6].map(idle_pose), [0, 1, 0, 2, 0, 3, 3]);
        assert_eq!(idle_pose(7.2 + 2.0), 1, "tilts again next cycle");
        let art = Art::new().unwrap();
        let tilt = art.sheets.contains_key("idle-v1");
        let still = art.pose("idle", 0.3, true);
        assert_eq!((still.index, still.lift), (if tilt { 0 } else { 18 }, 0.));
        let open = art.pose("idle", 1.0, false);
        assert_eq!(open.index, if tilt { 0 } else { 18 });
        assert_eq!(open.lift, 0., "no vertical bob while the frames themselves move");
        assert_eq!(art.pose("idle", 2.0, false).index, if tilt { 1 } else { 18 });
        assert_eq!(art.pose("idle", 3.6, false).index, if tilt { 3 } else { 17 }, "blink uses the eyes-closed drawing");
        assert_eq!(art.pose("idle", 3.8, false).index, if tilt { 0 } else { 18 });
        // Sleepy: yawn frames first, then an endless nod between the two sleeping drawings.
        assert_eq!(art.pose("sleepy", 0.2, false).index, 8);
        let (a, b) = (art.pose("sleepy", 7.0, false), art.pose("sleepy", 8.5, false));
        assert_eq!((a.index, b.index), (10, 11));
        assert_eq!((a.lift, b.lift), (0., 0.));
        assert_eq!(art.pose("sleepy", 60.0, false).index, 10);
        assert_eq!(art.pose("sleepy", 60.0, true).index, 11, "reduced motion holds the last frame");
    }
    #[test]
    fn rope_has_alpha_retracts_and_never_changes_pet_art() {
        let mut pix = Pixmap::new(300, 500).unwrap();
        let hands = [(100., 400.), (80., 440.)];
        rope(&mut pix, (110., 20.), &hands, 0.4, "climb", 1., 1.);
        assert!(pix.data().chunks_exact(4).any(|p| p[3] > 0));
        assert_eq!(pix.pixel(10, 10).unwrap().alpha(), 0);
        let mut done = Pixmap::new(300, 500).unwrap();
        rope(&mut done, (110., 20.), &hands, 0.71, "pull", 1., 1.);
        assert!(done.data().iter().all(|n| *n == 0));
        let art = Art::new().unwrap();
        for mode in ["grab", "climb", "pull", "lower", "descend"] {
            for i in 0..8 {
                let p = art.pose(mode, i as f64 * 0.1, false);
                assert_eq!(art.hands(&p).len(), 2);
            }
        }
        // Lowering mirrors pulling: frames reversed, body offset growing, rope paying out.
        let pull = art.pose("pull", 0., false);
        let lower = art.pose("lower", 0.7, false);
        assert_eq!((pull.index, lower.index), (4, 4));
        assert_eq!(art.pose("lower", 0., false).index, 7);
        assert!((art.pose("lower", 0., false).offset).abs() < 1e-9);
        assert!((lower.offset - pull.offset).abs() < 1e-9);
        assert_eq!(art.pose("descend", 0., false).index, 3);
        let mut start = Pixmap::new(300, 500).unwrap();
        rope(&mut start, (110., 20.), &hands, 0., "lower", 1., 1.);
        assert!(start.data().iter().all(|n| *n == 0));
        let mut out = Pixmap::new(300, 500).unwrap();
        rope(&mut out, (110., 20.), &hands, 0.5, "lower", 1., 1.);
        assert!(out.data().chunks_exact(4).any(|p| p[3] > 0));
    }
}
