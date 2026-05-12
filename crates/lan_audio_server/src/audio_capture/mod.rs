//! Audio capture source abstraction for server-side media input.
//!
//! This module defines the pluggable media input interface used by the desktop
//! service before encoding/packetization. The default internal format is:
//! - sample rate: 48kHz
//! - channels: stereo (2)
//! - sample format: f32
//! - frame duration: 10ms
//!
//! Current stage:
//! - `SyntheticAudioSource` is production-ready for MVP debug flow.
//! - `WindowsLoopbackCapture` has a real initialization/read attempt path on Windows,
//!   but still requires more real-machine validation and format/clock hardening.

use std::f32::consts::PI;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use thiserror::Error;
use tokio::time::{sleep_until, Instant};

pub mod pcm_accumulator;

#[cfg(target_os = "windows")]
#[path = "windows_loopback_windows.rs"]
mod windows_loopback_impl;
#[cfg(not(target_os = "windows"))]
#[path = "windows_loopback_non_windows.rs"]
mod windows_loopback_impl;

use pcm_accumulator::compute_peak_rms;
pub use pcm_accumulator::PacketKind;
pub use windows_loopback_impl::WindowsLoopbackCapture;

#[derive(Debug, Clone)]
pub struct CaptureDebugDumpConfig {
    pub enabled: bool,
    pub seconds: u32,
    pub output_dir: String,
}

/// PCM sample storage type for captured audio frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// 32-bit float little-endian, normalized to [-1.0, 1.0].
    F32,
}

/// Capture lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSourceState {
    Created,
    DeviceResolved,
    ClientInitialized,
    Started,
    Stopped,
    Failed,
}

impl CaptureSourceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::DeviceResolved => "device_resolved",
            Self::ClientInitialized => "client_initialized",
            Self::Started => "started",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }
}

/// Media format metadata associated with each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub sample_format: SampleFormat,
    pub frame_duration_ms: u16,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: SampleFormat::F32,
            frame_duration_ms: 10,
        }
    }
}

impl AudioFormat {
    pub fn samples_per_channel_per_frame(&self) -> usize {
        (self.sample_rate_hz as usize * self.frame_duration_ms as usize) / 1000
    }

    pub fn total_samples_per_frame(&self) -> usize {
        self.samples_per_channel_per_frame() * self.channels as usize
    }
}

/// Single captured audio frame from the source interface.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub pts_ms: u64,
    pub format: AudioFormat,
    pub samples_f32: Vec<f32>,
    pub is_silence: bool,
    pub source_buffer_frames: Option<u32>,
    pub packet_kind: PacketKind,
    pub peak: f32,
    pub rms: f32,
}

