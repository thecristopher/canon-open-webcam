use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Decides how long to wait before asking the camera for the next frame.
/// Asking too early just earns a "not ready" and burns USB time, asking late adds lag.
pub struct FramePacer {
    /// The T7 makes a frame every 41.7ms, but moving one over USB eats about 14ms of that,
    /// so counting from the end of the last transfer the next frame lands around 27ms later
    pub earliest_ask: Duration,
    pub retry_delay: Duration,
}

impl Default for FramePacer {
    fn default() -> Self {
        Self { earliest_ask: Duration::from_millis(20), retry_delay: Duration::from_millis(2) }
    }
}

impl FramePacer {
    pub fn wait_before_asking(&self, since_last_frame: Duration) -> Duration {
        self.earliest_ask.saturating_sub(since_last_frame)
    }
}

/// Smooths frame intervals into a frames per second number that doesn't jitter in `status`.
#[derive(Default)]
pub struct FrameRateMeter {
    frames_per_second: f64,
    last_frame: Option<Instant>,
}

impl FrameRateMeter {
    const SMOOTHING: f64 = 0.1;

    pub fn frames_per_second(&self) -> f64 {
        self.frames_per_second
    }

    pub fn frame_arrived(&mut self, at: Instant) {
        if let Some(last) = self.last_frame.replace(at) {
            let interval = (at - last).as_secs_f64();
            if interval > 0.0 {
                let instant = 1.0 / interval;
                self.frames_per_second = if self.frames_per_second == 0.0 {
                    instant
                } else {
                    self.frames_per_second + (instant - self.frames_per_second) * Self::SMOOTHING
                };
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Fires at most once per interval. Used for event polling, keep alive and card refreshes.
pub struct Every {
    interval: Duration,
    last_fired: Option<Instant>,
}

impl Every {
    pub fn new(interval: Duration) -> Self {
        Self { interval, last_fired: None }
    }

    pub fn is_due(&mut self, at: Instant) -> bool {
        if self.last_fired.is_some_and(|last| at - last < self.interval) {
            return false;
        }
        self.last_fired = Some(at);
        true
    }

    pub fn reset(&mut self) {
        self.last_fired = None;
    }
}

/// Opt in timing breakdown (CANON_WEBCAM_DEBUG=1) for when frames feel slow.
pub struct TimingStats {
    pub enabled: bool,
    totals: BTreeMap<&'static str, (Duration, u32)>,
    report: Every,
}

impl TimingStats {
    pub fn new(enabled: bool) -> Self {
        let mut report = Every::new(Duration::from_secs(5));
        report.is_due(Instant::now());
        Self { enabled, totals: BTreeMap::new(), report }
    }

    pub fn record(&mut self, bucket: &'static str, since: Instant) {
        if self.enabled {
            let entry = self.totals.entry(bucket).or_default();
            entry.0 += since.elapsed();
            entry.1 += 1;
        }
    }

    pub fn count(&mut self, bucket: &'static str) {
        if self.enabled {
            self.totals.entry(bucket).or_default().1 += 1;
        }
    }

    pub fn summary_if_due(&mut self, at: Instant) -> Option<String> {
        if !self.enabled || !self.report.is_due(at) {
            return None;
        }
        let parts: Vec<String> = std::mem::take(&mut self.totals)
            .into_iter()
            .map(|(bucket, (total, count))| {
                if total.is_zero() {
                    format!("{bucket} x{count}")
                } else {
                    format!("{bucket} x{count} avg {:.1}ms", total.as_secs_f64() * 1000.0 / count as f64)
                }
            })
            .collect();
        Some(format!("timing: {}", parts.join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pacer_waits_out_the_rest_of_the_early_window() {
        let pacer = FramePacer::default();
        assert_eq!(pacer.wait_before_asking(Duration::from_millis(5)), Duration::from_millis(15));
        assert_eq!(pacer.wait_before_asking(Duration::from_millis(50)), Duration::ZERO);
    }

    #[test]
    fn the_meter_settles_on_the_real_rate() {
        let mut meter = FrameRateMeter::default();
        let start = Instant::now();
        for frame in 0..200 {
            meter.frame_arrived(start + Duration::from_micros(41_667 * frame));
        }
        assert!((meter.frames_per_second() - 24.0).abs() < 0.1, "{}", meter.frames_per_second());
    }

    #[test]
    fn every_fires_once_per_interval() {
        let mut every = Every::new(Duration::from_secs(1));
        let start = Instant::now();
        assert!(every.is_due(start));
        assert!(!every.is_due(start + Duration::from_millis(500)));
        assert!(every.is_due(start + Duration::from_millis(1100)));
    }
}
