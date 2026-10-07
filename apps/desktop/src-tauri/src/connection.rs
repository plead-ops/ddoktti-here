//! Transient, non-blocking service notices. Never controls pet/timer availability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Slack,
    Calendar,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Reconnecting,
    Authorization,
}
impl Problem {
    pub fn from_error(error: &str) -> Self {
        if error.contains("다시 연결") && !error.contains("기다") {
            Self::Authorization
        } else {
            Self::Reconnecting
        }
    }
    pub fn status(self, service: Service) -> &'static str {
        match (service, self) {
            (Service::Slack, Self::Reconnecting) => "재연결 중 · Slack 알림이 일시 중단됐어요",
            (Service::Calendar, Self::Reconnecting) => "재연결 중 · 일정 동기화가 지연되고 있어요",
            (Service::Slack, Self::Authorization) => {
                "인증이 만료됐어요 · Slack에 다시 연결해 주세요"
            }
            (Service::Calendar, Self::Authorization) => {
                "인증이 만료됐어요 · Google에 다시 연결해 주세요"
            }
        }
    }
}
/// Brief retry failures stay silent; only an outage lasting this long is announced.
const RECONNECT_GRACE: u64 = 60;
#[derive(Default)]
struct Entry {
    problem: Option<Problem>,
    since: Option<u64>,
    announced: bool,
    pending: Option<u64>,
    last: Option<u64>,
}
#[derive(Default)]
pub struct Notices {
    entries: [Entry; 2],
    last_shown: Option<u64>,
}
impl Notices {
    pub fn observe(&mut self, service: Service, problem: Option<Problem>, now: u64) {
        let e = &mut self.entries[service as usize];
        let Some(p) = problem else {
            e.problem = None;
            e.since = None;
            e.pending = None;
            return;
        };
        if e.problem != problem {
            e.problem = problem;
            e.since = Some(now);
            e.announced = false;
            e.pending = None;
        }
        let due = p == Problem::Authorization
            || e.since.is_some_and(|t| now.saturating_sub(t) >= RECONNECT_GRACE);
        if due && !e.announced {
            e.announced = true;
            if e.last.is_none_or(|t| now.saturating_sub(t) >= 300) {
                e.last = Some(now);
                e.pending = Some(now);
            }
        }
    }
    pub fn pending(&self, now: u64) -> bool {
        self.last_shown.is_none_or(|t| now.saturating_sub(t) >= 30)
            && self
                .entries
                .iter()
                .any(|e| e.pending.is_some_and(|t| now.saturating_sub(t) < 20))
    }
    pub fn take(&mut self, now: u64) -> Option<&'static str> {
        if !self.pending(now) {
            return None;
        }
        let active: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.pending.is_some_and(|t| now.saturating_sub(t) < 20))
            .map(|(i, e)| (i, e.problem.unwrap()))
            .collect();
        for e in &mut self.entries {
            e.pending = None;
        }
        self.last_shown = Some(now);
        Some(match active.as_slice() {
            [(0, Problem::Reconnecting)] => "Slack 연결이 잠시 끊겼어요. 다시 연결하고 있어요.",
            [(1, Problem::Reconnecting)] => "Google 연결이 잠시 끊겼어요. 다시 연결하고 있어요.",
            [(0, Problem::Authorization)] => "Slack에 다시 연결해 주세요. 설정에서 할 수 있어요.",
            [(1, Problem::Authorization)] => "Google에 다시 연결해 주세요. 설정에서 할 수 있어요.",
            _ => "서비스 연결을 확인하고 있어요. 자세한 상태는 설정에서 볼 수 있어요.",
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_retry_is_not_mistaken_for_expired_authorization() {
        assert_eq!(
            Problem::from_error("오프라인 · 다시 연결을 기다려요"),
            Problem::Reconnecting
        );
        assert_eq!(
            Problem::from_error("Google 인증 서버에 잠시 연결하지 못했어요"),
            Problem::Reconnecting
        );
        assert_eq!(
            Problem::from_error("인증 갱신 실패 · Google에 다시 연결해 주세요"),
            Problem::Authorization
        );
        assert_eq!(
            Problem::from_error("Slack에 다시 연결해 주세요"),
            Problem::Authorization
        );
    }
    #[test]
    fn brief_failures_stay_silent_until_the_outage_persists() {
        let mut n = Notices::default();
        for t in [0, 6, 18, 42] {
            n.observe(Service::Slack, Some(Problem::Reconnecting), t);
            assert!(n.take(t).is_none());
        }
        n.observe(Service::Slack, None, 50);
        n.observe(Service::Slack, Some(Problem::Reconnecting), 100);
        assert!(n.take(100).is_none());
        n.observe(Service::Slack, Some(Problem::Reconnecting), 160);
        assert!(n.take(160).unwrap().contains("Slack"));
    }
    #[test]
    fn sustained_outage_and_flapping_do_not_repeat_notices() {
        let mut n = Notices::default();
        n.observe(Service::Slack, Some(Problem::Reconnecting), 0);
        n.observe(Service::Slack, Some(Problem::Reconnecting), 60);
        assert!(n.take(60).unwrap().contains("Slack"));
        for t in 61..600 {
            n.observe(Service::Slack, Some(Problem::Reconnecting), t);
            assert!(n.take(t).is_none());
        }
        n.observe(Service::Slack, None, 600);
        n.observe(Service::Slack, Some(Problem::Reconnecting), 601);
        n.observe(Service::Slack, Some(Problem::Reconnecting), 661);
        assert!(n.take(661).is_some());
        n.observe(Service::Slack, None, 662);
        n.observe(Service::Slack, Some(Problem::Reconnecting), 663);
        n.observe(Service::Slack, Some(Problem::Reconnecting), 723);
        assert!(n.take(723).is_none());
    }
    #[test]
    fn recovery_and_expiry_remove_queued_notices_and_services_are_independent() {
        let mut n = Notices::default();
        n.observe(Service::Slack, Some(Problem::Reconnecting), 0);
        n.observe(Service::Slack, Some(Problem::Reconnecting), 60);
        n.observe(Service::Slack, None, 61);
        assert!(!n.pending(62));
        n.observe(Service::Calendar, Some(Problem::Authorization), 2);
        assert!(n.take(2).unwrap().contains("Google에 다시 연결"));
        n.observe(Service::Slack, Some(Problem::Reconnecting), 400);
        n.observe(Service::Slack, Some(Problem::Reconnecting), 460);
        assert!(n.take(481).is_none());
    }
}
