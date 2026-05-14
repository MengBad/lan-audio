use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use lan_audio_protocol::{
    AudioCodecPreference, AudioMode, ConnectionState, DataPlanePath, ProtocolCapabilities,
    RollbackState, ServiceMetricsSnapshot, ServiceSnapshot, TransportType,
};
use lan_audio_server::config::{
    select_data_plane_format, CodecSelection, DataPlaneFormat, ServerConfig, TransportMode,
};
use lan_audio_server::data_plane::DataPlaneRouter;
use lan_audio_server::service::LanAudioService;
use tauri::State;

use crate::state::{AppState, DesktopMetrics, DesktopServiceConfig, RunningService, ServiceStatus};

pub(crate) fn can_start_service(status: &ServiceStatus) -> Result<(), String> {
    if matches!(status, ServiceStatus::Stopping) {
        return Err("service is stopping; try again shortly".to_string());
    }
    Ok(())
}

pub(crate) fn start_service_impl(state: &State<'_, AppState>) -> Result<(), String> {
    let (cfg, run_id) = {
        let mut guard = state.inner.lock().expect("state lock");
        can_start_service(&guard.status)?;
        if matches!(
            guard.status,
            ServiceStatus::Running | ServiceStatus::Starting
        ) {
            return Ok(());
        }
        guard.status = ServiceStatus::Starting;
        guard.last_error = None;
        guard.run_id = guard.run_id.saturating_add(1);
        (guard.config.clone(), guard.run_id)
    };

    let state_for_start = Arc::clone(&state.inner);
    let logs_for_start = Arc::clone(&state.logs);
    state.runtime.spawn(async move {
        let server_cfg = match build_server_config(&cfg) {
            Ok(cfg) => cfg,
            Err(err) => {
                let mut guard = state_for_start.lock().expect("state lock");
                if guard.run_id == run_id {
                    guard.status = ServiceStatus::Error;
                    guard.last_error = Some(err.clone());
                    push_log(&logs_for_start, format!("start failed: {err}"));
                }
                return;
            }
        };

        let service = match LanAudioService::new(server_cfg).await {
            Ok(s) => Arc::new(s),
            Err(err) => {
                let message = err.to_string();
                let mut guard = state_for_start.lock().expect("state lock");
                if guard.run_id == run_id {
                    guard.status = ServiceStatus::Error;
                    guard.last_error = Some(message.clone());
                    push_log(&logs_for_start, format!("start failed: {message}"));
                }
                return;
            }
        };

        {
            let mut guard = state_for_start.lock().expect("state lock");
            if guard.run_id != run_id || !matches!(guard.status, ServiceStatus::Starting) {
                return;
            }
            guard.status = ServiceStatus::Running;
            guard.last_error = None;
            guard.running = Some(RunningService {
                run_id,
                service: Arc::clone(&service),
                task: None,
            });
        }

        push_log(
            &logs_for_start,
            format!(
                "service started (audio_source={}, data_plane={}, loopback_v2_gray={})",
                cfg.audio_source.as_str(),
                cfg.data_plane_format.as_str(),
                cfg.allow_loopback_v2_header_gray
            ),
        );

        let service_for_task = Arc::clone(&service);
        let state_for_task = Arc::clone(&state_for_start);
        let logs_for_task = Arc::clone(&logs_for_start);
        let task = tokio::spawn(async move {
            let result = service_for_task.run_until_shutdown().await;
            let mut guard = state_for_task.lock().expect("state lock");
            if guard.run_id != run_id {
                return;
            }
            guard.running = None;
            match result {
                Ok(()) => {
                    guard.status = ServiceStatus::NotStarted;
                    push_log(&logs_for_task, "service stopped");
                }
                Err(err) => {
                    let message = err.to_string();
                    guard.status = ServiceStatus::Error;
                    guard.last_error = Some(message.clone());
                    push_log(&logs_for_task, format!("service error: {message}"));
                }
            }
        });

        let mut guard = state_for_start.lock().expect("state lock");
        if guard.run_id != run_id {
            return;
        }
        if let Some(running) = guard.running.as_mut() {
            running.task = Some(task);
        }
    });

    push_log(&state.logs, "service startup scheduled");
    Ok(())
}

