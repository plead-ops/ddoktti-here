//! Sleeping under the blanket while the person is away, waking when they return. Input is only the
//! seconds since the last system input event; no key or pointer data is read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// No input for `DOZE_AFTER`: into bed under the blanket.
    Doze,
    /// The person is back after this many seconds away.
    Return(f64),
}
#[derive(Default)]
pub struct Presence {
    asleep: bool,
    away: f64,
}
/// Seconds without any input before the character goes to sleep.
pub const DOZE_AFTER: f64 = 5. * 60.;
/// Absences this long or longer get a warmer welcome back.
pub const LONG_AWAY: f64 = 30. * 60.;
impl Presence {
    pub fn asleep(&self) -> bool {
        self.asleep
    }
    /// `idle` is the platform's seconds-since-last-input; `None` means unknown and
    /// never changes state.
    pub fn observe(&mut self, idle: Option<f64>) -> Option<Event> {
        let idle = idle?;
        if !idle.is_finite() || idle < 0. {
            return None;
        }
        if self.asleep {
            self.away = self.away.max(idle);
            if idle < 1.5 {
                self.asleep = false;
                let away = self.away;
                self.away = 0.;
                return Some(Event::Return(away));
            }
            return None;
        }
        if idle >= DOZE_AFTER {
            self.asleep = true;
            self.away = idle;
            return Some(Event::Doze);
        }
        None
    }
    /// A click or drag on the character counts as the person being back.
    pub fn wake(&mut self) {
        self.asleep = false;
        self.away = 0.;
    }
}
pub fn welcome(away: f64) -> &'static str {
    if away >= LONG_AWAY {
        "오랜만이에요! 기다렸어요."
    } else {
        "다녀오셨어요?"
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sleeps_after_five_idle_minutes_and_wakes_on_first_input() {
        let mut p = Presence::default();
        assert_eq!(p.observe(Some(0.)), None);
        assert_eq!(p.observe(Some(DOZE_AFTER - 1.)), None);
        assert!(!p.asleep());
        assert_eq!(p.observe(Some(DOZE_AFTER)), Some(Event::Doze));
        assert!(p.asleep());
        assert_eq!(p.observe(Some(DOZE_AFTER + 60.)), None, "stays asleep");
        assert_eq!(p.observe(Some(0.2)), Some(Event::Return(DOZE_AFTER + 60.)));
        assert!(!p.asleep());
        assert_eq!(p.observe(Some(0.4)), None, "a second input is not another return");
    }
    #[test]
    fn long_absences_wake_as_a_reunion() {
        let mut p = Presence::default();
        assert_eq!(p.observe(Some(DOZE_AFTER)), Some(Event::Doze));
        assert_eq!(p.observe(Some(LONG_AWAY + 600.)), None, "reported once");
        assert_eq!(p.observe(Some(0.)), Some(Event::Return(LONG_AWAY + 600.)));
        assert_eq!(welcome(LONG_AWAY + 600.), "오랜만이에요! 기다렸어요.");
        p.observe(Some(DOZE_AFTER));
        p.wake();
        assert!(!p.asleep());
    }
    #[test]
    fn unknown_or_invalid_idle_never_changes_state_and_manual_wake_is_silent() {
        let mut p = Presence::default();
        assert_eq!(p.observe(None), None);
        assert_eq!(p.observe(Some(f64::NAN)), None);
        assert_eq!(p.observe(Some(-5.)), None);
        p.observe(Some(DOZE_AFTER));
        assert_eq!(p.observe(None), None);
        assert!(p.asleep());
        p.wake();
        assert!(!p.asleep());
        assert_eq!(p.observe(Some(0.1)), None, "no welcome after a manual wake");
    }
    #[test]
    fn welcome_depends_on_absence_length() {
        assert_eq!(welcome(DOZE_AFTER), "다녀오셨어요?");
        assert_eq!(welcome(LONG_AWAY), "오랜만이에요! 기다렸어요.");
    }
}
