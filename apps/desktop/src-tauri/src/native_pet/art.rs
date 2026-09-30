use super::physics::climb_frame;
use resvg::{
    tiny_skia::{LineCap, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform},
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
pub struct Art {
    paths: HashMap<String, String>,
    pub sheets: HashMap<String, Sheet>,
    hands: HashMap<String, Vec<[f64; 2]>>,
    cache: VecDeque<(String, u32, Pixmap)>,
}
pub struct Pose {
    pub sheet: &'static str,
    pub index: usize,
    pub lift: f64,
    pub offset: f64,
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
        })
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
            "sulking" => Some(("emotions-v2", 2, 0.55, false)),
            "cheering" => Some(("emotions-v2", 3, 0.55, false)),
            "relieved" => Some(("emotions-v2", 4, 0.45, false)),
            _ => None,
        };
        if mode == "run" {
            p.sheet = "run-v3";
            p.index = super::gait::frame(mode, t, reduced);
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
            return p;
        }
        match mode {
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
            "fall" | "land" => {
                p.sheet = "edge-v2";
                p.index = if mode == "fall" { 5 } else { 6 };
            }
            "wobble" => p.index = ((t / 0.25).floor() as usize).min(3),
            "grab" | "climb" | "pull" | "prepare" | "travel-jump" => {
                p.sheet = "surfaces-v2";
                p.index = match mode {
                    "grab" => 12 + ((t / 0.1).floor() as usize).min(3),
                    "climb" => climb_frame(t),
                    "pull" => 4 + ((t / 0.175).floor() as usize).min(3),
                    "prepare" => 8,
                    _ => {
                        if t < 0.15 {
                            9
                        } else {
                            10
                        }
                    }
                };
                if matches!(mode, "grab" | "climb" | "pull") {
                    p.offset = (119.6 - (self.sheets[p.sheet].frames[p.index].right - 200.))
                        * if mode == "pull" {
                            1. - (t / 0.7).min(1.)
                        } else {
                            1.
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
        while self.cache.len() > 16 {
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
    let retract = if mode == "pull" {
        ((phase - 0.3) / 0.4).clamp(0., 1.) as f32
    } else {
        0.
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
pub fn composite(target: &mut Pixmap, pet: &Pixmap, x: f32, y: f32, flip: bool) {
    let ts = if flip {
        Transform::from_row(-1., 0., 0., 1., x + pet.width() as f32, y)
    } else {
        Transform::from_translate(x, y)
    };
    target.draw_pixmap(0, 0, pet.as_ref(), &PixmapPaint::default(), ts, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_original_frame_rasterizes_transparently_and_cache_is_bounded() {
        let mut art = Art::new().unwrap();
        let keys: Vec<_> = art.paths.keys().cloned().collect();
        assert_eq!(keys.len(), 124);
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
        composite(&mut out, &p, 0., 0., true);
        for y in 0..260 {
            for x in 0..400 {
                assert_eq!(out.pixel(x, y), p.pixel(399 - x, y));
            }
        }
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
        for mode in ["grab", "climb", "pull"] {
            for i in 0..8 {
                let p = art.pose(mode, i as f64 * 0.1, false);
                assert_eq!(art.hands(&p).len(), 2);
            }
        }
    }
}
