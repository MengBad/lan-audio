//! Kalman + PID audio sync prediction engine.
//!
//! This module owns the closed-loop control that compensates clock drift
//! between the Windows capture clock and the Android playback clock. The
//! algorithm operates on water-mark feedback reported by the client over the
//! TCP control channel.
//!
//! Pipeline (see `docs/roadmap-v1.4.md` Module 2):
//!
//! ```text
//!     client_buffer_ms  ──► Kalman Filter ──► predicted_buffer_ms
//!                                              │
//!                                              ▼
//!                                       PID Controller
//!                                              │
//!                                              ▼
//!                              resample_offset_mhz  (±20 mHz limit)
//! ```
//!
//! All public types are deterministic and free of external state, so the
//! engine can be exercised from unit tests without a network or audio device.

use std::time::Duration;

/// Hard cap on the resample offset emitted by the engine. Anything beyond
/// roughly ±0.04% (±20 mHz at 48 kHz) starts to introduce audible pitch
/// changes, so the PID output is clipped to this band.
pub const MAX_RESAMPLE_OFFSET_MHZ: f64 = 20.0;

/// Hard cap on the integrator. Without anti-windup the integral term can
/// accumulate during long buffer-empty periods and overshoot when the
/// connection recovers.
const PID_INTEGRAL_CLAMP_MHZ: f64 = 50.0;

/// Bang-bang threshold. When the absolute error exceeds this value the PID is
/// bypassed and the output is forced to its limit so the buffer is pulled back
/// quickly. This is necessary during long underruns where the integrator alone
/// would take many seconds to react.
const BANG_BANG_THRESHOLD_MS: f64 = 30.0;

/// Dead-zone around zero error. Inside this band the controller emits zero so
/// repeated micro-adjustments do not chase steady-state jitter.
const DEAD_ZONE_MS: f64 = 5.0;

/// 2-state Kalman filter that estimates `[buffer_level_ms, drift_rate_ms_per_sec]`.
///
/// Tuning notes:
/// * `process_noise_buffer` — how quickly the buffer level can change without
///   us trusting the measurement less.
/// * `process_noise_drift` — how quickly we believe the drift rate itself can
///   change. Should be small (drift is a slowly varying property of the
///   system).
#[derive(Debug, Clone, Copy)]
pub struct BufferKalmanFilter {
    // State: x = [buffer_ms, drift_rate_ms_per_s]
    pub buffer_ms: f64,
    pub drift_rate: f64,
    // Covariance matrix (2x2 symmetric, stored as flat array)
    p00: f64,
    p01: f64,
    p11: f64,
    pub process_noise_buffer: f64,
    pub process_noise_drift: f64,
    pub measurement_noise: f64,
}

impl BufferKalmanFilter {
    pub fn new(initial_buffer_ms: f64) -> Self {
        Self {
            buffer_ms: initial_buffer_ms,
            drift_rate: 0.0,
            p00: 100.0,
            p01: 0.0,
            p11: 1.0,
            process_noise_buffer: 0.5,
            process_noise_drift: 0.01,
            measurement_noise: 4.0,
        }
    }

    /// Adapt observation noise based on jitter quality. High jitter networks
    /// should trust the model more than the noisy single-shot measurement.
    pub fn set_jitter_p95_us(&mut self, jitter_p95_us: u32) {
        // 4ms² baseline (R) + scaled jitter contribution
        let jitter_ms = jitter_p95_us as f64 / 1000.0;
        self.measurement_noise = 4.0 + jitter_ms * jitter_ms * 0.25;
    }

    /// Run one predict + update step.
    ///
    /// `dt_seconds` should be the wall clock interval between successive
    /// observations (typically 1.0). `measurement_ms` is the observed buffer
    /// level reported by the client.
    pub fn step(&mut self, dt_seconds: f64, measurement_ms: f64) {
        // ---- predict ----
        // x = F * x, F = [[1, dt], [0, 1]]
        self.buffer_ms += self.drift_rate * dt_seconds;
        // drift_rate unchanged

        // P = F P F^T + Q
        // F P F^T:
        //   p00' = p00 + 2*dt*p01 + dt^2*p11
        //   p01' = p01 + dt*p11
        //   p11' = p11
        let dt = dt_seconds;
        let p00_new = self.p00 + 2.0 * dt * self.p01 + dt * dt * self.p11;
        let p01_new = self.p01 + dt * self.p11;
        let p11_new = self.p11;
        self.p00 = p00_new + self.process_noise_buffer;
        self.p01 = p01_new;
        self.p11 = p11_new + self.process_noise_drift;

        // ---- update ----
        // H = [1, 0], so innovation = z - x[0], S = p00 + R
        let s = self.p00 + self.measurement_noise;
        if s.abs() < 1e-9 {
            return;
        }
        let k0 = self.p00 / s;
        let k1 = self.p01 / s;
        let innovation = measurement_ms - self.buffer_ms;
        self.buffer_ms += k0 * innovation;
        self.drift_rate += k1 * innovation;

        // P = (I - K H) P
        // (I - K H) = [[1-k0, 0],[-k1, 1]]
        let p00 = (1.0 - k0) * self.p00;
        let p01 = (1.0 - k0) * self.p01;
        let p11 = -k1 * self.p01 + self.p11;
        self.p00 = p00;
        self.p01 = p01;
        self.p11 = p11;
    }

