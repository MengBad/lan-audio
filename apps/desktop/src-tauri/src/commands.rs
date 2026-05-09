use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use lan_audio_protocol::{audio_mode_profile, DataPlanePath, PROTOCOL_VERSION_V2};
use lan_audio_server::config::{AudioSourceKind, CodecSelection, DataPlaneFormat, TransportMode};
use lan_audio_server::usb_transport::adb_devices;
use serde::Serialize;
use tauri::menu::MenuBuilder;
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::orchestrator::{
    build_service_snapshot, configured_data_plane_router, default_protocol_capabilities,
    detect_local_ip, effective_codec_for_desktop_config, empty_metrics, push_log,
    recommended_connection, selected_data_plane_for_desktop_config, start_service_impl,
    stop_service_impl,
};
use crate::state::{
    AppState, DesktopMetrics, DesktopSnapshot, DiagnosticsReport, ServiceSettingsInput,
    ServiceStatus, UpdateBanner, UpdateState,
};
use crate::update_checker;

fn apply_runtime_path_mode_to_config(
    config: &mut crate::state::DesktopServiceConfig,
    rollback: bool,
) {
    config.audio_source = AudioSourceKind::WindowsLoopback;
    config.allow_loopback_v2_header_gray = false;
    if rollback {
        config.data_plane_format = DataPlaneFormat::LegacyLas1;
        config.codec_selection = CodecSelection::Pcm16;
    } else {
        config.data_plane_format = DataPlaneFormat::V2Header;
        config.codec_selection = CodecSelection::Opus;
    }
}

fn apply_runtime_path_mode(state: &State<'_, AppState>, rollback: bool) -> Result<(), String> {
    let restart_needed = {
        let mut guard = state.inner.lock().expect("state lock");
        apply_runtime_path_mode_to_config(&mut guard.config, rollback);
        matches!(
            guard.status,
            ServiceStatus::Running | ServiceStatus::Starting
        )
    };

    let (data_plane, codec) = if rollback {
        ("legacy_las1", "pcm16")
    } else {
        ("v2_header", "opus")
    };
    push_log(
        &state.logs,
        format!("runtime path switched: windows_loopback + {data_plane} + {codec}"),
    );

    if restart_needed {
        stop_service_impl(state, true)?;
        start_service_impl(state)?;
    }

    Ok(())
}

