use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use lan_audio_protocol::{
    DataPlanePath, UdpAudioCodecV2, UdpAudioHeaderV2, UdpAudioHeaderV3, UdpAudioPacket,
    UdpAudioPacketV2, UdpAudioPacketV3, PROTOCOL_VERSION_V2, PROTOCOL_VERSION_V3,
    UDP_AUDIO_HEADER_V2_LEN, UDP_AUDIO_HEADER_V3_LEN, UDP_AUDIO_MAGIC_V2, UDP_AUDIO_MAGIC_V3,
};
use thiserror::Error;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

use crate::config::{CodecSelection, DataPlaneFormat, ServerConfig, TransportMode};
use crate::session::write_length_prefixed_frame;

#[derive(Debug, Clone)]
pub struct EncodedFrame {
    bytes: Vec<u8>,
}

impl EncodedFrame {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[async_trait]
pub trait DataPlane: Send + Sync {
    async fn send_frame(&self, frame: &EncodedFrame) -> Result<(), DataPlaneError>;

    fn path_name(&self) -> &'static str;

    async fn probe(&self) -> DataPlaneHealth;

    async fn close(&self);
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DataPlaneError {
    #[error("send failed: {0}")]
    SendFailed(String),
    #[error("buffer full")]
    BufferFull,
    #[error("closed")]
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPlaneHealth {
    pub rtt_ms: Option<u32>,
    pub is_healthy: bool,
}

pub struct LegacyLas1DataPlane {
    socket: Option<Arc<UdpSocket>>,
    target: Option<SocketAddr>,
}

impl LegacyLas1DataPlane {
    pub fn new(_config: &ServerConfig) -> Self {
        Self {
            socket: None,
            target: None,
        }
    }

    pub fn with_udp_target(socket: Arc<UdpSocket>, target: SocketAddr) -> Self {
        Self {
            socket: Some(socket),
            target: Some(target),
        }
    }
}

#[async_trait]
impl DataPlane for LegacyLas1DataPlane {
    async fn send_frame(&self, frame: &EncodedFrame) -> Result<(), DataPlaneError> {
        let socket = self.socket.as_ref().ok_or(DataPlaneError::Closed)?;
        let target = self.target.ok_or(DataPlaneError::Closed)?;
        socket
            .send_to(frame.bytes(), target)
            .await
            .map(|_| ())
            .map_err(|err| DataPlaneError::SendFailed(err.to_string()))
    }

    fn path_name(&self) -> &'static str {
        "legacy_las1"
    }

    async fn probe(&self) -> DataPlaneHealth {
        DataPlaneHealth {
            rtt_ms: None,
            is_healthy: self.socket.is_some() && self.target.is_some(),
        }
    }

    async fn close(&self) {}
}

pub struct V2HeaderDataPlane {
    socket: Option<Arc<UdpSocket>>,
    target: Option<SocketAddr>,
}

impl V2HeaderDataPlane {
    pub fn new(_config: &ServerConfig) -> Self {
        Self {
            socket: None,
            target: None,
        }
    }

    pub fn with_udp_target(socket: Arc<UdpSocket>, target: SocketAddr) -> Self {
        Self {
            socket: Some(socket),
            target: Some(target),
        }
    }
}

#[async_trait]
impl DataPlane for V2HeaderDataPlane {
    async fn send_frame(&self, frame: &EncodedFrame) -> Result<(), DataPlaneError> {
        let socket = self.socket.as_ref().ok_or(DataPlaneError::Closed)?;
        let target = self.target.ok_or(DataPlaneError::Closed)?;
        socket
            .send_to(frame.bytes(), target)
            .await
            .map(|_| ())
            .map_err(|err| DataPlaneError::SendFailed(err.to_string()))
    }

    fn path_name(&self) -> &'static str {
        "v2_header"
    }

    async fn probe(&self) -> DataPlaneHealth {
        DataPlaneHealth {
            rtt_ms: None,
            is_healthy: self.socket.is_some() && self.target.is_some(),
        }
    }

    async fn close(&self) {}
}

pub struct UsbDirectDataPlane {
    writer: Option<Arc<Mutex<OwnedWriteHalf>>>,
}

impl UsbDirectDataPlane {
    pub fn new(_config: &ServerConfig) -> Self {
        Self { writer: None }
    }

