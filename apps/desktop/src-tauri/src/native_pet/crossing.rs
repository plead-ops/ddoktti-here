//! Traversal between touching displays in OS desktop coordinates. Different
//! floors use a rope on each side of the shared visible corridor.
use super::physics::climb_distance;
#[derive(Clone, Debug)]
pub struct Screen {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub wx: f64,
    pub wy: f64,
    pub ww: f64,
    pub wh: f64,
    pub factor: f64,
}
pub struct Crossing {
    pub from: String,
    pub to: String,
    start: (f64, f64),
    end: (f64, f64),
    edge: f64,
    direction: f64,
    age: f64,
    traverse: f64,
    up: f64,
    down: f64,
    corridor_floor: f64,
    anchor_y: f64,
    from_size: f64,
    to_size: f64,
    same_floor: bool,
    arc: f64,
}
pub struct Sample {
    pub screen: String,
    pub x: f64,
    pub y: f64,
    pub direction: f64,
    pub walk: bool,
    pub distance: f64,
    pub done: bool,
    pub mode: &'static str,
    pub age: f64,
    /// Rope attachment in global OS coordinates, like x/y above.
    pub anchor: Option<(f64, f64)>,
}
fn climb_duration(distance: f64, size: f64) -> f64 {
    if distance <= 0.001 {
        0.
    } else {
        (distance / (size * 0.4)).ceil() * 0.8
    }
}
impl Crossing {
    pub fn new(
        from: &Screen,
        screens: &[Screen],
        point: (f64, f64),
        direction: f64,
        size: f64,
        speed: f64,
    ) -> Option<Self> {
        if size <= 0. || from.factor <= 0. || direction == 0. {
            return None;
        }
        let direction = direction.signum();
        let edge = if direction > 0. {
            from.x + from.w
        } else {
            from.x
        };
        // Try every adjacent display: the first can be too short at this scale.
        screens
            .iter()
            .filter(|s| s.id != from.id && s.factor > 0.)
            .find_map(|to| {
                let boundary = if direction > 0. { to.x } else { to.x + to.w };
                if (boundary - edge).abs() >= 3. {
                    return None;
                }
                let from_size = size * from.factor;
                let to_size = size * to.factor;
                let corridor_top = from.wy.max(to.wy).max(from.y).max(to.y);
                let corridor_floor = (from.wy + from.wh)
                    .min(to.wy + to.wh)
                    .min(from.y + from.h)
                    .min(to.y + to.h);
                let body_height = from_size.max(to_size) * 0.83;
                // Both scales must fit during the handoff; don't crop the antenna.
                let headroom = corridor_floor - corridor_top - body_height;
                if headroom < 8. || from.ww < from_size * 0.92 || to.ww < to_size * 0.92 {
                    return None;
                }
                let end = (
                    if direction > 0. {
                        to.wx + to_size * 0.46
                    } else {
                        to.wx + to.ww - to_size * 0.46
                    },
                    to.wy + to.wh,
                );
                if point.0 < from.wx + from_size * 0.46 - 3.
                    || point.0 > from.wx + from.ww - from_size * 0.46 + 3.
                    || point.1 < from.wy + from_size * 0.83
                    || point.1 > from.wy + from.wh + 1.
                    || point.1 < corridor_floor - 3.
                {
                    return None;
                }
                let same_floor =
                    (end.1 - point.1).abs() < 3. && (point.1 - corridor_floor).abs() < 3.;
                Some(Self {
                    from: from.id.clone(),
                    to: to.id.clone(),
                    start: point,
                    end,
                    edge,
                    direction,
                    age: 0.,
                    traverse: ((end.0 - point.0).abs() / speed.max(10.)).clamp(0.6, 6.),
                    up: if same_floor {
                        0.
                    } else {
                        climb_duration((point.1 - corridor_floor).max(0.), from_size)
                    },
                    down: if same_floor {
                        0.
                    } else {
                        climb_duration((end.1 - corridor_floor).max(0.), to_size)
                    },
                    corridor_floor,
                    anchor_y: corridor_top + 8.,
                    from_size,
                    to_size,
                    same_floor,
                    arc: if same_floor {
                        0.
                    } else {
                        (from_size.min(to_size) * 0.2).min(headroom - 4.)
                    },
                })
            })
    }
    pub fn step(&mut self, dt: f64) -> Sample {
        self.age += dt.max(0.);
        let distance = (self.end.0 - self.start.0).abs();
        let (x, y, mode, age, anchor, traveled, done) = if self.age < self.up {
            let y =
                (self.start.1 - climb_distance(self.age, self.from_size)).max(self.corridor_floor);
            (
                self.start.0,
                y,
                "climb",
                self.age,
                Some((
                    self.start.0 + self.direction * self.from_size * 0.46,
                    self.anchor_y,
                )),
                0.,
                false,
            )
        } else if self.age < self.up + self.traverse {
            let age = self.age - self.up;
            let t = (age / self.traverse).clamp(0., 1.);
            let x = self.start.0 + (self.end.0 - self.start.0) * t;
            let y = if self.same_floor {
                self.start.1 + (self.end.1 - self.start.1) * t
            } else {
                self.corridor_floor - (std::f64::consts::PI * t).sin() * self.arc
            };
            (
                x,
                y,
                if self.same_floor {
                    "walk"
                } else {
                    "travel-jump"
                },
                age,
                None,
                distance * t,
                false,
            )
        } else if self.age < self.up + self.traverse + self.down {
            let age = self.age - self.up - self.traverse;
            let y = (self.corridor_floor + climb_distance(age, self.to_size)).min(self.end.1);
            (
                self.end.0,
                y,
                "climb",
                age,
                Some((
                    self.end.0 - self.direction * self.to_size * 0.46,
                    self.anchor_y,
                )),
                distance,
                false,
            )
        } else {
            let age = (self.age - self.up - self.traverse - self.down).max(0.);
            (
                self.end.0,
                self.end.1,
                if self.same_floor { "walk" } else { "land" },
                age,
                None,
                distance,
                self.same_floor || age >= 0.45,
            )
        };
        Sample {
            screen: if (x - self.edge) * self.direction >= 0. {
                self.to.clone()
            } else {
                self.from.clone()
            },
            x,
            y,
            // On arrival the rope is behind us; face it while descending.
            direction: if mode == "climb" && self.age >= self.up + self.traverse {
                -self.direction
            } else {
                self.direction
            },
            walk: mode == "walk",
            distance: traveled,
            done,
            mode,
            age,
            anchor,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn screen(id: &str, x: f64, factor: f64) -> Screen {
        Screen {
            id: id.into(),
            x,
            y: 0.,
            w: 1000.,
            h: 800.,
            wx: x,
            wy: 0.,
            ww: 1000.,
            wh: 800.,
            factor,
        }
    }
    #[test]
    fn touching_displays_cross_without_teleport() {
        let a = screen("a", 0., 1.);
        let b = screen("b", 1000., 2.);
        let mut c = Crossing::new(&a, &[a.clone(), b], (917., 800.), 1., 180., 60.).unwrap();
        let mut x = 917.;
        let mut changed = false;
        for _ in 0..400 {
            let s = c.step(1. / 60.);
            assert!((s.x - x).abs() < 2.);
            assert_eq!(s.mode, "walk");
            assert_eq!(s.y, 800.);
            assert!(s.anchor.is_none());
            x = s.x;
            changed |= s.screen == "b";
            if s.done {
                break;
            }
        }
        assert!(changed && x > 1000.);
    }
    #[test]
    fn disconnected_or_non_touching_screen_is_not_a_bridge() {
        let a = screen("a", 0., 1.);
        assert!(
            Crossing::new(&a, &[screen("b", 1100., 1.)], (917., 800.), 1., 180., 60.).is_none()
        );
        let mut b = screen("b", 1000., 2.);
        b.y = 650.;
        b.wy = 650.;
        b.h = 800.;
        assert!(Crossing::new(&a, &[b], (917., 800.), 1., 180., 60.).is_none());
    }
    fn validate_route(a: &Screen, b: &Screen, size: f64, direction: f64) -> Vec<&'static str> {
        let start = (
            if direction > 0. {
                a.wx + a.ww - size * a.factor * 0.46
            } else {
                a.wx + size * a.factor * 0.46
            },
            a.wy + a.wh,
        );
        let mut c = Crossing::new(a, &[a.clone(), b.clone()], start, direction, size, 90.).unwrap();
        let mut modes = Vec::new();
        let mut previous = start;
        let mut done = false;
        for _ in 0..10000 {
            let s = c.step(1. / 120.);
            let active = if s.screen == a.id { a } else { b };
            let height = size * active.factor * 0.83;
            assert!(
                s.y - height >= active.wy - 0.01,
                "head cropped: {} {}",
                s.mode,
                s.y
            );
            assert!(s.y <= active.wy + active.wh + 0.01, "feet cropped");
            assert!(
                (s.x - previous.0).abs() < 5. && (s.y - previous.1).abs() < 10.,
                "teleport"
            );
            if s.mode == "climb" {
                let anchor = s.anchor.expect("climbing keeps a visible rope");
                assert_eq!((anchor.0 - s.x).signum(), s.direction);
                assert!(anchor.1 >= active.wy && anchor.1 < s.y);
                assert!(s.x - size * active.factor * 0.46 >= active.wx - 0.01);
                assert!(s.x + size * active.factor * 0.46 <= active.wx + active.ww + 0.01);
            }
            if s.mode == "travel-jump" {
                let max_height = size * a.factor.max(b.factor) * 0.83;
                assert!(s.y - max_height >= a.wy.max(b.wy));
                assert!(s.y <= (a.wy + a.wh).min(b.wy + b.wh));
                assert!(!s.walk);
            }
            modes.push(s.mode);
            previous = (s.x, s.y);
            if s.done {
                assert_eq!(s.screen, b.id);
                assert_eq!(s.y, b.wy + b.wh);
                done = true;
                break;
            }
        }
        assert!(done);
        modes
    }
    #[test]
    fn actual_mac_offset_retina_layout_climbs_and_descends_without_clipping() {
        // OS coordinates are points on macOS, so both factors are 1 after
        // converting the Retina display's physical bounds/work area by 2.
        let main = Screen {
            id: "main".into(),
            x: 0.,
            y: 0.,
            w: 3840.,
            h: 2160.,
            wx: 0.,
            wy: 30.,
            ww: 3840.,
            wh: 2040.,
            factor: 1.,
        };
        let sub = Screen {
            id: "retina".into(),
            x: -1728.,
            y: 360.,
            w: 1728.,
            h: 1117.,
            wx: -1728.,
            wy: 360.,
            ww: 1728.,
            wh: 1084.,
            factor: 1.,
        };
        for size in [100., 529.4] {
            for (a, b, d) in [(&main, &sub, -1.), (&sub, &main, 1.)] {
                let modes = validate_route(a, b, size, d);
                for mode in ["climb", "travel-jump", "land"] {
                    assert!(modes.contains(&mode));
                }
            }
        }
    }
    #[test]
    fn windows_mixed_dpi_handoff_reserves_the_larger_body_on_both_sides() {
        let a = screen("one", 0., 1.);
        let mut b = screen("two", 1000., 2.);
        b.y = 100.;
        b.wy = 100.;
        b.h = 1100.;
        b.wh = 1100.;
        validate_route(&a, &b, 180., 1.);
        validate_route(&b, &a, 180., -1.);
        assert!(Crossing::new(&a, &[b], (770., 800.), 1., 500., 90.).is_none());
    }
}