#[tauri::command]
pub(crate) fn get_desktop_snapshot(state: State<'_, AppState>) -> DesktopSnapshot {
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

    let current_audio_mode = service
        .as_ref()
        .map(|svc| svc.current_audio_mode())
        .unwrap_or(cfg.current_audio_mode);
    let mode_profile = audio_mode_profile(current_audio_mode);
    let selected_data_plane = selected_data_plane_for_desktop_config(&cfg);
    let configured_router = configured_data_plane_router(&cfg);
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
        logs,
        update_banner,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdbDeviceInfo {
    serial: String,
    model: String,
    transport_id: String,
}

#[derive(Debug, Serialize)]
struct SupportBundleSystemInfo {
    exported_at_unix_seconds: u64,
    os: &'static str,
    os_family: &'static str,
    arch: &'static str,
    app_version: String,
    local_ip: String,
    connect_address: String,
    ws_port: u16,
    udp_port: u16,
    transport_mode: String,
    usb_serial: Option<String>,
    audio_devices: Vec<String>,
    network_interfaces: Vec<String>,
}

fn unix_seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn write_json_file<T: Serialize>(path: PathBuf, value: &T) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn list_adb_devices() -> Result<Vec<AdbDeviceInfo>, String> {
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
pub(crate) fn enable_usb_mode(
    state: State<'_, AppState>,
    serial: String,
) -> Result<DesktopSnapshot, String> {
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
pub(crate) fn disable_usb_mode(state: State<'_, AppState>) -> Result<DesktopSnapshot, String> {
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
pub(crate) fn switch_to_rollback_mode(
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    apply_runtime_path_mode(&state, true)?;
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
pub(crate) fn restore_recommended_mode(
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    apply_runtime_path_mode(&state, false)?;
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
pub(crate) fn export_diagnostics_report(state: State<'_, AppState>) -> Result<String, String> {
    let exported_at_unix_seconds = unix_seconds_now();
    let snapshot = get_desktop_snapshot(state);
    let report = DiagnosticsReport {
        exported_at_unix_seconds,
        snapshot,
    };

    let mut output_dir = PathBuf::from("dist");
    output_dir.push("diagnostics");
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let file_name = format!("desktop-diagnostics-{exported_at_unix_seconds}.json");
    output_dir.push(file_name);
    write_json_file(output_dir.clone(), &report)?;
    Ok(output_dir.display().to_string())
}

#[tauri::command]
pub(crate) fn export_support_bundle(state: State<'_, AppState>) -> Result<String, String> {
    let exported_at_unix_seconds = unix_seconds_now();
    let snapshot = get_desktop_snapshot(state);

    let mut output_dir = PathBuf::from("dist");
    output_dir.push("diagnostics");
    output_dir.push(format!("support-bundle-{exported_at_unix_seconds}"));
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let mut snapshot_path = output_dir.clone();
    snapshot_path.push("snapshot.json");
    write_json_file(snapshot_path, &snapshot)?;

    let mut recent_events = snapshot
        .logs
        .iter()
        .rev()
        .take(50)
        .cloned()
        .collect::<Vec<_>>();
    recent_events.reverse();
    let mut events_path = output_dir.clone();
    events_path.push("recent_events.json");
    write_json_file(events_path, &recent_events)?;

    let mut audio_devices = Vec::new();
    if !snapshot.metrics.capture_device_name.is_empty() {
        audio_devices.push(snapshot.metrics.capture_device_name.clone());
    }
    if audio_devices.is_empty() {
        audio_devices.push("unknown capture device".to_string());
    }
    let system_info = SupportBundleSystemInfo {
        exported_at_unix_seconds,
        os: std::env::consts::OS,
        os_family: std::env::consts::FAMILY,
        arch: std::env::consts::ARCH,
        app_version: snapshot.version.clone(),
        local_ip: snapshot.local_ip.clone(),
        connect_address: snapshot.connect_address.clone(),
        ws_port: snapshot.ws_port,
        udp_port: snapshot.udp_port,
        transport_mode: snapshot.transport_mode.clone(),
        usb_serial: snapshot.usb_serial.clone(),
        audio_devices,
        network_interfaces: vec![format!("local_ip={}", snapshot.local_ip)],
    };
    let mut system_path = output_dir.clone();
    system_path.push("system_info.json");
    write_json_file(system_path, &system_info)?;

    let mut readme_path = output_dir.clone();
    readme_path.push("README.txt");
    fs::write(
        readme_path,
        "LAN Audio support bundle\n\nAttach this directory when filing an issue. It includes a desktop snapshot, basic system information, and the most recent runtime events.\n",
    )
    .map_err(|e| e.to_string())?;

    Ok(output_dir.display().to_string())
}

#[tauri::command]
pub(crate) fn check_for_updates(state: State<'_, AppState>) {
    run_update_check(Arc::clone(&state.update_state));
}

#[tauri::command]
pub(crate) fn open_release_page(state: State<'_, AppState>) -> Result<(), String> {
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
pub(crate) fn start_service(state: State<'_, AppState>) -> Result<DesktopSnapshot, String> {
    start_service_impl(&state)?;
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
pub(crate) fn stop_service(state: State<'_, AppState>) -> Result<DesktopSnapshot, String> {
    stop_service_impl(&state, false)?;
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
pub(crate) fn restart_service(state: State<'_, AppState>) -> Result<DesktopSnapshot, String> {
    stop_service_impl(&state, true)?;
    start_service_impl(&state)?;
    Ok(get_desktop_snapshot(state))
}

#[tauri::command]
pub(crate) fn update_service_settings(
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

pub(crate) fn run_update_check(update_state: Arc<Mutex<UpdateState>>) {
    if let Some(info) = update_checker::check_update(env!("CARGO_PKG_VERSION")) {
        let mut guard = update_state.lock().expect("update lock");
        guard.available = Some(info);
    }
}

pub(crate) fn spawn_silent_startup_update_check(
    app: &AppHandle,
    update_state: Arc<Mutex<UpdateState>>,
) {
    let handle = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(5));
        run_update_check(Arc::clone(&update_state));
        let _ = handle.emit("update-check-finished", ());
    });
}

#[allow(dead_code)]
pub(crate) fn setup_tray_menu(app: &tauri::App) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("check_updates", "检查更新")
        .build()?;

    TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "check_updates" {
                if let Some(state) = app.try_state::<AppState>() {
                    run_update_check(Arc::clone(&state.update_state));
                    let _ = app.emit("update-check-finished", ());
                }
            }
        })
        .build(app)?;
    Ok(())
}

pub(crate) fn setup_phase_two_tray_menu(app: &tauri::App) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("switch_rollback_mode", "切换到回滚模式（legacy PCM16）")
        .text("restore_recommended_mode", "恢复推荐模式（Opus v2）")
        .text("check_updates", "检查更新")
        .build()?;

    TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| {
            if let Some(state) = app.try_state::<AppState>() {
                match event.id().as_ref() {
                    "switch_rollback_mode" => {
                        let _ = apply_runtime_path_mode(&state, true);
                        let _ = app.emit("desktop-snapshot-changed", ());
                    }
                    "restore_recommended_mode" => {
                        let _ = apply_runtime_path_mode(&state, false);
                        let _ = app.emit("desktop-snapshot-changed", ());
                    }
                    "check_updates" => {
                        run_update_check(Arc::clone(&state.update_state));
                        let _ = app.emit("update-check-finished", ());
                    }
                    _ => {}
                }
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DesktopServiceConfig;

    #[test]
    fn runtime_path_mode_updates_data_plane_and_codec() {
        let mut cfg = DesktopServiceConfig::default();

        apply_runtime_path_mode_to_config(&mut cfg, true);
        assert_eq!(cfg.audio_source, AudioSourceKind::WindowsLoopback);
        assert_eq!(cfg.data_plane_format, DataPlaneFormat::LegacyLas1);
        assert_eq!(cfg.codec_selection, CodecSelection::Pcm16);

        apply_runtime_path_mode_to_config(&mut cfg, false);
        assert_eq!(cfg.audio_source, AudioSourceKind::WindowsLoopback);
        assert_eq!(cfg.data_plane_format, DataPlaneFormat::V2Header);
        assert_eq!(cfg.codec_selection, CodecSelection::Opus);
    }
}
