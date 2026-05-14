use std::net::SocketAddr;

use anyhow::{anyhow, Result};
use lan_audio_protocol::{AudioCodecPreference, AudioMode, UdpAudioCodecV2};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodecSelection {
    Pcm16,
    Opus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportMode {
    WiFi,
    Usb { serial: String },
}

impl TransportMode {
    pub fn parse(input: &str, adb_serial: Option<String>) -> Result<Self> {
        match input {
            "wifi" => Ok(Self::WiFi),
            "usb" => Ok(Self::Usb {
                serial: adb_serial
                    .ok_or_else(|| anyhow!("--transport usb requires --adb-serial"))?,
            }),
            other => Err(anyhow!("unsupported transport mode: {other}")),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WiFi => "wifi",
            Self::Usb { .. } => "usb",
        }
    }

    pub fn adb_serial(&self) -> Option<&str> {
        match self {
            Self::WiFi => None,
            Self::Usb { serial } => Some(serial.as_str()),
        }
    }
}

impl CodecSelection {
    pub fn parse(input: &str) -> Result<Self> {
        match input {
            "pcm16" | "pcm" => Ok(Self::Pcm16),
            "opus" | "opus_experimental" => Ok(Self::Opus),
            other => Err(anyhow!("unsupported codec: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pcm16 => "pcm16",
            Self::Opus => "opus",
        }
    }

    pub fn as_protocol_preference(self) -> AudioCodecPreference {
        match self {
            Self::Pcm16 => AudioCodecPreference::Pcm16,
            Self::Opus => AudioCodecPreference::Opus,
        }
    }

    pub fn as_udp_codec(self) -> UdpAudioCodecV2 {
        match self {
            Self::Pcm16 => UdpAudioCodecV2::Pcm16,
            Self::Opus => UdpAudioCodecV2::Opus,
        }
    }
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
    pub codec_selection: CodecSelection,
    pub data_plane_format: DataPlaneFormat,
    pub allow_loopback_v2_header_gray: bool,
    pub transport_mode: TransportMode,
    pub force_rollback: bool,
    pub reverse_channel_enabled: bool,
    /// Phase 4 adaptive runtime (CPU watchdog + tier-based encoder degrade).
    /// Default ON; pass `--no-adaptive-runtime` on the command line to bypass
    /// for emergency rollback.
    pub adaptive_runtime_enabled: bool,
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
            audio_source: AudioSourceKind::WindowsLoopback,
            audio_source_fallback_to_synthetic: true,
            synthetic_signal: SyntheticSignalKind::Sine,
            synthetic_frequency_hz: 440.0,
            capture_debug_dump_wav: false,
            capture_debug_dump_seconds: 5,
            capture_debug_dump_dir: "debug_captures".to_string(),
            current_audio_mode: AudioMode::Balanced,
            codec_selection: CodecSelection::Opus,
            data_plane_format: DataPlaneFormat::V2Header,
            allow_loopback_v2_header_gray: false,
            transport_mode: TransportMode::WiFi,
            force_rollback: false,
            reverse_channel_enabled: false,
            adaptive_runtime_enabled: true,
        }
    }
}

impl ServerConfig {
    pub fn selected_data_plane_format(&self) -> DataPlaneFormat {
        select_data_plane_format(
            self.data_plane_format,
            self.audio_source,
            self.allow_loopback_v2_header_gray,
        )
    }

    pub fn effective_codec_selection(&self) -> CodecSelection {
        match (self.codec_selection, self.selected_data_plane_format()) {
            (CodecSelection::Opus, DataPlaneFormat::V2Header) => CodecSelection::Opus,
            _ => CodecSelection::Pcm16,
        }
    }

    pub fn apply_args<I>(&mut self, args: I) -> Result<()>
    where
        I: IntoIterator<Item = String>,
    {
        let mut transport_mode_name = self.transport_mode.as_str().to_string();
        let mut adb_serial = self.transport_mode.adb_serial().map(ToOwned::to_owned);
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
                "--codec" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --codec"))?;
                    self.codec_selection = CodecSelection::parse(&value)?;
                }
                "--allow-loopback-v2-header-gray" | "--enable-loopback-v2-header-gray" => {
                    self.allow_loopback_v2_header_gray = true;
                }
                "--transport" => {
                    transport_mode_name = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --transport"))?;
                }
                "--adb-serial" => {
                    adb_serial = Some(
                        iter.next()
                            .ok_or_else(|| anyhow!("missing value for --adb-serial"))?,
                    );
                }
                "--force-rollback" => {
                    self.force_rollback = true;
                }
                "--reverse-channel" => {
                    self.reverse_channel_enabled = true;
                }
                "--no-reverse-channel" => {
                    self.reverse_channel_enabled = false;
                }
                "--no-adaptive-runtime" => {
                    self.adaptive_runtime_enabled = false;
                }
                "--adaptive-runtime" => {
                    self.adaptive_runtime_enabled = true;
                }
                _ => {}
            }
        }
        self.transport_mode = TransportMode::parse(&transport_mode_name, adb_serial)?;
        Ok(())
    }
}

