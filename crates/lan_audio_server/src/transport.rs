use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use lan_audio_protocol::{
    detect_data_plane_packet_kind, AudioMode, DataPlanePacketKind, UdpAudioCodecV2, UdpAudioPacket,
    UDP_FLAG_V2_CONFIG_CHANGED, UDP_FLAG_V2_DISCONTINUITY, UDP_FLAG_V2_SILENCE,
};
use opus::{
    Application as LibOpusApplication, Bitrate as LibOpusBitrate, Channels as LibOpusChannels,
    Encoder as LibOpusEncoder,
};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

use crate::adaptive_runtime::{tier_encoder_profile, AdaptiveRuntime, TierEncoderProfile};
use crate::audio_capture::{
    AudioCaptureSource, AudioFormat, AudioFrame, CaptureDebugDumpConfig, CaptureError,
    CaptureSourceState, PacketKind, SyntheticAudioSource, WindowsLoopbackCapture,
};
use crate::config::{
    AudioSourceKind, CodecSelection, DataPlaneFormat, ServerConfig, SyntheticSignalKind,
    TransportMode,
};
use crate::data_plane::{
    encode_packet_by_data_plane, encode_packets_by_data_plane, DataPlane, DataPlaneRouter,
    EncodedFrame as DataPlaneEncodedFrame, LegacyLas1DataPlane, UsbDirectDataPlane,
    V2HeaderDataPlane,
};
use crate::metrics::Metrics;
use crate::session::{BroadcastClient, ClientRegistry, ClientTransportSnapshot};
use crate::thread_priority::{boost_current_thread, MmcssTask};
use crate::watchdog::{DegradationTier, WatchdogConfig};

#[derive(Clone)]
pub struct UdpTransport {
    socket: Arc<UdpSocket>,
    metrics: Arc<Metrics>,
    cfg: Arc<ServerConfig>,
    current_audio_mode: Arc<StdMutex<AudioMode>>,
}

#[derive(Clone)]
pub struct BroadcastTransport {
    socket: Arc<UdpSocket>,
    tcp_listener: Option<Arc<TcpListener>>,
    metrics: Arc<Metrics>,
    registry: ClientRegistry,
    capture_helper: UdpTransport,
}

#[derive(Debug)]
struct EncodedFrame {
    pts_ms: u64,
    sample_rate: u32,
    channels: u8,
    frames_per_packet: u16,
    codec: UdpAudioCodecV2,
    is_silence: bool,
    source_peak: f32,
    source_rms: f32,
    payload: Vec<u8>,
}

const FLUSH_REASON_IMMEDIATE_FRAME_READY: &str = "immediate_frame_ready";

// ---- Phase 5 encode worker --------------------------------------------------
//
// One dedicated `std::thread` owns the encoder pool and runs all opus / pcm16
// work synchronously. The thread name is `lan-audio-encode` so it is visible
// in profilers, and on Windows it registers the MMCSS "Audio" task at thread
// entry so the kernel scheduler treats it like a real-time audio thread.
//
// The async transport layer hands the worker `EncodeJob` envelopes
// (capture-side metadata + the recipient list) and gets back `EncodeResult`
// envelopes (one wire frame per recipient). Bridging is asymmetric on
// purpose:
//   * Async  -> worker : `std::sync::mpsc` (worker uses sync `recv()`)
//   * Worker -> async  : `tokio::sync::mpsc::unbounded_channel`
//                        (worker uses sync `send()` because tokio unbounded
//                        senders are cheap and lock-free; async side polls
//                        with `recv().await`)

struct EncodeJob {
    frame: AudioFrame,
    clients: Vec<BroadcastClient>,
    active_tier: DegradationTier,
    /// Shared sequence counter. The worker increments it once per encoded
    /// packet so the async dispatch loop never races on it.
    sequence: Arc<std::sync::atomic::AtomicU32>,
    /// Reserved for future end-to-end latency telemetry. The worker echoes
    /// it back on the result so the dispatch loop can compute encode time.
    #[allow(dead_code)]
    job_received_at: Instant,
}

struct WireFrameOut {
    client: BroadcastClient,
    packet: UdpAudioPacket,
    wire_bytes: Vec<u8>,
    v2_flags: u16,
    mode_changed: bool,
    codec: UdpAudioCodecV2,
    source_peak: f32,
    source_rms: f32,
    detected_wire_kind: DataPlanePacketKind,
}

struct EncodeResult {
    wire_frames: Vec<WireFrameOut>,
    /// Reserved for future end-to-end latency telemetry.
    #[allow(dead_code)]
    job_received_at: Instant,
}

/// Spawn the encode worker on a dedicated `std::thread`. The thread runs to
/// completion when the input channel is dropped — there's no separate
/// shutdown signal because `std_mpsc::Receiver::recv()` already returns
/// `Err` once all senders are gone.
fn spawn_encode_worker(
    job_rx: std_mpsc::Receiver<EncodeJob>,
    result_tx: mpsc::UnboundedSender<EncodeResult>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("lan-audio-encode".to_string())
        .spawn(move || {
            // Phase 5: register MMCSS for the encode thread. The handle
            // lives for the whole worker — only dropped when the loop
            // exits. On non-Windows this is a no-op.
            let _mmcss = boost_current_thread(MmcssTask::Audio);
            info!(
                target: "lan_audio_server::encode_worker",
                "encode worker started"
            );

            let mut encoders: HashMap<(CodecSelection, AudioMode, u32), AudioFrameEncoder> =
                HashMap::new();
            let mut last_applied_tier = DegradationTier::Green;

            while let Ok(job) = job_rx.recv() {
                // Re-baseline encoders if the watchdog tier just changed.
                if job.active_tier != last_applied_tier {
                    for ((_, mode, _), encoder) in encoders.iter_mut() {
                        let profile = tier_encoder_profile(job.active_tier, *mode);
                        encoder.apply_tier_profile(profile, *mode);
                    }
                    info!(
                        target: "lan_audio_server::encode_worker",
                        from = ?last_applied_tier,
                        to = ?job.active_tier,
                        "tier transition applied to encoder pool"
                    );
                    last_applied_tier = job.active_tier;
                }

                let mut wire_frames: Vec<WireFrameOut> = Vec::with_capacity(job.clients.len());

                // Group recipients by encoder key so each (codec, mode, rate)
                // tuple encodes the frame once, then fans out to all matching
                // recipients.
                let mut grouped: HashMap<(CodecSelection, AudioMode, u32), Vec<BroadcastClient>> =
                    HashMap::new();
                for client in job.clients.iter().cloned() {
                    let preferred_sample_rate =
                        normalize_encoder_sample_rate(client.preferred_sample_rate);
                    grouped
                        .entry((client.codec, client.audio_mode, preferred_sample_rate))
                        .or_default()
                        .push(client);
                }

                for ((codec, mode, sample_rate), recipients) in grouped {
                    let encoder = encoders
                        .entry((codec, mode, sample_rate))
                        .or_insert_with(|| {
                            let mut e = AudioFrameEncoder::new(codec, mode, sample_rate);
                            let profile = tier_encoder_profile(job.active_tier, mode);
                            e.apply_tier_profile(profile, mode);
                            e
                        });
                    let encoded_frames = encoder.encode(job.frame.clone(), mode);
                    for encoded in encoded_frames {
                        let packet_codec = encoded.codec;
                        let source_peak = encoded.source_peak;
                        let source_rms = encoded.source_rms;
                        let seq = job
                            .sequence
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let packet = UdpAudioPacket {
                            version: 1,
                            flags: legacy_flags_for_frame(&encoded),
                            sequence: seq,
                            timestamp_ms: encoded.pts_ms,
                            sample_rate: encoded.sample_rate,
                            channels: encoded.channels,
                            frames_per_packet: encoded.frames_per_packet,
                            payload: encoded.payload,
                        };

                        for client in &recipients {
                            let v2_flags = v2_flags_for_frame(
                                &packet,
                                client.mode_changed,
                                client.first_packet,
                            );
                            // Phase 6 fragmentation. For PCM24 this returns
                            // multiple wire packets (one per frag); for
                            // every other codec it returns a single packet
                            // identical to the v1.9.x behaviour.
                            let mut next_seq = packet.sequence;
                            let wire_packets = encode_packets_by_data_plane(
                                &packet,
                                client.data_plane,
                                v2_flags,
                                packet_codec,
                                &mut next_seq,
                            );
                            // Bump the shared atomic by the number of extra
                            // packets we produced beyond the original one
                            // (encoder.encode already incremented for the
                            // first one). For the common single-packet
                            // case, `wire_packets.len() == 1` so this is a
                            // no-op.
                            let extras = wire_packets.len().saturating_sub(1) as u32;
                            if extras > 0 {
                                job.sequence
                                    .fetch_add(extras, std::sync::atomic::Ordering::Relaxed);
                            }
                            for (i, wire_bytes) in wire_packets.into_iter().enumerate() {
                                let detected_wire_kind = detect_data_plane_packet_kind(&wire_bytes);
                                // For the first chunk reuse the parent
                                // packet metadata; for subsequent chunks
                                // synthesize a per-frag UdpAudioPacket so
                                // tx_stats can still log the sequence.
                                let frag_packet = if i == 0 {
                                    packet.clone()
                                } else {
                                    UdpAudioPacket {
                                        version: packet.version,
                                        flags: packet.flags,
                                        sequence: packet.sequence.wrapping_add(i as u32),
                                        timestamp_ms: packet.timestamp_ms,
                                        sample_rate: packet.sample_rate,
                                        channels: packet.channels,
                                        frames_per_packet: packet.frames_per_packet,
                                        payload: Vec::new(),
                                    }
                                };
                                wire_frames.push(WireFrameOut {
                                    client: client.clone(),
                                    packet: frag_packet,
                                    wire_bytes,
                                    v2_flags,
                                    mode_changed: client.mode_changed,
                                    codec: packet_codec,
                                    source_peak,
                                    source_rms,
                                    detected_wire_kind,
                                });
                            }
                        }
                    }
                }

                let result = EncodeResult {
                    wire_frames,
                    job_received_at: job.job_received_at,
                };
                if result_tx.send(result).is_err() {
                    warn!(
                        target: "lan_audio_server::encode_worker",
                        "result channel closed; encode worker exiting"
                    );
                    break;
                }
            }

            info!(
                target: "lan_audio_server::encode_worker",
                "encode worker stopped"
            );
            // _mmcss drops here, reverting MMCSS registration on Windows.
        })
        .expect("spawn encode worker thread")
}

#[derive(Debug)]
struct CaptureHandoffStatsWindow {
    started_at: Instant,
    frame_tx_block_samples: u64,
    frame_tx_block_total_ms: f64,
    frame_tx_block_max_ms: f64,
}

impl CaptureHandoffStatsWindow {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            frame_tx_block_samples: 0,
            frame_tx_block_total_ms: 0.0,
            frame_tx_block_max_ms: 0.0,
        }
    }

    fn record_frame_tx_block(&mut self, blocked_for: Duration) {
        let block_ms = blocked_for.as_secs_f64() * 1000.0;
        self.frame_tx_block_samples += 1;
        self.frame_tx_block_total_ms += block_ms;
        if block_ms > self.frame_tx_block_max_ms {
            self.frame_tx_block_max_ms = block_ms;
        }
    }

    fn maybe_log(&mut self) {
        let elapsed = self.started_at.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }

        let frame_tx_block_avg_ms = if self.frame_tx_block_samples == 0 {
            0.0
        } else {
            self.frame_tx_block_total_ms / self.frame_tx_block_samples as f64
        };
        info!(
            frame_tx_block_avg_ms,
            frame_tx_block_max_ms = self.frame_tx_block_max_ms,
            frame_tx_block_samples = self.frame_tx_block_samples,
            "capture handoff summary"
        );
        *self = Self::new();
    }
}

