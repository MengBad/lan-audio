use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use lan_audio_protocol::UdpAudioPacket;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

use crate::audio_capture::{
    AudioCaptureSource, AudioFormat, AudioFrame, CaptureDebugDumpConfig, CaptureError,
    CaptureSourceState, PacketKind, SyntheticAudioSource, WindowsLoopbackCapture,
};
use crate::config::{AudioSourceKind, ServerConfig, SyntheticSignalKind};
use crate::metrics::Metrics;

#[derive(Clone)]
pub struct UdpTransport {
    socket: Arc<UdpSocket>,
    metrics: Arc<Metrics>,
    cfg: Arc<ServerConfig>,
}

#[derive(Debug)]
struct EncodedFrame {
    pts_ms: u64,
    sample_rate: u32,
    channels: u8,
    frames_per_packet: u16,
    payload: Vec<u8>,
}

impl UdpTransport {
    pub async fn new(cfg: Arc<ServerConfig>, metrics: Arc<Metrics>) -> anyhow::Result<Self> {
        let socket = UdpSocket::bind(cfg.udp_bind)
            .await
            .with_context(|| format!("bind udp transport: {}", cfg.udp_bind))?;
        info!(bind = %cfg.udp_bind, "udp transport bound");
        Ok(Self {
            socket: Arc::new(socket),
            metrics,
            cfg,
        })
    }

    pub async fn spawn_stream(
        &self,
        session_id: Uuid,
        target: SocketAddr,
        mut shutdown: broadcast::Receiver<()>,
    ) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        let (mut source, source_name) = self.build_capture_source()?;

        self.metrics.inc_capture_start_attempts();
        self.metrics.set_current_audio_source(source_name.clone());
        self.metrics
            .set_capture_source_state(source.state().as_str());
        self.metrics
            .set_capture_device_name(source.device_name().unwrap_or_else(|| "n/a".to_string()));

        if let Err(err) = source.start().await {
            self.metrics.inc_capture_start_failures();
            self.metrics
                .set_capture_source_state(CaptureSourceState::Failed.as_str());
            return Err(anyhow!(err.to_string()));
        }

        let active_format = source.format();
        self.metrics
            .set_capture_format(active_format.sample_rate_hz, active_format.channels);
        self.metrics
            .set_capture_source_state(source.state().as_str());
        self.metrics
            .set_capture_device_name(source.device_name().unwrap_or_else(|| "n/a".to_string()));

        let socket = Arc::clone(&self.socket);
        let metrics = Arc::clone(&self.metrics);
        let configured_source = self.cfg.audio_source.as_str();
        let configured_source_name = configured_source.to_string();

