use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use lan_audio_protocol::{
    detect_data_plane_packet_kind, AudioMode, DataPlanePacketKind, UdpAudioCodecV2,
    UdpAudioHeaderV2, UdpAudioPacket, UdpAudioPacketV2, PROTOCOL_VERSION_V2,
    UDP_AUDIO_HEADER_V2_LEN, UDP_AUDIO_MAGIC_V2, UDP_FLAG_V2_CONFIG_CHANGED,
    UDP_FLAG_V2_DISCONTINUITY, UDP_FLAG_V2_SILENCE,
};
use opus::{
    Application as LibOpusApplication, Bitrate as LibOpusBitrate, Channels as LibOpusChannels,
    Encoder as LibOpusEncoder,
};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

use crate::audio_capture::{
    AudioCaptureSource, AudioFormat, AudioFrame, CaptureDebugDumpConfig, CaptureError,
    CaptureSourceState, PacketKind, SyntheticAudioSource, WindowsLoopbackCapture,
};
use crate::config::{
    AudioSourceKind, CodecSelection, DataPlaneFormat, ServerConfig, SyntheticSignalKind,
    TransportMode,
};
use crate::metrics::Metrics;
use crate::session::{
    write_length_prefixed_frame, BroadcastClient, ClientRegistry, ClientTransportSnapshot,
};

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
        self.metrics.set_capture_source_state(source.state().as_str());
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
        self.metrics.set_capture_source_state(source.state().as_str());
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

        let mut encoders: HashMap<(CodecSelection, AudioMode), AudioFrameEncoder> = HashMap::new();
        let mut sequence: u32 = 0;
        let mut tx_stats = TxStats::new();

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
                    self.broadcast_frame(
                        frame,
                        &clients,
                        &mut encoders,
                        &mut sequence,
                        &mut tx_stats,
                    ).await;
                }
            }
        }

        capture_handle.abort();
        Ok(())
    }

    async fn broadcast_frame(
        &self,
        frame: AudioFrame,
        clients: &[BroadcastClient],
        encoders: &mut HashMap<(CodecSelection, AudioMode), AudioFrameEncoder>,
        sequence: &mut u32,
        tx_stats: &mut TxStats,
    ) {
        let mut grouped: HashMap<(CodecSelection, AudioMode), Vec<BroadcastClient>> = HashMap::new();
        for client in clients.iter().cloned() {
            grouped
                .entry((client.codec, client.audio_mode))
                .or_default()
                .push(client);
        }

        let mut failed_clients = Vec::new();
        for ((codec, mode), recipients) in grouped {
            let encoded_frames = encoders
                .entry((codec, mode))
                .or_insert_with(|| AudioFrameEncoder::new(codec, mode))
                .encode(frame.clone(), mode);
            for encoded in encoded_frames {
                let packet = UdpAudioPacket {
                    version: 1,
                    flags: legacy_flags_for_frame(&encoded),
                    sequence: *sequence,
                    timestamp_ms: encoded.pts_ms,
                    sample_rate: encoded.sample_rate,
                    channels: encoded.channels,
                    frames_per_packet: encoded.frames_per_packet,
                    payload: encoded.payload,
                };
                *sequence = (*sequence).wrapping_add(1);

                for client in &recipients {
                    let v2_flags = v2_flags_for_frame(&packet, client.mode_changed, client.first_packet);
                    let bytes = encode_packet_by_data_plane(
                        &packet,
                        client.data_plane,
                        v2_flags,
                        encoded.codec,
                    );
                    let send_result = match &client.transport {
                        ClientTransportSnapshot::Wifi(addr) => self
                            .socket
                            .send_to(&bytes, addr)
                            .await
                            .map(|_| ())
                            .map_err(anyhow::Error::from),
                        ClientTransportSnapshot::Usb(writer) => write_length_prefixed_frame(writer, &bytes).await,
                    };
                    match send_result {
                        Ok(()) => {
                            self.metrics.inc_packets(bytes.len());
                            tx_stats.observe(
                                &packet,
                                bytes.len(),
                                Duration::from_millis(0),
                                client.data_plane,
                                detect_data_plane_packet_kind(&bytes),
                                v2_flags,
                                client.mode_changed,
                                encoded.codec,
                                encoded.source_peak,
                                encoded.source_rms,
                            );
                        }
                        Err(err) => {
                            warn!(client = %client.name, error = %err, "broadcast send failed");
                            failed_clients.push(client.id);
                        }
                    }
                }
                tx_stats.maybe_log(*sequence);
            }
        }

        failed_clients.sort_unstable();
        failed_clients.dedup();
        for client_id in failed_clients {
            let _ = self.registry.remove_client(client_id).await;
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

        let handle = tokio::spawn(async move {
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
            let encode_handle = tokio::spawn(async move {
                let mut packet_build_stats = PacketBuildStatsWindow::new();
                let mut frame_encoder = AudioFrameEncoder::new(
                    effective_codec,
                    read_current_audio_mode(&encode_audio_mode),
                );
                loop {
                    tokio::select! {
                        _ = encode_shutdown.recv() => break,
                        maybe_frame = tokio::time::timeout(Duration::from_millis(30), frame_rx.recv()) => {
                            match maybe_frame {
                                Ok(Some(frame)) => {
                                    let frame_age_ms_before_build = now_ms().saturating_sub(frame.pts_ms);
                                    let active_mode = read_current_audio_mode(&encode_audio_mode);
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
            });

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
    opus: Option<ExperimentalOpusEncoder>,
    opus_frame_buffer: Option<OpusFrameBuffer>,
}

impl AudioFrameEncoder {
    fn new(codec: CodecSelection, initial_mode: AudioMode) -> Self {
        let opus = if codec == CodecSelection::Opus {
            match ExperimentalOpusEncoder::new(initial_mode) {
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
            Some(OpusFrameBuffer::default())
        } else {
            None
        };
        Self {
            codec,
            opus,
            opus_frame_buffer,
        }
    }

    fn encode(&mut self, frame: AudioFrame, mode: AudioMode) -> Vec<EncodedFrame> {
        let samples = to_fixed_48k_stereo_10ms(&frame);
        if self.codec == CodecSelection::Opus {
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
                            return vec![encode_pcm16_from_samples(&frame, &samples)];
                        }
                    }
                }
                return encoded_frames;
            }
        }
        vec![encode_pcm16_from_samples(&frame, &samples)]
    }
}

const FIXED_OUTPUT_SAMPLE_RATE: u32 = 48_000;
const FIXED_OUTPUT_CHANNELS: usize = 2;
const PCM_FRAME_SAMPLES_PER_CHANNEL_10MS: usize = 480;
const OPUS_FRAME_SAMPLES_PER_CHANNEL_20MS: usize = 960;
const OPUS_FRAME_TOTAL_SAMPLES_20MS: usize =
    OPUS_FRAME_SAMPLES_PER_CHANNEL_20MS * FIXED_OUTPUT_CHANNELS;

#[derive(Debug, Clone)]
struct OpusInputFrame {
    pts_ms: u64,
    samples: Vec<f32>,
    is_silence: bool,
    source_peak: f32,
    source_rms: f32,
}

#[derive(Debug, Default)]
struct OpusFrameBuffer {
    samples: VecDeque<f32>,
    pending_pts_ms: Option<u64>,
    pending_is_silence: bool,
    pending_peak: f32,
    pending_rms_sum_sq: f64,
    pending_rms_frames: u32,
}

impl OpusFrameBuffer {
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
        if self.samples.len() < OPUS_FRAME_TOTAL_SAMPLES_20MS {
            return None;
        }
        let pts_ms = self.pending_pts_ms.take()?;
        let mut out = Vec::with_capacity(OPUS_FRAME_TOTAL_SAMPLES_20MS);
        for _ in 0..OPUS_FRAME_TOTAL_SAMPLES_20MS {
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
}

impl ExperimentalOpusEncoder {
    fn new(mode: AudioMode) -> anyhow::Result<Self> {
        let mut inner = LibOpusEncoder::new(
            FIXED_OUTPUT_SAMPLE_RATE,
            LibOpusChannels::Stereo,
            opus_application(mode),
        )
        .map_err(|err| anyhow!("opus init: {err}"))?;
        apply_opus_mode_settings(&mut inner, mode);
        Ok(Self { inner, mode })
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
            sample_rate: FIXED_OUTPUT_SAMPLE_RATE,
            channels: 2,
            frames_per_packet: OPUS_FRAME_SAMPLES_PER_CHANNEL_20MS as u16,
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
        AudioMode::LowLatency => LibOpusApplication::LowDelay,
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
    let mut encoder = AudioFrameEncoder::new(CodecSelection::Opus, mode);
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
    let samples = to_fixed_48k_stereo_10ms(&frame);
    encode_pcm16_from_samples(&frame, &samples)
}

fn encode_pcm16_from_samples(frame: &AudioFrame, samples: &[f32]) -> EncodedFrame {
    // v1/default path remains fixed for Android diagnostics: 48kHz stereo PCM16 LE, 10ms.
    let mut payload = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let v = sample.clamp(-1.0, 1.0);
        let s = (v * i16::MAX as f32) as i16;
        payload.extend_from_slice(&s.to_le_bytes());
    }
    EncodedFrame {
        pts_ms: frame.pts_ms,
        sample_rate: FIXED_OUTPUT_SAMPLE_RATE,
        channels: 2,
        frames_per_packet: PCM_FRAME_SAMPLES_PER_CHANNEL_10MS as u16,
        codec: UdpAudioCodecV2::Pcm16,
        is_silence: frame.is_silence,
        source_peak: frame.peak,
        source_rms: frame.rms,
        payload,
    }
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

fn build_v2_header_preview(
    packet: &UdpAudioPacket,
    flags: u16,
    codec: UdpAudioCodecV2,
) -> UdpAudioHeaderV2 {
    UdpAudioHeaderV2 {
        magic: UDP_AUDIO_MAGIC_V2,
        protocol_version: PROTOCOL_VERSION_V2,
        header_size: UDP_AUDIO_HEADER_V2_LEN as u16,
        flags,
        sequence: packet.sequence,
        timestamp_ms: packet.timestamp_ms,
        codec,
        channels: packet.channels,
        sample_rate: packet.sample_rate,
        frame_duration_ms: if packet.sample_rate == 0 {
            0
        } else {
            (u32::from(packet.frames_per_packet) * 1000 / packet.sample_rate) as u16
        },
        payload_size: packet.payload.len() as u16,
        reserved: 0,
    }
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

fn encode_packet_by_data_plane(
    packet: &UdpAudioPacket,
    data_plane: DataPlaneFormat,
    v2_flags: u16,
    codec: UdpAudioCodecV2,
) -> Vec<u8> {
    match data_plane {
        DataPlaneFormat::LegacyLas1 => packet.encode(),
        DataPlaneFormat::V2Header => {
            let v2 = UdpAudioPacketV2 {
                header: build_v2_header_preview(packet, v2_flags, codec),
                payload: packet.payload.clone(),
            };
            v2.encode()
        }
    }
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

fn to_fixed_48k_stereo_10ms(frame: &AudioFrame) -> Vec<f32> {
    const OUT_RATE: u32 = FIXED_OUTPUT_SAMPLE_RATE;
    const OUT_CHANNELS: usize = FIXED_OUTPUT_CHANNELS;
    const OUT_FRAMES: usize = PCM_FRAME_SAMPLES_PER_CHANNEL_10MS;

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
        let mut encoder = AudioFrameEncoder::new(CodecSelection::Opus, AudioMode::Balanced);

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
        let mut encoder = AudioFrameEncoder::new(CodecSelection::Opus, AudioMode::Balanced);
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
}
