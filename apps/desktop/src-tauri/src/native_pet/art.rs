use super::physics::{climb_frame, descend_frame};
use resvg::{
    tiny_skia::{
        FilterQuality, LineCap, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
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
/// between the two eyes-closed drawings (offset within the row) and breathe slowly.
/// Keep in sync with `napPose` in pet-behaviors.ts.
pub fn nap_pose(nap: f64) -> (usize, f64) {
    let frame = if ((nap / 1.4).floor() as usize) % 2 == 0 { 2 } else { 3 };
    (frame, (nap * std::f64::consts::TAU / 3.6).sin() * 1.2)
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
                // drawings and breathe slowly for as long as the nap lasts.
                let (frame, lift) = nap_pose(t - ms * 4.);
                p.index = row * 4 + frame;
                p.lift = lift;
            }
            return p;
        }
        match mode {
            "idle" | "connection" => {
                // Standing still: gentle breathing, a slow head tilt left and right
                // and a blink every few seconds. The relaxed relieved drawing stands
                // in until the tilt artwork is part of the atlas.
                if self.sheets.contains_key("idle-v1") {
                    p.sheet = "idle-v1";
                    p.index = if reduced { 0 } else { idle_pose(t) };
                } else {
                    p.sheet = "emotions-v2";
                    p.index = if !reduced && idle_pose(t) == 3 { 17 } else { 18 };
                }
                if !reduced {
                    p.lift = (t * std::f64::consts::TAU / 3.4).sin() * 1.4;
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
                let (frame, lift) = nap_pose(t);
                if self.sheets.contains_key("sleep-v1") {
                    p.sheet = "sleep-v1";
                    p.index = if reduced { 0 } else { ((t / 1.3).floor() as usize) % 4 };
                } else {
                    p.sheet = "behaviors-v2";
                    p.index = if reduced { 10 } else { 8 + frame };
                }
                if !reduced {
                    p.lift = lift;
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
        let body = self.paths.get(key).ok_or("Missing native frame")?;
        let text = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"400\" height=\"260\">{body}</svg>"
        );
        let tree =
            usvg::Tree::from_str(&text, &usvg::Options::default()).map_err(|e| e.to_string())?;
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
/// `rotation` = (degrees, pivot x, pivot y) in target pixels; clockwise on screen.
pub fn composite_rotated(
    target: &mut Pixmap,
    pet: &Pixmap,
    x: f32,
    y: f32,
    flip: bool,
    rotation: Option<(f32, f32, f32)>,
) {
    let mut ts = if flip {
        Transform::from_row(-1., 0., 0., 1., x + pet.width() as f32, y)
    } else {
        Transform::from_translate(x, y)
    };
    let mut paint = PixmapPaint::default();
    if let Some((degrees, px, py)) = rotation {
        ts = ts.post_rotate_at(degrees, px, py);
        paint.quality = FilterQuality::Bicubic;
    }
    target.draw_pixmap(0, 0, pet.as_ref(), &paint, ts, None);
}

#[cfg(test)]
mod tests {
    use super::*;
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
    #[test]
    fn mirror_and_hidpi_preserve_dimensions_and_alpha() {
        let mut art = Art::new().unwrap();
        let p = art.bitmap("walk-0", 260).unwrap();
        let hi = art.bitmap("walk-0", 520).unwrap();
        assert_eq!((hi.width(), hi.height()), (p.width() * 2, p.height() * 2));
        let mut out = Pixmap::new(400, 260).unwrap();
        composite_rotated(&mut out, &p, 0., 0., true, None);
        for y in 0..260 {
            for x in 0..400 {
                assert_eq!(out.pixel(x, y), p.pixel(399 - x, y));
            }
        }
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
        assert!(open.lift.abs() <= 1.4 && open.lift != 0.);
        assert_eq!(art.pose("idle", 2.0, false).index, if tilt { 1 } else { 18 });
        assert_eq!(art.pose("idle", 3.6, false).index, if tilt { 3 } else { 17 }, "blink uses the eyes-closed drawing");
        assert_eq!(art.pose("idle", 3.8, false).index, if tilt { 0 } else { 18 });
        // Sleepy: yawn frames first, then an endless nod between the two sleeping drawings.
        assert_eq!(art.pose("sleepy", 0.2, false).index, 8);
        let (a, b) = (art.pose("sleepy", 7.0, false), art.pose("sleepy", 8.5, false));
        assert_eq!((a.index, b.index), (10, 11));
        assert_ne!(a.lift, b.lift);
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