#[derive(Debug)]
struct PacketBuildStatsWindow {
    started_at: Instant,
    packet_build_count: u64,
    total_frames_per_packet: u64,
    last_frames_per_packet: u16,
    last_payload_bytes: usize,
    total_frame_age_ms_before_build: u64,
    max_frame_age_ms_before_build: u64,
    encode_input_timeout_count: u64,
    encoded_tx_block_samples: u64,
    encoded_tx_block_total_ms: f64,
    encoded_tx_block_max_ms: f64,
    last_flush_reason: &'static str,
}

impl PacketBuildStatsWindow {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            packet_build_count: 0,
            total_frames_per_packet: 0,
            last_frames_per_packet: 0,
            last_payload_bytes: 0,
            total_frame_age_ms_before_build: 0,
            max_frame_age_ms_before_build: 0,
            encode_input_timeout_count: 0,
            encoded_tx_block_samples: 0,
            encoded_tx_block_total_ms: 0.0,
            encoded_tx_block_max_ms: 0.0,
            last_flush_reason: FLUSH_REASON_IMMEDIATE_FRAME_READY,
        }
    }

    fn record_packet_built(
        &mut self,
        frames_per_packet: u16,
        payload_bytes: usize,
        frame_age_ms_before_build: u64,
        flush_reason: &'static str,
    ) {
        self.packet_build_count += 1;
        self.total_frames_per_packet += u64::from(frames_per_packet);
        self.last_frames_per_packet = frames_per_packet;
        self.last_payload_bytes = payload_bytes;
        self.total_frame_age_ms_before_build += frame_age_ms_before_build;
        if frame_age_ms_before_build > self.max_frame_age_ms_before_build {
            self.max_frame_age_ms_before_build = frame_age_ms_before_build;
        }
        self.last_flush_reason = flush_reason;
    }

    fn record_encode_timeout(&mut self) {
        self.encode_input_timeout_count += 1;
    }

    fn record_encoded_tx_block(&mut self, blocked_for: Duration) {
        let block_ms = blocked_for.as_secs_f64() * 1000.0;
        self.encoded_tx_block_samples += 1;
        self.encoded_tx_block_total_ms += block_ms;
        if block_ms > self.encoded_tx_block_max_ms {
            self.encoded_tx_block_max_ms = block_ms;
        }
    }

    fn maybe_log(&mut self) {
        let elapsed = self.started_at.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }

        let elapsed_secs = elapsed.as_secs_f64().max(f64::EPSILON);
        let packet_build_count_per_sec = self.packet_build_count as f64 / elapsed_secs;
        let avg_frames_per_packet = if self.packet_build_count == 0 {
            0.0
        } else {
            self.total_frames_per_packet as f64 / self.packet_build_count as f64
        };
        let frame_age_ms_before_build_avg = if self.packet_build_count == 0 {
            0.0
        } else {
            self.total_frame_age_ms_before_build as f64 / self.packet_build_count as f64
        };
        let encoded_tx_block_avg_ms = if self.encoded_tx_block_samples == 0 {
            0.0
        } else {
            self.encoded_tx_block_total_ms / self.encoded_tx_block_samples as f64
        };

        info!(
            packet_build_count_per_sec,
            avg_frames_per_packet,
            frames_per_packet = self.last_frames_per_packet,
            payload_bytes = self.last_payload_bytes as u64,
            frame_age_ms_before_build_avg,
            frame_age_ms_before_build_max = self.max_frame_age_ms_before_build,
            encoded_tx_block_avg_ms,
            encoded_tx_block_max_ms = self.encoded_tx_block_max_ms,
            encode_input_timeout_count = self.encode_input_timeout_count,
            flush_reason = self.last_flush_reason,
            "packet build summary"
        );
        *self = Self::new();
    }
}

impl BroadcastTransport {
    pub async fn new(
        cfg: Arc<ServerConfig>,
        metrics: Arc<Metrics>,
        registry: ClientRegistry,
        _data_plane_router: Arc<std::sync::Mutex<DataPlaneRouter>>,
    ) -> anyhow::Result<Self> {
        let socket = UdpSocket::bind(cfg.udp_bind)
            .await
            .with_context(|| format!("bind udp transport: {}", cfg.udp_bind))?;
        info!(bind = %cfg.udp_bind, "udp transport bound");
        let socket = Arc::new(socket);
        let tcp_listener = if matches!(cfg.transport_mode, TransportMode::Usb { .. }) {
            Some(Arc::new(
                TcpListener::bind(cfg.udp_bind)
                    .await
                    .with_context(|| format!("bind tcp transport: {}", cfg.udp_bind))?,
            ))
        } else {
            None
        };
        if let Some(listener) = &tcp_listener {
            info!(bind = %listener.local_addr().unwrap_or(cfg.udp_bind), "tcp transport bound");
        }
        let capture_helper = UdpTransport {
            socket: Arc::clone(&socket),
            metrics: Arc::clone(&metrics),
            cfg: Arc::clone(&cfg),
            current_audio_mode: Arc::new(StdMutex::new(cfg.current_audio_mode)),
        };
        Ok(Self {
            socket,
            tcp_listener,
            metrics,
            registry,
            capture_helper,
        })
    }

    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) -> anyhow::Result<()> {
        if let Some(listener) = &self.tcp_listener {
            let listener = Arc::clone(listener);
            let registry = self.registry.clone();
            let mut tcp_shutdown = shutdown.resubscribe();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = tcp_shutdown.recv() => break,
                        incoming = listener.accept() => {
                            match incoming {
                                Ok((stream, peer)) => {
                                    info!(peer = %peer, "usb tcp data stream accepted");
                                    let (_read_half, write_half) = stream.into_split();
                                    registry.attach_usb_stream(write_half).await;
                                }
                                Err(err) => {
                                    warn!(error = %err, "usb tcp accept failed");
                                }
                            }
                        }
                    }
                }
            });
        }

        let (mut source, source_name) = self.capture_helper.build_capture_source()?;
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

        let (frame_tx, mut frame_rx) = mpsc::channel::<AudioFrame>(64);
        let metrics = Arc::clone(&self.metrics);
        let mut capture_shutdown = shutdown.resubscribe();
        let capture_handle = tokio::spawn(async move {
            let mut handoff_stats = CaptureHandoffStatsWindow::new();
            loop {
                tokio::select! {
                    _ = capture_shutdown.recv() => {
                        if let Err(err) = source.stop().await {
                            warn!(error = %err, "capture stop failed");
                        }
                        metrics.set_capture_source_state(source.state().as_str());
                        break;
                    }
                    frame_result = source.read_frame() => {
                        metrics.set_capture_source_state(source.state().as_str());
                        metrics
                            .set_capture_device_name(source.device_name().unwrap_or_else(|| "n/a".to_string()));
                        match frame_result {
                            Ok(frame) => {
                                metrics.inc_capture_frames_produced();
                                metrics.set_last_capture_pts_ms(frame.pts_ms);
                                metrics.set_capture_format(frame.format.sample_rate_hz, frame.format.channels);
                                metrics.set_capture_level(frame.peak, frame.rms);
                                match frame.packet_kind {
                                    PacketKind::NoPacket => metrics.inc_capture_no_packet_count(),
                                    PacketKind::SilentPacket => metrics.inc_capture_silent_frames(),
                                    PacketKind::AudioPacket | PacketKind::Mixed | PacketKind::Synthetic => {
                                        if frame.is_silence {
                                            metrics.inc_capture_silent_frames();
                                        } else {
                                            metrics.inc_capture_non_silent_frames();
                                        }
                                    }
                                }
                                if let Some(buffer_frames) = frame.source_buffer_frames {
                                    metrics.set_capture_buffer_frames(buffer_frames);
                                }
                                let frame_tx_started_at = Instant::now();
                                let send_result = frame_tx.send(frame).await;
                                handoff_stats.record_frame_tx_block(frame_tx_started_at.elapsed());
                                handoff_stats.maybe_log();
                                if send_result.is_err() {
                                    break;
                                }
                            }
                            Err(err) => {
                                metrics.inc_capture_read_errors();
                                metrics.set_capture_source_state(CaptureSourceState::Failed.as_str());
                                warn!(error = %err, "capture read error");
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        }
                    }
                }
            }
        });

        // Phase 5 encode worker: all opus / pcm16 encoding runs on a
        // dedicated `std::thread` with MMCSS "Audio" registered. This
        // protects the audio pipeline from the noise of tokio worker
        // threads picking up other futures, which used to make MMCSS
        // registration unreliable in v1.9.0.
        let (job_tx, job_rx) = std_mpsc::channel::<EncodeJob>();
        let (result_tx, mut result_rx) = mpsc::unbounded_channel::<EncodeResult>();
        let encode_worker_handle = spawn_encode_worker(job_rx, result_tx);
        let sequence = Arc::new(std::sync::atomic::AtomicU32::new(0));

        let mut tx_stats = TxStats::new();

        // Phase 4 adaptive runtime: drives a CPU + queue-pressure watchdog
        // that publishes a tier (Green/Yellow/Red) into a shared slot the
        // broadcast loop re-reads on every frame. When the tier changes we
        // reconfigure every active encoder via `apply_tier_profile`.
        let adaptive_enabled = self.capture_helper.cfg.adaptive_runtime_enabled;
        let watermark_slot = self.registry.watermark_slot();
        let adaptive_runtime = if adaptive_enabled {
            Some(Arc::new(StdMutex::new(AdaptiveRuntime::new(
                WatchdogConfig::default(),
                3.0,
                100.0,
                100.0,
                Duration::from_millis(500),
            ))))
        } else {
            None
        };
        let current_tier = Arc::new(StdMutex::new(DegradationTier::Green));
        let watchdog_handle = if let Some(rt_arc) = adaptive_runtime.as_ref() {
            let rt_arc = Arc::clone(rt_arc);
            let watchdog_metrics = Arc::clone(&self.metrics);
            let watchdog_tier = Arc::clone(&current_tier);
            let mut watchdog_shutdown = shutdown.resubscribe();
            let watermark_slot = Arc::clone(&watermark_slot);
            let cadence = Duration::from_millis(500);
            Some(tokio::spawn(async move {
                let mut interval = tokio::time::interval(cadence);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        _ = watchdog_shutdown.recv() => break,
                        _ = interval.tick() => {
                            // Phase 3: drain the latest client watermark.
                            let observation = watermark_slot
                                .lock()
                                .ok()
                                .and_then(|mut g| g.take())
                                .map(|r| crate::sync_engine::WatermarkObservation {
                                    jitter_buf_ms: r.jitter_buf_ms,
                                    ring_buf_ms: r.ring_buf_ms,
                                    silence_fill_delta: r.silence_fill_delta,
                                    underrun_delta: r.underrun_delta,
                                    jitter_p95_us: r.jitter_p95_us,
                                });
                            // Queue depth: we don't have a single backpressure
                            // queue for the live broadcast path (it encodes
                            // inline). Pass zero so the watchdog only reacts
                            // to CPU and watermark pressure, not the absent
                            // queue.
                            let decision = {
                                let mut rt_guard = match rt_arc.lock() {
                                    Ok(g) => g,
                                    Err(e) => e.into_inner(),
                                };
                                rt_guard.tick(0, 64, 0, observation)
                            };
                            if let Ok(mut g) = watchdog_tier.lock() {
                                *g = decision.tier;
                            }
                            watchdog_metrics.set_adaptive_tier(decision.tier.label());
                            watchdog_metrics
                                .set_adaptive_predicted_cpu_percent(decision.predicted_cpu);
                            watchdog_metrics.set_adaptive_queue_ratio(0.0);
                        }
                    }
                }
            }))
        } else {
            None
        };

        let mut last_applied_tier = DegradationTier::Green;

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    info!("broadcast transport stopping");
                    break;
                }
                maybe_frame = frame_rx.recv() => {
                    let Some(frame) = maybe_frame else {
                        break;
                    };
                    let clients = self.registry.take_broadcast_clients().await;
                    if clients.is_empty() {
                        continue;
                    }

                    // Read the latest watchdog tier and forward it to the
                    // encode worker via the job envelope. The worker
                    // applies tier transitions in lock-step with the
                    // encode call so the bitrate change always lands on
                    // a packet boundary.
                    let tier_now = if adaptive_enabled {
                        current_tier
                            .lock()
                            .map(|g| *g)
                            .unwrap_or(DegradationTier::Green)
                    } else {
                        DegradationTier::Green
                    };
                    if tier_now != last_applied_tier {
                        info!(
                            from = ?last_applied_tier,
                            to = ?tier_now,
                            "broadcast adaptive tier transition forwarded to encode worker"
                        );
                        last_applied_tier = tier_now;
                    }

                    let job = EncodeJob {
                        frame,
                        clients,
                        active_tier: tier_now,
                        sequence: Arc::clone(&sequence),
                        job_received_at: Instant::now(),
                    };

                    if job_tx.send(job).is_err() {
                        warn!("encode worker channel closed; broadcast transport stopping");
                        break;
                    }
                }
                maybe_result = result_rx.recv() => {
                    let Some(result) = maybe_result else {
                        warn!("encode result channel closed; broadcast transport stopping");
                        break;
                    };
                    self.dispatch_wire_frames(result.wire_frames, &mut tx_stats).await;
                }
            }
        }

        capture_handle.abort();
        if let Some(handle) = watchdog_handle {
            handle.abort();
        }
        // Drop the job sender so the worker exits cleanly.
        drop(job_tx);
        if let Err(err) = encode_worker_handle.join() {
            warn!(?err, "encode worker thread panicked");
        }
        Ok(())
    }

    /// Phase 5 dispatch path. Takes wire frames produced by the encode
    /// worker, fans them out to recipients via the appropriate data plane,
    /// and updates `tx_stats` / `metrics` / `registry` based on send
    /// outcome. Failed sends record per-client failure for the existing
    /// drop-and-cleanup logic.
    async fn dispatch_wire_frames(&self, wire_frames: Vec<WireFrameOut>, tx_stats: &mut TxStats) {
        let mut failed_wifi_clients: Vec<Uuid> = Vec::new();
        let mut failed_usb_clients: Vec<Uuid> = Vec::new();

        for wire in wire_frames {
            let plane = self.sender_for_client(&wire.client);
            let frame_len = wire.wire_bytes.len();
            let dataplane_frame = DataPlaneEncodedFrame::new(wire.wire_bytes);
            match plane.send_frame(&dataplane_frame).await {
                Ok(()) => {
                    self.metrics.inc_packets(frame_len);
                    tx_stats.observe(
                        &wire.packet,
                        frame_len,
                        Duration::from_millis(0),
                        wire.client.data_plane,
                        wire.detected_wire_kind,
                        wire.v2_flags,
                        wire.mode_changed,
                        wire.codec,
                        wire.source_peak,
                        wire.source_rms,
                    );
                }
                Err(err) => {
                    warn!(
                        client = %wire.client.name,
                        data_plane = plane.path_name(),
                        error = %err,
                        "broadcast send failed"
                    );
                    match &wire.client.transport {
                        ClientTransportSnapshot::Wifi(_) => {
                            failed_wifi_clients.push(wire.client.id)
                        }
                        ClientTransportSnapshot::Usb(_) => failed_usb_clients.push(wire.client.id),
                    }
                }
            }
            tx_stats.maybe_log(wire.packet.sequence);
        }

        failed_wifi_clients.sort_unstable();
        failed_wifi_clients.dedup();
        for client_id in failed_wifi_clients {
            let _ = self.registry.remove_client(client_id).await;
        }

        failed_usb_clients.sort_unstable();
        failed_usb_clients.dedup();
        for client_id in failed_usb_clients {
            if let Some(name) = self.registry.mark_usb_transport_lost(client_id).await {
                info!(client = %name, "usb transport lost, waiting for forwarded tcp stream reattach");
            }
        }
    }

    fn sender_for_client(&self, client: &BroadcastClient) -> Arc<dyn DataPlane> {
        match &client.transport {
            ClientTransportSnapshot::Wifi(addr) => match client.data_plane {
                DataPlaneFormat::LegacyLas1 => Arc::new(LegacyLas1DataPlane::with_udp_target(
                    Arc::clone(&self.socket),
                    *addr,
                )),
                DataPlaneFormat::V2Header => Arc::new(V2HeaderDataPlane::with_udp_target(
                    Arc::clone(&self.socket),
                    *addr,
                )),
            },
            ClientTransportSnapshot::Usb(writer) => {
                Arc::new(UsbDirectDataPlane::with_writer(Arc::clone(writer)))
            }
        }
    }
}