        let handle = tokio::spawn(async move {
            info!(
                session = %session_id,
                target = %target,
                configured_source = %configured_source_name,
                active_source = %source_name,
                "start udp stream"
            );

            let (frame_tx, mut frame_rx) = mpsc::channel::<AudioFrame>(32);
            let (encoded_tx, mut encoded_rx) = mpsc::channel::<EncodedFrame>(32);

            let capture_metrics = Arc::clone(&metrics);
            let mut capture_shutdown = shutdown.resubscribe();
            let capture_handle = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = capture_shutdown.recv() => {
                            if let Err(err) = source.stop().await {
                                warn!(error = %err, "capture stop failed");
                            }
                            capture_metrics.set_capture_source_state(source.state().as_str());
                            break;
                        }
                        frame_result = source.read_frame() => {
                            capture_metrics.set_capture_source_state(source.state().as_str());
                            capture_metrics
                                .set_capture_device_name(source.device_name().unwrap_or_else(|| "n/a".to_string()));
                            match frame_result {
                                Ok(frame) => {
                                    capture_metrics.inc_capture_frames_produced();
                                    capture_metrics.set_last_capture_pts_ms(frame.pts_ms);
                                    capture_metrics.set_capture_format(frame.format.sample_rate_hz, frame.format.channels);
                                    capture_metrics.set_capture_level(frame.peak, frame.rms);
                                    match frame.packet_kind {
                                        PacketKind::NoPacket => {
                                            capture_metrics.inc_capture_no_packet_count();
                                            trace!("capture frame kind=no_packet");
                                        }
                                        PacketKind::SilentPacket => {
                                            capture_metrics.inc_capture_silent_frames();
                                            trace!("capture frame kind=silent_packet");
                                        }
                                        PacketKind::AudioPacket | PacketKind::Mixed | PacketKind::Synthetic => {
                                            if frame.is_silence {
                                                capture_metrics.inc_capture_silent_frames();
                                            } else {
                                                capture_metrics.inc_capture_non_silent_frames();
                                            }
                                            trace!(
                                                packet_kind = ?frame.packet_kind,
                                                peak = frame.peak,
                                                rms = frame.rms,
                                                "capture frame kind=audio"
                                            );
                                        }
                                    }
                                    if let Some(buffer_frames) = frame.source_buffer_frames {
                                        capture_metrics.set_capture_buffer_frames(buffer_frames);
                                    }
                                    if frame_tx.send(frame).await.is_err() {
                                        break;
                                    }
                                }
                                Err(err) => {
                                    capture_metrics.inc_capture_read_errors();
                                    capture_metrics.set_capture_source_state(CaptureSourceState::Failed.as_str());
                                    warn!(error = %err, "capture read error");
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                }
                            }
                        }
                    }
                }
            });

            let encode_metrics = Arc::clone(&metrics);
            let mut encode_shutdown = shutdown.resubscribe();
            let encode_handle = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = encode_shutdown.recv() => break,
                        maybe_frame = tokio::time::timeout(Duration::from_millis(30), frame_rx.recv()) => {
                            match maybe_frame {
                                Ok(Some(frame)) => {
                                    let encoded = encode_passthrough(frame);
                                    if encoded_tx.send(encoded).await.is_err() {
                                        break;
                                    }
                                }
                                Ok(None) => break,
                                Err(_) => {
                                    encode_metrics.inc_capture_underruns();
                                }
                            }
                        }
                    }
                }
            });

            let mut sequence: u32 = 0;
            let mut tx_stats = TxStats::new();
            loop {
                tokio::select! {
                    _ = shutdown.recv() => {
                        info!(session = %session_id, "stop udp stream");
                        break;
                    }
                    maybe_encoded = encoded_rx.recv() => {
                        if let Some(encoded) = maybe_encoded {
                            let packet = UdpAudioPacket {
                                version: 1,
                                flags: 0,
                                sequence,
                                timestamp_ms: encoded.pts_ms,
                                sample_rate: encoded.sample_rate,
                                channels: encoded.channels,
                                frames_per_packet: encoded.frames_per_packet,
                                payload: encoded.payload,
                            };
                            let bytes = packet.encode();
                            match socket.send_to(&bytes, target).await {
                                Ok(_) => {
                                    metrics.inc_packets(bytes.len());
                                    tx_stats.observe(&packet);
                                    tx_stats.maybe_log(sequence);
                                }
                                Err(err) => warn!(session = %session_id, error = %err, "udp send failed"),
                            }
                            sequence = sequence.wrapping_add(1);
                        } else {
                            break;
                        }
                    }
                }
            }

            capture_handle.abort();
            encode_handle.abort();
            debug!(session = %session_id, "udp pipeline task exited");
        });

        Ok(handle)
    }

    fn build_capture_source(&self) -> anyhow::Result<(Box<dyn AudioCaptureSource>, String)> {
        match self.cfg.audio_source {
            AudioSourceKind::Synthetic => {
                let source = self.build_synthetic_source();
                Ok((source, "synthetic".to_string()))
            }
            AudioSourceKind::WindowsLoopback => match self.build_windows_loopback_source() {
                Ok(source) => Ok((source, "windows_loopback".to_string())),
                Err(err) => {
                    if self.cfg.audio_source_fallback_to_synthetic {
                        warn!(
                            error = %err,
                            "windows_loopback init failed, fallback to synthetic"
                        );
                        Ok((
                            self.build_synthetic_source(),
                            "synthetic(fallback)".to_string(),
                        ))
                    } else {
                        Err(err)
                    }
                }
            },
        }
    }

    fn build_synthetic_source(&self) -> Box<dyn AudioCaptureSource> {
        let format = AudioFormat {
            sample_rate_hz: self.cfg.sample_rate,
            channels: self.cfg.channels as u16,
            ..AudioFormat::default()
        };
        match self.cfg.synthetic_signal {
            SyntheticSignalKind::Silence => Box::new(SyntheticAudioSource::silence(format)),
            SyntheticSignalKind::Sine => Box::new(SyntheticAudioSource::sine(
                format,
                self.cfg.synthetic_frequency_hz,
            )),
        }
    }

    fn build_windows_loopback_source(&self) -> anyhow::Result<Box<dyn AudioCaptureSource>> {
        let format = AudioFormat {
            sample_rate_hz: self.cfg.sample_rate,
            channels: self.cfg.channels as u16,
            ..AudioFormat::default()
        };
        let source = WindowsLoopbackCapture::new_default_output(
            format,
            CaptureDebugDumpConfig {
                enabled: self.cfg.capture_debug_dump_wav,
                seconds: self.cfg.capture_debug_dump_seconds,
                output_dir: self.cfg.capture_debug_dump_dir.clone(),
            },
        )
        .map_err(|err: CaptureError| anyhow!(err.to_string()))?;
        Ok(Box::new(source))
    }
}