    pub fn with_writer(writer: Arc<Mutex<OwnedWriteHalf>>) -> Self {
        Self {
            writer: Some(writer),
        }
    }
}

#[async_trait]
impl DataPlane for UsbDirectDataPlane {
    async fn send_frame(&self, frame: &EncodedFrame) -> Result<(), DataPlaneError> {
        let writer = self.writer.as_ref().ok_or(DataPlaneError::Closed)?;
        write_length_prefixed_frame(writer, frame.bytes())
            .await
            .map_err(|err| DataPlaneError::SendFailed(err.to_string()))
    }

    fn path_name(&self) -> &'static str {
        "usb_direct"
    }

    async fn probe(&self) -> DataPlaneHealth {
        DataPlaneHealth {
            rtt_ms: Some(0),
            is_healthy: self.writer.is_some(),
        }
    }

    async fn close(&self) {}
}

pub fn data_plane_format_to_path(format: DataPlaneFormat) -> DataPlanePath {
    match format {
        DataPlaneFormat::LegacyLas1 => DataPlanePath::LegacyLas1,
        DataPlaneFormat::V2Header => DataPlanePath::V2Header,
    }
}

fn data_plane_path_from_name(name: &str) -> DataPlanePath {
    match name {
        "legacy_las1" => DataPlanePath::LegacyLas1,
        "usb_direct" => DataPlanePath::UsbDirect,
        _ => DataPlanePath::V2Header,
    }
}

fn config_plane(
    format: DataPlaneFormat,
    transport_mode: &TransportMode,
    config: &ServerConfig,
) -> Arc<dyn DataPlane> {
    match (format, transport_mode) {
        (DataPlaneFormat::LegacyLas1, _) => Arc::new(LegacyLas1DataPlane::new(config)),
        (DataPlaneFormat::V2Header, TransportMode::Usb { .. }) => {
            Arc::new(UsbDirectDataPlane::new(config))
        }
        (DataPlaneFormat::V2Header, TransportMode::WiFi) => {
            Arc::new(V2HeaderDataPlane::new(config))
        }
    }
}

pub struct DataPlaneRouter {
    main_format: DataPlaneFormat,
    main_codec: CodecSelection,
    rollback_format: DataPlaneFormat,
    rollback_codec: CodecSelection,
    active_format: DataPlaneFormat,
    active_codec: CodecSelection,
    active: Arc<dyn DataPlane>,
    active_is_main: bool,
}

impl DataPlaneRouter {
    pub fn from_config(config: &ServerConfig) -> Self {
        let main_format = config.selected_data_plane_format();
        let main_codec = config.effective_codec_selection();
        let active = config_plane(main_format, &config.transport_mode, config);
        let mut router = Self {
            main_format,
            main_codec,
            rollback_format: DataPlaneFormat::LegacyLas1,
            rollback_codec: CodecSelection::Pcm16,
            active_format: main_format,
            active_codec: main_codec,
            active,
            active_is_main: true,
        };
        if config.force_rollback {
            router.force_rollback(config);
        }
        router
    }

    pub fn active_format(&self) -> DataPlaneFormat {
        self.active_format
    }

    pub fn active_codec(&self) -> CodecSelection {
        self.active_codec
    }

    pub fn active_path(&self) -> DataPlanePath {
        data_plane_path_from_name(self.active.path_name())
    }

    pub fn rollback_available(&self) -> bool {
        self.active_is_main
            && (self.main_format != self.rollback_format || self.main_codec != self.rollback_codec)
    }

    pub fn force_rollback(&mut self, config: &ServerConfig) {
        self.active = Arc::new(LegacyLas1DataPlane::new(config));
        self.active_format = self.rollback_format;
        self.active_codec = self.rollback_codec;
        self.active_is_main = false;
    }

    pub fn is_on_main_path(&self) -> bool {
        self.active_is_main
    }
}

