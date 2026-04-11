//! Shared protocol definitions for desktop and mobile.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const DISCOVERY_PORT: u16 = 39990;
pub const WS_PORT: u16 = 39991;
pub const UDP_AUDIO_PORT: u16 = 39992;

pub const AUDIO_MAGIC: [u8; 4] = *b"LAS1";
pub const AUDIO_HEADER_LEN: usize = 4 + 1 + 1 + 4 + 8 + 4 + 1 + 2 + 2;

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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PacketCodecError {
    #[error("packet too short")]
    TooShort,
    #[error("invalid magic")]
    InvalidMagic,
    #[error("payload length mismatch")]
    PayloadLengthMismatch,
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
}
