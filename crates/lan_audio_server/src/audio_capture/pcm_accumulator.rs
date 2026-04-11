//! PCM frame accumulator for fixed-duration frame output.

use super::{AudioFormat, AudioFrame, CaptureError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketKind {
    NoPacket,
    SilentPacket,
    AudioPacket,
    Mixed,
    Synthetic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccumulatedFrame {
    pub samples: Vec<f32>,
    pub is_silence: bool,
    pub peak: f32,
    pub rms: f32,
    pub packet_kind: PacketKind,
}

/// Accumulates arbitrary-sized PCM chunks and emits fixed-size frames.
pub struct PcmFrameAccumulator {
    format: AudioFormat,
    samples: Vec<f32>,
    silence_flags: Vec<bool>,
}

impl PcmFrameAccumulator {
    pub fn new(format: AudioFormat) -> Self {
        Self {
            format,
            samples: Vec::new(),
            silence_flags: Vec::new(),
        }
    }

    pub fn format(&self) -> AudioFormat {
        self.format
    }

    pub fn buffered_samples(&self) -> usize {
        self.samples.len()
    }

    pub fn push_samples(&mut self, input: &[f32], is_silence: bool) {
        if input.is_empty() {
            return;
        }
        self.samples.extend_from_slice(input);
        self.silence_flags
            .resize(self.silence_flags.len() + input.len(), is_silence);
    }

    pub fn push_silence_samples(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        self.samples.resize(self.samples.len() + count, 0.0);
        self.silence_flags
            .resize(self.silence_flags.len() + count, true);
    }

    pub fn pop_frame(&mut self) -> Option<AccumulatedFrame> {
        let frame_samples = self.format.total_samples_per_frame();
        if self.samples.len() < frame_samples {
            return None;
        }

        let samples: Vec<f32> = self.samples.drain(0..frame_samples).collect();
        let flags: Vec<bool> = self.silence_flags.drain(0..frame_samples).collect();

        let all_silent = flags.iter().all(|f| *f);
        let any_silent = flags.iter().any(|f| *f);
        let any_non_silent = flags.iter().any(|f| !*f);

        let packet_kind = match (all_silent, any_silent, any_non_silent) {
            (true, _, _) => PacketKind::SilentPacket,
            (false, false, true) => PacketKind::AudioPacket,
            (false, true, true) => PacketKind::Mixed,
            _ => PacketKind::AudioPacket,
        };

        let (peak, rms) = compute_peak_rms(&samples);

        Some(AccumulatedFrame {
            samples,
            is_silence: all_silent,
            peak,
            rms,
            packet_kind,
        })
    }

    pub fn into_audio_frame(
        &self,
        pts_ms: u64,
        frame: AccumulatedFrame,
        source_buffer_frames: Option<u32>,
    ) -> Result<AudioFrame, CaptureError> {
        AudioFrame::new_with_meta(
            pts_ms,
            self.format,
            frame.samples,
            frame.is_silence,
            source_buffer_frames,
            frame.packet_kind,
            frame.peak,
            frame.rms,
        )
    }
}

pub fn compute_peak_rms(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let mut peak = 0.0_f32;
    let mut sum_sq = 0.0_f32;
    for s in samples {
        let a = s.abs();
        if a > peak {
            peak = a;
        }
        sum_sq += s * s;
    }
    let rms = (sum_sq / samples.len() as f32).sqrt();
    (peak, rms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_capture::{AudioFormat, SampleFormat};

    #[test]
    fn split_fixed_frames() {
        let format = AudioFormat {
            sample_rate_hz: 48_000,
            channels: 2,
            sample_format: SampleFormat::F32,
            frame_duration_ms: 10,
        };
        let frame_len = format.total_samples_per_frame();
        let mut acc = PcmFrameAccumulator::new(format);
        acc.push_samples(&vec![0.1; frame_len * 2], false);
        assert!(acc.pop_frame().is_some());
        assert!(acc.pop_frame().is_some());
        assert!(acc.pop_frame().is_none());
    }

    #[test]
    fn non_integral_packets_accumulate() {
        let format = AudioFormat::default();
        let frame_len = format.total_samples_per_frame();
        let mut acc = PcmFrameAccumulator::new(format);

        acc.push_samples(&vec![0.2; frame_len / 3], false);
        assert!(acc.pop_frame().is_none());
        acc.push_samples(&vec![0.2; frame_len / 3], false);
        assert!(acc.pop_frame().is_none());
        acc.push_samples(&vec![0.2; frame_len - (2 * (frame_len / 3))], false);
        assert!(acc.pop_frame().is_some());
    }

    #[test]
    fn peak_rms_for_silence_and_signal() {
        let (p0, r0) = compute_peak_rms(&vec![0.0; 16]);
        assert_eq!(p0, 0.0);
        assert_eq!(r0, 0.0);

        let (p1, r1) = compute_peak_rms(&[1.0, -1.0, 0.0, 0.0]);
        assert!((p1 - 1.0).abs() < 1e-6);
        assert!((r1 - 0.70710677).abs() < 1e-5);
    }

    #[test]
    fn mixed_silence_and_audio_is_marked_mixed() {
        let format = AudioFormat::default();
        let frame_len = format.total_samples_per_frame();
        let mut acc = PcmFrameAccumulator::new(format);
        acc.push_silence_samples(frame_len / 2);
        acc.push_samples(&vec![0.3; frame_len / 2], false);

        let frame = acc.pop_frame().expect("frame");
        assert!(!frame.is_silence);
        assert_eq!(frame.packet_kind, PacketKind::Mixed);
    }
}