pub(crate) fn stop_service_impl(state: &State<'_, AppState>, wait: bool) -> Result<(), String> {
    let (service, task, run_id) = {
        let mut guard = state.inner.lock().expect("state lock");
        if guard.running.is_none() {
            if matches!(guard.status, ServiceStatus::Starting) {
                guard.run_id = guard.run_id.saturating_add(1);
                guard.last_error = None;
                guard.status = ServiceStatus::NotStarted;
                push_log(&state.logs, "service startup canceled");
                return Ok(());
            }
            guard.status = ServiceStatus::NotStarted;
            return Ok(());
        }
        guard.status = ServiceStatus::Stopping;
        let running = guard.running.as_mut().expect("running service");
        let task = if wait { running.task.take() } else { None };
        (Arc::clone(&running.service), task, running.run_id)
    };

    service.stop();
    push_log(&state.logs, "stop signal sent");

    if let Some(join_handle) = task {
        let _ = state.runtime.block_on(join_handle);
        let mut guard = state.inner.lock().expect("state lock");
        if guard.run_id == run_id {
            guard.running = None;
            guard.status = ServiceStatus::NotStarted;
        }
        push_log(&state.logs, "service fully stopped");
    }

    Ok(())
}

pub(crate) fn build_server_config(cfg: &DesktopServiceConfig) -> Result<ServerConfig, String> {
    let mut server = ServerConfig::default();
    server.audio_source = cfg.audio_source;
    server.data_plane_format = cfg.data_plane_format;
    server.codec_selection = cfg.codec_selection;
    server.allow_loopback_v2_header_gray = cfg.allow_loopback_v2_header_gray;
    server.audio_source_fallback_to_synthetic = cfg.fallback_to_synthetic;
    server.capture_debug_dump_wav = cfg.capture_dump_wav;
    server.current_audio_mode = cfg.current_audio_mode;
    server.transport_mode = cfg.transport_mode.clone();
    let bind_host = match cfg.transport_mode {
        TransportMode::WiFi => "0.0.0.0",
        TransportMode::Usb { .. } => "127.0.0.1",
    };
    server.ws_bind = parse_bind(bind_host, cfg.ws_port)?;
    server.udp_bind = parse_bind(bind_host, cfg.udp_port)?;
    Ok(server)
}

pub(crate) fn recommended_connection(cfg: &DesktopServiceConfig) -> &'static str {
    if matches!(cfg.transport_mode, TransportMode::Usb { .. }) {
        "USB localhost tunnel"
    } else if selected_data_plane_for_desktop_config(cfg) == DataPlaneFormat::V2Header {
        "USB tethering or 5GHz Wi-Fi"
    } else {
        "Same Wi-Fi network"
    }
}

pub(crate) fn selected_data_plane_for_desktop_config(
    cfg: &DesktopServiceConfig,
) -> DataPlaneFormat {
    select_data_plane_format(
        cfg.data_plane_format,
        cfg.audio_source,
        cfg.allow_loopback_v2_header_gray,
    )
}

pub(crate) fn effective_codec_for_desktop_config(cfg: &DesktopServiceConfig) -> CodecSelection {
    match (
        cfg.codec_selection,
        selected_data_plane_for_desktop_config(cfg),
    ) {
        (CodecSelection::Opus, DataPlaneFormat::V2Header) => CodecSelection::Opus,
        _ => CodecSelection::Pcm16,
    }
}

pub(crate) fn configured_data_plane_router(cfg: &DesktopServiceConfig) -> Option<DataPlaneRouter> {
    build_server_config(cfg)
        .ok()
        .map(|server_cfg| DataPlaneRouter::from_config(&server_cfg))
}

pub(crate) fn build_service_snapshot(
    cfg: &DesktopServiceConfig,
    metrics: &DesktopMetrics,
    status: &ServiceStatus,
    current_audio_mode: AudioMode,
    selected_data_plane: &DataPlaneFormat,
    active_data_plane: DataPlanePath,
    rollback_available: bool,
    effective_codec: &str,
) -> ServiceSnapshot {
    let transport = match cfg.transport_mode {
        TransportMode::WiFi => TransportType::Wifi,
        TransportMode::Usb { .. } => TransportType::Usb,
    };
    let data_plane = match selected_data_plane {
        DataPlaneFormat::LegacyLas1 => DataPlanePath::LegacyLas1,
        DataPlaneFormat::V2Header => DataPlanePath::V2Header,
    };
    let requested_codec = match cfg.codec_selection {
        CodecSelection::Pcm16 => AudioCodecPreference::Pcm16,
        CodecSelection::Opus => AudioCodecPreference::Opus,
        CodecSelection::Pcm24 => AudioCodecPreference::Pcm24,
    };
    let effective_codec = match effective_codec {
        "opus" => AudioCodecPreference::Opus,
        "pcm24" => AudioCodecPreference::Pcm24,
        _ => AudioCodecPreference::Pcm16,
    };
    let rollback_state = if matches!(data_plane, DataPlanePath::LegacyLas1)
        && matches!(effective_codec, AudioCodecPreference::Pcm16)
    {
        RollbackState::ForcedLegacyLas1Pcm16
    } else {
        RollbackState::MainPathActive
    };
    let state = match status {
        ServiceStatus::Starting => ConnectionState::Handshaking,
        ServiceStatus::Running if metrics.active_sessions > 0 => ConnectionState::Streaming,
        ServiceStatus::Stopping | ServiceStatus::Error => ConnectionState::Closed,
        ServiceStatus::NotStarted | ServiceStatus::Running => ConnectionState::Disconnected,
    };

    ServiceSnapshot {
        transport,
        mode: current_audio_mode,
        data_plane,
        active_data_plane,
        rollback_available,
        codec: requested_codec,
        effective_codec,
        state,
        rollback_state,
        metrics: ServiceMetricsSnapshot::default(),
        last_error: None,
    }
}

