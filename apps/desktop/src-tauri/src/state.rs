use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use lan_audio_protocol::{AudioMode, AudioModeProfile, ProtocolCapabilities, ServiceSnapshot};
use lan_audio_server::config::{
    AudioSourceKind, CodecSelection, DataPlaneFormat, ServerConfig, TransportMode,
};
use lan_audio_server::metrics::MetricsSnapshot;
use lan_audio_server::service::LanAudioService;
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::update_checker::UpdateInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceStatus {
    NotStarted,
    Starting,
    Running,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DesktopMetrics {
    pub(crate) tx_packets: u64,
    pub(crate) tx_bytes: u64,
    pub(crate) active_sessions: u64,
    pub(crate) capture_frames_produced: u64,
    pub(crate) capture_read_errors: u64,
    pub(crate) capture_underruns: u64,
    pub(crate) capture_start_attempts: u64,
    pub(crate) capture_start_failures: u64,
    pub(crate) capture_silent_frames: u64,
    pub(crate) capture_non_silent_frames: u64,
    pub(crate) capture_no_packet_count: u64,
    pub(crate) current_audio_source: String,
    pub(crate) capture_source_state: String,
    pub(crate) capture_device_name: String,
    pub(crate) negotiated_data_plane: String,
    pub(crate) negotiated_codec: String,
    pub(crate) capture_sample_rate: u64,
    pub(crate) capture_channels: u64,
    pub(crate) capture_buffer_frames: u64,
    pub(crate) capture_last_peak: f32,
    pub(crate) capture_last_rms: f32,
    pub(crate) last_capture_pts_ms: u64,
    pub(crate) recent_clients: Vec<String>,
}

impl From<MetricsSnapshot> for DesktopMetrics {
    fn from(value: MetricsSnapshot) -> Self {
        Self {
            tx_packets: value.tx_packets,
            tx_bytes: value.tx_bytes,
            active_sessions: value.active_sessions,
            capture_frames_produced: value.capture_frames_produced,
            capture_read_errors: value.capture_read_errors,
            capture_underruns: value.capture_underruns,
            capture_start_attempts: value.capture_start_attempts,
            capture_start_failures: value.capture_start_failures,
            capture_silent_frames: value.capture_silent_frames,
            capture_non_silent_frames: value.capture_non_silent_frames,
            capture_no_packet_count: value.capture_no_packet_count,
            current_audio_source: value.current_audio_source,
            capture_source_state: value.capture_source_state,
            capture_device_name: value.capture_device_name,
            negotiated_data_plane: value.negotiated_data_plane,
            negotiated_codec: value.negotiated_codec,
            capture_sample_rate: value.capture_sample_rate,
            capture_channels: value.capture_channels,
            capture_buffer_frames: value.capture_buffer_frames,
            capture_last_peak: value.capture_last_peak,
            capture_last_rms: value.capture_last_rms,
            last_capture_pts_ms: value.last_capture_pts_ms,
            recent_clients: value.recent_clients,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DesktopSnapshot {
    pub(crate) service_status: ServiceStatus,
    pub(crate) error_message: Option<String>,
    pub(crate) service_snapshot: ServiceSnapshot,
    pub(crate) audio_source: String,
    pub(crate) data_plane_format: String,
    pub(crate) protocol_path: String,
    pub(crate) gray_mode: bool,
    pub(crate) codec_selection: String,
    pub(crate) effective_codec: String,
    pub(crate) recommended_connection: String,
    pub(crate) loopback_v2_header_gray_enabled: bool,
    pub(crate) fallback_to_synthetic: bool,
    pub(crate) capture_dump_wav: bool,
    pub(crate) local_ip: String,
    pub(crate) ws_port: u16,
    pub(crate) udp_port: u16,
    pub(crate) connect_address: String,
    pub(crate) connected_devices: u64,
    pub(crate) recent_clients: Vec<String>,
    pub(crate) connection_status: String,
    pub(crate) session_status: String,
    pub(crate) current_audio_mode: String,
    pub(crate) mode_profile: AudioModeProfile,
    pub(crate) protocol_version: u8,
    pub(crate) capabilities: ProtocolCapabilities,
    pub(crate) version: String,
    pub(crate) transport_mode: String,
    pub(crate) usb_serial: Option<String>,
    pub(crate) metrics: DesktopMetrics,
    pub(crate) logs: Vec<String>,
    pub(crate) update_banner: Option<UpdateBanner>,
}

#[derive(Debug, Clone)]
pub(crate) struct DesktopServiceConfig {
    pub(crate) audio_source: AudioSourceKind,
    pub(crate) data_plane_format: DataPlaneFormat,
    pub(crate) codec_selection: CodecSelection,
    pub(crate) allow_loopback_v2_header_gray: bool,
    pub(crate) fallback_to_synthetic: bool,
    pub(crate) capture_dump_wav: bool,
    pub(crate) ws_port: u16,
    pub(crate) udp_port: u16,
    pub(crate) current_audio_mode: AudioMode,
    pub(crate) transport_mode: TransportMode,
}

impl Default for DesktopServiceConfig {
    fn default() -> Self {
        let cfg = ServerConfig::default();
        Self {
            audio_source: cfg.audio_source,
            data_plane_format: cfg.data_plane_format,
            codec_selection: cfg.codec_selection,
            allow_loopback_v2_header_gray: cfg.allow_loopback_v2_header_gray,
            fallback_to_synthetic: cfg.audio_source_fallback_to_synthetic,
            capture_dump_wav: cfg.capture_debug_dump_wav,
            ws_port: cfg.ws_bind.port(),
            udp_port: cfg.udp_bind.port(),
            current_audio_mode: cfg.current_audio_mode,
            transport_mode: cfg.transport_mode,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ServiceSettingsInput {
    pub(crate) audio_source: String,
    pub(crate) data_plane_format: String,
    pub(crate) codec_selection: String,
    pub(crate) allow_loopback_v2_header_gray: bool,
    pub(crate) fallback_to_synthetic: bool,
    pub(crate) capture_dump_wav: bool,
}

pub(crate) struct RunningService {
    pub(crate) run_id: u64,
    pub(crate) service: Arc<LanAudioService>,
    pub(crate) task: Option<JoinHandle<()>>,
}

pub(crate) struct AppStateInner {
    pub(crate) status: ServiceStatus,
    pub(crate) last_error: Option<String>,
    pub(crate) run_id: u64,
    pub(crate) config: DesktopServiceConfig,
    pub(crate) running: Option<RunningService>,
}

pub(crate) struct AppState {
    pub(crate) runtime: Arc<Runtime>,
    pub(crate) inner: Arc<Mutex<AppStateInner>>,
    pub(crate) logs: Arc<Mutex<VecDeque<String>>>,
    pub(crate) update_state: Arc<Mutex<UpdateState>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DiagnosticsReport {
    pub(crate) exported_at_unix_seconds: u64,
    pub(crate) snapshot: DesktopSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UpdateBanner {
    pub(crate) latest_version: String,
    pub(crate) release_url: String,
}

#[derive(Debug, Default)]
pub(crate) struct UpdateState {
    pub(crate) available: Option<UpdateInfo>,
}