    /// Predict the buffer level `horizon_seconds` into the future.
    pub fn predict(&self, horizon_seconds: f64) -> f64 {
        self.buffer_ms + self.drift_rate * horizon_seconds
    }
}

/// PID controller specialised for buffer-level → resample-offset control.
#[derive(Debug, Clone, Copy)]
pub struct ResamplePidController {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    pub target_buffer_ms: f64,
    integral: f64,
    last_error: Option<f64>,
    last_d_filtered: f64,
    /// Low-pass coefficient applied to the derivative term to reduce noise
    /// amplification (alpha closer to 1.0 = more smoothing).
    pub d_filter_alpha: f64,
    pub integral_frozen: bool,
}

impl ResamplePidController {
    pub fn new(target_buffer_ms: f64) -> Self {
        Self {
            kp: 0.30,
            ki: 0.02,
            kd: 0.10,
            target_buffer_ms,
            integral: 0.0,
            last_error: None,
            last_d_filtered: 0.0,
            d_filter_alpha: 0.6,
            integral_frozen: false,
        }
    }

    /// Apply degraded-mode tuning. When the system is under load we freeze the
    /// integrator and lower Kp so the controller does not amplify timing
    /// noise, but we keep emitting (so static drift compensation stays alive).
    pub fn set_yellow(&mut self) {
        self.integral_frozen = true;
        self.kp = 0.20;
    }

    pub fn set_green(&mut self) {
        self.integral_frozen = false;
        self.kp = 0.30;
    }

    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.last_error = None;
        self.last_d_filtered = 0.0;
    }

    /// Compute the resample offset (in mHz) for a single control step.
    pub fn step(&mut self, predicted_buffer_ms: f64, dt_seconds: f64) -> f64 {
        let error = self.target_buffer_ms - predicted_buffer_ms;

        // Bang-bang outside the linear band.
        if error.abs() > BANG_BANG_THRESHOLD_MS {
            self.last_error = Some(error);
            // Maintain integrator during bang-bang to avoid windup, but only
            // accumulate inside a sane band.
            return MAX_RESAMPLE_OFFSET_MHZ.copysign(error);
        }

        // Dead zone — no micro adjustments around the setpoint.
        if error.abs() < DEAD_ZONE_MS {
            self.last_error = Some(error);
            self.last_d_filtered *= 1.0 - self.d_filter_alpha;
            return 0.0;
        }

        if !self.integral_frozen {
            self.integral += error * dt_seconds;
            self.integral = self
                .integral
                .clamp(-PID_INTEGRAL_CLAMP_MHZ, PID_INTEGRAL_CLAMP_MHZ);
        }

        let derivative_raw = match self.last_error {
            Some(prev) => (error - prev) / dt_seconds.max(1e-6),
            None => 0.0,
        };
        // Low-pass on the derivative term.
        self.last_d_filtered = self.d_filter_alpha * derivative_raw
            + (1.0 - self.d_filter_alpha) * self.last_d_filtered;
        self.last_error = Some(error);

        let raw = self.kp * error + self.ki * self.integral + self.kd * self.last_d_filtered;
        raw.clamp(-MAX_RESAMPLE_OFFSET_MHZ, MAX_RESAMPLE_OFFSET_MHZ)
    }
}

/// One observation reported by the Android client over the control channel.
#[derive(Debug, Clone, Copy)]
pub struct WatermarkObservation {
    pub jitter_buf_ms: u32,
    pub ring_buf_ms: u32,
    pub silence_fill_delta: u32,
    pub underrun_delta: u32,
    pub jitter_p95_us: u32,
}

impl WatermarkObservation {
    pub fn total_buffer_ms(&self) -> f64 {
        (self.jitter_buf_ms as f64) + (self.ring_buf_ms as f64)
    }
}

/// Top-level engine that wraps the Kalman + PID pair and exposes the
/// resample offset to the rest of the server.
pub struct AudioSyncEngine {
    pub kalman: BufferKalmanFilter,
    pub pid: ResamplePidController,
    pub forecast_horizon_seconds: f64,
    last_offset_mhz: f64,
}

impl AudioSyncEngine {
    pub fn new(target_buffer_ms: f64, initial_buffer_ms: f64) -> Self {
        Self {
            kalman: BufferKalmanFilter::new(initial_buffer_ms),
            pid: ResamplePidController::new(target_buffer_ms),
            forecast_horizon_seconds: 2.0,
            last_offset_mhz: 0.0,
        }
    }

