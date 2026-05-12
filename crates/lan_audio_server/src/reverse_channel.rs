use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// Public state snapshot for UI readout
#[derive(Debug, Clone, Default)]
pub struct ReverseChannelState {
    pub mic_active: bool,
    pub mic_peak_db: f32,
    pub mic_rms_db: f32,
    pub mic_device_name: String,
    pub android_volume_pct: u8,
    pub virtual_device_detected: bool,
    pub virtual_device_warning: Option<String>,
}

/// Server that accepts reverse-channel audio (port 7878) and control (port 7879) connections
pub struct ReverseChannelServer {
    reverse_port: u16,
    control_port: u16,
    audio_shutdown_tx: Option<broadcast::Sender<()>>,
    control_shutdown_tx: Option<broadcast::Sender<()>>,
    audio_task: Option<JoinHandle<()>>,
    control_task: Option<JoinHandle<()>>,
    pub state: Arc<Mutex<ReverseChannelState>>,
}

impl Default for ReverseChannelServer {
    fn default() -> Self {
        Self::new()
    }
}

impl ReverseChannelServer {
    pub fn new() -> Self {
        Self {
            reverse_port: 7878,
            control_port: 7879,
            audio_shutdown_tx: None,
            control_shutdown_tx: None,
            audio_task: None,
            control_task: None,
            state: Arc::new(Mutex::new(ReverseChannelState::default())),
        }
    }

    /// Start both listeners. Returns Ok even if a listener fails to bind (best-effort).
    pub async fn start(&mut self) -> anyhow::Result<()> {
        let (audio_tx, audio_rx) = broadcast::channel::<()>(1);
        let (control_tx, control_rx) = broadcast::channel::<()>(1);
        self.audio_shutdown_tx = Some(audio_tx);
        self.control_shutdown_tx = Some(control_tx);

        let state = Arc::clone(&self.state);

        // Audio listener (port 7878)
        let reverse_bind: SocketAddr = format!("0.0.0.0:{}", self.reverse_port).parse()?;
        match TcpListener::bind(reverse_bind).await {
            Ok(listener) => {
                info!("Reverse audio listener bound on port {}", self.reverse_port);
                let state_clone = Arc::clone(&state);
                self.audio_task = Some(tokio::spawn(async move {
                    if let Err(e) = run_reverse_audio(listener, state_clone, audio_rx).await {
                        error!("Reverse audio task exited: {}", e);
                    }
                }));
            }
            Err(e) => {
                warn!(
                    "Could not bind reverse audio port {}: {}",
                    self.reverse_port, e
                );
            }
        }

        // Control listener (port 7879)
        let control_bind: SocketAddr = format!("0.0.0.0:{}", self.control_port).parse()?;
        match TcpListener::bind(control_bind).await {
            Ok(listener) => {
                info!(
                    "Reverse control listener bound on port {}",
                    self.control_port
                );
                let state_clone = Arc::clone(&state);
                self.control_task = Some(tokio::spawn(async move {
                    if let Err(e) = run_control_listener(listener, state_clone, control_rx).await {
                        error!("Reverse control task exited: {}", e);
                    }
                }));
            }
            Err(e) => {
                warn!(
                    "Could not bind reverse control port {}: {}",
                    self.control_port, e
                );
            }
        }

        Ok(())
    }

    /// Signal all tasks to stop and abort them
    pub fn stop(&mut self) {
        if let Some(tx) = self.audio_shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(tx) = self.control_shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.audio_task.take() {
            handle.abort();
        }
        if let Some(handle) = self.control_task.take() {
            handle.abort();
        }
        if let Ok(mut s) = self.state.lock() {
            s.mic_active = false;
        }
    }
}

// ---- stub implementations (filled in later tasks) ----

async fn run_reverse_audio(
    _listener: TcpListener,
    _state: Arc<Mutex<ReverseChannelState>>,
    mut _shutdown: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    // Will be implemented in Task 1.2
    info!("Reverse audio receiver started (stub — full implementation in next task)");
    // Just wait for shutdown without accepting connections for now
    let _ = _shutdown.recv().await;
    Ok(())
}

async fn run_control_listener(
    _listener: TcpListener,
    _state: Arc<Mutex<ReverseChannelState>>,
    mut _shutdown: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    // Will be implemented in Task 1.3
    info!("Reverse control listener started (stub — full implementation in next task)");
    let _ = _shutdown.recv().await;
    Ok(())
}