fn parse_bind(host: &str, port: u16) -> Result<SocketAddr, String> {
    format!("{host}:{port}")
        .parse::<SocketAddr>()
        .map_err(|e| e.to_string())
}

pub(crate) fn detect_local_ip() -> String {
    let socket = UdpSocket::bind("0.0.0.0:0").ok();
    if let Some(socket) = socket {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(local) = socket.local_addr() {
                if let IpAddr::V4(ip) = local.ip() {
                    return ip.to_string();
                }
            }
        }
    }
    "127.0.0.1".to_string()
}

pub(crate) fn empty_metrics(audio_source: &str) -> DesktopMetrics {
    DesktopMetrics {
        tx_packets: 0,
        tx_bytes: 0,
        active_sessions: 0,
        capture_frames_produced: 0,
        capture_read_errors: 0,
        capture_underruns: 0,
        capture_start_attempts: 0,
        capture_start_failures: 0,
        capture_silent_frames: 0,
        capture_non_silent_frames: 0,
        capture_no_packet_count: 0,
        current_audio_source: audio_source.to_string(),
        capture_source_state: "idle".to_string(),
        capture_device_name: "n/a".to_string(),
        negotiated_data_plane: String::new(),
        negotiated_codec: String::new(),
        capture_sample_rate: 0,
        capture_channels: 0,
        capture_buffer_frames: 0,
        capture_last_peak: 0.0,
        capture_last_rms: 0.0,
        last_capture_pts_ms: 0,
        recent_clients: Vec::new(),
    }
}

pub(crate) fn default_protocol_capabilities() -> ProtocolCapabilities {
    ProtocolCapabilities {
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
    }
}

pub(crate) fn push_log(logs: &Arc<Mutex<VecDeque<String>>>, message: impl Into<String>) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut guard = logs.lock().expect("logs lock");
    guard.push_front(format!("[{timestamp}] {}", message.into()));
    while guard.len() > 120 {
        let _ = guard.pop_back();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn service_snapshot_marks_forced_rollback_on_legacy_pcm16() {
        let cfg = DesktopServiceConfig::default();
        let metrics = empty_metrics(cfg.audio_source.as_str());
        let snapshot = build_service_snapshot(
            &cfg,
            &metrics,
            &ServiceStatus::Running,
            AudioMode::Balanced,
            &DataPlaneFormat::LegacyLas1,
            DataPlanePath::LegacyLas1,
            false,
            "pcm16",
        );
        assert_eq!(
            snapshot.rollback_state,
            RollbackState::ForcedLegacyLas1Pcm16
        );
        assert_eq!(snapshot.data_plane, DataPlanePath::LegacyLas1);
        assert_eq!(snapshot.effective_codec, AudioCodecPreference::Pcm16);
    }

    #[test]
    fn cannot_start_service_while_stopping() {
        let result = can_start_service(&ServiceStatus::Stopping);
        assert!(result.is_err());
    }

    #[test]
    fn service_snapshot_serializes_stable_contract() {
        let cfg = DesktopServiceConfig::default();
        let metrics = empty_metrics(cfg.audio_source.as_str());
        let snapshot = build_service_snapshot(
            &cfg,
            &metrics,
            &ServiceStatus::NotStarted,
            AudioMode::Balanced,
            &DataPlaneFormat::V2Header,
            DataPlanePath::UsbDirect,
            true,
            "opus",
        );
        assert_eq!(
            serde_json::to_value(snapshot).unwrap(),
            json!({
                "transport": "wifi",
                "mode": "balanced",
                "data_plane": "v2_header",
                "active_data_plane": "usb_direct",
                "rollback_available": true,
                "codec": "opus",
                "effective_codec": "opus",
                "state": "disconnected",
                "rollback_state": "main_path_active",
                "metrics": {
                    "buffered_ms": 0,
                    "underrun": 0,
                    "late_packets": 0,
                    "dropped_packets": 0,
                    "rtt_ms": 0,
                    "reconnect_count": 0,
                    "decode_errors": 0,
                    "sink_write_gap_ms_p95": 0
                }
            })
        );
    }
}
