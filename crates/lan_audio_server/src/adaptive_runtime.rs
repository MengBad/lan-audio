//! Adaptive runtime coordinator.
//!
//! Glues the watchdog ([`crate::watchdog`]) and the audio sync engine
//! ([`crate::sync_engine`]) together behind a single API so the transport
//! layer can drive them with one tick per cadence.
//!
//! The coordinator is kept intentionally small and side-effect free: it
//! consumes raw signals (CPU %, encode queue depth, client watermark
//! reports) and produces a [`AdaptiveDecision`] that the transport layer is
//! free to ignore or apply. This keeps the policy decoupled from the
//! mechanism so we can unit-test it without spinning up an audio pipeline.

use std::time::Duration;

use lan_audio_protocol::AudioMode;

use crate::cpu_sampler::ProcessCpuSampler;
use crate::sync_engine::{AudioSyncEngine, WatermarkObservation};
use crate::watchdog::{CpuWatchdog, DegradationTier, PressureSignals, WatchdogConfig};

/// One full snapshot of the adaptive coordinator. Returned each tick.
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveDecision {
    pub tier: DegradationTier,
    pub previous_tier: DegradationTier,
    pub predicted_cpu: f64,
    pub predicted_buffer_ms: f64,
    pub estimated_drift_rate_ms_per_s: f64,
    pub resample_offset_mhz: f64,
}

/// Tier-specific encoder parameters. The transport layer can read these and
/// reconfigure the Opus encoder + buffer policy accordingly. Defaults map to
/// the existing v1.8 behaviour.
#[derive(Debug, Clone, Copy)]
pub struct TierEncoderProfile {
    pub bitrate_bps: i32,
    pub complexity: i32,
    pub use_vbr: bool,
    pub force_pcm16_fallback: bool,
}

pub fn tier_encoder_profile(tier: DegradationTier, mode: AudioMode) -> TierEncoderProfile {
    // Baseline = the per-mode defaults that existed before adaptive degrade.
    let baseline = match mode {
        AudioMode::UltraLowLatency => TierEncoderProfile {
            bitrate_bps: 48_000,
            complexity: 0,
            use_vbr: false,
            force_pcm16_fallback: true,
        },
        AudioMode::LowLatency => TierEncoderProfile {
            bitrate_bps: 64_000,
            complexity: 1,
            use_vbr: false,
            force_pcm16_fallback: false,
        },
        AudioMode::Balanced => TierEncoderProfile {
            bitrate_bps: 96_000,
            complexity: 2,
            use_vbr: true,
            force_pcm16_fallback: false,
        },
        AudioMode::HighQuality => TierEncoderProfile {
            bitrate_bps: 128_000,
            complexity: 4,
            use_vbr: true,
            force_pcm16_fallback: false,
        },
    };
    match tier {
        DegradationTier::Green => baseline,
        DegradationTier::Yellow => TierEncoderProfile {
            bitrate_bps: baseline.bitrate_bps.min(96_000),
            complexity: baseline.complexity.min(2),
            use_vbr: false,
            force_pcm16_fallback: false,
        },
        DegradationTier::Red => TierEncoderProfile {
            bitrate_bps: 64_000,
            complexity: 1,
            use_vbr: false,
            // Under extreme load fall back to PCM16 — its CPU cost is fixed
            // and predictable, eliminating the encode-queue backpressure
            // that pushed us into Red in the first place.
            force_pcm16_fallback: true,
        },
    }
}

pub struct AdaptiveRuntime {
    cpu_sampler: ProcessCpuSampler,
    watchdog: CpuWatchdog,
    sync_engine: AudioSyncEngine,
    cadence: Duration,
    last_observed_tier: DegradationTier,
}

