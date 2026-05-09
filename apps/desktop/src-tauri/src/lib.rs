use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tauri::Manager;
use tokio::runtime::Runtime;

mod commands;
mod orchestrator;
mod state;
mod update_checker;

use commands::{setup_phase_two_tray_menu, spawn_silent_startup_update_check};
use orchestrator::push_log;
use state::{AppState, AppStateInner, DesktopServiceConfig, ServiceStatus, UpdateState};

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
            setup_phase_two_tray_menu(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_desktop_snapshot,
            commands::start_service,
            commands::stop_service,
            commands::restart_service,
            commands::update_service_settings,
            commands::list_adb_devices,
            commands::enable_usb_mode,
            commands::disable_usb_mode,
            commands::switch_to_rollback_mode,
            commands::restore_recommended_mode,
            commands::export_diagnostics_report,
            commands::check_for_updates,
            commands::open_release_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
