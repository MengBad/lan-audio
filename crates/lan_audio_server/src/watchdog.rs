//! CPU pressure watchdog with predictive degradation.
//!
//! Polls process CPU load (and externally-supplied frame-drop / queue
//! depth signals) at a fixed cadence and chooses a degradation tier
//! (Green / Yellow / Red) based on **predicted** future load. The
//! prediction is an EWMA of the current sample plus a linear trend
//! extrapolated by `forecast_horizon_seconds`, which lets the server
//! enter a lower-cost mode before the user actually hears stutter.
//!
//! This module owns no timers and no audio device handles — it is a
//! pure decision engine that the transport layer drives once per
//! sampling tick. That keeps it trivially testable.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradationTier {
    Green,
    Yellow,
    Red,
}

impl DegradationTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Red => "red",
        }
    }
}

/// Smoothed predictor of process CPU load.
///
/// `ewma` follows recent samples with smoothing factor `α=0.3`.
/// `trend` is a smoothed first-difference, also α=0.3.
/// The forecast at horizon `h` is `ewma + trend * h`.
#[derive(Debug, Clone, Copy)]
pub struct CpuPredictor {
    pub ewma: f64,
    pub trend: f64,
    pub last_sample: f64,
    pub forecast_horizon_seconds: f64,
    pub alpha_ewma: f64,
    pub alpha_trend: f64,
}

impl CpuPredictor {
    pub fn new(forecast_horizon_seconds: f64) -> Self {
        Self {
            ewma: 0.0,
            trend: 0.0,
            last_sample: 0.0,
            forecast_horizon_seconds,
            alpha_ewma: 0.3,
            alpha_trend: 0.3,
        }
    }

    pub fn observe(&mut self, current_cpu: f64) -> f64 {
        let delta = current_cpu - self.last_sample;
        self.trend = (1.0 - self.alpha_trend) * self.trend + self.alpha_trend * delta;
        self.ewma = (1.0 - self.alpha_ewma) * self.ewma + self.alpha_ewma * current_cpu;
        self.last_sample = current_cpu;
        self.forecast()
    }

    pub fn forecast(&self) -> f64 {
        (self.ewma + self.trend * self.forecast_horizon_seconds).clamp(0.0, 100.0)
    }
}

/// Pressure indicators that flow into the tier decision.
#[derive(Debug, Clone, Copy, Default)]
pub struct PressureSignals {
    pub current_cpu_percent: f64,
    pub consecutive_dropped_frames: u32,
    pub encode_queue_depth: u32,
    pub encode_queue_capacity: u32,
}

impl PressureSignals {
    pub fn queue_fill_ratio(&self) -> f64 {
        if self.encode_queue_capacity == 0 {
            return 0.0;
        }
        (self.encode_queue_depth as f64) / (self.encode_queue_capacity as f64)
    }
}

/// Configurable thresholds used by the watchdog state machine.
#[derive(Debug, Clone, Copy)]
pub struct WatchdogConfig {
    /// Predicted CPU above which we leave Green.
    pub yellow_cpu_threshold: f64,
    /// Predicted CPU above which we leave Yellow.
    pub red_cpu_threshold: f64,
    /// Frame-drop streak that forces immediate Red.
    pub red_drop_streak: u32,
    /// Queue fill ratio that forces Yellow.
    pub yellow_queue_ratio: f64,
    /// Queue fill ratio that forces Red.
    pub red_queue_ratio: f64,
    /// How long the system must look healthy before we promote a tier.
    pub upgrade_hold: Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            yellow_cpu_threshold: 60.0,
            red_cpu_threshold: 85.0,
            red_drop_streak: 3,
            yellow_queue_ratio: 0.5,
            red_queue_ratio: 0.85,
            upgrade_hold: Duration::from_secs(3),
        }
    }
}

/// Predictive watchdog. Hold one of these per server instance and pulse it
/// every ~500 ms with the latest signals.
pub struct CpuWatchdog {
    cfg: WatchdogConfig,
    predictor: CpuPredictor,
    tier: DegradationTier,
    /// Wall-clock-equivalent counter (ticks * cadence) tracking how long the
    /// system has continuously looked good enough to promote.
    healthy_streak: Duration,
}

impl CpuWatchdog {
    pub fn new(cfg: WatchdogConfig, forecast_horizon_seconds: f64) -> Self {
        Self {
            cfg,
            predictor: CpuPredictor::new(forecast_horizon_seconds),
            tier: DegradationTier::Green,
            healthy_streak: Duration::from_secs(0),
        }
    }

    pub fn tier(&self) -> DegradationTier {
        self.tier
    }

    pub fn forecast_cpu(&self) -> f64 {
        self.predictor.forecast()
    }