    /// Apply a single observation and update the controller. Returns the
    /// offset that should be applied to the resample ratio (in mHz, can be
    /// negative).
    pub fn observe(
        &mut self,
        observation: WatermarkObservation,
        dt: Duration,
    ) -> AudioSyncDecision {
        let dt_secs = dt.as_secs_f64().max(0.05);
        self.kalman.set_jitter_p95_us(observation.jitter_p95_us);
        self.kalman.step(dt_secs, observation.total_buffer_ms());

        let predicted = self.kalman.predict(self.forecast_horizon_seconds);
        let offset = self.pid.step(predicted, dt_secs);
        self.last_offset_mhz = offset;

        AudioSyncDecision {
            estimated_buffer_ms: self.kalman.buffer_ms,
            estimated_drift_rate_ms_per_s: self.kalman.drift_rate,
            predicted_buffer_ms: predicted,
            resample_offset_mhz: offset,
        }
    }

    pub fn last_offset_mhz(&self) -> f64 {
        self.last_offset_mhz
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AudioSyncDecision {
    pub estimated_buffer_ms: f64,
    pub estimated_drift_rate_ms_per_s: f64,
    pub predicted_buffer_ms: f64,
    pub resample_offset_mhz: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(buffer_ms: u32) -> WatermarkObservation {
        WatermarkObservation {
            jitter_buf_ms: buffer_ms,
            ring_buf_ms: 0,
            silence_fill_delta: 0,
            underrun_delta: 0,
            jitter_p95_us: 1500,
        }
    }

    #[test]
    fn kalman_converges_on_constant_observations() {
        let mut k = BufferKalmanFilter::new(0.0);
        for _ in 0..30 {
            k.step(1.0, 100.0);
        }
        assert!((k.buffer_ms - 100.0).abs() < 1.0);
        // Drift rate should remain near 0 for a constant signal.
        assert!(k.drift_rate.abs() < 0.5);
    }

    #[test]
    fn kalman_estimates_positive_drift_rate() {
        let mut k = BufferKalmanFilter::new(80.0);
        // Buffer growing 1ms/s over 30 samples.
        for i in 0..30 {
            let v = 80.0 + i as f64 * 1.0;
            k.step(1.0, v);
        }
        assert!(k.drift_rate > 0.5, "drift={}", k.drift_rate);
        assert!(k.drift_rate < 1.5, "drift={}", k.drift_rate);
    }

    #[test]
    fn pid_dead_zone_returns_zero() {
        let mut pid = ResamplePidController::new(100.0);
        // 2ms below target — inside dead zone.
        let out = pid.step(102.0, 1.0);
        assert_eq!(out, 0.0);
    }

    #[test]
    fn pid_clamps_to_max_offset_under_bang_bang() {
        let mut pid = ResamplePidController::new(100.0);
        // 50ms below target → bang-bang should drive to +max.
        let out = pid.step(50.0, 1.0);
        assert!((out - MAX_RESAMPLE_OFFSET_MHZ).abs() < 1e-6);
    }

    #[test]
    fn pid_emits_negative_offset_when_buffer_overshoots() {
        let mut pid = ResamplePidController::new(100.0);
        // Buffer too large → predicted 130ms → bang-bang to -max.
        let out = pid.step(130.0, 1.0);
        assert!(out < 0.0);
    }

    #[test]
    fn engine_keeps_offset_within_limits() {
        let mut engine = AudioSyncEngine::new(100.0, 50.0);
        for _ in 0..20 {
            let decision = engine.observe(obs(40), Duration::from_secs(1));
            assert!(
                decision.resample_offset_mhz.abs() <= MAX_RESAMPLE_OFFSET_MHZ + 1e-6,
                "offset {} exceeded limit",
                decision.resample_offset_mhz
            );
        }
    }

    #[test]
    fn engine_eventually_zeroes_offset_when_at_target() {
        let mut engine = AudioSyncEngine::new(100.0, 100.0);
        for _ in 0..15 {
            engine.observe(obs(100), Duration::from_secs(1));
        }
        let decision = engine.observe(obs(100), Duration::from_secs(1));
        assert!(decision.resample_offset_mhz.abs() < 0.5);
    }

    #[test]
    fn yellow_mode_freezes_integrator() {
        let mut pid = ResamplePidController::new(100.0);
        for _ in 0..10 {
            pid.step(85.0, 1.0); // Persistent error grows the integrator.
        }
        let baseline_int = pid.integral;
        pid.set_yellow();
        for _ in 0..10 {
            pid.step(85.0, 1.0);
        }
        assert!(
            (pid.integral - baseline_int).abs() < 1e-6,
            "integrator changed after freeze: {} vs {}",
            pid.integral,
            baseline_int
        );
    }
}