impl UdpTransport {
    pub async fn new(
        cfg: Arc<ServerConfig>,
        metrics: Arc<Metrics>,
        current_audio_mode: Arc<StdMutex<AudioMode>>,
    ) -> anyhow::Result<Self> {
        let socket = UdpSocket::bind(cfg.udp_bind)
            .await
            .with_context(|| format!("bind udp transport: {}", cfg.udp_bind))?;
        info!(bind = %cfg.udp_bind, "udp transport bound");
        Ok(Self {
            socket: Arc::new(socket),
            metrics,
            cfg,
            current_audio_mode,
        })
    }

    pub async fn spawn_stream(
        &self,
        session_id: Uuid,
        target: SocketAddr,
        selected_data_plane: DataPlaneFormat,
        effective_codec: CodecSelection,
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
        let desired_data_plane = self.cfg.data_plane_format;
        let requested_codec = self.cfg.codec_selection;
        if requested_codec == CodecSelection::Opus && effective_codec == CodecSelection::Pcm16 {
            warn!(
                requested_codec = %requested_codec.as_str(),
                selected_data_plane = %selected_data_plane.as_str(),
                "opus experimental was not negotiated for this session; falling back to pcm16"
            );
        }
        let current_audio_mode = Arc::clone(&self.current_audio_mode);
        let adaptive_runtime_enabled = self.cfg.adaptive_runtime_enabled;

        let handle = tokio::spawn(async move {
            // Phase 5 caveat: MMCSS thread priority must be registered on a
            // stable OS thread. Tokio worker threads can migrate tasks
            // between polls, so calling boost_current_thread() inside this
            // async block is unreliable. Wiring real MMCSS boost requires
            // moving the capture/encode/send loops onto std::thread, which
            // is tracked as a follow-up. The thread_priority module is kept
            // available so the migration can land independently.
            info!(
                session = %session_id,
                target = %target,
                configured_source = %configured_source_name,
                active_source = %source_name,
                desired_data_plane = %desired_data_plane.as_str(),
                selected_data_plane = %selected_data_plane.as_str(),
                requested_codec = %requested_codec.as_str(),
                effective_codec = %effective_codec.as_str(),
                "start udp stream"
            );

            let (frame_tx, mut frame_rx) = mpsc::channel::<AudioFrame>(32);
            let (encoded_tx, mut encoded_rx) = mpsc::channel::<EncodedFrame>(32);

            let capture_metrics = Arc::clone(&metrics);
            let mut capture_shutdown = shutdown.resubscribe();
            let capture_handle = tokio::spawn(async move {
                let mut handoff_stats = CaptureHandoffStatsWindow::new();
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
                                    let frame_tx_started_at = Instant::now();
                                    let send_result = frame_tx.send(frame).await;
                                    handoff_stats.record_frame_tx_block(frame_tx_started_at.elapsed());
                                    handoff_stats.maybe_log();
                                    if send_result.is_err() {
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
            let encode_audio_mode = Arc::clone(&current_audio_mode);
            let encoder_sample_rate = normalize_encoder_sample_rate(active_format.sample_rate_hz);
            let adaptive_enabled = adaptive_runtime_enabled;
            let adaptive_tier = Arc::new(StdMutex::new(DegradationTier::Green));
            // Clone the encoded-frame sender for the watchdog so it can read
            // queue depth without taking ownership away from the encoder.
            let watchdog_queue_observer = encoded_tx.clone();
            let encode_handle = {
                let adaptive_tier = Arc::clone(&adaptive_tier);
                tokio::spawn(async move {
                    let mut packet_build_stats = PacketBuildStatsWindow::new();
                    let initial_mode = read_current_audio_mode(&encode_audio_mode);
                    let mut frame_encoder =
                        AudioFrameEncoder::new(effective_codec, initial_mode, encoder_sample_rate);
                    let mut last_applied_tier = DegradationTier::Green;
                    loop {
                        tokio::select! {
                            _ = encode_shutdown.recv() => break,
                            maybe_frame = tokio::time::timeout(Duration::from_millis(30), frame_rx.recv()) => {
                                match maybe_frame {
                                    Ok(Some(frame)) => {
                                        let frame_age_ms_before_build = now_ms().saturating_sub(frame.pts_ms);
                                        let active_mode = read_current_audio_mode(&encode_audio_mode);

                                        // Phase 4: pull the latest tier the
                                        // watchdog has chosen and reconfigure
                                        // the encoder if it changed.
                                        if adaptive_enabled {
                                            let current_tier = match adaptive_tier.lock() {
                                                Ok(g) => *g,
                                                Err(e) => *e.into_inner(),
                                            };
                                            if current_tier != last_applied_tier {
                                                let profile = tier_encoder_profile(current_tier, active_mode);
                                                frame_encoder.apply_tier_profile(profile, active_mode);
                                                info!(
                                                    from = ?last_applied_tier,
                                                    to = ?current_tier,
                                                    bitrate_bps = profile.bitrate_bps,
                                                    complexity = profile.complexity,
                                                    force_pcm16 = profile.force_pcm16_fallback,
                                                    "adaptive runtime tier transition applied"
                                                );
                                                last_applied_tier = current_tier;
                                            }
                                        }

                                        let encoded_frames = frame_encoder.encode(frame, active_mode);
                                        let mut encoded_channel_closed = false;
                                        for encoded in encoded_frames {
                                            packet_build_stats.record_packet_built(
                                                encoded.frames_per_packet,
                                                encoded.payload.len(),
                                                frame_age_ms_before_build,
                                                FLUSH_REASON_IMMEDIATE_FRAME_READY,
                                            );
                                            let encoded_tx_started_at = Instant::now();
                                            let send_result = encoded_tx.send(encoded).await;
                                            packet_build_stats.record_encoded_tx_block(encoded_tx_started_at.elapsed());
                                            packet_build_stats.maybe_log();
                                            if send_result.is_err() {
                                                encoded_channel_closed = true;
                                                break;
                                            }
                                        }
                                        if encoded_channel_closed {
                                            break;
                                        }
                                    }
                                    Ok(None) => break,
                                    Err(_) => {
                                        encode_metrics.inc_capture_underruns();
                                        packet_build_stats.record_encode_timeout();
                                        packet_build_stats.maybe_log();
                                    }
                                }
                            }
                        }
                    }
                })
            };

            // Phase 4 watchdog tick task. Runs at 500ms cadence, samples CPU,
            // observes the encode-queue depth, decides a tier, publishes it
            // to the encoder via `adaptive_tier`. Disabled when the operator
            // passed `--no-adaptive-runtime` so the v1.8.7 baseline remains
            // available as a rollback path.
            let watchdog_handle = if adaptive_runtime_enabled {
                let watchdog_metrics = Arc::clone(&metrics);
                let watchdog_tier = Arc::clone(&adaptive_tier);
                let mut watchdog_shutdown = shutdown.resubscribe();
                let queue_observer = watchdog_queue_observer;
                let cadence = Duration::from_millis(500);
                Some(tokio::spawn(async move {
                    let mut runtime =
                        AdaptiveRuntime::new(WatchdogConfig::default(), 3.0, 100.0, 100.0, cadence);
                    let mut interval = tokio::time::interval(cadence);
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        tokio::select! {
                            _ = watchdog_shutdown.recv() => break,
                            _ = interval.tick() => {
                                let queue_capacity = queue_observer.max_capacity() as u32;
                                let queue_depth = queue_capacity
                                    .saturating_sub(queue_observer.capacity() as u32);
                                let decision = runtime.tick(
                                    queue_depth,
                                    queue_capacity,
                                    0,
                                    None,
                                );
                                if let Ok(mut g) = watchdog_tier.lock() {
                                    *g = decision.tier;
                                }
                                watchdog_metrics.set_adaptive_tier(decision.tier.label());
                                watchdog_metrics
                                    .set_adaptive_predicted_cpu_percent(decision.predicted_cpu);
                                let queue_ratio = if queue_capacity == 0 {
                                    0.0
                                } else {
                                    queue_depth as f64 / queue_capacity as f64
                                };
                                watchdog_metrics.set_adaptive_queue_ratio(queue_ratio);
                            }
                        }
                    }
                }))
            } else {
                None
            };

            let mut sequence: u32 = 0;
            let mut tx_stats = TxStats::new();
            let mut last_sent_mode = read_current_audio_mode(&current_audio_mode);
            let mut first_packet = true;
            let mut packet_sample_budget = 5_u8;
            loop {
                tokio::select! {
                    _ = shutdown.recv() => {
                        info!(session = %session_id, "stop udp stream");
                        break;
                    }
                    maybe_encoded = encoded_rx.recv() => {
                        if let Some(encoded) = maybe_encoded {
                            let active_mode = read_current_audio_mode(&current_audio_mode);
                            let mode_changed = active_mode != last_sent_mode;
                            if mode_changed {
                                info!(
                                    session = %session_id,
                                    from = ?last_sent_mode,
                                    to = ?active_mode,
                                    "audio mode changed; mark config_changed/discontinuity in outgoing packet"
                                );
                            }
                            let packet_codec = encoded.codec;
                            let source_peak = encoded.source_peak;
                            let source_rms = encoded.source_rms;
                            let packet = UdpAudioPacket {
                                version: 1,
                                flags: legacy_flags_for_frame(&encoded),
                                sequence,
                                timestamp_ms: encoded.pts_ms,
                                sample_rate: encoded.sample_rate,
                                channels: encoded.channels,
                                frames_per_packet: encoded.frames_per_packet,
                                payload: encoded.payload,
                            };
                            let v2_flags = v2_flags_for_frame(&packet, mode_changed, first_packet);
                            let bytes = encode_packet_by_data_plane(&packet, selected_data_plane, v2_flags, packet_codec);
                            let detected_wire_kind = detect_data_plane_packet_kind(&bytes);
                            if packet_sample_budget > 0 {
                                debug!(
                                    session = %session_id,
                                    sequence = packet.sequence,
                                    timestamp_ms = packet.timestamp_ms,
                                    payload_size = packet.payload.len(),
                                    wire_bytes = bytes.len(),
                                    detected_wire_kind = ?detected_wire_kind,
                                    codec = ?packet_codec,
                                    v2_flags,
                                    flush_reason = FLUSH_REASON_IMMEDIATE_FRAME_READY,
                                    "packet sample"
                                );
                                packet_sample_budget -= 1;
                            }
                            let udp_send_started_at = Instant::now();
                            match socket.send_to(&bytes, target).await {
                                Ok(_) => {
                                    metrics.inc_packets(bytes.len());
                                    tx_stats.observe(
                                        &packet,
                                        bytes.len(),
                                        udp_send_started_at.elapsed(),
                                        selected_data_plane,
                                        detected_wire_kind,
                                        v2_flags,
                                        mode_changed,
                                        packet_codec,
                                        source_peak,
                                        source_rms,
                                    );
                                    tx_stats.maybe_log(sequence);
                                }
                                Err(err) => warn!(session = %session_id, error = %err, "udp send failed"),
                            }
                            last_sent_mode = active_mode;
                            first_packet = false;
                            sequence = sequence.wrapping_add(1);
                        } else {
                            break;
                        }
                    }
                }
            }

            capture_handle.abort();
            encode_handle.abort();
            if let Some(handle) = watchdog_handle {
                handle.abort();
            }
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
        let frame_duration_ms = if self.cfg.current_audio_mode == AudioMode::UltraLowLatency {
            5
        } else {
            10
        };
        let format = AudioFormat {
            sample_rate_hz: self.cfg.sample_rate,
            channels: self.cfg.channels as u16,
            frame_duration_ms,
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
        let frame_duration_ms = if self.cfg.current_audio_mode == AudioMode::UltraLowLatency {
            5
        } else {
            10
        };
        let format = AudioFormat {
            sample_rate_hz: self.cfg.sample_rate,
            channels: self.cfg.channels as u16,
            frame_duration_ms,
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
    packets: u64,
    bytes_sent: u64,
    last_peak: f32,
    last_rms: f32,
    last_frame_bytes: usize,
    last_wire_bytes: usize,
    last_sample_rate: u32,
    last_channels: u8,
    last_frame_duration_ms: u32,
    udp_send_await_samples: u64,
    udp_send_await_total_ms: f64,
    udp_send_await_max_ms: f64,
    data_plane: DataPlaneFormat,
    last_detected_wire_kind: DataPlanePacketKind,
    last_codec: UdpAudioCodecV2,
    last_v2_flags: u16,
    mode_changed_count: u64,
}

impl TxStats {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            packets: 0,
            bytes_sent: 0,
            last_peak: 0.0,
            last_rms: 0.0,
            last_frame_bytes: 0,
            last_wire_bytes: 0,
            last_sample_rate: 48_000,
            last_channels: 2,
            last_frame_duration_ms: 10,
            udp_send_await_samples: 0,
            udp_send_await_total_ms: 0.0,
            udp_send_await_max_ms: 0.0,
            data_plane: DataPlaneFormat::LegacyLas1,
            last_detected_wire_kind: DataPlanePacketKind::Unknown,
            last_codec: UdpAudioCodecV2::Pcm16,
            last_v2_flags: 0,
            mode_changed_count: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn observe(
        &mut self,
        packet: &UdpAudioPacket,
        wire_bytes: usize,
        udp_send_await: Duration,
        data_plane: DataPlaneFormat,
        detected_wire_kind: DataPlanePacketKind,
        v2_flags: u16,
        mode_changed: bool,
        codec: UdpAudioCodecV2,
        source_peak: f32,
        source_rms: f32,
    ) {
        self.packets += 1;
        self.bytes_sent += wire_bytes as u64;
        self.last_frame_bytes = packet.payload.len();
        self.last_wire_bytes = wire_bytes;
        self.last_sample_rate = packet.sample_rate;
        self.last_channels = packet.channels;
        self.last_frame_duration_ms = if packet.sample_rate == 0 {
            0
        } else {
            (u32::from(packet.frames_per_packet) * 1000) / packet.sample_rate
        };
        self.data_plane = data_plane;
        self.last_detected_wire_kind = detected_wire_kind;
        self.last_codec = codec;
        self.last_v2_flags = v2_flags;
        if mode_changed {
            self.mode_changed_count += 1;
        }

        let udp_send_await_ms = udp_send_await.as_secs_f64() * 1000.0;
        self.udp_send_await_samples += 1;
        self.udp_send_await_total_ms += udp_send_await_ms;
        if udp_send_await_ms > self.udp_send_await_max_ms {
            self.udp_send_await_max_ms = udp_send_await_ms;
        }

        if codec == UdpAudioCodecV2::Pcm16 {
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
        } else {
            self.last_peak = source_peak;
            self.last_rms = source_rms;
        }
    }

    fn maybe_log(&mut self, seq: u32) {
        let elapsed = self.started_at.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }

        let elapsed_secs = elapsed.as_secs_f64().max(f64::EPSILON);
        let tx_packets_per_sec = self.packets as f64 / elapsed_secs;
        let tx_frames_per_sec = tx_packets_per_sec;
        let tx_bytes_per_sec = self.bytes_sent as f64 / elapsed_secs;
        let udp_send_await_avg_ms = if self.udp_send_await_samples == 0 {
            0.0
        } else {
            self.udp_send_await_total_ms / self.udp_send_await_samples as f64
        };
        info!(
            tx_peak = self.last_peak,
            tx_rms = self.last_rms,
            tx_frame_bytes = self.last_frame_bytes,
            tx_packets_per_sec,
            tx_frames_per_sec,
            tx_bytes_per_sec,
            udp_send_await_avg_ms,
            udp_send_await_max_ms = self.udp_send_await_max_ms,
            wire_bytes = self.last_wire_bytes,
            data_plane = %self.data_plane.as_str(),
            detected_wire_kind = ?self.last_detected_wire_kind,
            codec = ?self.last_codec,
            v2_flags_last = self.last_v2_flags,
            mode_changed_count = self.mode_changed_count,
            sample_rate = self.last_sample_rate,
            channels = self.last_channels,
            frame_duration_ms = self.last_frame_duration_ms,
            seq,
            "tx summary"
        );

        *self = Self {
            started_at: Instant::now(),
            data_plane: self.data_plane,
            last_detected_wire_kind: self.last_detected_wire_kind,
            last_codec: self.last_codec,
            last_v2_flags: self.last_v2_flags,
            ..Self::new()
        };
    }
}

struct AudioFrameEncoder {
    codec: CodecSelection,
    output_sample_rate: u32,
    opus: Option<ExperimentalOpusEncoder>,
    opus_frame_buffer: Option<OpusFrameBuffer>,
    /// Phase 4 — when forced into Red tier, fall back to PCM16 regardless of
    /// negotiated codec. The flag is consulted on every `encode()` call.
    force_pcm16_fallback: bool,
    /// Phase 6.4 v6 — persistent resampler for non-48kHz capture inputs.
    /// Re-created only when the input rate changes (very rare). Re-using
    /// the same instance across frames preserves the sinc filter's
    /// internal history buffer, eliminating per-frame edge artifacts
    /// that produced audible "电音/爆音" on 96/192 kHz USB DACs.
    persistent_resampler: Option<PersistentSincResampler>,
}

/// Phase 6.4 v6: persistent stereo sinc resampler. Maintains the
/// rubato `SincFixedIn` instance across calls so sinc convolution
/// has continuous history at frame boundaries. Re-built on input rate
/// change. Holds two scratch planar input vectors so we don't
/// re-allocate per frame.
struct PersistentSincResampler {
    inner: rubato::SincFixedIn<f32>,
    in_rate: u32,
    out_rate: u32,
    in_frames: usize,
    planar_in: [Vec<f32>; 2],
}

impl PersistentSincResampler {
    fn new(in_rate: u32, out_rate: u32, in_frames: usize) -> Option<Self> {
        use rubato::{
            SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
        };
        let params = SincInterpolationParameters {
            sinc_len: 64,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Cubic,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };
        let inner =
            SincFixedIn::<f32>::new(out_rate as f64 / in_rate as f64, 2.0, params, in_frames, 2)
                .ok()?;
        Some(Self {
            inner,
            in_rate,
            out_rate,
            in_frames,
            planar_in: [Vec::with_capacity(in_frames), Vec::with_capacity(in_frames)],
        })
    }

    fn matches(&self, in_rate: u32, out_rate: u32, in_frames: usize) -> bool {
        self.in_rate == in_rate && self.out_rate == out_rate && self.in_frames == in_frames
    }
}

impl AudioFrameEncoder {
    fn new(codec: CodecSelection, initial_mode: AudioMode, output_sample_rate: u32) -> Self {
        let output_sample_rate = normalize_encoder_sample_rate(output_sample_rate);
        let opus = if codec == CodecSelection::Opus {
            match ExperimentalOpusEncoder::new(initial_mode, output_sample_rate) {
                Ok(encoder) => Some(encoder),
                Err(err) => {
                    warn!(error = %err, "opus encoder init failed; falling back to pcm16");
                    None
                }
            }
        } else {
            None
        };
        let opus_frame_buffer = if codec == CodecSelection::Opus {
            Some(OpusFrameBuffer::new(output_sample_rate))
        } else {
            None
        };
        Self {
            codec,
            output_sample_rate,
            opus,
            opus_frame_buffer,
            force_pcm16_fallback: false,
            persistent_resampler: None,
        }
    }

    fn apply_tier_profile(&mut self, profile: TierEncoderProfile, mode: AudioMode) {
        self.force_pcm16_fallback = profile.force_pcm16_fallback;
        if let Some(opus) = self.opus.as_mut() {
            opus.apply_profile(profile, mode);
        }
    }

    fn encode(&mut self, frame: AudioFrame, mode: AudioMode) -> Vec<EncodedFrame> {
        // Phase 6 Hi-Res: PCM24 path skips the 48 kHz resampler and
        // packetizes native-rate samples directly. Returns one EncodedFrame
        // per logical 5 ms (or whatever frame_duration_ms the input has)
        // with codec=Pcm24, sample_rate=actual capture rate, payload =
        // big-endian 24-bit signed interleaved L/R.
        //
        // Red-tier override: if the watchdog is asking for force_pcm16
        // fallback (extreme load), we drop down to Opus first if a
        // pre-built encoder exists, otherwise to PCM16. This protects
        // bandwidth under congestion without changing the negotiated
        // codec on the WS control plane.
        if self.codec == CodecSelection::Pcm24 && !self.force_pcm16_fallback {
            return vec![encode_pcm24_from_native(&frame)];
        }

        let samples = self.resample_to_stereo_10ms(&frame);
        if self.codec == CodecSelection::Opus && !self.force_pcm16_fallback {
            if let (Some(opus), Some(buffer)) =
                (self.opus.as_mut(), self.opus_frame_buffer.as_mut())
            {
                buffer.push_10ms(
                    frame.pts_ms,
                    &samples,
                    frame.is_silence,
                    frame.peak,
                    frame.rms,
                );
                let mut encoded_frames = Vec::with_capacity(2);
                while let Some(opus_input) = buffer.pop_20ms() {
                    match opus.encode(&opus_input, mode) {
                        Ok(encoded) => encoded_frames.push(encoded),
                        Err(err) => {
                            warn!(error = %err, "opus encode failed for one aligned frame; falling back to pcm16");
                            return vec![encode_pcm16_from_samples(
                                &frame,
                                &samples,
                                self.output_sample_rate,
                            )];
                        }
                    }
                }
                return encoded_frames;
            }
        }
        vec![encode_pcm16_from_samples(
            &frame,
            &samples,
            self.output_sample_rate,
        )]
    }

    /// Phase 6.4 v6: stateful resample to fixed 10ms stereo at output rate.
    /// Replaces the per-frame `to_fixed_stereo_10ms` call which created
    /// a new SincFixedIn resampler on every frame, dropping the sinc
    /// filter's history buffer between frames and producing audible
    /// edge artifacts ("电音/爆音") on non-48kHz captures (USB DACs at
    /// 96/192 kHz).
    ///
    /// This helper:
    /// - Uses the same fast path as before when in_rate == out_rate
    /// - Lazily builds a persistent SincFixedIn on first non-48kHz call
    /// - Reuses it across frames (preserving filter history)
    /// - Rebuilds it only if the input rate or frame size changes
    ///   (e.g. when the WASAPI mix format renegotiates)
    fn resample_to_stereo_10ms(&mut self, frame: &AudioFrame) -> Vec<f32> {
        let out_rate = normalize_encoder_sample_rate(self.output_sample_rate);
        let in_rate = frame.format.sample_rate_hz.max(1);
        let in_channels = usize::from(frame.format.channels.max(1));
        let in_frames = frame.samples_f32.len() / in_channels;
        let out_frames: usize = (out_rate as usize / 100).max(1);

        if in_frames == 0 {
            return vec![0.0; out_frames * FIXED_OUTPUT_CHANNELS];
        }

        // Same-rate fast path. No resampler needed; we still drop the
        // persistent one to free memory in case the input was just on
        // a non-default rate and is now back to native.
        if in_rate == out_rate {
            self.persistent_resampler = None;
            return same_rate_stereo_fold(frame, out_frames);
        }

        // Build or rebuild the persistent resampler if rate/frame size
        // changed. Rebuilds are rare — only when the WASAPI mix format
        // renegotiates (driver reload, device switch).
        let needs_rebuild = match self.persistent_resampler.as_ref() {
            Some(r) => !r.matches(in_rate, out_rate, in_frames),
            None => true,
        };
        if needs_rebuild {
            match PersistentSincResampler::new(in_rate, out_rate, in_frames) {
                Some(r) => {
                    self.persistent_resampler = Some(r);
                }
                None => {
                    warn!(
                        in_rate,
                        out_rate,
                        in_frames,
                        "rubato persistent init failed, falling back to nearest"
                    );
                    return to_fixed_stereo_10ms_nearest(frame, out_rate);
                }
            }
        }

        let resampler = self.persistent_resampler.as_mut().unwrap();
        // Stereo-fold input into the persistent planar scratch buffers.
        resampler.planar_in[0].clear();
        resampler.planar_in[1].clear();
        for f in 0..in_frames {
            let base = f * in_channels;
            let l = frame.samples_f32.get(base).copied().unwrap_or(0.0);
            let r = if in_channels > 1 {
                frame.samples_f32.get(base + 1).copied().unwrap_or(l)
            } else {
                l
            };
            resampler.planar_in[0].push(l);
            resampler.planar_in[1].push(r);
        }
        let resampled =
            match rubato::Resampler::process(&mut resampler.inner, &resampler.planar_in, None) {
                Ok(r) => r,
                Err(err) => {
                    warn!(
                        ?err,
                        in_rate,
                        out_rate,
                        "rubato persistent process failed, falling back to nearest"
                    );
                    // Drop the broken resampler so the next call rebuilds.
                    self.persistent_resampler = None;
                    return to_fixed_stereo_10ms_nearest(frame, out_rate);
                }
            };
        let avail = resampled[0].len().min(resampled[1].len());
        let mut out = Vec::with_capacity(out_frames * FIXED_OUTPUT_CHANNELS);
        for f in 0..out_frames {
            if f >= avail {
                let last_l = resampled[0].last().copied().unwrap_or(0.0);
                let last_r = resampled[1].last().copied().unwrap_or(last_l);
                out.push(last_l);
                out.push(last_r);
            } else {
                out.push(resampled[0][f]);
                out.push(resampled[1][f]);
            }
        }
        out
    }
}

/// Same-rate stereo-fold helper used by both the encoder fast path and
/// the legacy `to_fixed_stereo_10ms` standalone helper.
fn same_rate_stereo_fold(frame: &AudioFrame, out_frames: usize) -> Vec<f32> {
    let in_channels = usize::from(frame.format.channels.max(1));
    let in_frames = frame.samples_f32.len() / in_channels;
    let mut out = Vec::with_capacity(out_frames * FIXED_OUTPUT_CHANNELS);
    for out_frame in 0..out_frames {
        let src_frame = out_frame.min(in_frames.saturating_sub(1));
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

const DEFAULT_OUTPUT_SAMPLE_RATE: u32 = 48_000;
const FIXED_OUTPUT_CHANNELS: usize = 2;

#[derive(Debug, Clone)]
struct OpusInputFrame {
    pts_ms: u64,
    samples: Vec<f32>,
    is_silence: bool,
    source_peak: f32,
    source_rms: f32,
}

#[derive(Debug)]
struct OpusFrameBuffer {
    samples_per_20ms_total: usize,
    samples: VecDeque<f32>,
    pending_pts_ms: Option<u64>,
    pending_is_silence: bool,
    pending_peak: f32,
    pending_rms_sum_sq: f64,
    pending_rms_frames: u32,
}

impl OpusFrameBuffer {
    fn new(sample_rate: u32) -> Self {
        let samples_per_20ms_total = (((sample_rate as usize) / 50).max(1)) * FIXED_OUTPUT_CHANNELS;
        Self {
            samples_per_20ms_total,
            samples: VecDeque::new(),
            pending_pts_ms: None,
            pending_is_silence: true,
            pending_peak: 0.0,
            pending_rms_sum_sq: 0.0,
            pending_rms_frames: 0,
        }
    }

    fn push_10ms(
        &mut self,
        pts_ms: u64,
        samples: &[f32],
        is_silence: bool,
        source_peak: f32,
        source_rms: f32,
    ) {
        if self.pending_pts_ms.is_none() {
            self.pending_pts_ms = Some(pts_ms);
            self.pending_is_silence = is_silence;
            self.pending_peak = source_peak;
            self.pending_rms_sum_sq = f64::from(source_rms) * f64::from(source_rms);
            self.pending_rms_frames = 1;
        } else {
            self.pending_is_silence &= is_silence;
            self.pending_peak = self.pending_peak.max(source_peak);
            self.pending_rms_sum_sq += f64::from(source_rms) * f64::from(source_rms);
            self.pending_rms_frames += 1;
        }
        self.samples.extend(samples.iter().copied());
    }

    fn pop_20ms(&mut self) -> Option<OpusInputFrame> {
        if self.samples.len() < self.samples_per_20ms_total {
            return None;
        }
        let pts_ms = self.pending_pts_ms.take()?;
        let mut out = Vec::with_capacity(self.samples_per_20ms_total);
        for _ in 0..self.samples_per_20ms_total {
            out.push(self.samples.pop_front().unwrap_or(0.0));
        }
        let is_silence = self.pending_is_silence;
        let source_peak = self.pending_peak;
        let source_rms = if self.pending_rms_frames == 0 {
            0.0
        } else {
            (self.pending_rms_sum_sq / f64::from(self.pending_rms_frames)).sqrt() as f32
        };
        self.pending_is_silence = true;
        self.pending_peak = 0.0;
        self.pending_rms_sum_sq = 0.0;
        self.pending_rms_frames = 0;

        if !self.samples.is_empty() {
            self.pending_pts_ms = Some(pts_ms.saturating_add(10));
        }

        Some(OpusInputFrame {
            pts_ms,
            samples: out,
            is_silence,
            source_peak,
            source_rms,
        })
    }
}

struct ExperimentalOpusEncoder {
    inner: LibOpusEncoder,
    mode: AudioMode,
    sample_rate: u32,
}

impl ExperimentalOpusEncoder {
    fn new(mode: AudioMode, sample_rate: u32) -> anyhow::Result<Self> {
        let sample_rate = normalize_encoder_sample_rate(sample_rate);
        let mut inner =
            LibOpusEncoder::new(sample_rate, LibOpusChannels::Stereo, opus_application(mode))
                .map_err(|err| anyhow!("opus init: {err}"))?;
        apply_opus_mode_settings(&mut inner, mode);
        Ok(Self {
            inner,
            mode,
            sample_rate,
        })
    }

    /// Apply a tier-specific encoder profile (Phase 4 watchdog feedback).
    /// Resets the running mode so the next encode() will reapply the
    /// per-mode settings on top, then layers on the tier overrides.
    fn apply_profile(&mut self, profile: TierEncoderProfile, mode: AudioMode) {
        // Re-baseline first so multiple tier transitions in a row stay
        // idempotent.
        apply_opus_mode_settings(&mut self.inner, mode);
        self.mode = mode;
        if let Err(err) = self
            .inner
            .set_bitrate(LibOpusBitrate::Bits(profile.bitrate_bps))
        {
            warn!(error = %err, bitrate = profile.bitrate_bps, "tier bitrate apply failed");
        }
        if let Err(err) = self.inner.set_complexity(profile.complexity) {
            warn!(error = %err, complexity = profile.complexity, "tier complexity apply failed");
        }
        if let Err(err) = self.inner.set_vbr(profile.use_vbr) {
            warn!(error = %err, vbr = profile.use_vbr, "tier vbr apply failed");
        }
    }

    fn encode(&mut self, frame: &OpusInputFrame, mode: AudioMode) -> anyhow::Result<EncodedFrame> {
        if self.mode != mode {
            apply_opus_mode_settings(&mut self.inner, mode);
            self.mode = mode;
        }
        let pcm16 = samples_to_i16(&frame.samples);
        let mut payload = vec![0_u8; 4000];
        let encoded_len = self
            .inner
            .encode(&pcm16, &mut payload)
            .map_err(|err| anyhow!("opus encode: {err}"))?;
        payload.truncate(encoded_len);

        Ok(EncodedFrame {
            pts_ms: frame.pts_ms,
            sample_rate: self.sample_rate,
            channels: 2,
            frames_per_packet: (self.sample_rate / 50).max(1) as u16,
            codec: UdpAudioCodecV2::Opus,
            is_silence: frame.is_silence,
            source_peak: frame.source_peak,
            source_rms: frame.source_rms,
            payload,
        })
    }
}

fn apply_opus_mode_settings(encoder: &mut LibOpusEncoder, mode: AudioMode) {
    let (bitrate_bps, complexity, use_vbr) = match mode {
        AudioMode::UltraLowLatency => (48_000, 0, false),
        AudioMode::LowLatency => (64_000, 1, false),
        AudioMode::Balanced => (96_000, 2, true),
        AudioMode::HighQuality => (128_000, 4, true),
    };
    if let Err(err) = encoder.set_application(opus_application(mode)) {
        warn!(error = %err, mode = ?mode, "opus set application failed");
    }
    if let Err(err) = encoder.set_bitrate(LibOpusBitrate::Bits(bitrate_bps)) {
        warn!(error = %err, mode = ?mode, "opus set bitrate failed");
    }
    if let Err(err) = encoder.set_complexity(complexity) {
        warn!(error = %err, mode = ?mode, "opus set complexity failed");
    }
    if let Err(err) = encoder.set_vbr(use_vbr) {
        warn!(error = %err, mode = ?mode, "opus set vbr failed");
    }
    if let Err(err) = encoder.set_inband_fec(false) {
        warn!(error = %err, mode = ?mode, "opus disable inband fec failed");
    }
    if let Err(err) = encoder.set_packet_loss_perc(0) {
        warn!(error = %err, mode = ?mode, "opus set packet loss failed");
    }
    if let Err(err) = encoder.set_dtx(false) {
        warn!(error = %err, mode = ?mode, "opus disable dtx failed");
    }
}

fn opus_application(mode: AudioMode) -> LibOpusApplication {
    match mode {
        AudioMode::UltraLowLatency | AudioMode::LowLatency => LibOpusApplication::LowDelay,
        AudioMode::Balanced | AudioMode::HighQuality => LibOpusApplication::Audio,
    }
}

fn samples_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpusStressStats {
    pub encoded_packets: usize,
    pub p99_encode_us: u64,
    pub channel_full_drop_rate: f64,
}

pub fn run_opus_encoder_stress(
    total_input_frames: usize,
    mode: AudioMode,
) -> anyhow::Result<OpusStressStats> {
    let mut encoder =
        AudioFrameEncoder::new(CodecSelection::Opus, mode, DEFAULT_OUTPUT_SAMPLE_RATE);
    let mut encode_durations_us = Vec::with_capacity(total_input_frames / 2);
    let mut encoded_packets = 0usize;
    let channel_full_drops = 0usize;

    for idx in 0..total_input_frames {
        let pts_ms = (idx as u64) * 10;
        let phase_offset = idx as f32 * 480.0;
        let frame = AudioFrame {
            pts_ms,
            format: AudioFormat {
                sample_rate_hz: 48_000,
                channels: 2,
                sample_format: crate::audio_capture::SampleFormat::F32,
                frame_duration_ms: 10,
            },
            samples_f32: (0..960)
                .map(|sample_idx| {
                    let phase =
                        (phase_offset + sample_idx as f32 / 2.0) * 440.0 * std::f32::consts::TAU
                            / 48_000.0;
                    phase.sin() * 0.2
                })
                .collect(),
            is_silence: false,
            packet_kind: PacketKind::Synthetic,
            peak: 0.2,
            rms: 0.14,
            source_buffer_frames: None,
        };

        let started = Instant::now();
        let encoded = encoder.encode(frame, mode);
        let elapsed_us = started.elapsed().as_micros() as u64;
        if encoded.is_empty() {
            continue;
        }
        if encoded
            .iter()
            .any(|frame| frame.codec != UdpAudioCodecV2::Opus)
        {
            return Err(anyhow!(
                "opus stress helper emitted non-opus packet; encoder fallback was triggered"
            ));
        }
        encode_durations_us.push(elapsed_us);
        encoded_packets += encoded.len();
    }

    if encode_durations_us.is_empty() {
        return Err(anyhow!("opus stress helper produced no encoded packets"));
    }

    encode_durations_us.sort_unstable();
    let p99_index =
        ((encode_durations_us.len() * 99) / 100).min(encode_durations_us.len().saturating_sub(1));
    let p99_encode_us = encode_durations_us[p99_index];
    let channel_full_drop_rate = if encoded_packets == 0 {
        0.0
    } else {
        channel_full_drops as f64 / encoded_packets as f64
    };

    Ok(OpusStressStats {
        encoded_packets,
        p99_encode_us,
        channel_full_drop_rate,
    })
}

#[cfg(test)]
fn encode_passthrough(frame: AudioFrame) -> EncodedFrame {
    let samples = to_fixed_stereo_10ms(&frame, DEFAULT_OUTPUT_SAMPLE_RATE);
    encode_pcm16_from_samples(&frame, &samples, DEFAULT_OUTPUT_SAMPLE_RATE)
}

fn encode_pcm16_from_samples(
    frame: &AudioFrame,
    samples: &[f32],
    output_sample_rate: u32,
) -> EncodedFrame {
    let mut payload = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let v = sample.clamp(-1.0, 1.0);
        let s = (v * i16::MAX as f32) as i16;
        payload.extend_from_slice(&s.to_le_bytes());
    }
    EncodedFrame {
        pts_ms: frame.pts_ms,
        sample_rate: output_sample_rate,
        channels: 2,
        frames_per_packet: ((output_sample_rate / 100).max(1)) as u16,
        codec: UdpAudioCodecV2::Pcm16,
        is_silence: frame.is_silence,
        source_peak: frame.peak,
        source_rms: frame.rms,
        payload,
    }
}

/// Phase 6 PCM24 encoder. Takes the native-rate `AudioFrame` (no
/// resampling) and emits a payload of big-endian 24-bit signed
/// interleaved L/R samples. Sample rate, channels, and frame duration
/// are reported as-is so the v3 wire header can carry them.
///
/// Output bytes-per-frame: `samples_count * 3` (24-bit per sample). For
/// 96 kHz / 5 ms / stereo this is 960 samples × 3 = 2880 B, which
/// the transport layer's fragmentation logic splits into 2 packets to
/// fit MTU.
/// Phase 6.4 Hi-Res cap. Per the design spec
/// (`docs/hires_pcm24.md` Q1), PCM24 only supports up to 96 kHz to keep
/// LAN bandwidth bounded and packet fragmentation simple. When the
/// capture device runs at a higher native rate (some Windows mix
/// formats are 192 kHz / 384 kHz), we resample down to 96 kHz before
/// emitting the 24-bit payload. Below 96 kHz we pass through native.
const MAX_PCM24_OUTPUT_SAMPLE_RATE: u32 = 96_000;

fn pcm24_target_sample_rate(input_rate: u32) -> u32 {
    if input_rate > MAX_PCM24_OUTPUT_SAMPLE_RATE {
        MAX_PCM24_OUTPUT_SAMPLE_RATE
    } else {
        input_rate
    }
}

fn encode_pcm24_from_native(frame: &AudioFrame) -> EncodedFrame {
    let in_channels = usize::from(frame.format.channels.max(1));
    let in_rate = frame.format.sample_rate_hz.max(1);
    let target_rate = pcm24_target_sample_rate(in_rate);

    // Build the working float sample stream. When the native capture
    // rate exceeds the PCM24 cap we run a sinc resampler (rubato) so
    // wire bandwidth stays sane (~4.6 Mbps at 96 kHz vs ~18 Mbps at
    // 384 kHz). Same-rate fast path skips the resampler.
    let resampled: Option<Vec<f32>> = if target_rate != in_rate {
        Some(resample_stereo_for_pcm24(frame, in_rate, target_rate))
    } else {
        None
    };
    let (samples_f32, working_channels): (&[f32], usize) = match resampled.as_ref() {
        Some(r) => (r.as_slice(), 2),
        None => (frame.samples_f32.as_slice(), in_channels),
    };
    let in_frames = samples_f32.len() / working_channels;
    let out_channels: u8 = 2;
    let mut payload = Vec::with_capacity(in_frames * out_channels as usize * 3);

    // 24-bit signed integer range. Multiplying by 2^23 - 1 maps -1.0..1.0
    // to the full signed-24 range without overflow.
    const SCALE: f32 = 8_388_607.0; // 2^23 - 1
    for f in 0..in_frames {
        let base = f * working_channels;
        let l = samples_f32.get(base).copied().unwrap_or(0.0);
        let r = if working_channels > 1 {
            samples_f32.get(base + 1).copied().unwrap_or(l)
        } else {
            l
        };
        let l_i32 = (l.clamp(-1.0, 1.0) * SCALE) as i32;
        let r_i32 = (r.clamp(-1.0, 1.0) * SCALE) as i32;
        // Big-endian 24-bit: drop the most-significant byte from i32.
        // sample = bits[23..0]; emit bytes high → low.
        for &v in &[l_i32, r_i32] {
            payload.push(((v >> 16) & 0xFF) as u8);
            payload.push(((v >> 8) & 0xFF) as u8);
            payload.push((v & 0xFF) as u8);
        }
    }

    EncodedFrame {
        pts_ms: frame.pts_ms,
        sample_rate: target_rate,
        channels: out_channels,
        frames_per_packet: in_frames as u16,
        codec: UdpAudioCodecV2::Pcm24,
        is_silence: frame.is_silence,
        source_peak: frame.peak,
        source_rms: frame.rms,
        payload,
    }
}

/// Resample the working `AudioFrame` to `out_rate` for the PCM24 path.
/// Returns interleaved L/R `Vec<f32>` at the target rate. Falls back to
/// nearest-neighbor on rubato failure, mirroring the Opus resampler's
/// degradation path.
fn resample_stereo_for_pcm24(frame: &AudioFrame, in_rate: u32, out_rate: u32) -> Vec<f32> {
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };
    let in_channels = usize::from(frame.format.channels.max(1));
    let in_frames = frame.samples_f32.len() / in_channels;
    if in_frames == 0 || in_rate == out_rate {
        return frame.samples_f32.clone();
    }
    // Build planar input: L channel, R channel.
    let mut planar_in = vec![Vec::with_capacity(in_frames); 2];
    for f in 0..in_frames {
        let base = f * in_channels;
        let l = frame.samples_f32.get(base).copied().unwrap_or(0.0);
        let r = if in_channels > 1 {
            frame.samples_f32.get(base + 1).copied().unwrap_or(l)
        } else {
            l
        };
        planar_in[0].push(l);
        planar_in[1].push(r);
    }
    let params = SincInterpolationParameters {
        sinc_len: 64,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Cubic,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    let resampler =
        SincFixedIn::<f32>::new(out_rate as f64 / in_rate as f64, 2.0, params, in_frames, 2);
    let mut resampler = match resampler {
        Ok(r) => r,
        Err(err) => {
            warn!(?err, in_rate, out_rate, "pcm24 rubato init failed");
            return frame.samples_f32.clone();
        }
    };
    let resampled = match resampler.process(&planar_in, None) {
        Ok(r) => r,
        Err(err) => {
            warn!(?err, in_rate, out_rate, "pcm24 rubato process failed");
            return frame.samples_f32.clone();
        }
    };
    // Interleave back to L/R.
    let out_frames = resampled[0].len();
    let mut interleaved = Vec::with_capacity(out_frames * 2);
    for f in 0..out_frames {
        interleaved.push(resampled[0][f]);
        interleaved.push(resampled[1].get(f).copied().unwrap_or(resampled[0][f]));
    }
    interleaved
}

fn legacy_flags_for_frame(frame: &EncodedFrame) -> u8 {
    // TODO(protocol-v2): map these semantics to the new 16-bit flags field.
    // `config_changed` and `discontinuity` are intentionally left as reserved
    // insertion points for future mode/sample-format transitions.
    let mut flags: u16 = 0;
    if frame.is_silence {
        flags |= UDP_FLAG_V2_SILENCE;
    }
    let _reserved_config_changed = UDP_FLAG_V2_CONFIG_CHANGED;
    let _reserved_discontinuity = UDP_FLAG_V2_DISCONTINUITY;

    (flags & 0xFF) as u8
}

fn v2_flags_for_frame(packet: &UdpAudioPacket, mode_changed: bool, first_packet: bool) -> u16 {
    let mut flags: u16 = 0;
    if packet.flags & 0x01 != 0 {
        flags |= UDP_FLAG_V2_SILENCE;
    }
    if mode_changed {
        flags |= UDP_FLAG_V2_CONFIG_CHANGED | UDP_FLAG_V2_DISCONTINUITY;
    } else if first_packet {
        flags |= UDP_FLAG_V2_DISCONTINUITY;
    }
    flags
}

#[cfg(test)]
fn select_data_plane_format(
    desired: DataPlaneFormat,
    audio_source: AudioSourceKind,
    _allow_loopback_v2_header_gray: bool,
) -> DataPlaneFormat {
    if desired != DataPlaneFormat::V2Header {
        return desired;
    }

    match audio_source {
        AudioSourceKind::Synthetic => desired,
        AudioSourceKind::WindowsLoopback => desired,
    }
}

fn read_current_audio_mode(mode: &Arc<StdMutex<AudioMode>>) -> AudioMode {
    *mode.lock().expect("current_audio_mode lock")
}

fn to_fixed_stereo_10ms(frame: &AudioFrame, out_rate: u32) -> Vec<f32> {
    let out_rate = normalize_encoder_sample_rate(out_rate);
    const OUT_CHANNELS: usize = FIXED_OUTPUT_CHANNELS;
    let out_frames: usize = (out_rate as usize / 100).max(1);

    let in_rate = frame.format.sample_rate_hz.max(1);
    let in_channels = usize::from(frame.format.channels.max(1));
    let in_frames = frame.samples_f32.len() / in_channels;
    if in_frames == 0 {
        return vec![0.0; out_frames * OUT_CHANNELS];
    }

    // Fast path: same sample rate. Common case — Windows mix format is
    // 48 kHz on ~99% of devices, output rate is 48 kHz, no resampling
    // needed. Just stereo-fold and copy.
    if in_rate == out_rate {
        let mut out = Vec::with_capacity(out_frames * OUT_CHANNELS);
        for out_frame in 0..out_frames {
            let src_frame = out_frame.min(in_frames - 1);
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
        return out;
    }

    // Phase 6.2 Sinc resample. Replaces the previous nearest-neighbor
    // implementation which aliased badly on rates like 96→48. We pay the
    // setup cost once per frame which is acceptable because (a) the
    // resampler is only constructed on the rare non-48 kHz capture path
    // and (b) our 10 ms frame is short enough that the FFT/conv work is
    // dominated by per-frame overhead anyway.
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };

    // First, stereo-fold the input into two planar channels, downmixing
    // anything beyond stereo by averaging. Build the two channel vectors
    // explicitly so each gets its own real capacity (the `vec![X; n]`
    // form clones the same Vec and discards the inner capacity).
    let mut planar_in: Vec<Vec<f32>> =
        vec![Vec::with_capacity(in_frames), Vec::with_capacity(in_frames)];
    for f in 0..in_frames {
        let base = f * in_channels;
        let l = frame.samples_f32.get(base).copied().unwrap_or(0.0);
        let r = if in_channels > 1 {
            frame.samples_f32.get(base + 1).copied().unwrap_or(l)
        } else {
            l
        };
        planar_in[0].push(l);
        planar_in[1].push(r);
    }

    let params = SincInterpolationParameters {
        sinc_len: 64,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Cubic,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    let resampler =
        SincFixedIn::<f32>::new(out_rate as f64 / in_rate as f64, 2.0, params, in_frames, 2);
    let mut resampler = match resampler {
        Ok(r) => r,
        Err(err) => {
            warn!(
                ?err,
                in_rate, out_rate, "rubato init failed, falling back to nearest"
            );
            return to_fixed_stereo_10ms_nearest(frame, out_rate);
        }
    };

    let resampled = match resampler.process(&planar_in, None) {
        Ok(r) => r,
        Err(err) => {
            warn!(
                ?err,
                in_rate, out_rate, "rubato process failed, falling back to nearest"
            );
            return to_fixed_stereo_10ms_nearest(frame, out_rate);
        }
    };

    // The fixed-in resampler may produce slightly more or fewer output
    // frames than out_frames; truncate or zero-pad to match the
    // contract. In practice for integer rate ratios (24→48, 48→24) it
    // matches exactly.
    let mut out = Vec::with_capacity(out_frames * OUT_CHANNELS);
    let avail = resampled[0].len().min(resampled[1].len());
    for f in 0..out_frames {
        let l = resampled[0].get(f).copied().unwrap_or(0.0);
        let r = resampled[1].get(f).copied().unwrap_or(l);
        if f >= avail {
            // Defensive: if rubato returned fewer frames than expected,
            // pad with the last available sample to avoid an audible
            // hard-zero edge.
            let last_l = resampled[0].last().copied().unwrap_or(0.0);
            let last_r = resampled[1].last().copied().unwrap_or(last_l);
            out.push(last_l);
            out.push(last_r);
        } else {
            out.push(l);
            out.push(r);
        }
    }
    out
}

/// Fallback nearest-neighbor resampler. Only used when rubato init or
/// process fails (very rare). Identical to the pre-Phase-6.2 behavior.
fn to_fixed_stereo_10ms_nearest(frame: &AudioFrame, out_rate: u32) -> Vec<f32> {
    const OUT_CHANNELS: usize = FIXED_OUTPUT_CHANNELS;
    let out_frames: usize = (out_rate as usize / 100).max(1);

    let in_rate = frame.format.sample_rate_hz.max(1);
    let in_channels = usize::from(frame.format.channels.max(1));
    let in_frames = frame.samples_f32.len() / in_channels;
    if in_frames == 0 {
        return vec![0.0; out_frames * OUT_CHANNELS];
    }

    let mut out = Vec::with_capacity(out_frames * OUT_CHANNELS);
    for out_frame in 0..out_frames {
        let src_frame = ((out_frame as u64 * in_rate as u64) / out_rate as u64)
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

fn normalize_encoder_sample_rate(sample_rate: u32) -> u32 {
    match sample_rate {
        8_000 | 12_000 | 16_000 | 24_000 | 48_000 => sample_rate,
        _ => DEFAULT_OUTPUT_SAMPLE_RATE,
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
    use crate::audio_capture::SampleFormat;
    use lan_audio_protocol::UdpAudioPacketV2;

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
        assert_eq!(encoded.codec, UdpAudioCodecV2::Pcm16);
        assert_eq!(encoded.payload.len(), 1920);
    }

    #[test]
    fn opus_encoder_aligns_to_fixed_20ms_packets() {
        let frame_a = AudioFrame {
            pts_ms: 123,
            format: AudioFormat {
                sample_rate_hz: 48_000,
                channels: 2,
                sample_format: SampleFormat::F32,
                frame_duration_ms: 10,
            },
            samples_f32: vec![0.1; 480 * 2],
            is_silence: false,
            packet_kind: PacketKind::Synthetic,
            peak: 0.1,
            rms: 0.1,
            source_buffer_frames: None,
        };
        let mut encoder = AudioFrameEncoder::new(
            CodecSelection::Opus,
            AudioMode::Balanced,
            DEFAULT_OUTPUT_SAMPLE_RATE,
        );

        let first = encoder.encode(frame_a, AudioMode::Balanced);
        assert!(
            first.is_empty(),
            "first 10ms chunk should wait for 20ms alignment"
        );

        let frame_b = AudioFrame {
            pts_ms: 133,
            format: AudioFormat {
                sample_rate_hz: 48_000,
                channels: 2,
                sample_format: SampleFormat::F32,
                frame_duration_ms: 10,
            },
            samples_f32: vec![0.1; 480 * 2],
            is_silence: false,
            packet_kind: PacketKind::Synthetic,
            peak: 0.1,
            rms: 0.1,
            source_buffer_frames: None,
        };
        let encoded = encoder.encode(frame_b, AudioMode::Balanced);
        assert_eq!(encoded.len(), 1);
        let encoded = &encoded[0];

        assert_eq!(encoded.codec, UdpAudioCodecV2::Opus);
        assert_eq!(encoded.sample_rate, 48_000);
        assert_eq!(encoded.channels, 2);
        assert_eq!(encoded.frames_per_packet, 960);
        assert!(!encoded.payload.is_empty());
        assert!(encoded.payload.len() < 1920);
    }

    #[test]
    fn opus_roundtrip_decodes_non_silent_pcm() {
        let build_frame = |pts_ms| AudioFrame {
            pts_ms,
            format: AudioFormat {
                sample_rate_hz: 48_000,
                channels: 2,
                sample_format: SampleFormat::F32,
                frame_duration_ms: 10,
            },
            samples_f32: (0..960)
                .map(|i| {
                    let phase = (i as f32 / 2.0) * 440.0 * std::f32::consts::TAU / 48_000.0;
                    phase.sin() * 0.2
                })
                .collect(),
            is_silence: false,
            packet_kind: PacketKind::Synthetic,
            peak: 0.2,
            rms: 0.14,
            source_buffer_frames: None,
        };
        let mut encoder = AudioFrameEncoder::new(
            CodecSelection::Opus,
            AudioMode::Balanced,
            DEFAULT_OUTPUT_SAMPLE_RATE,
        );
        assert!(encoder
            .encode(build_frame(123), AudioMode::Balanced)
            .is_empty());
        let encoded = encoder.encode(build_frame(133), AudioMode::Balanced);
        assert_eq!(encoded.len(), 1);
        let encoded = &encoded[0];
        let mut decoder =
            opus::Decoder::new(48_000, LibOpusChannels::Stereo).expect("standard opus decoder");
        let mut out = vec![0_i16; 1920];

        let decoded = decoder
            .decode(&encoded.payload, &mut out, false)
            .expect("standard opus decode");
        let peak = out
            .iter()
            .take(decoded * 2)
            .fold(0_i16, |acc, sample| acc.max(sample.abs()));

        assert_eq!(decoded, 960);
        assert!(
            peak > 300,
            "opus roundtrip decoded near-silence peak={peak}"
        );
    }

    #[test]
    fn select_data_plane_keeps_v2_for_synthetic_and_loopback() {
        assert_eq!(
            select_data_plane_format(DataPlaneFormat::V2Header, AudioSourceKind::Synthetic, false,),
            DataPlaneFormat::V2Header
        );
        assert_eq!(
            select_data_plane_format(
                DataPlaneFormat::V2Header,
                AudioSourceKind::WindowsLoopback,
                false,
            ),
            DataPlaneFormat::V2Header
        );
    }

    #[test]
    fn select_data_plane_keeps_loopback_v2_even_with_legacy_gray_flag() {
        assert_eq!(
            select_data_plane_format(
                DataPlaneFormat::V2Header,
                AudioSourceKind::WindowsLoopback,
                true,
            ),
            DataPlaneFormat::V2Header
        );
    }

    #[test]
    fn v2_flags_include_config_changed_and_discontinuity_on_mode_change() {
        let packet = UdpAudioPacket {
            version: 1,
            flags: 0,
            sequence: 1,
            timestamp_ms: 1,
            sample_rate: 48_000,
            channels: 2,
            frames_per_packet: 480,
            payload: vec![1, 2, 3, 4],
        };
        let flags = v2_flags_for_frame(&packet, true, false);
        assert_ne!(flags & UDP_FLAG_V2_CONFIG_CHANGED, 0);
        assert_ne!(flags & UDP_FLAG_V2_DISCONTINUITY, 0);
    }

    #[test]
    fn encode_packet_by_data_plane_switches_magic() {
        let packet = UdpAudioPacket {
            version: 1,
            flags: 0,
            sequence: 1,
            timestamp_ms: 1,
            sample_rate: 48_000,
            channels: 2,
            frames_per_packet: 480,
            payload: vec![1, 2, 3, 4],
        };
        let v1 = encode_packet_by_data_plane(
            &packet,
            DataPlaneFormat::LegacyLas1,
            0,
            UdpAudioCodecV2::Pcm16,
        );
        assert_eq!(&v1[0..4], b"LAS1");

        let v2 = encode_packet_by_data_plane(
            &packet,
            DataPlaneFormat::V2Header,
            0,
            UdpAudioCodecV2::Pcm16,
        );
        assert_eq!(&v2[0..4], b"LAV2");
    }

    #[test]
    fn v2_header_carries_opus_codec() {
        let packet = UdpAudioPacket {
            version: 1,
            flags: 0,
            sequence: 7,
            timestamp_ms: 100,
            sample_rate: 48_000,
            channels: 2,
            frames_per_packet: 480,
            payload: vec![1, 2, 3, 4],
        };
        let bytes = encode_packet_by_data_plane(
            &packet,
            DataPlaneFormat::V2Header,
            0,
            UdpAudioCodecV2::Opus,
        );
        let decoded = UdpAudioPacketV2::decode(&bytes).expect("decode v2");
        assert_eq!(decoded.header.codec, UdpAudioCodecV2::Opus);
    }

    /// Phase 6 fragmentation. PCM24 96 kHz / 5 ms / stereo = 2880 B
    /// payload, must split into 2 frags of 1392 + 1488 bytes (well, 1488
    /// would exceed our 1392 cap so it splits 1392+1392+96 → 3 frags).
    /// Verify the helper produces the right number of v3 wire packets,
    /// the headers carry consistent logical_seq, and frag indices/totals
    /// are right.
    #[test]
    fn encode_packets_pcm24_fragments_above_mtu() {
        use lan_audio_protocol::UdpAudioPacketV3;

        // Synthetic 2880 B payload (96 kHz / 5 ms / stereo / 24bit).
        let payload = vec![0xAB; 2880];
        let packet = UdpAudioPacket {
            version: 1,
            flags: 0,
            sequence: 100,
            timestamp_ms: 12_345,
            sample_rate: 96_000,
            channels: 2,
            frames_per_packet: 480, // 96 kHz × 5 ms
            payload,
        };
        let mut next_seq = 100u32;
        let frags = encode_packets_by_data_plane(
            &packet,
            DataPlaneFormat::V2Header,
            0,
            UdpAudioCodecV2::Pcm24,
            &mut next_seq,
        );
        // 2880 / 1392 = 2.07 → 3 frags
        assert_eq!(frags.len(), 3);

        // Each frag should decode cleanly and share logical_seq=100.
        for (i, bytes) in frags.iter().enumerate() {
            let v3 = UdpAudioPacketV3::decode(bytes).expect("decode v3 frag");
            assert_eq!(v3.header.codec, UdpAudioCodecV2::Pcm24);
            assert_eq!(v3.header.sample_rate, 96_000);
            assert_eq!(v3.header.logical_seq, 100);
            assert_eq!(v3.header.total_frags, 3);
            assert_eq!(v3.header.frag_index, i as u8);
            assert_eq!(v3.header.sequence, 100 + i as u32);
        }
        assert_eq!(next_seq, 103);
    }

    /// Phase 6 single-packet path. PCM24 48 kHz / 5 ms / stereo = 1440 B
    /// payload, exceeds the 1392 cap by exactly 48 bytes so it splits to
    /// 2 frags. Sanity check the boundary case.
    #[test]
    fn encode_packets_pcm24_single_frag_at_48k() {
        // 48 kHz / 10 ms / stereo / 24bit = 480 × 2 × 3 = 2880 B → 3 frags
        // 48 kHz / 5 ms / stereo / 24bit = 240 × 2 × 3 = 1440 B → 2 frags
        // 48 kHz / 3 ms / stereo / 24bit = 144 × 2 × 3 = 864 B → 1 frag
        let payload = vec![0xCD; 864]; // 3 ms / 48 kHz
        let packet = UdpAudioPacket {
            version: 1,
            flags: 0,
            sequence: 7,
            timestamp_ms: 0,
            sample_rate: 48_000,
            channels: 2,
            frames_per_packet: 144,
            payload,
        };
        let mut next_seq = 7u32;
        let frags = encode_packets_by_data_plane(
            &packet,
            DataPlaneFormat::V2Header,
            0,
            UdpAudioCodecV2::Pcm24,
            &mut next_seq,
        );
        assert_eq!(frags.len(), 1);
        assert_eq!(next_seq, 8);
    }

    /// Phase 6 non-PCM24 codecs must keep the v1.9.x single-packet
    /// behavior so the fragmentation helper is a strict superset of
    /// `encode_packet_by_data_plane`.
    #[test]
    fn encode_packets_non_pcm24_returns_single_packet() {
        let packet = UdpAudioPacket {
            version: 1,
            flags: 0,
            sequence: 1,
            timestamp_ms: 0,
            sample_rate: 48_000,
            channels: 2,
            frames_per_packet: 480,
            payload: vec![1, 2, 3, 4],
        };
        let mut next_seq = 1u32;
        let frags = encode_packets_by_data_plane(
            &packet,
            DataPlaneFormat::V2Header,
            0,
            UdpAudioCodecV2::Opus,
            &mut next_seq,
        );
        assert_eq!(frags.len(), 1);
        assert_eq!(&frags[0][0..4], b"LAV2");
        assert_eq!(next_seq, 2);
    }

    /// Phase 5 encode worker smoke test. Spins up a real worker thread,
    /// sends one job that targets two simulated PCM16 wifi clients, and
    /// asserts the worker emits one wire frame per recipient and advances
    /// the shared sequence counter exactly once per encoded packet.
    #[test]
    fn encode_worker_emits_one_wire_frame_per_recipient() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let (job_tx, job_rx) = std_mpsc::channel::<EncodeJob>();
        let (result_tx, mut result_rx) = mpsc::unbounded_channel::<EncodeResult>();
        let worker = spawn_encode_worker(job_rx, result_tx);

        let frame = AudioFrame {
            pts_ms: 0,
            format: AudioFormat {
                sample_rate_hz: 48_000,
                channels: 2,
                sample_format: SampleFormat::F32,
                frame_duration_ms: 10,
            },
            samples_f32: vec![0.0; 960 * 2],
            is_silence: true,
            packet_kind: PacketKind::SilentPacket,
            peak: 0.0,
            rms: 0.0,
            source_buffer_frames: None,
        };

        let make_client = |id: Uuid, port: u16| BroadcastClient {
            id,
            name: format!("test-{}", port),
            data_plane: DataPlaneFormat::LegacyLas1,
            codec: CodecSelection::Pcm16,
            audio_mode: AudioMode::Balanced,
            preferred_sample_rate: 48_000,
            transport: ClientTransportSnapshot::Wifi(
                format!("127.0.0.1:{}", port).parse().expect("addr"),
            ),
            first_packet: true,
            mode_changed: false,
        };
        let clients = vec![
            make_client(Uuid::new_v4(), 60_001),
            make_client(Uuid::new_v4(), 60_002),
        ];

        let sequence = Arc::new(AtomicU32::new(42));
        let job = EncodeJob {
            frame,
            clients: clients.clone(),
            active_tier: DegradationTier::Green,
            sequence: Arc::clone(&sequence),
            job_received_at: Instant::now(),
        };

        job_tx.send(job).expect("send job");

        // Drain on the same thread without a runtime — `try_recv` polls.
        let mut received: Option<EncodeResult> = None;
        for _ in 0..100 {
            if let Ok(result) = result_rx.try_recv() {
                received = Some(result);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let result = received.expect("worker must produce a result");

        // Two recipients on the same (codec, mode, rate) -> one encoded
        // packet, two wire frames.
        assert_eq!(result.wire_frames.len(), 2);
        // Sequence counter advanced exactly once for the single packet.
        assert_eq!(sequence.load(Ordering::Relaxed), 43);
        // Both wire frames carry the same sequence number (one packet,
        // fanned out to two recipients).
        assert_eq!(result.wire_frames[0].packet.sequence, 42);
        assert_eq!(result.wire_frames[1].packet.sequence, 42);
        // Both targeted at legacy_las1 magic.
        assert_eq!(
            result.wire_frames[0].detected_wire_kind,
            DataPlanePacketKind::LegacyLas1,
        );

        drop(job_tx);
        worker.join().expect("worker join");
    }
}
