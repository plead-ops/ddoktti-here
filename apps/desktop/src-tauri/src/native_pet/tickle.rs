#[derive(Default)]
pub struct Tickle {
    heat: f64,
    cool: f64,
    held: f64,
}
#[derive(Debug, PartialEq)]
pub enum Response {
    Laugh,
    Enough,
}
impl Tickle {
    pub fn advance(&mut self, dt: f64, held: bool) -> bool {
        self.cool = (self.cool - dt).max(0.);
        if !held {
            self.heat = (self.heat - dt * 0.15).max(0.);
        }
        if held {
            self.held += dt;
            if self.held >= 0.75 {
                self.held = 0.;
                return true;
            }
        } else {
            self.held = 0.;
        }
        false
    }
    pub fn request(&mut self) -> Option<Response> {
        if self.cool > 0. {
            return None;
        }
        self.heat += 1.;
        if self.heat >= 3.0 {
            self.cool = 6.;
            self.heat = 0.;
            Some(Response::Enough)
        } else {
            Some(Response::Laugh)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_tickle_sets_boundary_then_recovers() {
        let mut t = Tickle::default();
        for _ in 0..2 {
            assert_eq!(t.request(), Some(Response::Laugh));
        }
        assert_eq!(t.request(), Some(Response::Enough));
        assert_eq!(t.request(), None);
        t.advance(7., false);
        assert_eq!(t.request(), Some(Response::Laugh));
    }
    #[test]
    fn steady_clicks_and_holding_reach_enough_without_randomness() {
        let mut t = Tickle::default();
        for _ in 0..3 {
            assert_eq!(t.request(), Some(Response::Laugh));
            t.advance(0.7, false);
        }
        assert_eq!(t.request(), Some(Response::Enough));
        let mut held = Tickle::default();
        let mut enough = 0;
        for _ in 0..180 {
            if held.advance(1. / 60., true) && held.request() == Some(Response::Enough) {
                enough += 1;
            }
        }
        assert_eq!(enough, 1);
        assert_eq!(held.request(), None);
    }
    #[test]
    fn holding_repeats_without_counting_drag() {
        let mut t = Tickle::default();
        let mut n = 0;
        for _ in 0..240 {
            if t.advance(1. / 60., true) {
                n += 1;
            }
        }
        assert!(n >= 4);
        assert!(!t.advance(1., false));
    }
}