pub fn build_v2_header_preview(
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

pub fn encode_packet_by_data_plane(
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

/// Phase 6 multi-packet variant. Returns one wire-bytes Vec per UDP
/// datagram that should be sent. For everything except PCM24 this
/// returns a single-element Vec identical to `encode_packet_by_data_plane`.
/// For PCM24 the payload is fragmented to fit a per-packet target of
/// `MAX_PCM24_FRAG_PAYLOAD_BYTES`, with a v3 header carrying
/// `frag_index/total_frags/logical_seq` for reassembly.
///
/// `next_sequence` is a monotonic counter the worker hands in; the helper
/// advances it as packets are produced and returns the new value via the
/// out-parameter so the worker stays in sync with the wire-level
/// `sequence` field.
pub fn encode_packets_by_data_plane(
    packet: &UdpAudioPacket,
    data_plane: DataPlaneFormat,
    v2_flags: u16,
    codec: UdpAudioCodecV2,
    next_sequence: &mut u32,
) -> Vec<Vec<u8>> {
    // Non-PCM24: single packet, advance sequence by one to match the
    // existing per-encoded-frame semantics.
    if codec != UdpAudioCodecV2::Pcm24 {
        let bytes = encode_packet_by_data_plane(packet, data_plane, v2_flags, codec);
        *next_sequence = next_sequence.wrapping_add(1);
        return vec![bytes];
    }

    // PCM24 + legacy_las1 is impossible (the legacy header has no codec
    // discriminator). Caller's responsibility to never pair these — but
    // be defensive and degrade to an empty result so the dispatcher's
    // failed-client logic doesn't block.
    if data_plane == DataPlaneFormat::LegacyLas1 {
        return vec![];
    }

    // Slice payload into frags. The slicing is per-byte (not per-sample)
    // since the receiver reassembles bytes 1:1; samples align as long as
    // the slice boundary is on a 6-byte stride (3 B/sample × 2 ch). We
    // round MAX down to the nearest multiple of 6 to preserve alignment.
    const MAX_PCM24_FRAG_PAYLOAD_BYTES: usize = 1392; // = floor(1400/6)*6
    let total_payload = packet.payload.len();
    let total_frags_u = if total_payload == 0 {
        1
    } else {
        total_payload.div_ceil(MAX_PCM24_FRAG_PAYLOAD_BYTES).max(1)
    };
    if total_frags_u > u8::MAX as usize {
        // Shouldn't happen at sane sample rates (48 kHz/5 ms → 1 frag,
        // 96 kHz/5 ms → 2 frags). Defensive empty result.
        return vec![];
    }
    let total_frags = total_frags_u as u8;
    let logical_seq = packet.sequence;

    let mut out = Vec::with_capacity(total_frags_u);
    for frag_index in 0..total_frags_u {
        let start = frag_index * MAX_PCM24_FRAG_PAYLOAD_BYTES;
        let end = ((frag_index + 1) * MAX_PCM24_FRAG_PAYLOAD_BYTES).min(total_payload);
        let chunk = packet.payload[start..end].to_vec();

        let frame_duration_ms = if packet.sample_rate == 0 {
            0
        } else {
            (u32::from(packet.frames_per_packet) * 1000 / packet.sample_rate) as u16
        };

        let header = UdpAudioHeaderV3 {
            magic: UDP_AUDIO_MAGIC_V3,
            protocol_version: PROTOCOL_VERSION_V3,
            header_size: UDP_AUDIO_HEADER_V3_LEN as u16,
            flags: v2_flags,
            sequence: *next_sequence,
            timestamp_ms: packet.timestamp_ms,
            codec,
            channels: packet.channels,
            sample_rate: packet.sample_rate,
            frame_duration_ms,
            payload_size: chunk.len() as u16,
            frag_index: frag_index as u8,
            total_frags,
            logical_seq,
        };
        *next_sequence = next_sequence.wrapping_add(1);
        let v3 = UdpAudioPacketV3 {
            header,
            payload: chunk,
        };
        out.push(v3.encode());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_selects_correct_plane_from_config() {
        let legacy_cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::LegacyLas1,
            ..ServerConfig::default()
        };
        let legacy_router = DataPlaneRouter::from_config(&legacy_cfg);
        assert_eq!(legacy_router.active_path(), DataPlanePath::LegacyLas1);

        let wifi_v2_cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            transport_mode: TransportMode::WiFi,
            ..ServerConfig::default()
        };
        let wifi_v2_router = DataPlaneRouter::from_config(&wifi_v2_cfg);
        assert_eq!(wifi_v2_router.active_path(), DataPlanePath::V2Header);

        let usb_v2_cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            transport_mode: TransportMode::Usb {
                serial: "device-123".to_string(),
            },
            ..ServerConfig::default()
        };
        let usb_v2_router = DataPlaneRouter::from_config(&usb_v2_cfg);
        assert_eq!(usb_v2_router.active_path(), DataPlanePath::UsbDirect);
    }

    #[test]
    fn force_rollback_switches_to_legacy() {
        let cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            codec_selection: CodecSelection::Opus,
            ..ServerConfig::default()
        };
        let mut router = DataPlaneRouter::from_config(&cfg);
        assert_eq!(router.active_path(), DataPlanePath::V2Header);

        router.force_rollback(&cfg);

        assert_eq!(router.active_path(), DataPlanePath::LegacyLas1);
        assert_eq!(router.active_format(), DataPlaneFormat::LegacyLas1);
        assert_eq!(router.active_codec(), CodecSelection::Pcm16);
        assert!(!router.is_on_main_path());
    }
}
