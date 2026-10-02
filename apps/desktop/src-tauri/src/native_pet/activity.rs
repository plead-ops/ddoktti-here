//! In-monitor play for maximized/split windows. No OS calls; interrupted by user actions.
use super::{gait, physics::climb_distance};
use crate::surfaces::World;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Tour,
    Peek,
    Divider,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    Approach,
    Up,
    Top,
    Down,
    Land,
}
#[derive(Clone, Copy)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub mode: &'static str,
    pub age: f64,
    pub direction: f64,
    pub anchor: Option<(f64, f64)>,
}
pub struct Activity {
    pub monitor: String,
    kind: Kind,
    stage: Stage,
    age: f64,
    x: f64,
    y: f64,
    start_x: f64,
    start_y: f64,
    goal: f64,
    size: f64,
    width: f64,
    height: f64,
    direction: f64,
    distance: f64,
    seam: Option<f64>,
    ceiling: f64,
    /// Tour: the far edge we cross to along the top, chosen on arrival up there.
    traverse: Option<f64>,
    /// Tour: age at which we reached that edge; we rest a moment before coming down.
    arrived: Option<f64>,
}
fn smooth(t: f64) -> f64 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
pub fn layout(w: &World) -> Option<Option<f64>> {
    let tall: Vec<_> = w
        .windows
        .iter()
        .filter(|r| {
            r.y <= w.height * 0.12 && r.y + r.height >= w.height * 0.85 && r.width >= w.width * 0.28
        })
        .collect();
    for a in &tall {
        for b in &tall {
            if a.id != b.id
                && (a.x + a.width - b.x).abs() < 24.
                && a.x < 24.
                && b.x + b.width > w.width - 24.
            {
                return Some(Some((a.x + a.width + b.x) / 2.));
            }
        }
    }
    tall.iter()
        .any(|r| r.x < 24. && r.x + r.width > w.width - 24.)
        .then_some(None)
}
impl Activity {
    pub fn new(w: &World, kind: Kind, ceiling: f64) -> Option<Self> {
        let seam = layout(w)?;
        if kind == Kind::Divider && seam.is_none() {
            return None;
        }
        // All poses must fit; at very large sizes keep normal floor activities available.
        if w.height < w.size * 1.35 || w.width < w.size * 2.2 {
            return None;
        }
        let direction = if kind == Kind::Divider {
            if w.x < seam? {
                1.
            } else {
                -1.
            }
        } else if w.x > w.width / 2. {
            1.
        } else {
            -1.
        };
        let anchor = seam.unwrap_or(if direction > 0. { w.width - 8. } else { 8. });
        let goal = anchor - direction * w.size * 0.46;
        Some(Self {
            monitor: w.monitor.clone(),
            kind,
            stage: Stage::Approach,
            age: 0.,
            x: w.x,
            y: w.y,
            start_x: w.x,
            start_y: w.y,
            goal,
            size: w.size,
            width: w.width,
            height: w.height,
            direction,
            distance: 0.,
            seam,
            ceiling,
            traverse: None,
            arrived: None,
        })
    }
    pub fn approach_from(&mut self, direction: f64) {
        if self.kind != Kind::Divider {
            self.direction = if direction < 0. { -1. } else { 1. };
            let edge = if self.direction < 0. {
                8.
            } else {
                self.width - 8.
            };
            self.goal = edge - self.direction * self.size * 0.46;
        }
    }
    pub fn valid(&self, w: &World) -> bool {
        w.monitor == self.monitor
            && (w.size - self.size).abs() < 0.1
            && (w.width - self.width).abs() < 1.
            && (w.height - self.height).abs() < 1.
            && layout(w).is_some_and(|seam| match (self.seam, seam) {
                (Some(a), Some(b)) => (a - b).abs() < 3.,
                (None, None) => true,
                _ => false,
            })
    }
    fn enter(&mut self, s: Stage) {
        self.stage = s;
        self.age = 0.;
        self.start_x = self.x;
        self.start_y = self.y;
    }
    pub fn step(&mut self, dt: f64, speed: f64, cursor: Option<(f64, f64)>) -> Option<Frame> {
        let dt = dt.clamp(0., 0.1);
        let previous = self.age;
        self.age += dt * speed;
        let k = self.size / 260.;
        let top = self.ceiling + super::art::hanging_reach() * k;
        let middle = (self.height * 0.48).max(self.size * 0.85);
        let target = if self.kind == Kind::Tour { top } else { middle };
        let mut mode = "climb";
        let mut age = self.age;
        let mut anchor = Some((self.x + self.direction * self.size * 0.46, self.ceiling));
        match self.stage {
            Stage::Approach => {
                let step = (self.goal - self.x).clamp(
                    -gait::speed("walk", self.size, speed) * dt,
                    gait::speed("walk", self.size, speed) * dt,
                );
                self.x += step;
                self.distance += step.abs();
                self.direction = if self.goal >= self.x { 1. } else { -1. };
                mode = "walk";
                age = gait::elapsed("walk", self.distance, self.size);
                anchor = None;
                if (self.goal - self.x).abs() < 0.1 {
                    self.direction = if self.goal < self.width / 2. && self.kind != Kind::Divider {
                        -1.
                    } else if self.kind == Kind::Divider
                        && self.goal > self.seam.unwrap_or(self.width / 2.)
                    {
                        -1.
                    } else {
                        1.
                    };
                    self.enter(Stage::Up);
                }
            }
            Stage::Up => {
                self.y = (self.y
                    - (climb_distance(self.age, self.size) - climb_distance(previous, self.size)))
                .max(target);
                anchor = Some((self.goal + self.direction * self.size * 0.46, self.ceiling));
                if self.y <= target {
                    self.enter(Stage::Top);
                }
            }
            Stage::Top => match self.kind {
                Kind::Tour => {
                    // Cross the whole top to the far edge, pausing to look around on
                    // the way (4.5 s on, 2.5 s off); a nearby pointer up here draws us
                    // along instead. Rest at the far edge, then come down.
                    mode = "hang";
                    anchor = None;
                    let margin = self.size * 0.46;
                    let far = *self.traverse.get_or_insert(if self.x < self.width / 2. {
                        self.width - margin
                    } else {
                        margin
                    });
                    let drawn = cursor
                        .filter(|(x, y)| (*x - self.x).abs() < self.size * 2. && *y < self.size)
                        .map(|(x, _)| x - (x - self.x).signum() * self.size * 0.5);
                    let resting = drawn.is_none() && self.age % 7. >= 4.5;
                    let limit = self.size * 0.3 * dt * speed;
                    let dx = if resting {
                        0.
                    } else {
                        (drawn.unwrap_or(far) - self.x).clamp(-limit, limit)
                    };
                    if dx.abs() > 0.001 {
                        self.direction = dx.signum();
                    }
                    self.x = (self.x + dx).clamp(margin, self.width - margin);
                    if drawn.is_none() && (far - self.x).abs() < 1. && self.arrived.is_none() {
                        self.arrived = Some(self.age);
                    }
                    if self.arrived.is_some_and(|a| self.age - a > 2.5) || self.age > 90. {
                        self.enter(Stage::Down);
                    }
                }
                Kind::Peek => {
                    mode = "peek";
                    let out = if self.age < 2. {
                        smooth(self.age / 2.)
                    } else if self.age < 4. {
                        1.
                    } else {
                        1. - smooth((self.age - 4.) / 2.)
                    };
                    self.x = self.start_x + self.direction * self.size * 0.53 * out;
                    anchor = Some((self.goal + self.direction * self.size * 0.46, self.ceiling));
                    if self.age > 7. {
                        self.x = self.start_x;
                        self.enter(Stage::Down);
                    }
                }
                Kind::Divider => {
                    mode = "climb";
                    age = if (self.age / 2.).floor() as usize % 2 == 0 {
                        0.
                    } else {
                        0.4
                    };
                    anchor = Some((self.seam.unwrap(), self.ceiling));
                    if self.age > 8. {
                        self.enter(Stage::Down);
                    }
                }
            },
            Stage::Down => {
                self.y = (self.start_y
                    + (self.height - self.start_y) * smooth((self.age - 0.35) / 3.5))
                .min(self.height);
                age = (4. - self.age).max(0.);
                if self.y >= self.height {
                    self.enter(Stage::Land);
                }
            }
            Stage::Land => {
                mode = "land";
                anchor = None;
                if self.age > 0.5 {
                    return None;
                }
            }
        }
        Some(Frame {
            x: self.x,
            y: self.y,
            mode,
            age,
            direction: self.direction,
            anchor,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::WindowRect;
    fn world() -> World {
        World {
            monitor: "a".into(),
            x: 600.,
            y: 700.,
            width: 1000.,
            height: 700.,
            size: 180.,
            windows: vec![WindowRect {
                id: "inactive".into(),
                x: 0.,
                y: 0.,
                width: 1000.,
                height: 700.,
            }],
        }
    }
    #[test]
    fn complete_tour_is_continuous_and_returns_to_floor() {
        let w = world();
        let mut a = Activity::new(&w, Kind::Tour, 0.).unwrap();
        let (mut x, mut y) = (w.x, w.y);
        let mut top = false;
        let mut finished = false;
        for _ in 0..12000 {
            match a.step(1. / 60., 1., None) {
                Some(f) => {
                    assert!((f.x - x).abs() < 5., "horizontal teleport");
                    assert!((f.y - y).abs() < 8., "vertical teleport");
                    x = f.x;
                    y = f.y;
                    top |= f.mode == "hang";
                }
                None => {
                    finished = true;
                    break;
                }
            }
        }
        assert!(top && finished);
        assert_eq!(y, w.height);
        // The tour crosses the whole top: it comes down beside the far edge, not the middle.
        assert!(x < w.size * 0.46 + 1. || x > w.width - w.size * 0.46 - 1., "x={x}");
    }
    #[test]
    fn tour_pauses_on_the_way_and_rests_at_the_far_edge_before_descending() {
        let w = world();
        let mut a = Activity::new(&w, Kind::Tour, 0.).unwrap();
        a.stage = Stage::Top;
        a.x = 200.;
        let mut moving = 0;
        let mut still = 0;
        let mut top_frames = 0;
        let mut last = a.x;
        let mut arrived_at = None;
        for i in 0..(120 * 60) {
            let Some(f) = a.step(1. / 60., 1., None) else { break };
            if a.stage != Stage::Top {
                break;
            }
            top_frames += 1;
            if (f.x - last).abs() > 1e-6 { moving += 1 } else { still += 1 }
            if arrived_at.is_none() && (f.x - (w.width - w.size * 0.46)).abs() < 1. {
                arrived_at = Some(i);
            }
            last = f.x;
        }
        assert!(moving > 0 && still > 0, "moves and pauses: {moving}/{still}");
        let arrived = arrived_at.expect("reaches the far edge");
        assert!(top_frames - arrived >= 150 - 2, "rests about 2.5 s at the edge");
        assert!(top_frames - arrived < 200);
        assert_eq!(a.stage, Stage::Down);
    }
    #[test]
    fn hanging_hand_touches_physical_screen_top_above_work_area() {
        let art = super::super::art::Art::new().unwrap();
        for ceiling in [0., -24., -38.] {
            for size in [90., 180., 450.] {
                let mut w = world();
                w.size = size;
                let mut a = Activity::new(&w, Kind::Tour, ceiling).unwrap();
                a.stage = Stage::Top;
                a.y = ceiling + super::super::art::hanging_reach() * size / 260.;
                for age in [0., 0.6, 1.2, 1.8] {
                    a.age = age;
                    let f = a.step(0., 1., None).unwrap();
                    let pose = art.pose(f.mode, f.age, false);
                    let hand = art
                        .hands(&pose)
                        .iter()
                        .map(|p| p[1])
                        .fold(f64::INFINITY, f64::min);
                    let image_y = f.y - (250. + pose.lift) * size / 260.;
                    assert!(
                        image_y < ceiling,
                        "transparent SVG margin must extend above screen"
                    );
                    assert!((image_y + hand * size / 260. - ceiling).abs() < 1e-8);
                }
            }
        }
    }
    #[test]
    fn traversal_faces_motion_and_keeps_direction_when_stopped() {
        for (start, expected) in [(750., -1.), (250., 1.)] {
            let w = world();
            let mut a = Activity::new(&w, Kind::Tour, -30.).unwrap();
            a.stage = Stage::Top;
            a.x = start;
            a.direction = -expected;
            let f = a.step(1. / 60., 1., None).unwrap();
            assert_eq!(f.direction, expected);
            assert!((f.x - start) * expected > 0.);
            a.x = w.width / 2.;
            let f = a.step(1. / 60., 1., None).unwrap();
            assert_eq!(f.direction, expected);
        }
    }
    #[test]
    fn alternating_approach_uses_both_sides_even_from_same_position() {
        let w = world();
        for side in [-1., 1.] {
            let mut a = Activity::new(&w, Kind::Tour, 0.).unwrap();
            a.approach_from(side);
            assert_eq!(a.direction, side);
            assert!((a.goal - w.width / 2.) * side > 0.);
        }
    }
    #[test]
    fn ignores_small_windows_and_cancels_layout_changes() {
        let mut w = world();
        let a = Activity::new(&w, Kind::Peek, 0.).unwrap();
        w.windows[0].width = 400.;
        assert!(!a.valid(&w));
        assert!(Activity::new(&w, Kind::Tour, 0.).is_none());
    }
    #[test]
    fn split_detects_inactive_neighbor_without_focus_information() {
        let mut w = world();
        w.windows[0].width = 500.;
        w.windows.push(WindowRect {
            id: "front".into(),
            x: 500.,
            y: 0.,
            width: 500.,
            height: 700.,
        });
        assert_eq!(layout(&w), Some(Some(500.)));
        assert!(Activity::new(&w, Kind::Divider, 0.).is_some());
    }
}
