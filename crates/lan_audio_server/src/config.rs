use std::net::SocketAddr;

use anyhow::{anyhow, Result};
use lan_audio_protocol::AudioMode;
use lan_audio_protocol::{DISCOVERY_PORT, UDP_AUDIO_PORT, WS_PORT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSourceKind {
    Synthetic,
    WindowsLoopback,
}

impl AudioSourceKind {
    pub fn parse(input: &str) -> Result<Self> {
        match input {
            "synthetic" => Ok(Self::Synthetic),
            "windows_loopback" => Ok(Self::WindowsLoopback),
            other => Err(anyhow!("unsupported audio source: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Synthetic => "synthetic",
            Self::WindowsLoopback => "windows_loopback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntheticSignalKind {
    Silence,
    Sine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPlaneFormat {
    LegacyLas1,
    V2Header,
}

impl DataPlaneFormat {
    pub fn parse(input: &str) -> Result<Self> {
        match input {
            "legacy_las1" | "legacy" | "v1" => Ok(Self::LegacyLas1),
            "v2_header" | "v2" => Ok(Self::V2Header),
            other => Err(anyhow!("unsupported data plane format: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::LegacyLas1 => "legacy_las1",
            Self::V2Header => "v2_header",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub server_name: String,
    pub discovery_bind: SocketAddr,
    pub discovery_broadcast: SocketAddr,
    pub ws_bind: SocketAddr,
    pub udp_bind: SocketAddr,
    pub sample_rate: u32,
    pub channels: u8,
    pub frames_per_packet: u16,
    pub packet_interval_ms: u64,
    pub audio_source: AudioSourceKind,
    pub audio_source_fallback_to_synthetic: bool,
    pub synthetic_signal: SyntheticSignalKind,
    pub synthetic_frequency_hz: f32,
    pub capture_debug_dump_wav: bool,
    pub capture_debug_dump_seconds: u32,
    pub capture_debug_dump_dir: String,
    pub current_audio_mode: AudioMode,
    pub data_plane_format: DataPlaneFormat,
    pub allow_loopback_v2_header_gray: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            server_name: whoami_fallback(),
            discovery_bind: "0.0.0.0:0".parse().expect("addr"),
            discovery_broadcast: format!("255.255.255.255:{DISCOVERY_PORT}")
                .parse()
                .expect("addr"),
            ws_bind: format!("0.0.0.0:{WS_PORT}").parse().expect("addr"),
            udp_bind: format!("0.0.0.0:{UDP_AUDIO_PORT}").parse().expect("addr"),
            sample_rate: 48_000,
            channels: 2,
            frames_per_packet: 480,
            packet_interval_ms: 10,
            audio_source: AudioSourceKind::Synthetic,
            audio_source_fallback_to_synthetic: true,
            synthetic_signal: SyntheticSignalKind::Sine,
            synthetic_frequency_hz: 440.0,
            capture_debug_dump_wav: false,
            capture_debug_dump_seconds: 5,
            capture_debug_dump_dir: "debug_captures".to_string(),
            current_audio_mode: AudioMode::Balanced,
            data_plane_format: DataPlaneFormat::LegacyLas1,
            allow_loopback_v2_header_gray: false,
        }
    }
}

impl ServerConfig {
    pub fn apply_args<I>(&mut self, args: I) -> Result<()>
    where
        I: IntoIterator<Item = String>,
    {
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--audio-source" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --audio-source"))?;
                    self.audio_source = AudioSourceKind::parse(&value)?;
                }
                "--synthetic-signal" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --synthetic-signal"))?;
                    self.synthetic_signal = match value.as_str() {
                        "silence" => SyntheticSignalKind::Silence,
                        "sine" => SyntheticSignalKind::Sine,
                        _ => return Err(anyhow!("unsupported synthetic signal: {value}")),
                    };
                }
                "--synthetic-frequency-hz" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --synthetic-frequency-hz"))?;
                    self.synthetic_frequency_hz = value
                        .parse::<f32>()
                        .map_err(|e| anyhow!("invalid synthetic frequency: {e}"))?;
                }
                "--no-audio-fallback" => {
                    self.audio_source_fallback_to_synthetic = false;
                }
                "--capture-dump-wav" => {
                    self.capture_debug_dump_wav = true;
                }
                "--capture-dump-seconds" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --capture-dump-seconds"))?;
                    self.capture_debug_dump_seconds = value
                        .parse::<u32>()
                        .map_err(|e| anyhow!("invalid capture dump seconds: {e}"))?;
                }
                "--capture-dump-dir" => {
                    self.capture_debug_dump_dir = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --capture-dump-dir"))?;
                }
                "--audio-mode" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --audio-mode"))?;
                    self.current_audio_mode = match value.as_str() {
                        "low_latency" => AudioMode::LowLatency,
                        "balanced" => AudioMode::Balanced,
                        "high_quality" => AudioMode::HighQuality,
                        _ => return Err(anyhow!("unsupported audio mode: {value}")),
                    };
                }
                "--data-plane" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --data-plane"))?;
                    self.data_plane_format = DataPlaneFormat::parse(&value)?;
                }
                "--allow-loopback-v2-header-gray" | "--enable-loopback-v2-header-gray" => {
                    self.allow_loopback_v2_header_gray = true;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn whoami_fallback() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "windows-desktop".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_audio_source_kind() {
        assert!(matches!(
            AudioSourceKind::parse("synthetic"),
            Ok(AudioSourceKind::Synthetic)
        ));
        assert!(matches!(
            AudioSourceKind::parse("windows_loopback"),
            Ok(AudioSourceKind::WindowsLoopback)
        ));
        assert!(AudioSourceKind::parse("unknown").is_err());
    }

    #[test]
    fn apply_args_capture_dump_flags() {
        let mut cfg = ServerConfig::default();
        cfg.apply_args(vec![
            "--capture-dump-wav".to_string(),
            "--capture-dump-seconds".to_string(),
            "9".to_string(),
            "--capture-dump-dir".to_string(),
            "tmp_out".to_string(),
        ])
        .expect("apply args");
        assert!(cfg.capture_debug_dump_wav);
        assert_eq!(cfg.capture_debug_dump_seconds, 9);
        assert_eq!(cfg.capture_debug_dump_dir, "tmp_out");
    }

    #[test]
    fn parse_data_plane_format() {
        assert!(matches!(
            DataPlaneFormat::parse("legacy_las1"),
            Ok(DataPlaneFormat::LegacyLas1)
        ));
        assert!(matches!(
            DataPlaneFormat::parse("v2_header"),
            Ok(DataPlaneFormat::V2Header)
        ));
        assert!(DataPlaneFormat::parse("unknown").is_err());
    }

    #[test]
    fn apply_args_enables_loopback_v2_gray_only_with_explicit_flag() {
        let mut cfg = ServerConfig::default();
        cfg.apply_args(vec!["--allow-loopback-v2-header-gray".to_string()])
            .expect("apply args");
        assert!(cfg.allow_loopback_v2_header_gray);
    }
}