impl AudioFrame {
    pub fn new(
        pts_ms: u64,
        format: AudioFormat,
        samples_f32: Vec<f32>,
    ) -> Result<Self, CaptureError> {
        Self::new_with_meta(
            pts_ms,
            format,
            samples_f32,
            false,
            None,
            PacketKind::Synthetic,
            0.0,
            0.0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_meta(
        pts_ms: u64,
        format: AudioFormat,
        samples_f32: Vec<f32>,
        is_silence: bool,
        source_buffer_frames: Option<u32>,
        packet_kind: PacketKind,
        peak: f32,
        rms: f32,
    ) -> Result<Self, CaptureError> {
        if samples_f32.len() != format.total_samples_per_frame() {
            return Err(CaptureError::InvalidFrameLength {
                expected: format.total_samples_per_frame(),
                actual: samples_f32.len(),
            });
        }
        Ok(Self {
            pts_ms,
            format,
            samples_f32,
            is_silence,
            source_buffer_frames,
            packet_kind,
            peak,
            rms,
        })
    }

    pub fn silence(pts_ms: u64, format: AudioFormat) -> Self {
        Self {
            pts_ms,
            format,
            samples_f32: vec![0.0; format.total_samples_per_frame()],
            is_silence: true,
            source_buffer_frames: None,
            packet_kind: PacketKind::SilentPacket,
            peak: 0.0,
            rms: 0.0,
        }
    }
}

/// Unified error for capture source initialization and frame reads.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CaptureError {
    #[error("capture source is not started")]
    NotStarted,
    #[error("default output device not found")]
    DefaultDeviceNotFound,
    #[error("device activation failed: {0}")]
    DeviceActivationFailed(String),
    #[error("audio client init failed: {0}")]
    AudioClientInitFailed(String),
    #[error("capture client init failed: {0}")]
    CaptureClientInitFailed(String),
    #[error("unsupported mix format: {0}")]
    UnsupportedMixFormat(String),
    #[error("capture start failed: {0}")]
    StartFailed(String),
    #[error("capture read buffer failed: {0}")]
    ReadBufferFailed(String),
    #[error("unsupported platform: {0}")]
    UnsupportedPlatform(String),
    #[error("invalid frame length, expected={expected}, actual={actual}")]
    InvalidFrameLength { expected: usize, actual: usize },
    #[error("capture source not implemented: {0}")]
    NotImplemented(String),
}

/// Service-side audio input trait.
#[async_trait]
pub trait AudioCaptureSource: Send {
    async fn start(&mut self) -> Result<(), CaptureError>;
    async fn read_frame(&mut self) -> Result<AudioFrame, CaptureError>;
    async fn stop(&mut self) -> Result<(), CaptureError>;
    fn format(&self) -> AudioFormat;
    fn state(&self) -> CaptureSourceState;
    fn source_name(&self) -> &'static str;
    fn device_name(&self) -> Option<String> {
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SyntheticMode {
    Silence,
    Sine { frequency_hz: f32 },
}

/// Deterministic 10ms synthetic source used for local debug and fallback.
pub struct SyntheticAudioSource {
    format: AudioFormat,
    mode: SyntheticMode,
    phase: f32,
    state: CaptureSourceState,
    next_tick: Option<Instant>,
}

impl SyntheticAudioSource {
    pub fn silence(format: AudioFormat) -> Self {
        Self {
            format,
            mode: SyntheticMode::Silence,
            phase: 0.0,
            state: CaptureSourceState::Created,
            next_tick: None,
        }
    }

    pub fn sine(format: AudioFormat, frequency_hz: f32) -> Self {
        Self {
            format,
            mode: SyntheticMode::Sine { frequency_hz },
            phase: 0.0,
            state: CaptureSourceState::Created,
            next_tick: None,
        }
    }
}

#[async_trait]
impl AudioCaptureSource for SyntheticAudioSource {
    async fn start(&mut self) -> Result<(), CaptureError> {
        self.state = CaptureSourceState::Started;
        self.next_tick = Some(Instant::now());
        Ok(())
    }

    async fn read_frame(&mut self) -> Result<AudioFrame, CaptureError> {
        if self.state != CaptureSourceState::Started {
            return Err(CaptureError::NotStarted);
        }

        let now = Instant::now();
        let current_tick = self.next_tick.unwrap_or(now);
        if current_tick > now {
            sleep_until(current_tick).await;
        }
        self.next_tick =
            Some(current_tick + Duration::from_millis(self.format.frame_duration_ms as u64));

        let pts_ms = now_ms();
        let mut samples = vec![0.0_f32; self.format.total_samples_per_frame()];
        let mut is_silence = true;
        if let SyntheticMode::Sine { frequency_hz } = self.mode {
            is_silence = false;
            let per_channel = self.format.samples_per_channel_per_frame();
            for i in 0..per_channel {
                let sample = (2.0 * PI * self.phase).sin() * 0.2;
                self.phase += frequency_hz / self.format.sample_rate_hz as f32;
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                }
                for ch in 0..self.format.channels as usize {
                    samples[i * self.format.channels as usize + ch] = sample;
                }
            }
        }
        let (peak, rms) = compute_peak_rms(&samples);
        AudioFrame::new_with_meta(
            pts_ms,
            self.format,
            samples,
            is_silence,
            None,
            PacketKind::Synthetic,
            peak,
            rms,
        )
    }

    async fn stop(&mut self) -> Result<(), CaptureError> {
        self.state = CaptureSourceState::Stopped;
        self.next_tick = None;
        Ok(())
    }

    fn format(&self) -> AudioFormat {
        self.format
    }

    fn state(&self) -> CaptureSourceState {
        self.state
    }

    fn source_name(&self) -> &'static str {
        "synthetic"
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn synthetic_sine_produces_frames() {
        let format = AudioFormat::default();
        let mut source = SyntheticAudioSource::sine(format, 440.0);
        source.start().await.expect("start");
        let frame = source.read_frame().await.expect("frame");
        assert_eq!(frame.format, format);
        assert_eq!(frame.samples_f32.len(), format.total_samples_per_frame());
        assert!(frame.samples_f32.iter().any(|v| *v != 0.0));
        assert!(!frame.is_silence);
        source.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn synthetic_silence_produces_silence() {
        let format = AudioFormat::default();
        let mut source = SyntheticAudioSource::silence(format);
        source.start().await.expect("start");
        let frame = source.read_frame().await.expect("frame");
        assert!(frame.samples_f32.iter().all(|v| *v == 0.0));
        assert!(frame.is_silence);
    }

    #[test]
    fn audio_frame_constructor_validates_len() {
        let format = AudioFormat::default();
        let err = AudioFrame::new(0, format, vec![0.0; 1]).expect_err("must fail");
        match err {
            CaptureError::InvalidFrameLength { expected, actual } => {
                assert_eq!(expected, format.total_samples_per_frame());
                assert_eq!(actual, 1);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[tokio::test]
    async fn trait_object_adapter_works() {
        let mut source: Box<dyn AudioCaptureSource> =
            Box::new(SyntheticAudioSource::sine(AudioFormat::default(), 220.0));
        source.start().await.expect("start");
        let frame = source.read_frame().await.expect("frame");
        assert_eq!(
            frame.samples_f32.len(),
            source.format().total_samples_per_frame()
        );
        source.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn synthetic_state_transitions() {
        let mut source = SyntheticAudioSource::silence(AudioFormat::default());
        assert_eq!(source.state(), CaptureSourceState::Created);
        source.start().await.expect("start");
        assert_eq!(source.state(), CaptureSourceState::Started);
        source.stop().await.expect("stop");
        assert_eq!(source.state(), CaptureSourceState::Stopped);
    }

    #[test]
    fn error_variants_are_stable() {
        let e = CaptureError::DefaultDeviceNotFound;
        assert_eq!(e.to_string(), "default output device not found");
        assert!(matches!(
            CaptureError::UnsupportedPlatform("x".to_string()),
            CaptureError::UnsupportedPlatform(_)
        ));
    }

    #[test]
    fn audio_frame_meta_fields_are_preserved() {
        let format = AudioFormat::default();
        let frame = AudioFrame::new_with_meta(
            10,
            format,
            vec![0.0; format.total_samples_per_frame()],
            true,
            Some(512),
            PacketKind::SilentPacket,
            0.0,
            0.0,
        )
        .expect("frame");
        assert!(frame.is_silence);
        assert_eq!(frame.source_buffer_frames, Some(512));
    }
}
