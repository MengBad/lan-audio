use std::collections::VecDeque;
use std::fs;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

mod update_checker;

use lan_audio_protocol::{
    audio_mode_profile, AudioCodecPreference, AudioMode, AudioModeProfile, ConnectionState,
    DataPlanePath, ProtocolCapabilities, RollbackState, ServiceMetricsSnapshot, ServiceSnapshot,
    TransportType, PROTOCOL_VERSION_V2,
};
use lan_audio_server::config::{
    select_data_plane_format, AudioSourceKind, CodecSelection, DataPlaneFormat, ServerConfig,
    TransportMode,
};
use lan_audio_server::data_plane::DataPlaneRouter;
use lan_audio_server::metrics::MetricsSnapshot;
use lan_audio_server::service::LanAudioService;
use lan_audio_server::usb_transport::adb_devices;
use serde::{Deserialize, Serialize};
use tauri::menu::{MenuBuilder, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use update_checker::UpdateInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum ServiceStatus {
    NotStarted,
    Starting,
    Running,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Serialize)]
struct DesktopMetrics {
    tx_packets: u64,
    tx_bytes: u64,
    active_sessions: u64,
    capture_frames_produced: u64,
    capture_read_errors: u64,
    capture_underruns: u64,
    capture_start_attempts: u64,
    capture_start_failures: u64,
    capture_silent_frames: u64,
    capture_non_silent_frames: u64,
    capture_no_packet_count: u64,
    current_audio_source: String,
    capture_source_state: String,
    capture_device_name: String,
    negotiated_data_plane: String,
    negotiated_codec: String,
    capture_sample_rate: u64,
    capture_channels: u64,
    capture_buffer_frames: u64,
    capture_last_peak: f32,
    capture_last_rms: f32,
    last_capture_pts_ms: u64,
    recent_clients: Vec<String>,
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
struct DesktopSnapshot {
    service_status: ServiceStatus,
    error_message: Option<String>,
    service_snapshot: ServiceSnapshot,
    audio_source: String,
    data_plane_format: String,
    protocol_path: String,
    gray_mode: bool,
    codec_selection: String,
    effective_codec: String,
    recommended_connection: String,
    loopback_v2_header_gray_enabled: bool,
    fallback_to_synthetic: bool,
    capture_dump_wav: bool,
    local_ip: String,
    ws_port: u16,
    udp_port: u16,
    connect_address: String,
    connected_devices: u64,
    recent_clients: Vec<String>,
    connection_status: String,
    session_status: String,
    current_audio_mode: String,
    mode_profile: AudioModeProfile,
    protocol_version: u8,
    capabilities: ProtocolCapabilities,
    version: String,
    transport_mode: String,
    usb_serial: Option<String>,
    metrics: DesktopMetrics,
    mic_active: bool,
    mic_peak_db: f32,
    mic_rms_db: f32,
    mic_device_name: String,
    android_volume_pct: u8,
    reverse_channel_enabled: bool,
    logs: Vec<String>,
    update_banner: Option<UpdateBanner>,
}

#[derive(Debug, Clone)]
struct DesktopServiceConfig {
    audio_source: AudioSourceKind,
    data_plane_format: DataPlaneFormat,
    codec_selection: CodecSelection,
    allow_loopback_v2_header_gray: bool,
    fallback_to_synthetic: bool,
    capture_dump_wav: bool,
    reverse_channel_enabled: bool,
    ws_port: u16,
    udp_port: u16,
    current_audio_mode: AudioMode,
    transport_mode: TransportMode,
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
            reverse_channel_enabled: true,
            ws_port: cfg.ws_bind.port(),
            udp_port: cfg.udp_bind.port(),
            current_audio_mode: cfg.current_audio_mode,
            transport_mode: cfg.transport_mode,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ServiceSettingsInput {
    audio_source: String,
    data_plane_format: String,
    codec_selection: String,
    allow_loopback_v2_header_gray: bool,
    fallback_to_synthetic: bool,
    capture_dump_wav: bool,
}

struct RunningService {
    run_id: u64,
    service: Arc<LanAudioService>,
    task: Option<JoinHandle<()>>,
}

struct AppStateInner {
    status: ServiceStatus,
    last_error: Option<String>,
    run_id: u64,
    config: DesktopServiceConfig,
    running: Option<RunningService>,
}

struct AppState {
    runtime: Arc<Runtime>,
    inner: Arc<Mutex<AppStateInner>>,
    logs: Arc<Mutex<VecDeque<String>>>,
    update_state: Arc<Mutex<UpdateState>>,
}

#[derive(Debug, Serialize)]
struct DiagnosticsReport {
    exported_at_unix_seconds: u64,
    snapshot: DesktopSnapshot,
}

#[derive(Debug, Clone, Serialize)]
struct UpdateBanner {
    latest_version: String,
    release_url: String,
}

#[derive(Debug, Default)]
struct UpdateState {
    available: Option<UpdateInfo>,
}

#[tauri::command]
fn get_desktop_snapshot(state: State<'_, AppState>) -> DesktopSnapshot {
    let (status, last_error, cfg, service) = {
        let guard = state.inner.lock().expect("state lock");
        (
            guard.status.clone(),
            guard.last_error.clone(),
            guard.config.clone(),
            guard.running.as_ref().map(|s| Arc::clone(&s.service)),
        )
    };

    let metrics = service
        .as_ref()
        .map(|svc| svc.metrics_snapshot())
        .map(DesktopMetrics::from)
        .unwrap_or_else(|| empty_metrics(cfg.audio_source.as_str()));

    let reverse_channel_state = service
        .as_ref()
        .map(|svc| svc.reverse_channel_state())
        .unwrap_or_default();

    let current_audio_mode = service
        .as_ref()
        .map(|svc| svc.current_audio_mode())
        .unwrap_or(cfg.current_audio_mode);
    let mode_profile = audio_mode_profile(current_audio_mode);
    let selected_data_plane = selected_data_plane_for_desktop_config(&cfg);
    let configured_router = build_server_config(&cfg)
        .ok()
        .map(|server_cfg| DataPlaneRouter::from_config(&server_cfg));
    let configured_active_data_plane = configured_router
        .as_ref()
        .map(|router| router.active_path())
        .unwrap_or_else(|| match selected_data_plane {
            DataPlaneFormat::LegacyLas1 => DataPlanePath::LegacyLas1,
            DataPlaneFormat::V2Header => DataPlanePath::V2Header,
        });
    let configured_active_codec = configured_router
        .as_ref()
        .map(|router| router.active_codec())
        .unwrap_or_else(|| effective_codec_for_desktop_config(&cfg));
    let rollback_available = service
        .as_ref()
        .map(|svc| svc.data_plane_status().rollback_available)
        .or_else(|| {
            configured_router
                .as_ref()
                .map(|router| router.rollback_available())
        })
        .unwrap_or(true);
    let active_data_plane = service
        .as_ref()
        .map(|svc| svc.data_plane_status().active_path)
        .unwrap_or(configured_active_data_plane);
    let protocol_path = if metrics.active_sessions > 0 && !metrics.negotiated_data_plane.is_empty()
    {
        metrics.negotiated_data_plane.clone()
    } else {
        selected_data_plane.as_str().to_string()
    };
    let gray_mode = false;
    let effective_codec = if metrics.active_sessions > 0 && !metrics.negotiated_codec.is_empty() {
        metrics.negotiated_codec.clone()
    } else {
        configured_active_codec.as_str().to_string()
    };
    let service_snapshot = build_service_snapshot(
        &cfg,
        &metrics,
        &status,
        current_audio_mode,
        &selected_data_plane,
        active_data_plane,
        rollback_available,
        &effective_codec,
    );

    let local_ip = detect_local_ip();
    let connect_host = match cfg.transport_mode {
        TransportMode::WiFi => local_ip.clone(),
        TransportMode::Usb { .. } => "127.0.0.1".to_string(),
    };
    let connected_devices = metrics.active_sessions;
    let recent_clients = metrics.recent_clients.clone();

    let connection_status = if connected_devices > 0 {
        "connected"
    } else {
        "idle"
    }
    .to_string();

    let session_status = match status {
        ServiceStatus::Running if connected_devices > 0 => "streaming",
        ServiceStatus::Running => "waiting_for_device",
        ServiceStatus::Starting => "starting",
        ServiceStatus::Stopping => "stopping",
        ServiceStatus::Error => "error",
        ServiceStatus::NotStarted => "stopped",
    }
    .to_string();

    let logs = state
        .logs
        .lock()
        .expect("logs lock")
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    let update_banner = state
        .update_state
        .lock()
        .expect("update lock")
        .available
        .as_ref()
        .map(|info| UpdateBanner {
            latest_version: info.latest_version.clone(),
            release_url: info.release_url.clone(),
        });

    DesktopSnapshot {
        service_status: status,
        error_message: last_error,
        service_snapshot,
        audio_source: cfg.audio_source.as_str().to_string(),
        data_plane_format: cfg.data_plane_format.as_str().to_string(),
        protocol_path,
        gray_mode,
        codec_selection: cfg.codec_selection.as_str().to_string(),
        effective_codec,
        recommended_connection: recommended_connection(&cfg).to_string(),
        loopback_v2_header_gray_enabled: cfg.allow_loopback_v2_header_gray,
        fallback_to_synthetic: cfg.fallback_to_synthetic,
        capture_dump_wav: cfg.capture_dump_wav,
        local_ip: local_ip.clone(),
        ws_port: cfg.ws_port,
        udp_port: cfg.udp_port,
        connect_address: format!("ws://{}:{}", connect_host, cfg.ws_port),
        connected_devices,
        recent_clients,
        connection_status,
        session_status,
        current_audio_mode: format!("{:?}", current_audio_mode).to_lowercase(),
        mode_profile,
        protocol_version: PROTOCOL_VERSION_V2,
        capabilities: default_protocol_capabilities(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        transport_mode: cfg.transport_mode.as_str().to_string(),
        usb_serial: cfg.transport_mode.adb_serial().map(ToOwned::to_owned),
        metrics,
        mic_active: reverse_channel_state.mic_active,
        mic_peak_db: reverse_channel_state.mic_peak_db,
        mic_rms_db: reverse_channel_state.mic_rms_db,
        mic_device_name: reverse_channel_state.mic_device_name,
        android_volume_pct: reverse_channel_state.android_volume_pct,
        reverse_channel_enabled: cfg.reverse_channel_enabled,
        logs,
        update_banner,
    }
}

#[derive(Debug, Clone, Serialize)]
struct AdbDeviceInfo {
    serial: String,
    model: String,
    transport_id: String,
}

#[tauri::command]
fn list_adb_devices() -> Result<Vec<AdbDeviceInfo>, String> {
    adb_devices()
        .map(|devices| {
            devices
                .into_iter()
                .map(|device| AdbDeviceInfo {
                    serial: device.serial,
                    model: device.model,
                    transport_id: device.transport_id,
                })
                .collect()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn enable_usb_mode(state: State<'_, AppState>, serial: String) -> Result<DesktopSnapshot, String> {
    let restart_needed = {
        let mut guard = state.inner.lock().expect("state lock");
        guard.config.transport_mode = TransportMode::Usb {
            serial: serial.clone(),
        };
        matches!(
            guard.status,
            ServiceStatus::Running | ServiceStatus::Starting
        )
    };
    push_log(&state.logs, format!("usb mode enabled serial={serial}"));
    if restart_needed {
        stop_service_impl(&state, true)?;
        start_service_impl(&state)?;
    }
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
fn disable_usb_mode(state: State<'_, AppState>) -> Result<DesktopSnapshot, String> {
    let restart_needed = {
        let mut guard = state.inner.lock().expect("state lock");
        guard.config.transport_mode = TransportMode::WiFi;
        matches!(
            guard.status,
            ServiceStatus::Running | ServiceStatus::Starting
        )
    };
    push_log(&state.logs, "usb mode disabled");
    if restart_needed {
        stop_service_impl(&state, true)?;
        start_service_impl(&state)?;
    }
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
fn export_diagnostics_report(state: State<'_, AppState>) -> Result<String, String> {
    let exported_at_unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let snapshot = get_desktop_snapshot(state);
    let report = DiagnosticsReport {
        exported_at_unix_seconds,
        snapshot,
    };
    let content = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;

    let mut output_dir = PathBuf::from("dist");
    output_dir.push("diagnostics");
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let file_name = format!("desktop-diagnostics-{exported_at_unix_seconds}.json");
    output_dir.push(file_name);
    fs::write(&output_dir, content).map_err(|e| e.to_string())?;
    Ok(output_dir.display().to_string())
}

#[tauri::command]
fn check_for_updates(state: State<'_, AppState>) {
    spawn_update_check(
        Arc::clone(&state.runtime),
        Arc::clone(&state.update_state),
        None,
    );
}

#[tauri::command]
fn open_release_page(state: State<'_, AppState>) -> Result<(), String> {
    let url = state
        .update_state
        .lock()
        .expect("update lock")
        .available
        .as_ref()
        .map(|it| it.release_url.clone())
        .ok_or_else(|| "no update release url".to_string())?;
    open::that_detached(url).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_android_volume(
    state: State<'_, AppState>,
    volume_pct: u8,
) -> Result<DesktopSnapshot, String> {
    let service = {
        let guard = state.inner.lock().expect("state lock");
        guard.running.as_ref().map(|s| Arc::clone(&s.service))
    };
    if let Some(svc) = service {
        svc.send_android_volume(volume_pct)
            .map_err(|e| e.to_string())?;
        push_log(
            &state.logs,
            format!("android volume set to {}%", volume_pct),
        );
    }
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
fn start_service(state: State<'_, AppState>) -> Result<DesktopSnapshot, String> {
    start_service_impl(&state)?;
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
fn stop_service(state: State<'_, AppState>) -> Result<DesktopSnapshot, String> {
    stop_service_impl(&state, false)?;
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
fn restart_service(state: State<'_, AppState>) -> Result<DesktopSnapshot, String> {
    stop_service_impl(&state, true)?;
    start_service_impl(&state)?;
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
fn update_service_settings(
    state: State<'_, AppState>,
    settings: ServiceSettingsInput,
) -> Result<DesktopSnapshot, String> {
    let source = AudioSourceKind::parse(&settings.audio_source).map_err(|e| e.to_string())?;
    let data_plane =
        DataPlaneFormat::parse(&settings.data_plane_format).map_err(|e| e.to_string())?;
    let codec_selection =
        CodecSelection::parse(&settings.codec_selection).map_err(|e| e.to_string())?;
    let restart_needed = {
        let mut guard = state.inner.lock().expect("state lock");
        guard.config.audio_source = source;
        guard.config.data_plane_format = data_plane;
        guard.config.codec_selection = codec_selection;
        guard.config.allow_loopback_v2_header_gray = settings.allow_loopback_v2_header_gray;
        guard.config.fallback_to_synthetic = settings.fallback_to_synthetic;
        guard.config.capture_dump_wav = settings.capture_dump_wav;
        matches!(
            guard.status,
            ServiceStatus::Running | ServiceStatus::Starting
        )
    };

    push_log(
        &state.logs,
        format!(
            "settings updated: source={}, data_plane={}, codec={}, loopback_v2_gray={}, fallback={}, dump_wav={}",
            source.as_str(),
            data_plane.as_str(),
            codec_selection.as_str(),
            settings.allow_loopback_v2_header_gray,
            settings.fallback_to_synthetic,
            settings.capture_dump_wav
        ),
    );

    if restart_needed {
        stop_service_impl(&state, true)?;
        start_service_impl(&state)?;
    }

    Ok(get_desktop_snapshot(state))
}

fn start_service_impl(state: &State<'_, AppState>) -> Result<(), String> {
    let (cfg, run_id) = {
        let mut guard = state.inner.lock().expect("state lock");
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

    let server_cfg = build_server_config(&cfg)?;
    let service = match state.runtime.block_on(LanAudioService::new(server_cfg)) {
        Ok(s) => Arc::new(s),
        Err(err) => {
            let message = err.to_string();
            let mut guard = state.inner.lock().expect("state lock");
            guard.status = ServiceStatus::Error;
            guard.last_error = Some(message.clone());
            push_log(&state.logs, format!("start failed: {message}"));
            return Err(message);
        }
    };

    let service_for_task = Arc::clone(&service);
    let state_for_task = Arc::clone(&state.inner);
    let logs_for_task = Arc::clone(&state.logs);
    let task = state.runtime.spawn(async move {
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

    {
        let mut guard = state.inner.lock().expect("state lock");
        guard.status = ServiceStatus::Running;
        guard.last_error = None;
        guard.running = Some(RunningService {
            run_id,
            service,
            task: Some(task),
        });
    }

    push_log(
        &state.logs,
        format!(
            "service started (audio_source={}, data_plane={}, loopback_v2_gray={})",
            cfg.audio_source.as_str(),
            cfg.data_plane_format.as_str(),
            cfg.allow_loopback_v2_header_gray
        ),
    );
    Ok(())
}

fn stop_service_impl(state: &State<'_, AppState>, wait: bool) -> Result<(), String> {
    let (service, task, run_id) = {
        let mut guard = state.inner.lock().expect("state lock");
        if guard.running.is_none() {
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

fn build_server_config(cfg: &DesktopServiceConfig) -> Result<ServerConfig, String> {
    let mut server = ServerConfig::default();
    server.audio_source = cfg.audio_source;
    server.data_plane_format = cfg.data_plane_format;
    server.codec_selection = cfg.codec_selection;
    server.allow_loopback_v2_header_gray = cfg.allow_loopback_v2_header_gray;
    server.audio_source_fallback_to_synthetic = cfg.fallback_to_synthetic;
    server.capture_debug_dump_wav = cfg.capture_dump_wav;
    server.reverse_channel_enabled = cfg.reverse_channel_enabled;
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

fn recommended_connection(cfg: &DesktopServiceConfig) -> &'static str {
    if matches!(cfg.transport_mode, TransportMode::Usb { .. }) {
        "USB localhost tunnel"
    } else if selected_data_plane_for_desktop_config(cfg) == DataPlaneFormat::V2Header {
        "USB tethering or 5GHz Wi-Fi"
    } else {
        "Same Wi-Fi network"
    }
}

fn selected_data_plane_for_desktop_config(cfg: &DesktopServiceConfig) -> DataPlaneFormat {
    select_data_plane_format(
        cfg.data_plane_format,
        cfg.audio_source,
        cfg.allow_loopback_v2_header_gray,
    )
}

fn effective_codec_for_desktop_config(cfg: &DesktopServiceConfig) -> CodecSelection {
    match (
        cfg.codec_selection,
        selected_data_plane_for_desktop_config(cfg),
    ) {
        (CodecSelection::Opus, DataPlaneFormat::V2Header) => CodecSelection::Opus,
        _ => CodecSelection::Pcm16,
    }
}

fn build_service_snapshot(
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
    };
    let effective_codec = match effective_codec {
        "opus" => AudioCodecPreference::Opus,
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

fn detect_local_ip() -> String {
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

fn empty_metrics(audio_source: &str) -> DesktopMetrics {
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

fn default_protocol_capabilities() -> ProtocolCapabilities {
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
        supports_reverse_channel: true,
    }
}

fn push_log(logs: &Arc<Mutex<VecDeque<String>>>, message: impl Into<String>) {
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

fn run_update_check(update_state: Arc<Mutex<UpdateState>>) {
    if let Some(info) = update_checker::check_update(env!("CARGO_PKG_VERSION")) {
        let mut guard = update_state.lock().expect("update lock");
        guard.available = Some(info);
    }
}

fn spawn_update_check(
    runtime: Arc<Runtime>,
    update_state: Arc<Mutex<UpdateState>>,
    notify: Option<AppHandle>,
) {
    runtime.spawn_blocking(move || {
        run_update_check(update_state);
        if let Some(app) = notify {
            let _ = app.emit("update-check-finished", ());
        }
    });
}

fn spawn_silent_startup_update_check(app: &AppHandle, update_state: Arc<Mutex<UpdateState>>) {
    let handle = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(5));
        run_update_check(Arc::clone(&update_state));
        let _ = handle.emit("update-check-finished", ());
    });
}

fn setup_tray_menu(app: &tauri::App) -> tauri::Result<()> {
    let volume_menu = SubmenuBuilder::new(app, "Android 音量")
        .text("vol_25", "25%")
        .text("vol_50", "50%")
        .text("vol_75", "75%")
        .text("vol_100", "100%")
        .build()?;

    let menu = MenuBuilder::new(app)
        .text("check_updates", "检查更新")
        .separator()
        .item(&volume_menu)
        .build()?;

    TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "check_updates" => {
                if let Some(state) = app.try_state::<AppState>() {
                    spawn_update_check(
                        Arc::clone(&state.runtime),
                        Arc::clone(&state.update_state),
                        Some(app.clone()),
                    );
                }
            }
            "vol_25" | "vol_50" | "vol_75" | "vol_100" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let pct: u8 = match event.id().as_ref() {
                        "vol_25" => 25,
                        "vol_50" => 50,
                        "vol_75" => 75,
                        "vol_100" => 100,
                        _ => 50,
                    };
                    let service = {
                        let guard = state.inner.lock().expect("state lock");
                        guard.running.as_ref().map(|s| Arc::clone(&s.service))
                    };
                    if let Some(svc) = service {
                        let _ = svc.send_android_volume(pct);
                    }
                }
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    let runtime = Arc::new(Runtime::new().expect("tokio runtime"));
    let logs = Arc::new(Mutex::new(VecDeque::new()));
    let update_state = Arc::new(Mutex::new(UpdateState::default()));
    let inner = Arc::new(Mutex::new(AppStateInner {
        status: ServiceStatus::NotStarted,
        last_error: None,
        run_id: 0,
        config: DesktopServiceConfig::default(),
        running: None,
    }));

    push_log(&logs, "desktop app ready");

    tauri::Builder::default()
        .manage(AppState {
            runtime,
            inner,
            logs,
            update_state,
        })
        .setup(|app| {
            let state = app.state::<AppState>();
            spawn_silent_startup_update_check(app.handle(), Arc::clone(&state.update_state));
            setup_tray_menu(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_desktop_snapshot,
            set_android_volume,
            start_service,
            stop_service,
            restart_service,
            update_service_settings,
            list_adb_devices,
            enable_usb_mode,
            disable_usb_mode,
            export_diagnostics_report,
            check_for_updates,
            open_release_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
