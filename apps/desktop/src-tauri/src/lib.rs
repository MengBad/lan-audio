use std::sync::Arc;

use lan_audio_server::config::ServerConfig;
use lan_audio_server::service::LanAudioService;
use serde::Serialize;
use tauri::State;
use tokio::runtime::Runtime;

struct AppState {
    runtime: Arc<Runtime>,
    service: Arc<LanAudioService>,
}

#[derive(Debug, Serialize)]
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
}

#[tauri::command]
fn get_metrics(state: State<'_, AppState>) -> DesktopMetrics {
    let _runtime = &state.runtime;
    let m = state.service.metrics_snapshot();
    DesktopMetrics {
        tx_packets: m.tx_packets,
        tx_bytes: m.tx_bytes,
        active_sessions: m.active_sessions,
        capture_frames_produced: m.capture_frames_produced,
        capture_read_errors: m.capture_read_errors,
        capture_underruns: m.capture_underruns,
        capture_start_attempts: m.capture_start_attempts,
        capture_start_failures: m.capture_start_failures,
        capture_silent_frames: m.capture_silent_frames,
        capture_non_silent_frames: m.capture_non_silent_frames,
        capture_no_packet_count: m.capture_no_packet_count,
        current_audio_source: m.current_audio_source,
        capture_source_state: m.capture_source_state,
        capture_device_name: m.capture_device_name,
        capture_sample_rate: m.capture_sample_rate,
        capture_channels: m.capture_channels,
        capture_buffer_frames: m.capture_buffer_frames,
        capture_last_peak: m.capture_last_peak,
        capture_last_rms: m.capture_last_rms,
        last_capture_pts_ms: m.last_capture_pts_ms,
    }
}

#[tauri::command]
fn stop_service(state: State<'_, AppState>) {
    state.service.stop();
}

pub fn run() {
    let runtime = Arc::new(Runtime::new().expect("tokio runtime"));
    let service = runtime
        .block_on(LanAudioService::new(ServerConfig::default()))
        .expect("create service");
    let service = Arc::new(service);

    let rt_for_bg = Arc::clone(&runtime);
    let service_for_bg = Arc::clone(&service);
    rt_for_bg.spawn(async move {
        // TODO(tauri-ui): expose service lifecycle and startup errors in the frontend.
        if let Err(err) = service_for_bg.run_until_shutdown().await {
            eprintln!("lan service error: {err}");
        }
    });

    tauri::Builder::default()
        .manage(AppState { runtime, service })
        .invoke_handler(tauri::generate_handler![get_metrics, stop_service])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