pub fn select_data_plane_format(
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

    #[test]
    fn parse_codec_selection() {
        assert!(matches!(
            CodecSelection::parse("pcm16"),
            Ok(CodecSelection::Pcm16)
        ));
        assert!(matches!(
            CodecSelection::parse("opus"),
            Ok(CodecSelection::Opus)
        ));
        assert!(CodecSelection::parse("bad").is_err());
        assert_eq!(
            CodecSelection::Opus.as_protocol_preference(),
            AudioCodecPreference::Opus
        );
    }

    #[test]
    fn effective_codec_requires_v2_header() {
        let mut cfg = ServerConfig {
            codec_selection: CodecSelection::Opus,
            ..ServerConfig::default()
        };
        cfg.data_plane_format = DataPlaneFormat::LegacyLas1;
        assert_eq!(cfg.effective_codec_selection(), CodecSelection::Pcm16);

        cfg.data_plane_format = DataPlaneFormat::V2Header;
        cfg.audio_source = AudioSourceKind::Synthetic;
        assert_eq!(cfg.effective_codec_selection(), CodecSelection::Opus);
    }

    #[test]
    fn loopback_v2_header_is_recommended_without_gray_flag() {
        let mut cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            audio_source: AudioSourceKind::WindowsLoopback,
            ..ServerConfig::default()
        };
        assert_eq!(cfg.selected_data_plane_format(), DataPlaneFormat::V2Header);

        cfg.allow_loopback_v2_header_gray = true;
        assert_eq!(cfg.selected_data_plane_format(), DataPlaneFormat::V2Header);
    }

    #[test]
    fn apply_args_parses_usb_transport_mode() {
        let mut cfg = ServerConfig::default();
        cfg.apply_args(vec![
            "--transport".to_string(),
            "usb".to_string(),
            "--adb-serial".to_string(),
            "device-123".to_string(),
        ])
        .expect("apply args");

        assert_eq!(
            cfg.transport_mode,
            TransportMode::Usb {
                serial: "device-123".to_string()
            }
        );
    }

    #[test]
    fn rollback_path_remains_forceable_via_cli_contract() {
        let mut cfg = ServerConfig::default();
        cfg.apply_args(vec![
            "--audio-source".to_string(),
            "windows_loopback".to_string(),
            "--data-plane".to_string(),
            "legacy_las1".to_string(),
            "--codec".to_string(),
            "pcm16".to_string(),
        ])
        .expect("apply rollback args");

        assert_eq!(
            cfg.selected_data_plane_format(),
            DataPlaneFormat::LegacyLas1
        );
        assert_eq!(cfg.effective_codec_selection(), CodecSelection::Pcm16);
    }
}
