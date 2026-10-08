//! Frame pacing statistics for DDOKTTI_TRACE: how evenly ticks reach the main
//! thread, how long they wait for it and how long they take. Timings only.
use std::time::Duration;
#[derive(Default)]
pub struct Pacing {
    /// Time between consecutive tick starts on the main thread.
    intervals: Vec<f64>,
    /// From the scheduler waking to the tick starting on the main thread.
    waits: Vec<f64>,
    /// Tick duration on the main thread.
    works: Vec<f64>,
    /// Scheduler wake-ups dropped because the previous tick had not run yet.
    pub skipped: u32,
    last_start: Option<std::time::Instant>,
    window_start: Option<std::time::Instant>,
}
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.
}
/// Nearest-rank percentile of an unsorted sample, 0 for none.
pub fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v[((p / 100. * v.len() as f64).ceil() as usize).clamp(1, v.len()) - 1]
}
impl Pacing {
    pub fn record(&mut self, start: std::time::Instant, wait: Duration, work: Duration) {
        if let Some(last) = self.last_start {
            self.intervals.push(ms(start - last));
        }
        self.last_start = Some(start);
        self.waits.push(ms(wait));
        self.works.push(ms(work));
    }
    /// A summary is due every five seconds.
    pub fn due(&mut self, now: std::time::Instant) -> bool {
        let start = *self.window_start.get_or_insert(now);
        if now - start < Duration::from_secs(5) {
            return false;
        }
        self.window_start = Some(now);
        true
    }
    /// One summary line, then start a new window. `period` is the target frame time.
    pub fn summary(&mut self, period: f64) -> String {
        let late = self.intervals.iter().filter(|&&d| d > period * 1.5).count();
        let line = format!(
            "pacing ticks={} skipped={} late(>{:.0}ms)={} interval p50={:.1} p95={:.1} max={:.1} wait p50={:.1} p95={:.1} max={:.1} work p50={:.1} p95={:.1} max={:.1}",
            self.works.len(),
            self.skipped,
            period * 1.5,
            late,
            percentile(&self.intervals, 50.),
            percentile(&self.intervals, 95.),
            percentile(&self.intervals, 100.),
            percentile(&self.waits, 50.),
            percentile(&self.waits, 95.),
            percentile(&self.waits, 100.),
            percentile(&self.works, 50.),
            percentile(&self.works, 95.),
            percentile(&self.works, 100.),
        );
        *self = Self {
            last_start: self.last_start,
            window_start: self.window_start,
            ..Self::default()
        };
        line
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    #[test]
    fn percentiles_and_late_frames() {
        assert_eq!(percentile(&[], 50.), 0.);
        assert_eq!(percentile(&[3., 1., 2.], 50.), 2.);
        assert_eq!(percentile(&[3., 1., 2.], 100.), 3.);
        let mut p = Pacing::default();
        let t = Instant::now();
        for (i, at) in [0u64, 17, 33, 70, 87].iter().enumerate() {
            p.record(
                t + Duration::from_millis(*at),
                Duration::from_millis(i as u64),
                Duration::from_millis(2),
            );
        }
        p.skipped = 1;
        let line = p.summary(16.7);
        assert!(line.contains("ticks=5 skipped=1 late(>25ms)=1"), "{line}");
        assert!(line.contains("interval p50=17.0"), "{line}");
        assert!(p.summary(16.7).contains("ticks=0 skipped=0"));
    }
}
