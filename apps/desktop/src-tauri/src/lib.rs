use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use lan_audio_server::config::{AudioSourceKind, ServerConfig};
use lan_audio_server::metrics::MetricsSnapshot;
use lan_audio_server::service::LanAudioService;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

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
    audio_source: String,
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
    version: String,
    metrics: DesktopMetrics,
    logs: Vec<String>,
}

#[derive(Debug, Clone)]
struct DesktopServiceConfig {
    audio_source: AudioSourceKind,
    fallback_to_synthetic: bool,
    capture_dump_wav: bool,
    ws_port: u16,
    udp_port: u16,
}

impl Default for DesktopServiceConfig {
    fn default() -> Self {
        let cfg = ServerConfig::default();
        Self {
            audio_source: cfg.audio_source,
            fallback_to_synthetic: cfg.audio_source_fallback_to_synthetic,
            capture_dump_wav: cfg.capture_debug_dump_wav,
            ws_port: cfg.ws_bind.port(),
            udp_port: cfg.udp_bind.port(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ServiceSettingsInput {
    audio_source: String,
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
        .map(|svc| svc.metrics_snapshot())
        .map(DesktopMetrics::from)
        .unwrap_or_else(|| empty_metrics(cfg.audio_source.as_str()));

    let local_ip = detect_local_ip();
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

    DesktopSnapshot {
        service_status: status,
        error_message: last_error,
        audio_source: cfg.audio_source.as_str().to_string(),
        fallback_to_synthetic: cfg.fallback_to_synthetic,
        capture_dump_wav: cfg.capture_dump_wav,
        local_ip: local_ip.clone(),
        ws_port: cfg.ws_port,
        udp_port: cfg.udp_port,
        connect_address: format!("ws://{}:{}", local_ip, cfg.ws_port),
        connected_devices,
        recent_clients,
        connection_status,
        session_status,
        version: env!("CARGO_PKG_VERSION").to_string(),
        metrics,
        logs,
    }
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
    let restart_needed = {
        let mut guard = state.inner.lock().expect("state lock");
        guard.config.audio_source = source;
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
            "settings updated: source={}, fallback={}, dump_wav={}",
            source.as_str(),
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
            "service started (audio_source={})",
            cfg.audio_source.as_str()
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
    server.audio_source_fallback_to_synthetic = cfg.fallback_to_synthetic;
    server.capture_debug_dump_wav = cfg.capture_dump_wav;
    server.ws_bind = parse_bind(cfg.ws_port)?;
    server.udp_bind = parse_bind(cfg.udp_port)?;
    Ok(server)
}

fn parse_bind(port: u16) -> Result<SocketAddr, String> {
    format!("0.0.0.0:{port}")
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
        capture_sample_rate: 0,
        capture_channels: 0,
        capture_buffer_frames: 0,
        capture_last_peak: 0.0,
        capture_last_rms: 0.0,
        last_capture_pts_ms: 0,
        recent_clients: Vec::new(),
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

pub fn run() {
    let runtime = Arc::new(Runtime::new().expect("tokio runtime"));
    let logs = Arc::new(Mutex::new(VecDeque::new()));
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
        })
        .invoke_handler(tauri::generate_handler![
            get_desktop_snapshot,
            start_service,
            stop_service,
            restart_service,
            update_service_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