impl AdaptiveRuntime {
    pub fn new(
        watchdog_cfg: WatchdogConfig,
        forecast_horizon_seconds: f64,
        target_buffer_ms: f64,
        initial_buffer_ms: f64,
        cadence: Duration,
    ) -> Self {
        Self {
            cpu_sampler: ProcessCpuSampler::new(),
            watchdog: CpuWatchdog::new(watchdog_cfg, forecast_horizon_seconds),
            sync_engine: AudioSyncEngine::new(target_buffer_ms, initial_buffer_ms),
            cadence,
            last_observed_tier: DegradationTier::Green,
        }
    }

    /// Push the latest queue + drop signals and (optionally) a fresh client
    /// watermark report. Returns the resulting decision.
    pub fn tick(
        &mut self,
        queue_depth: u32,
        queue_capacity: u32,
        consecutive_dropped_frames: u32,
        watermark: Option<WatermarkObservation>,
    ) -> AdaptiveDecision {
        let cpu = self.cpu_sampler.sample().map(|s| s.percent).unwrap_or(0.0);
        let signals = PressureSignals {
            current_cpu_percent: cpu,
            consecutive_dropped_frames,
            encode_queue_depth: queue_depth,
            encode_queue_capacity: queue_capacity,
        };
        let tick = self.watchdog.tick(signals, self.cadence);

        // Sync the PID frozen-integrator flag with the current tier.
        match tick.tier {
            DegradationTier::Green => self.sync_engine.pid.set_green(),
            DegradationTier::Yellow | DegradationTier::Red => {
                self.sync_engine.pid.set_yellow();
            }
        }

        let mut sync_decision = None;
        if let Some(obs) = watermark {
            sync_decision = Some(self.sync_engine.observe(obs, self.cadence));
        }
        self.last_observed_tier = tick.tier;

        AdaptiveDecision {
            tier: tick.tier,
            previous_tier: tick.previous_tier,
            predicted_cpu: tick.predicted_cpu,
            predicted_buffer_ms: sync_decision
                .map(|d| d.predicted_buffer_ms)
                .unwrap_or_else(|| self.sync_engine.kalman.buffer_ms),
            estimated_drift_rate_ms_per_s: sync_decision
                .map(|d| d.estimated_drift_rate_ms_per_s)
                .unwrap_or(self.sync_engine.kalman.drift_rate),
            resample_offset_mhz: sync_decision
                .map(|d| d.resample_offset_mhz)
                .unwrap_or(self.sync_engine.last_offset_mhz()),
        }
    }

    pub fn current_tier(&self) -> DegradationTier {
        self.last_observed_tier
    }

    pub fn forecast_cpu(&self) -> f64 {
        self.watchdog.forecast_cpu()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_returns_green_with_idle_signals() {
        let mut rt = AdaptiveRuntime::new(
            WatchdogConfig::default(),
            3.0,
            100.0,
            100.0,
            Duration::from_millis(500),
        );
        // Two idle ticks to seed the CPU sampler.
        rt.tick(0, 64, 0, None);
        let d = rt.tick(0, 64, 0, None);
        assert_eq!(d.tier, DegradationTier::Green);
        // No watermark observation → resample offset stays at zero.
        assert!(d.resample_offset_mhz.abs() < 1e-6);
    }

    #[test]
    fn adaptive_demotes_on_drop_streak() {
        let mut rt = AdaptiveRuntime::new(
            WatchdogConfig::default(),
            3.0,
            100.0,
            100.0,
            Duration::from_millis(500),
        );
        let d = rt.tick(0, 64, 5, None);
        assert_eq!(d.tier, DegradationTier::Red);
    }

    #[test]
    fn yellow_tier_uses_lower_bitrate() {
        let g = tier_encoder_profile(DegradationTier::Green, AudioMode::HighQuality);
        let y = tier_encoder_profile(DegradationTier::Yellow, AudioMode::HighQuality);
        let r = tier_encoder_profile(DegradationTier::Red, AudioMode::HighQuality);
        assert_eq!(g.bitrate_bps, 128_000);
        assert!(y.bitrate_bps <= g.bitrate_bps);
        assert!(r.bitrate_bps <= y.bitrate_bps);
        assert!(r.force_pcm16_fallback);
        assert!(!g.force_pcm16_fallback);
    }
}
