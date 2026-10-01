//! Gentle back-and-forth pointer strokes, independent of click/drag handling.
#[derive(Default)]
pub struct Petting {
    last: Option<(f64, f64)>,
    direction: f64,
    stroke: f64,
    turns: u8,
    idle: f64,
    cooldown: f64,
}
impl Petting {
    pub fn sample(&mut self, point: (f64, f64), dt: f64, size: f64, eligible: bool) -> bool {
        self.cooldown = (self.cooldown - dt).max(0.);
        if !eligible || self.cooldown > 0. {
            self.clear();
            return false;
        }
        let previous = self.last.replace(point);
        let Some(previous) = previous else {
            return false;
        };
        let dx = point.0 - previous.0;
        let dy = point.1 - previous.1;
        let distance = dx.hypot(dy);
        self.idle += dt;
        if distance < 0.6 {
            // A stationary pointer or subpixel jitter is not a stroke.
            if self.idle > 0.7 {
                self.clear();
            }
            return false;
        }
        if distance / dt.max(0.001) > size * 6. {
            self.clear();
            return false;
        }
        self.idle = 0.;
        if dx.abs() < dy.abs() || dx.abs() < 0.6 {
            return false;
        }
        let direction = dx.signum();
        if self.direction != 0. && self.direction != direction {
            if self.stroke >= size * 0.10 {
                self.turns += 1;
            } else {
                self.turns = 0;
            }
            self.stroke = 0.;
        }
        self.direction = direction;
        self.stroke += dx.abs();
        if self.turns >= 2 && self.stroke >= size * 0.06 {
            self.clear();
            self.cooldown = 4.;
            true
        } else {
            false
        }
    }
    fn clear(&mut self) {
        self.last = None;
        self.direction = 0.;
        self.stroke = 0.;
        self.turns = 0;
        self.idle = 0.;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn stroke(p: &mut Petting, allowed: bool) -> usize {
        let mut hits = 0;
        for x in (0..30).chain((0..30).rev()).chain(0..30) {
            hits += p.sample((x as f64 * 2., 100.), 1. / 60., 180., allowed) as usize;
        }
        hits
    }
    #[test]
    fn repeated_gentle_strokes_trigger_once_and_cool_down() {
        let mut p = Petting::default();
        assert_eq!(stroke(&mut p, true), 1);
        assert_eq!(stroke(&mut p, true), 0);
    }
    #[test]
    fn hovering_clicking_and_fast_passes_do_not_pet() {
        let mut p = Petting::default();
        for _ in 0..180 {
            assert!(!p.sample((50., 50.), 1. / 60., 180., true));
        }
        assert_eq!(stroke(&mut p, false), 0);
        for x in [0., 300., 0., 300., 0.] {
            assert!(!p.sample((x, 50.), 1. / 60., 180., true));
        }
    }
}
