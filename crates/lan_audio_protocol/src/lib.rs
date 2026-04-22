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

pub const PROTOCOL_VERSION_V2: u8 = 2;

pub const AUDIO_MAGIC: [u8; 4] = *b"LAS1";
pub const AUDIO_HEADER_LEN: usize = 4 + 1 + 1 + 4 + 8 + 4 + 1 + 2 + 2;

pub const UDP_AUDIO_MAGIC_V2: [u8; 4] = *b"LAV2";
pub const UDP_AUDIO_HEADER_V2_LEN: usize = 4 + 1 + 2 + 2 + 4 + 8 + 1 + 1 + 4 + 2 + 2 + 2;

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioModeChanged {
    pub mode: AudioMode,
    pub applied: bool,
    pub reason: String,
    #[serde(default)]
    pub mode_profile: AudioModeProfile,
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
}

impl UdpAudioCodecV2 {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Pcm16),
            2 => Some(Self::F32),
            3 => Some(Self::Opus),
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
        header.payload_size = u16::try_from(self.payload.len()).expect("v2 payload too large");

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
        let payload_len = u16::try_from(self.payload.len()).expect("v1 payload too large");
        out.extend_from_slice(&payload_len.to_le_bytes());
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
        };
        let json = serde_json::to_string(&caps).expect("serialize caps");
        let decoded: ProtocolCapabilities = serde_json::from_str(&json).expect("deserialize caps");
        assert_eq!(decoded, caps);
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
            },
            message: "ok".to_string(),
        });

        let json = serde_json::to_string(&msg).expect("serialize hello ack");
        assert!(json.contains("\"type\":\"hello_ack\""));
        let decoded: ControlMessageV2 = serde_json::from_str(&json).expect("deserialize hello ack");
        assert_eq!(decoded, msg);
    }

    #[test]
    fn v2_control_message_set_audio_mode_round_trip() {
        let msg = ControlMessageV2::SetAudioMode(SetAudioMode {
            mode: AudioMode::LowLatency,
            reason: "user_selected".to_string(),
            preferred_sample_rate: Some(48_000),
        });
        let json = serde_json::to_string(&msg).expect("serialize set audio mode");
        assert!(json.contains("\"type\":\"set_audio_mode\""));
        assert!(json.contains("\"preferred_sample_rate\":48000"));
        let decoded: ControlMessageV2 =
            serde_json::from_str(&json).expect("deserialize set audio mode");
        assert_eq!(decoded, msg);
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
    #[should_panic(expected = "v1 payload too large")]
    fn udp_packet_encode_rejects_payload_larger_than_u16() {
        let packet = UdpAudioPacket {
            version: 1,
            flags: 0,
            sequence: 1,
            timestamp_ms: 0,
            sample_rate: 48_000,
            channels: 2,
            frames_per_packet: 480,
            payload: vec![0; (u16::MAX as usize) + 1],
        };
        let _ = packet.encode();
    }

    #[test]
    #[should_panic(expected = "v2 payload too large")]
    fn udp_packet_v2_encode_rejects_payload_larger_than_u16() {
        let packet = UdpAudioPacketV2 {
            header: UdpAudioHeaderV2 {
                magic: UDP_AUDIO_MAGIC_V2,
                protocol_version: PROTOCOL_VERSION_V2,
                header_size: UDP_AUDIO_HEADER_V2_LEN as u16,
                flags: 0,
                sequence: 7,
                timestamp_ms: 99,
                codec: UdpAudioCodecV2::Pcm16,
                channels: 2,
                sample_rate: 48_000,
                frame_duration_ms: 10,
                payload_size: 0,
                reserved: 0,
            },
            payload: vec![0; (u16::MAX as usize) + 1],
        };
        let _ = packet.encode();
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
}
