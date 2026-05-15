//! Shared protocol definitions for desktop and mobile.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub use lan_audio_domain::{
    mode_contract, AudioCodecPreference, AudioMode, ConnectionState, ConnectionStateMachine,
    DataPlanePath, FailureCode, ReleaseDecision, ReleaseGate, RollbackState,
    ServiceMetricsSnapshot, ServiceSnapshot, TransportType,
};

pub const DISCOVERY_PORT: u16 = 39990;
pub const WS_PORT: u16 = 39991;
pub const UDP_AUDIO_PORT: u16 = 39992;
pub const REVERSE_TCP_PORT: u16 = 7878;
pub const REVERSE_CONTROL_PORT: u16 = 7879;

pub const PROTOCOL_VERSION_V2: u8 = 2;
pub const PROTOCOL_VERSION_V3: u8 = 3;

pub const AUDIO_MAGIC: [u8; 4] = *b"LAS1";
pub const AUDIO_HEADER_LEN: usize = 4 + 1 + 1 + 4 + 8 + 4 + 1 + 2 + 2;

pub const UDP_AUDIO_MAGIC_V2: [u8; 4] = *b"LAV2";
pub const UDP_AUDIO_HEADER_V2_LEN: usize = 4 + 1 + 2 + 2 + 4 + 8 + 1 + 1 + 4 + 2 + 2 + 2;

/// Phase 6 v3 header. Layout extends v2 by:
/// - reusing the 16-bit `reserved` slot as `frag_index_u8 | total_frags_u8`
///   (little-endian: byte 0 = frag_index, byte 1 = total_frags)
/// - appending a 4-byte `logical_seq` u32 at the end of the header so all
///   frags belonging to the same logical frame share an identifier the
///   reassembler can key on
///
/// `total_frags == 1` (single packet, no fragmentation) is the common case
/// for 48 kHz, where one v3 packet carries one logical frame and
/// `logical_seq == sequence`.
pub const UDP_AUDIO_MAGIC_V3: [u8; 4] = *b"LAV2";
pub const UDP_AUDIO_HEADER_V3_LEN: usize = UDP_AUDIO_HEADER_V2_LEN + 4;