    /// Run one tick. Returns the new tier — caller is responsible for
    /// applying its side effects (codec settings, buffer targets, etc).
    pub fn tick(&mut self, signals: PressureSignals, cadence: Duration) -> WatchdogTickResult {
        let predicted_cpu = self.predictor.observe(signals.current_cpu_percent);
        let queue_ratio = signals.queue_fill_ratio();
        let desired = self.compute_desired_tier(predicted_cpu, queue_ratio, signals);

        let previous = self.tier;
        match (previous, desired) {
            // Degradation is always immediate.
            (a, b) if Self::is_demotion(a, b) => {
                self.tier = b;
                self.healthy_streak = Duration::from_secs(0);
            }
            // Promotion requires a hold window.
            (a, b) if Self::is_promotion(a, b) => {
                self.healthy_streak = self.healthy_streak.saturating_add(cadence);
                if self.healthy_streak >= self.cfg.upgrade_hold {
                    self.tier = b;
                    self.healthy_streak = Duration::from_secs(0);
                }
            }
            // Steady state.
            _ => {
                if previous == DegradationTier::Green {
                    self.healthy_streak = Duration::from_secs(0);
                } else {
                    // Already at the level matching `desired`, accumulate so
                    // future promotions can fire.
                    self.healthy_streak = self.healthy_streak.saturating_add(cadence);
                }
            }
        }

        WatchdogTickResult {
            previous_tier: previous,
            tier: self.tier,
            predicted_cpu,
            healthy_streak: self.healthy_streak,
        }
    }

    fn compute_desired_tier(
        &self,
        predicted_cpu: f64,
        queue_ratio: f64,
        signals: PressureSignals,
    ) -> DegradationTier {
        if predicted_cpu >= self.cfg.red_cpu_threshold
            || signals.consecutive_dropped_frames >= self.cfg.red_drop_streak
            || queue_ratio >= self.cfg.red_queue_ratio
        {
            return DegradationTier::Red;
        }
        if predicted_cpu >= self.cfg.yellow_cpu_threshold
            || queue_ratio >= self.cfg.yellow_queue_ratio
        {
            return DegradationTier::Yellow;
        }
        DegradationTier::Green
    }

    fn is_demotion(from: DegradationTier, to: DegradationTier) -> bool {
        Self::tier_index(to) > Self::tier_index(from)
    }

    fn is_promotion(from: DegradationTier, to: DegradationTier) -> bool {
        Self::tier_index(to) < Self::tier_index(from)
    }

    fn tier_index(t: DegradationTier) -> u8 {
        match t {
            DegradationTier::Green => 0,
            DegradationTier::Yellow => 1,
            DegradationTier::Red => 2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WatchdogTickResult {
    pub previous_tier: DegradationTier,
    pub tier: DegradationTier,
    pub predicted_cpu: f64,
    pub healthy_streak: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals(cpu: f64) -> PressureSignals {
        PressureSignals {
            current_cpu_percent: cpu,
            consecutive_dropped_frames: 0,
            encode_queue_depth: 0,
            encode_queue_capacity: 64,
        }
    }

    #[test]
    fn predictor_extrapolates_rising_trend() {
        let mut p = CpuPredictor::new(3.0);
        for i in 0..10 {
            p.observe(40.0 + i as f64 * 2.0);
        }
        // Trend is positive → forecast is above current sample.
        assert!(p.forecast() > p.last_sample);
    }

    #[test]
    fn watchdog_demotes_immediately_under_load() {
        let mut wd = CpuWatchdog::new(WatchdogConfig::default(), 3.0);
        // First tick at high CPU should jump straight to Yellow at minimum.
        let r = wd.tick(signals(75.0), Duration::from_millis(500));
        assert_ne!(r.tier, DegradationTier::Green);
    }

    #[test]
    fn watchdog_drops_to_red_on_streak_drops() {
        let mut wd = CpuWatchdog::new(WatchdogConfig::default(), 3.0);
        let mut s = signals(20.0);
        s.consecutive_dropped_frames = 5;
        let r = wd.tick(s, Duration::from_millis(500));
        assert_eq!(r.tier, DegradationTier::Red);
    }

    #[test]
    fn watchdog_promotes_only_after_hold_window() {
        let mut wd = CpuWatchdog::new(WatchdogConfig::default(), 3.0);
        // Push to Red.
        wd.tick(signals(95.0), Duration::from_millis(500));
        assert_eq!(wd.tier(), DegradationTier::Red);

        // Now CPU is fine, but we need the upgrade hold (3s) before any
        // promotion takes effect.
        let cadence = Duration::from_millis(500);
        // 2s of healthy ticks — still Red.
        for _ in 0..4 {
            wd.tick(signals(10.0), cadence);
        }
        assert_eq!(wd.tier(), DegradationTier::Red);

        // 3rd second triggers promotion (one tier step).
        wd.tick(signals(10.0), cadence);
        wd.tick(signals(10.0), cadence);
        // After enough healthy time it should land at Green eventually.
        for _ in 0..20 {
            wd.tick(signals(10.0), cadence);
        }
        assert_eq!(wd.tier(), DegradationTier::Green);
    }
}