struct TxStats {
    started_at: Instant,
    frames: u64,
    last_peak: f32,
    last_rms: f32,
    last_frame_bytes: usize,
    last_sample_rate: u32,
    last_channels: u8,
    last_frame_duration_ms: u32,
}

impl TxStats {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            frames: 0,
            last_peak: 0.0,
            last_rms: 0.0,
            last_frame_bytes: 0,
            last_sample_rate: 48_000,
            last_channels: 2,
            last_frame_duration_ms: 10,
        }
    }

    fn observe(&mut self, packet: &UdpAudioPacket) {
        self.frames += 1;
        self.last_frame_bytes = packet.payload.len();
        self.last_sample_rate = packet.sample_rate;
        self.last_channels = packet.channels;
        self.last_frame_duration_ms = if packet.sample_rate == 0 {
            0
        } else {
            (u32::from(packet.frames_per_packet) * 1000) / packet.sample_rate
        };

        let mut peak = 0.0f32;
        let mut sum_sq = 0.0f32;
        let mut samples = 0usize;
        for chunk in packet.payload.chunks_exact(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32;
            peak = peak.max(sample.abs());
            sum_sq += sample * sample;
            samples += 1;
        }
        self.last_peak = peak;
        self.last_rms = if samples == 0 {
            0.0
        } else {
            (sum_sq / samples as f32).sqrt()
        };
    }

    fn maybe_log(&mut self, seq: u32) {
        let elapsed = self.started_at.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }

        let tx_frames_per_sec = self.frames as f64 / elapsed.as_secs_f64();
        info!(
            tx_peak = self.last_peak,
            tx_rms = self.last_rms,
            tx_frame_bytes = self.last_frame_bytes,
            tx_frames_per_sec,
            sample_rate = self.last_sample_rate,
            channels = self.last_channels,
            frame_duration_ms = self.last_frame_duration_ms,
            seq,
            "tx summary"
        );

        self.started_at = Instant::now();
        self.frames = 0;
    }
}

fn encode_passthrough(frame: AudioFrame) -> EncodedFrame {
    // TODO(real-opus): replace this stage with actual Opus encoder output.
    // v7 intentionally performs no loudness normalization, AGC, or limiter.
    // Network payload is fixed for Android diagnostics: 48kHz stereo PCM16 LE, 10ms.
    let samples = to_fixed_48k_stereo_10ms(&frame);
    let mut payload = Vec::with_capacity(samples.len() * 2);
    for sample in &samples {
        let v = sample.clamp(-1.0, 1.0);
        let s = (v * i16::MAX as f32) as i16;
        payload.extend_from_slice(&s.to_le_bytes());
    }
    EncodedFrame {
        pts_ms: frame.pts_ms,
        sample_rate: 48_000,
        channels: 2,
        frames_per_packet: 480,
        payload,
    }
}

fn to_fixed_48k_stereo_10ms(frame: &AudioFrame) -> Vec<f32> {
    const OUT_RATE: u32 = 48_000;
    const OUT_CHANNELS: usize = 2;
    const OUT_FRAMES: usize = 480;

    let in_rate = frame.format.sample_rate_hz.max(1);
    let in_channels = usize::from(frame.format.channels.max(1));
    let in_frames = frame.samples_f32.len() / in_channels;
    if in_frames == 0 {
        return vec![0.0; OUT_FRAMES * OUT_CHANNELS];
    }

    let mut out = Vec::with_capacity(OUT_FRAMES * OUT_CHANNELS);
    for out_frame in 0..OUT_FRAMES {
        let src_frame = ((out_frame as u64 * in_rate as u64) / OUT_RATE as u64)
            .min((in_frames - 1) as u64) as usize;
        let base = src_frame * in_channels;
        let left = frame.samples_f32.get(base).copied().unwrap_or(0.0);
        let right = if in_channels > 1 {
            frame.samples_f32.get(base + 1).copied().unwrap_or(left)
        } else {
            left
        };
        out.push(left);
        out.push(right);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_capture::SampleFormat;

    #[test]
    fn encode_passthrough_outputs_fixed_android_pcm_shape() {
        let frame = AudioFrame {
            pts_ms: 123,
            format: AudioFormat {
                sample_rate_hz: 96_000,
                channels: 2,
                sample_format: SampleFormat::F32,
                frame_duration_ms: 10,
            },
            samples_f32: vec![0.25; 960 * 2],
            is_silence: false,
            packet_kind: PacketKind::AudioPacket,
            peak: 0.25,
            rms: 0.25,
            source_buffer_frames: None,
        };

        let encoded = encode_passthrough(frame);
        assert_eq!(encoded.sample_rate, 48_000);
        assert_eq!(encoded.channels, 2);
        assert_eq!(encoded.frames_per_packet, 480);
        assert_eq!(encoded.payload.len(), 1920);
    }
}