pub const UDP_FLAG_V2_SILENCE: u16 = 1 << 0;
pub const UDP_FLAG_V2_CONFIG_CHANGED: u16 = 1 << 1;
pub const UDP_FLAG_V2_DISCONTINUITY: u16 = 1 << 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPlanePacketKind {
    LegacyLas1,
    V2Lav2,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SampleFormatPreference {
    Pcm16,
    F32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioModeProfile {
    pub mode: AudioMode,
    pub start_buffer_ms: u16,
    pub max_buffer_ms: u16,
    pub batch_frames: u8,
    pub drop_threshold_ms: u16,
    pub prefer_low_latency_path: bool,
    pub prefer_stable_audio_track: bool,
    pub preferred_codec: AudioCodecPreference,
    pub preferred_sample_format: SampleFormatPreference,
    pub frame_duration_ms: u16,
    pub reset_buffer_on_switch: bool,
}

pub fn audio_mode_profile(mode: AudioMode) -> AudioModeProfile {
    let contract = mode_contract(mode);
    let prefer_low_latency_path = contract
        .output_backend_priority
        .first()
        .is_some_and(|backend| matches!(backend, lan_audio_domain::OutputBackend::FastPath));
    let prefer_stable_audio_track = contract
        .output_backend_priority
        .iter()
        .any(|backend| matches!(backend, lan_audio_domain::OutputBackend::AudioTrack));

    AudioModeProfile {
        mode,
        start_buffer_ms: contract.tuning.start_buffer_ms,
        max_buffer_ms: contract.tuning.max_buffer_ms,
        batch_frames: contract.tuning.batch_frames,
        drop_threshold_ms: contract.tuning.drop_threshold_ms,
        prefer_low_latency_path,
        prefer_stable_audio_track,
        preferred_codec: contract.preferred_codec,
        preferred_sample_format: SampleFormatPreference::Pcm16,
        frame_duration_ms: contract.tuning.frame_duration_ms,
        reset_buffer_on_switch: contract.tuning.reset_buffer_on_switch,
    }
}

impl Default for AudioModeProfile {
    fn default() -> Self {
        audio_mode_profile(AudioMode::Balanced)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ProtocolCapabilities {
    pub supports_pcm16: bool,
    pub supports_f32: bool,
    pub supports_modes: bool,
    pub supports_metrics: bool,
    pub supports_opus_future: bool,
    pub supports_opus: bool,
    pub supports_opus_experimental: bool,
    pub supports_low_latency: bool,
    pub supports_high_quality: bool,
    pub supports_native_audio_track: bool,
    pub supports_fast_path: bool,
    pub supports_stable_audio_track: bool,
    pub supports_usb_tethering: bool,
    pub supports_usb_direct_future: bool,
    pub supports_reverse_channel: bool,
    /// Phase 6 capability. The peer can encode (server) or decode (client)
    /// PCM24 hi-res passthrough on the v3 data plane. Defaults to
    /// `false` so older peers stay on the Opus / Pcm16 path.
    #[serde(default)]
    pub supports_hires_pcm24: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hello {
    pub protocol_version: u8,
    pub device_name: String,
    pub client_id: String,
    pub udp_port: u16,
    pub desired_sample_rate: u32,
    pub channels: u8,
    pub capabilities: ProtocolCapabilities,
    pub preferred_audio_mode: AudioMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloAck {
    pub protocol_version: u8,
    pub accepted: bool,
    pub session_id: Uuid,
    pub current_audio_mode: AudioMode,
    #[serde(default)]
    pub transport_type: TransportType,
    #[serde(default)]
    pub mode_profile: AudioModeProfile,
    pub capabilities: ProtocolCapabilities,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerInfo {
    pub server_id: Uuid,
    pub server_name: String,
    pub platform: String,
    pub app_version: String,
    pub ws_port: u16,
    pub udp_port: u16,
    pub protocol_version: u8,
    pub current_audio_mode: AudioMode,
    #[serde(default)]
    pub mode_profile: AudioModeProfile,
    pub codec: AudioCodecPreference,
    pub data_plane: String,
    pub gray_mode: bool,
    pub recommended_connection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientInfo {
    pub client_name: String,
    pub platform: String,
    pub app_version: String,
    pub udp_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetAudioMode {
    pub mode: AudioMode,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_sample_rate: Option<u32>,
    /// Phase 7 codec selection. Optional so older clients (which never
    /// send this field) keep working — the server falls back to the
    /// per-mode default when it is absent. Older servers ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_codec: Option<AudioCodecPreference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioModeChanged {
    pub mode: AudioMode,
    pub applied: bool,
    pub reason: String,
    #[serde(default)]
    pub mode_profile: AudioModeProfile,
    /// Phase 7 — the codec the server is actually using after this
    /// transition. Defaults via serde(default) to keep wire compatibility
    /// with older servers; clients should treat absence as "unchanged".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_codec: Option<AudioCodecPreference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientListEntry {
    pub id: Uuid,
    pub name: String,
    pub mode: AudioMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientList {
    pub clients: Vec<ClientListEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientJoined {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientLeft {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStateKind {
    Idle,
    Buffering,
    Streaming,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaybackState {
    pub state: PlaybackStateKind,
    pub buffered_ms: u32,
    pub active_sessions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricsSnapshot {
    pub tx_packets: u64,
    pub tx_bytes: u64,
    pub capture_read_errors: u64,
    pub capture_underruns: u64,
    pub active_sessions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorMessage {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<FailureCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconnectHint {
    pub after_ms: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum NegotiationError {
    UnsupportedProtocolVersion { requested: u8, supported: u8 },
    UnsupportedCodec { codec: AudioCodecPreference },
    UnsupportedDataPlane { data_plane: DataPlanePath },
    CapabilityMismatch { capability: String },
    TooManyClients { max_clients: u8 },
    InvalidModeProfile { mode: AudioMode, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMessageV2 {
    Hello(Hello),
    HelloAck(HelloAck),
    ServerInfo(ServerInfo),
    ClientInfo(ClientInfo),
    ClientList(ClientList),
    ClientJoined(ClientJoined),
    ClientLeft(ClientLeft),
    SetAudioMode(SetAudioMode),
    AudioModeChanged(AudioModeChanged),
    PlaybackState(PlaybackState),
    MetricsSnapshot(MetricsSnapshot),
    Error(ErrorMessage),
    ReconnectHint(ReconnectHint),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveryBeacon {
    #[serde(rename = "type")]
    pub kind: String,
    pub server_id: Uuid,
    pub server_name: String,
    pub ws_port: u16,
    pub udp_port: u16,
    pub reverse_channel_port: u16,
    pub control_channel_port: u16,
    pub ts_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientControlMessage {
    ClientHello {
        client_name: String,
        udp_port: u16,
        desired_sample_rate: u32,
        channels: u8,
    },
    ClientPing {
        seq: u64,
        ts_unix_ms: u64,
    },
    /// Phase 3 watermark report. Sent by the Android client roughly once per
    /// second over the existing TCP control channel so the server can drive
    /// its Kalman+PID sync engine. All fields are derived from the client's
    /// own jitter buffer + Audio ring buffer state. Older servers simply
    /// ignore unknown tags.
    ClientWatermark(WatermarkReport),
}

/// Buffer-level snapshot reported by the playback client to drive the server's
/// adaptive sync engine. Fields are derived from the client jitter buffer and
/// the AudioTrack/Oboe ring buffer state; deltas are reset between reports so
/// the server can compute rates without storing per-client history.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatermarkReport {
    pub ts_unix_ms: u64,
    pub jitter_buf_ms: u32,
    pub ring_buf_ms: u32,
    pub silence_fill_delta: u32,
    pub underrun_delta: u32,
    pub jitter_p95_us: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerControlMessage {
    ServerWelcome {
        session_id: Uuid,
        codec: String,
        sample_rate: u32,
        channels: u8,
        frames_per_packet: u16,
    },
    ServerPong {
        seq: u64,
        ts_unix_ms: u64,
    },
    ServerMetrics {
        tx_packets: u64,
        tx_bytes: u64,
        sessions: u64,
    },
    ServerError {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum UdpAudioCodecV2 {
    Pcm16 = 1,
    F32 = 2,
    Opus = 3,
    /// Phase 6 Hi-Res passthrough. 24-bit signed integer samples in
    /// big-endian, interleaved per channel. No compression. Sample rate
    /// is whatever the wire header reports — the encoder does not
    /// resample. Only valid on v3 packets.
    Pcm24 = 4,
}

impl UdpAudioCodecV2 {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Pcm16),
            2 => Some(Self::F32),
            3 => Some(Self::Opus),
            4 => Some(Self::Pcm24),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdpAudioHeaderV2 {
    pub magic: [u8; 4],
    pub protocol_version: u8,
    pub header_size: u16,
    pub flags: u16,
    pub sequence: u32,
    pub timestamp_ms: u64,
    pub codec: UdpAudioCodecV2,
    pub channels: u8,
    pub sample_rate: u32,
    pub frame_duration_ms: u16,
    pub payload_size: u16,
    pub reserved: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdpAudioPacketV2 {
    pub header: UdpAudioHeaderV2,
    pub payload: Vec<u8>,
}

/// Phase 6 v3 wire header. Adds two pieces of information for Hi-Res
/// passthrough: per-packet fragmentation indices (so a single 96 kHz / 5 ms
/// PCM24 logical frame can be split across multiple UDP packets to fit MTU)
/// and a `logical_seq` so the receiver can reassemble frags belonging to
/// the same logical frame even when the wire-level `sequence` advances per
/// packet.
///
/// Wire layout matches `UdpAudioHeaderV2` byte-for-byte for the first
/// `UDP_AUDIO_HEADER_V2_LEN` bytes, with two semantic differences:
/// - `protocol_version = 3`
/// - The `reserved` u16 slot is repurposed to hold
///   `frag_index | total_frags` (LE: byte 0 = frag_index, byte 1 = total).
///   Then a fresh `logical_seq u32` is appended.
///
/// `total_frags == 1` (no fragmentation) means `frag_index == 0` and
/// `logical_seq == sequence`. The 48 kHz / 5 ms common case fits in a
/// single packet so the fragmentation overhead is just 4 bytes per packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdpAudioHeaderV3 {
    pub magic: [u8; 4],
    pub protocol_version: u8,
    pub header_size: u16,
    pub flags: u16,
    pub sequence: u32,
    pub timestamp_ms: u64,
    pub codec: UdpAudioCodecV2,
    pub channels: u8,
    pub sample_rate: u32,
    pub frame_duration_ms: u16,
    pub payload_size: u16,
    pub frag_index: u8,
    pub total_frags: u8,
    pub logical_seq: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdpAudioPacketV3 {
    pub header: UdpAudioHeaderV3,
    pub payload: Vec<u8>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HeaderV3CodecError {
    #[error("packet too short")]
    TooShort,
    #[error("invalid magic")]
    InvalidMagic,
    #[error("unsupported protocol version")]
    UnsupportedVersion,
    #[error("invalid header size")]
    InvalidHeaderSize,
    #[error("invalid codec")]
    InvalidCodec,
    #[error("payload length mismatch")]
    PayloadLengthMismatch,
    #[error("invalid fragmentation header")]
    InvalidFragHeader,
}

impl UdpAudioHeaderV3 {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(UDP_AUDIO_HEADER_V3_LEN);
        out.extend_from_slice(&self.magic);
        out.push(self.protocol_version);
        out.extend_from_slice(&self.header_size.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.timestamp_ms.to_le_bytes());
        out.push(self.codec as u8);
        out.push(self.channels);
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&self.frame_duration_ms.to_le_bytes());
        out.extend_from_slice(&self.payload_size.to_le_bytes());
        out.push(self.frag_index);
        out.push(self.total_frags);
        out.extend_from_slice(&self.logical_seq.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, HeaderV3CodecError> {
        if bytes.len() < UDP_AUDIO_HEADER_V3_LEN {
            return Err(HeaderV3CodecError::TooShort);
        }

        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if magic != UDP_AUDIO_MAGIC_V3 {
            return Err(HeaderV3CodecError::InvalidMagic);
        }

        let protocol_version = bytes[4];
        if protocol_version != PROTOCOL_VERSION_V3 {
            return Err(HeaderV3CodecError::UnsupportedVersion);
        }

        let header_size = u16::from_le_bytes(bytes[5..7].try_into().expect("slice len"));
        if header_size as usize != UDP_AUDIO_HEADER_V3_LEN {
            return Err(HeaderV3CodecError::InvalidHeaderSize);
        }

        let flags = u16::from_le_bytes(bytes[7..9].try_into().expect("slice len"));
        let sequence = u32::from_le_bytes(bytes[9..13].try_into().expect("slice len"));
        let timestamp_ms = u64::from_le_bytes(bytes[13..21].try_into().expect("slice len"));
        let codec = UdpAudioCodecV2::from_u8(bytes[21]).ok_or(HeaderV3CodecError::InvalidCodec)?;
        let channels = bytes[22];
        let sample_rate = u32::from_le_bytes(bytes[23..27].try_into().expect("slice len"));
        let frame_duration_ms = u16::from_le_bytes(bytes[27..29].try_into().expect("slice len"));
        let payload_size = u16::from_le_bytes(bytes[29..31].try_into().expect("slice len"));
        let frag_index = bytes[31];
        let total_frags = bytes[32];
        if total_frags == 0 || frag_index >= total_frags {
            return Err(HeaderV3CodecError::InvalidFragHeader);
        }
        let logical_seq = u32::from_le_bytes(bytes[33..37].try_into().expect("slice len"));

        Ok(Self {
            magic,
            protocol_version,
            header_size,
            flags,
            sequence,
            timestamp_ms,
            codec,
            channels,
            sample_rate,
            frame_duration_ms,
            payload_size,
            frag_index,
            total_frags,
            logical_seq,
        })
    }
}

impl UdpAudioPacketV3 {
    pub fn encode(&self) -> Vec<u8> {
        let mut header = self.header.clone();
        header.payload_size = self.payload.len() as u16;

        let mut out = header.encode();
        out.extend_from_slice(&self.payload);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, HeaderV3CodecError> {
        if bytes.len() < UDP_AUDIO_HEADER_V3_LEN {
            return Err(HeaderV3CodecError::TooShort);
        }
        let header = UdpAudioHeaderV3::decode(bytes)?;
        let payload_end = UDP_AUDIO_HEADER_V3_LEN + header.payload_size as usize;
        if bytes.len() != payload_end {
            return Err(HeaderV3CodecError::PayloadLengthMismatch);
        }
        Ok(Self {
            header,
            payload: bytes[UDP_AUDIO_HEADER_V3_LEN..payload_end].to_vec(),
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PacketCodecError {
    #[error("packet too short")]
    TooShort,
    #[error("invalid magic")]
    InvalidMagic,
    #[error("payload length mismatch")]
    PayloadLengthMismatch,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HeaderV2CodecError {
    #[error("packet too short")]
    TooShort,
    #[error("invalid magic")]
    InvalidMagic,
    #[error("unsupported protocol version")]
    UnsupportedVersion,
    #[error("invalid header size")]
    InvalidHeaderSize,
    #[error("invalid codec")]
    InvalidCodec,
    #[error("payload length mismatch")]
    PayloadLengthMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdpAudioPacket {
    pub version: u8,
    pub flags: u8,
    pub sequence: u32,
    pub timestamp_ms: u64,
    pub sample_rate: u32,
    pub channels: u8,
    pub frames_per_packet: u16,
    pub payload: Vec<u8>,
}

pub fn detect_data_plane_packet_kind(bytes: &[u8]) -> DataPlanePacketKind {
    if bytes.len() < 4 {
        return DataPlanePacketKind::Unknown;
    }
    match &bytes[0..4] {
        b"LAS1" => DataPlanePacketKind::LegacyLas1,
        b"LAV2" => DataPlanePacketKind::V2Lav2,
        _ => DataPlanePacketKind::Unknown,
    }
}

impl UdpAudioHeaderV2 {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(UDP_AUDIO_HEADER_V2_LEN);
        out.extend_from_slice(&self.magic);
        out.push(self.protocol_version);
        out.extend_from_slice(&self.header_size.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.timestamp_ms.to_le_bytes());
        out.push(self.codec as u8);
        out.push(self.channels);
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&self.frame_duration_ms.to_le_bytes());
        out.extend_from_slice(&self.payload_size.to_le_bytes());
        out.extend_from_slice(&self.reserved.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, HeaderV2CodecError> {
        if bytes.len() < UDP_AUDIO_HEADER_V2_LEN {
            return Err(HeaderV2CodecError::TooShort);
        }

        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if magic != UDP_AUDIO_MAGIC_V2 {
            return Err(HeaderV2CodecError::InvalidMagic);
        }

        let protocol_version = bytes[4];
        if protocol_version != PROTOCOL_VERSION_V2 {
            return Err(HeaderV2CodecError::UnsupportedVersion);
        }

        let header_size = u16::from_le_bytes(bytes[5..7].try_into().expect("slice len"));
        if header_size as usize != UDP_AUDIO_HEADER_V2_LEN {
            return Err(HeaderV2CodecError::InvalidHeaderSize);
        }

        let flags = u16::from_le_bytes(bytes[7..9].try_into().expect("slice len"));
        let sequence = u32::from_le_bytes(bytes[9..13].try_into().expect("slice len"));
        let timestamp_ms = u64::from_le_bytes(bytes[13..21].try_into().expect("slice len"));
        let codec = UdpAudioCodecV2::from_u8(bytes[21]).ok_or(HeaderV2CodecError::InvalidCodec)?;
        let channels = bytes[22];
        let sample_rate = u32::from_le_bytes(bytes[23..27].try_into().expect("slice len"));
        let frame_duration_ms = u16::from_le_bytes(bytes[27..29].try_into().expect("slice len"));
        let payload_size = u16::from_le_bytes(bytes[29..31].try_into().expect("slice len"));
        let reserved = u16::from_le_bytes(bytes[31..33].try_into().expect("slice len"));

        Ok(Self {
            magic,
            protocol_version,
            header_size,
            flags,
            sequence,
            timestamp_ms,
            codec,
            channels,
            sample_rate,
            frame_duration_ms,
            payload_size,
            reserved,
        })
    }
}

impl UdpAudioPacketV2 {
    pub fn encode(&self) -> Vec<u8> {
        let mut header = self.header.clone();
        header.payload_size = self.payload.len() as u16;

        let mut out = header.encode();
        out.extend_from_slice(&self.payload);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, HeaderV2CodecError> {
        if bytes.len() < UDP_AUDIO_HEADER_V2_LEN {
            return Err(HeaderV2CodecError::TooShort);
        }
        let header = UdpAudioHeaderV2::decode(bytes)?;
        let payload_end = UDP_AUDIO_HEADER_V2_LEN + header.payload_size as usize;
        if bytes.len() != payload_end {
            return Err(HeaderV2CodecError::PayloadLengthMismatch);
        }
        Ok(Self {
            header,
            payload: bytes[UDP_AUDIO_HEADER_V2_LEN..payload_end].to_vec(),
        })
    }
}

impl UdpAudioPacket {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(AUDIO_HEADER_LEN + self.payload.len());
        out.extend_from_slice(&AUDIO_MAGIC);
        out.push(self.version);
        out.push(self.flags);
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.timestamp_ms.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.push(self.channels);
        out.extend_from_slice(&self.frames_per_packet.to_le_bytes());
        out.extend_from_slice(&(self.payload.len() as u16).to_le_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, PacketCodecError> {
        if bytes.len() < AUDIO_HEADER_LEN {
            return Err(PacketCodecError::TooShort);
        }
        if bytes[0..4] != AUDIO_MAGIC {
            return Err(PacketCodecError::InvalidMagic);
        }
        let version = bytes[4];
        let flags = bytes[5];
        let sequence = u32::from_le_bytes(bytes[6..10].try_into().expect("slice len"));
        let timestamp_ms = u64::from_le_bytes(bytes[10..18].try_into().expect("slice len"));
        let sample_rate = u32::from_le_bytes(bytes[18..22].try_into().expect("slice len"));
        let channels = bytes[22];
        let frames_per_packet = u16::from_le_bytes(bytes[23..25].try_into().expect("slice len"));
        let payload_len = u16::from_le_bytes(bytes[25..27].try_into().expect("slice len")) as usize;
        if bytes.len() != AUDIO_HEADER_LEN + payload_len {
            return Err(PacketCodecError::PayloadLengthMismatch);
        }
        Ok(Self {
            version,
            flags,
            sequence,
            timestamp_ms,
            sample_rate,
            channels,
            frames_per_packet,
            payload: bytes[AUDIO_HEADER_LEN..].to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udp_packet_round_trip() {
        let packet = UdpAudioPacket {
            version: 1,
            flags: 0,
            sequence: 42,
            timestamp_ms: 123456,
            sample_rate: 48_000,
            channels: 2,
            frames_per_packet: 960,
            payload: vec![1, 2, 3, 4, 5],
        };
        let encoded = packet.encode();
        let decoded = UdpAudioPacket::decode(&encoded).expect("decode");
        assert_eq!(packet, decoded);
    }

    #[test]
    fn invalid_magic_should_fail() {
        let mut bytes = vec![0; AUDIO_HEADER_LEN];
        bytes[0..4].copy_from_slice(b"BAD!");
        let err = UdpAudioPacket::decode(&bytes).expect_err("must fail");
        assert_eq!(err, PacketCodecError::InvalidMagic);
    }

    #[test]
    fn control_message_json_tag() {
        let msg = ClientControlMessage::ClientPing {
            seq: 7,
            ts_unix_ms: 123,
        };
        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains("client_ping"));
    }

    #[test]
    fn client_watermark_round_trip_json() {
        let report = WatermarkReport {
            ts_unix_ms: 1_700_000_000_000,
            jitter_buf_ms: 80,
            ring_buf_ms: 25,
            silence_fill_delta: 1,
            underrun_delta: 0,
            jitter_p95_us: 2_500,
        };
        let msg = ClientControlMessage::ClientWatermark(report);
        let json = serde_json::to_string(&msg).expect("serialize watermark");
        assert!(json.contains("\"type\":\"client_watermark\""));
        assert!(json.contains("\"jitter_buf_ms\":80"));
        let decoded: ClientControlMessage =
            serde_json::from_str(&json).expect("deserialize watermark");
        assert_eq!(decoded, msg);
    }

    #[test]
    fn unknown_client_control_message_type_is_ignored_safely() {
        // The server must tolerate older/newer client variants — a payload
        // with an unknown tag should fail to deserialize but not panic, so
        // the WS handler can fall through to other variants.
        let json = "{\"type\":\"client_unknown_future\",\"foo\":1}";
        let result = serde_json::from_str::<ClientControlMessage>(json);
        assert!(result.is_err(), "unknown tag should not deserialize");
    }

    #[test]
    fn v2_audio_mode_round_trip_json() {
        let mode = AudioMode::HighQuality;
        let json = serde_json::to_string(&mode).expect("serialize mode");
        assert_eq!(json, "\"high_quality\"");
        let decoded: AudioMode = serde_json::from_str(&json).expect("deserialize mode");
        assert_eq!(decoded, AudioMode::HighQuality);
    }

    #[test]
    fn audio_mode_profile_has_low_latency_and_high_quality_tradeoffs() {
        let low = audio_mode_profile(AudioMode::LowLatency);
        let high = audio_mode_profile(AudioMode::HighQuality);
        assert!(low.prefer_low_latency_path);
        assert!(high.prefer_stable_audio_track);
        assert!(low.start_buffer_ms < high.start_buffer_ms);
        assert!(low.max_buffer_ms < high.max_buffer_ms);
        assert_eq!(
            low.start_buffer_ms,
            lan_audio_domain::mode_contract(AudioMode::LowLatency)
                .tuning
                .start_buffer_ms
        );
    }

    #[test]
    fn codec_preference_marks_opus_as_stable() {
        let json = serde_json::to_string(&AudioCodecPreference::Opus).expect("serialize");
        assert_eq!(json, "\"opus\"");
        let legacy: AudioCodecPreference =
            serde_json::from_str("\"opus_experimental\"").expect("legacy opus alias");
        assert_eq!(legacy, AudioCodecPreference::Opus);
        assert_eq!(UdpAudioCodecV2::from_u8(3), Some(UdpAudioCodecV2::Opus));
    }

    #[test]
    fn v2_capabilities_round_trip_json() {
        let caps = ProtocolCapabilities {
            supports_pcm16: true,
            supports_f32: false,
            supports_modes: true,
            supports_metrics: true,
            supports_opus_future: true,
            supports_opus: true,
            supports_opus_experimental: true,
            supports_low_latency: true,
            supports_high_quality: true,
            supports_native_audio_track: true,
            supports_fast_path: true,
            supports_stable_audio_track: true,
            supports_usb_tethering: true,
            supports_usb_direct_future: false,
            supports_reverse_channel: false,
            supports_hires_pcm24: false,
        };
        let json = serde_json::to_string(&caps).expect("serialize caps");
        let decoded: ProtocolCapabilities = serde_json::from_str(&json).expect("deserialize caps");
        assert_eq!(decoded, caps);
    }

    #[test]
    fn v6_capabilities_back_compat_without_hires() {
        // Older clients did not send `supports_hires_pcm24`. The serde
        // default (false) must apply.
        let json = "{\"supports_pcm16\":true}";
        let decoded: ProtocolCapabilities =
            serde_json::from_str(json).expect("deserialize caps without hires");
        assert!(!decoded.supports_hires_pcm24);
        assert!(decoded.supports_pcm16);
    }
    #[test]
    fn v2_control_message_hello_round_trip() {
        let msg = ControlMessageV2::Hello(Hello {
            protocol_version: PROTOCOL_VERSION_V2,
            device_name: "pixel-8".to_string(),
            client_id: "android-123".to_string(),
            udp_port: 54000,
            desired_sample_rate: 48_000,
            channels: 2,
            capabilities: ProtocolCapabilities {
                supports_pcm16: true,
                supports_f32: true,
                supports_modes: true,
                supports_metrics: true,
                supports_opus_future: true,
                supports_opus: false,
                supports_opus_experimental: false,
                supports_low_latency: true,
                supports_high_quality: true,
                supports_native_audio_track: true,
                supports_fast_path: true,
                supports_stable_audio_track: true,
                supports_usb_tethering: true,
                supports_usb_direct_future: false,
                supports_reverse_channel: false,
                supports_hires_pcm24: false,
            },
            preferred_audio_mode: AudioMode::Balanced,
        });

        let json = serde_json::to_string(&msg).expect("serialize hello");
        assert!(json.contains("\"type\":\"hello\""));

        let decoded: ControlMessageV2 = serde_json::from_str(&json).expect("deserialize hello");
        assert_eq!(decoded, msg);
    }

    #[test]
    fn v2_control_message_hello_ack_round_trip() {
        let msg = ControlMessageV2::HelloAck(HelloAck {
            protocol_version: PROTOCOL_VERSION_V2,
            accepted: true,
            session_id: Uuid::new_v4(),
            current_audio_mode: AudioMode::Balanced,
            transport_type: TransportType::Wifi,
            mode_profile: audio_mode_profile(AudioMode::Balanced),
            capabilities: ProtocolCapabilities {
                supports_pcm16: true,
                supports_f32: false,
                supports_modes: true,
                supports_metrics: true,
                supports_opus_future: false,
                supports_opus: false,
                supports_opus_experimental: false,
                supports_low_latency: true,
                supports_high_quality: true,
                supports_native_audio_track: true,
                supports_fast_path: false,
                supports_stable_audio_track: true,
                supports_usb_tethering: true,
                supports_usb_direct_future: false,
                supports_reverse_channel: false,
                supports_hires_pcm24: false,
            },
            message: "ok".to_string(),
        });

        let json = serde_json::to_string(&msg).expect("serialize hello ack");
        assert!(json.contains("\"type\":\"hello_ack\""));
        let decoded: ControlMessageV2 = serde_json::from_str(&json).expect("deserialize hello ack");
        assert_eq!(decoded, msg);
    }

    #[test]
    fn legacy_las1_and_v2_header_upgrade_handshake_are_distinct() {
        let legacy = ClientControlMessage::ClientHello {
            client_name: "legacy-android".to_string(),
            udp_port: 54000,
            desired_sample_rate: 48_000,
            channels: 2,
        };
        let legacy_json = serde_json::to_string(&legacy).expect("serialize legacy hello");
        assert!(legacy_json.contains("\"type\":\"client_hello\""));
        let legacy_decoded: ClientControlMessage =
            serde_json::from_str(&legacy_json).expect("deserialize legacy hello");
        assert_eq!(legacy_decoded, legacy);

        let v2 = ControlMessageV2::Hello(Hello {
            protocol_version: PROTOCOL_VERSION_V2,
            device_name: "android-v2".to_string(),
            client_id: "android-v2-1".to_string(),
            udp_port: 54001,
            desired_sample_rate: 48_000,
            channels: 2,
            capabilities: ProtocolCapabilities {
                supports_pcm16: true,
                supports_f32: false,
                supports_modes: true,
                supports_metrics: true,
                supports_opus_future: true,
                supports_opus: true,
                supports_opus_experimental: true,
                supports_low_latency: true,
                supports_high_quality: true,
                supports_native_audio_track: true,
                supports_fast_path: true,
                supports_stable_audio_track: true,
                supports_usb_tethering: true,
                supports_usb_direct_future: false,
                supports_reverse_channel: false,
                supports_hires_pcm24: false,
            },
            preferred_audio_mode: AudioMode::Balanced,
        });
        let v2_json = serde_json::to_string(&v2).expect("serialize v2 hello");
        assert!(v2_json.contains("\"type\":\"hello\""));
        assert!(v2_json.contains("\"protocol_version\":2"));

        assert_ne!(
            detect_data_plane_packet_kind(
                &UdpAudioPacket {
                    version: 1,
                    flags: 0,
                    sequence: 1,
                    timestamp_ms: 1,
                    sample_rate: 48_000,
                    channels: 2,
                    frames_per_packet: 480,
                    payload: vec![0, 1],
                }
                .encode()
            ),
            detect_data_plane_packet_kind(
                &UdpAudioPacketV2 {
                    header: UdpAudioHeaderV2 {
                        magic: UDP_AUDIO_MAGIC_V2,
                        protocol_version: PROTOCOL_VERSION_V2,
                        header_size: UDP_AUDIO_HEADER_V2_LEN as u16,
                        flags: UDP_FLAG_V2_CONFIG_CHANGED,
                        sequence: 1,
                        timestamp_ms: 1,
                        codec: UdpAudioCodecV2::Opus,
                        channels: 2,
                        sample_rate: 48_000,
                        frame_duration_ms: 20,
                        payload_size: 0,
                        reserved: 0,
                    },
                    payload: vec![2, 3],
                }
                .encode()
            )
        );
    }

    #[test]
    fn negotiation_error_all_variants_round_trip_json() {
        let cases = vec![
            NegotiationError::UnsupportedProtocolVersion {
                requested: 3,
                supported: PROTOCOL_VERSION_V2,
            },
            NegotiationError::UnsupportedCodec {
                codec: AudioCodecPreference::Opus,
            },
            NegotiationError::UnsupportedDataPlane {
                data_plane: DataPlanePath::UsbDirect,
            },
            NegotiationError::CapabilityMismatch {
                capability: "supports_opus".to_string(),
            },
            NegotiationError::TooManyClients { max_clients: 4 },
            NegotiationError::InvalidModeProfile {
                mode: AudioMode::LowLatency,
                reason: "buffer below minimum".to_string(),
            },
        ];

        for case in cases {
            let json = serde_json::to_string(&case).expect("serialize negotiation error");
            let decoded: NegotiationError =
                serde_json::from_str(&json).expect("deserialize negotiation error");
            assert_eq!(decoded, case);
        }
    }

    #[test]
    fn v2_control_message_set_audio_mode_round_trip() {
        let msg = ControlMessageV2::SetAudioMode(SetAudioMode {
            mode: AudioMode::LowLatency,
            reason: "user_selected".to_string(),
            preferred_sample_rate: Some(48_000),
            preferred_codec: Some(AudioCodecPreference::Opus),
        });
        let json = serde_json::to_string(&msg).expect("serialize set audio mode");
        assert!(json.contains("\"type\":\"set_audio_mode\""));
        assert!(json.contains("\"preferred_sample_rate\":48000"));
        assert!(json.contains("\"preferred_codec\":\"opus\""));
        let decoded: ControlMessageV2 =
            serde_json::from_str(&json).expect("deserialize set audio mode");
        assert_eq!(decoded, msg);
    }

    #[test]
    fn v2_set_audio_mode_back_compat_without_codec() {
        // Older clients did not send `preferred_codec`. The server must
        // still accept the message and leave the codec untouched on
        // deserialization. We assert by deserializing a canonical pre-
        // field payload and checking the field defaults to None.
        let json = "{\"type\":\"set_audio_mode\",\"mode\":\"balanced\",\"reason\":\"user\",\"preferred_sample_rate\":48000}";
        let decoded: ControlMessageV2 =
            serde_json::from_str(json).expect("deserialize set audio mode (no codec)");
        match decoded {
            ControlMessageV2::SetAudioMode(SetAudioMode {
                preferred_codec, ..
            }) => assert!(preferred_codec.is_none()),
            other => panic!("expected SetAudioMode, got {:?}", other),
        }
    }

    #[test]
    fn v2_control_message_client_list_round_trip() {
        let msg = ControlMessageV2::ClientList(ClientList {
            clients: vec![ClientListEntry {
                id: Uuid::new_v4(),
                name: "Pixel 8".to_string(),
                mode: AudioMode::Balanced,
            }],
        });
        let json = serde_json::to_string(&msg).expect("serialize client list");
        assert!(json.contains("\"type\":\"client_list\""));
        let decoded: ControlMessageV2 =
            serde_json::from_str(&json).expect("deserialize client list");
        assert_eq!(decoded, msg);
    }

    #[test]
    fn udp_v2_header_round_trip() {
        let header = UdpAudioHeaderV2 {
            magic: UDP_AUDIO_MAGIC_V2,
            protocol_version: PROTOCOL_VERSION_V2,
            header_size: UDP_AUDIO_HEADER_V2_LEN as u16,
            flags: UDP_FLAG_V2_SILENCE | UDP_FLAG_V2_CONFIG_CHANGED,
            sequence: 77,
            timestamp_ms: 987654,
            codec: UdpAudioCodecV2::Pcm16,
            channels: 2,
            sample_rate: 48_000,
            frame_duration_ms: 10,
            payload_size: 32,
            reserved: 0,
        };

        let encoded = header.encode();
        let decoded = UdpAudioHeaderV2::decode(&encoded).expect("decode v2 header");
        assert_eq!(decoded, header);
    }

    #[test]
    fn udp_v2_header_codec_and_flag_matrix_round_trip() {
        for codec in [
            UdpAudioCodecV2::Pcm16,
            UdpAudioCodecV2::F32,
            UdpAudioCodecV2::Opus,
        ] {
            let header = UdpAudioHeaderV2 {
                magic: UDP_AUDIO_MAGIC_V2,
                protocol_version: PROTOCOL_VERSION_V2,
                header_size: UDP_AUDIO_HEADER_V2_LEN as u16,
                flags: UDP_FLAG_V2_SILENCE | UDP_FLAG_V2_CONFIG_CHANGED | UDP_FLAG_V2_DISCONTINUITY,
                sequence: u32::MAX,
                timestamp_ms: u64::MAX - 1,
                codec,
                channels: 2,
                sample_rate: 48_000,
                frame_duration_ms: 20,
                payload_size: u16::MAX,
                reserved: 0,
            };
            assert_eq!(UdpAudioHeaderV2::decode(&header.encode()).unwrap(), header);
        }
    }

    #[test]
    fn udp_v2_packet_round_trip() {
        let packet = UdpAudioPacketV2 {
            header: UdpAudioHeaderV2 {
                magic: UDP_AUDIO_MAGIC_V2,
                protocol_version: PROTOCOL_VERSION_V2,
                header_size: UDP_AUDIO_HEADER_V2_LEN as u16,
                flags: UDP_FLAG_V2_DISCONTINUITY,
                sequence: 1001,
                timestamp_ms: 100000,
                codec: UdpAudioCodecV2::Pcm16,
                channels: 2,
                sample_rate: 48_000,
                frame_duration_ms: 10,
                payload_size: 0,
                reserved: 0,
            },
            payload: vec![1, 2, 3, 4, 5, 6],
        };

        let encoded = packet.encode();
        let decoded = UdpAudioPacketV2::decode(&encoded).expect("decode v2 packet");
        assert_eq!(decoded.payload, packet.payload);
        assert_eq!(decoded.header.sequence, 1001);
        assert_eq!(decoded.header.flags, UDP_FLAG_V2_DISCONTINUITY);
    }

    #[test]
    fn udp_v3_packet_round_trip_unfragmented() {
        // Common case: 48 kHz / 5 ms PCM24 stereo fits in one packet
        // (864 B payload + 37 B header = 901 B < MTU). total_frags=1
        // means logical_seq == sequence and the receiver hands the
        // payload to the decoder with no reassembly.
        let packet = UdpAudioPacketV3 {
            header: UdpAudioHeaderV3 {
                magic: UDP_AUDIO_MAGIC_V3,
                protocol_version: PROTOCOL_VERSION_V3,
                header_size: UDP_AUDIO_HEADER_V3_LEN as u16,
                flags: 0,
                sequence: 42,
                timestamp_ms: 1_700_000_000,
                codec: UdpAudioCodecV2::Pcm24,
                channels: 2,
                sample_rate: 48_000,
                frame_duration_ms: 5,
                payload_size: 0,
                frag_index: 0,
                total_frags: 1,
                logical_seq: 42,
            },
            payload: vec![0xAA; 864],
        };
        let encoded = packet.encode();
        let decoded = UdpAudioPacketV3::decode(&encoded).expect("decode v3 packet");
        assert_eq!(decoded.payload, packet.payload);
        assert_eq!(decoded.header.codec, UdpAudioCodecV2::Pcm24);
        assert_eq!(decoded.header.frag_index, 0);
        assert_eq!(decoded.header.total_frags, 1);
        assert_eq!(decoded.header.logical_seq, 42);
        assert_eq!(decoded.header.sample_rate, 48_000);
    }

    #[test]
    fn udp_v3_packet_round_trip_fragmented() {
        // 96 kHz / 5 ms PCM24 stereo = 2880 B per logical frame, must be
        // split into 2 frags. Each frag carries half (1440 B) and shares
        // the same logical_seq. Wire-level sequence advances per packet.
        let header_template = UdpAudioHeaderV3 {
            magic: UDP_AUDIO_MAGIC_V3,
            protocol_version: PROTOCOL_VERSION_V3,
            header_size: UDP_AUDIO_HEADER_V3_LEN as u16,
            flags: 0,
            sequence: 0,
            timestamp_ms: 1_700_000_000,
            codec: UdpAudioCodecV2::Pcm24,
            channels: 2,
            sample_rate: 96_000,
            frame_duration_ms: 5,
            payload_size: 0,
            frag_index: 0,
            total_frags: 2,
            logical_seq: 7,
        };
        let frag0 = UdpAudioPacketV3 {
            header: UdpAudioHeaderV3 {
                sequence: 100,
                frag_index: 0,
                total_frags: 2,
                ..header_template.clone()
            },
            payload: vec![0x11; 1440],
        };
        let frag1 = UdpAudioPacketV3 {
            header: UdpAudioHeaderV3 {
                sequence: 101,
                frag_index: 1,
                total_frags: 2,
                ..header_template
            },
            payload: vec![0x22; 1440],
        };
        let d0 = UdpAudioPacketV3::decode(&frag0.encode()).unwrap();
        let d1 = UdpAudioPacketV3::decode(&frag1.encode()).unwrap();
        assert_eq!(d0.header.logical_seq, d1.header.logical_seq);
        assert_eq!(d0.header.frag_index, 0);
        assert_eq!(d1.header.frag_index, 1);
        assert_ne!(d0.header.sequence, d1.header.sequence);
    }

    #[test]
    fn udp_v3_rejects_invalid_frag_header() {
        let packet = UdpAudioPacketV3 {
            header: UdpAudioHeaderV3 {
                magic: UDP_AUDIO_MAGIC_V3,
                protocol_version: PROTOCOL_VERSION_V3,
                header_size: UDP_AUDIO_HEADER_V3_LEN as u16,
                flags: 0,
                sequence: 1,
                timestamp_ms: 0,
                codec: UdpAudioCodecV2::Pcm24,
                channels: 2,
                sample_rate: 96_000,
                frame_duration_ms: 5,
                payload_size: 0,
                frag_index: 5, // Invalid: >= total
                total_frags: 2,
                logical_seq: 0,
            },
            payload: vec![],
        };
        let bytes = packet.encode();
        let err = UdpAudioPacketV3::decode(&bytes).unwrap_err();
        assert_eq!(err, HeaderV3CodecError::InvalidFragHeader);
    }

    #[test]
    fn udp_v3_rejects_v2_protocol_version() {
        // A v2-version packet must NOT decode as v3 even if the magic is
        // identical. v3 receivers strictly require protocol_version=3.
        let packet = UdpAudioPacketV3 {
            header: UdpAudioHeaderV3 {
                magic: UDP_AUDIO_MAGIC_V3,
                protocol_version: PROTOCOL_VERSION_V2, // v2 byte
                header_size: UDP_AUDIO_HEADER_V3_LEN as u16,
                flags: 0,
                sequence: 1,
                timestamp_ms: 0,
                codec: UdpAudioCodecV2::Opus,
                channels: 2,
                sample_rate: 48_000,
                frame_duration_ms: 10,
                payload_size: 0,
                frag_index: 0,
                total_frags: 1,
                logical_seq: 1,
            },
            payload: vec![],
        };
        let err = UdpAudioPacketV3::decode(&packet.encode()).unwrap_err();
        assert_eq!(err, HeaderV3CodecError::UnsupportedVersion);
    }

    #[test]
    fn detect_data_plane_packet_kind_by_magic() {
        assert_eq!(
            detect_data_plane_packet_kind(b"LAS1xxxxxxxx"),
            DataPlanePacketKind::LegacyLas1
        );
        assert_eq!(
            detect_data_plane_packet_kind(b"LAV2xxxxxxxx"),
            DataPlanePacketKind::V2Lav2
        );
        assert_eq!(
            detect_data_plane_packet_kind(b"NOPE"),
            DataPlanePacketKind::Unknown
        );
    }

    #[test]
    fn mode_profile_boundary_values_match_contracts() {
        let low = audio_mode_profile(AudioMode::LowLatency);
        let balanced = audio_mode_profile(AudioMode::Balanced);
        let high = audio_mode_profile(AudioMode::HighQuality);

        assert_eq!(low.start_buffer_ms, 40);
        assert_eq!(low.max_buffer_ms, 180);
        assert_eq!(low.batch_frames, 1);
        assert_eq!(low.drop_threshold_ms, 140);
        assert!(low.prefer_low_latency_path);
        assert!(low.reset_buffer_on_switch);

        assert_eq!(balanced.start_buffer_ms, 60);
        assert_eq!(balanced.max_buffer_ms, 300);
        assert_eq!(balanced.batch_frames, 2);
        assert_eq!(balanced.drop_threshold_ms, 220);
        assert!(balanced.prefer_stable_audio_track);
        assert!(balanced.reset_buffer_on_switch);

        assert_eq!(high.start_buffer_ms, 140);
        assert_eq!(high.max_buffer_ms, 500);
        assert_eq!(high.batch_frames, 3);
        assert_eq!(high.drop_threshold_ms, 420);
        assert!(high.prefer_stable_audio_track);
        assert!(!high.reset_buffer_on_switch);
    }

    #[test]
    fn discovery_beacon_includes_reverse_ports() {
        let beacon = DiscoveryBeacon {
            kind: "lan_audio_discovery_v1".into(),
            server_id: uuid::Uuid::nil(),
            server_name: "test".into(),
            ws_port: 39991,
            udp_port: 39992,
            reverse_channel_port: 7878,
            control_channel_port: 7879,
            ts_unix_ms: 0,
        };
        let json = serde_json::to_string(&beacon).unwrap();
        assert!(json.contains("reverse_channel_port"));
        assert!(json.contains("control_channel_port"));
    }

    #[test]
    fn protocol_capabilities_include_reverse_channel() {
        let caps = ProtocolCapabilities {
            supports_pcm16: true,
            supports_f32: false,
            supports_modes: true,
            supports_metrics: true,
            supports_opus_future: false,
            supports_opus: true,
            supports_opus_experimental: true,
            supports_low_latency: true,
            supports_high_quality: true,
            supports_native_audio_track: true,
            supports_fast_path: true,
            supports_stable_audio_track: true,
            supports_usb_tethering: true,
            supports_usb_direct_future: false,
            supports_reverse_channel: false,
            supports_hires_pcm24: false,
        };
        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("supports_reverse_channel"));
    }
}
