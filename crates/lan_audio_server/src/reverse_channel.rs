use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

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

/// Check if a virtual audio device is available on the system.
/// On Windows, warn by default and clear when a named pipe consumer connects.
/// On non-Windows, skip the check (no virtual device needed).
fn check_virtual_audio_device(state: &Arc<Mutex<ReverseChannelState>>) {
    #[cfg(windows)]
    {
        if let Ok(mut s) = state.lock() {
            s.virtual_device_detected = false;
            s.virtual_device_warning = Some(
                "Mic input requires a virtual audio device (e.g. VB-Cable). Named pipe fallback is active — check your audio settings.".into(),
            );
        }
        info!(
            "Virtual audio device check: warning set (cleared when named pipe consumer connects)"
        );
    }
    #[cfg(not(windows))]
    {
        if let Ok(mut s) = state.lock() {
            s.virtual_device_detected = true;
            s.virtual_device_warning = None;
        }
    }
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
        check_virtual_audio_device(&self.state);

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

// ---- audio receiver implementation ----

async fn run_reverse_audio(
    listener: TcpListener,
    state: Arc<Mutex<ReverseChannelState>>,
    mut shutdown: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                info!("Reverse audio receiver shutting down");
                break;
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, peer)) => {
                        info!("Reverse audio client connected from {}", peer);
                        if let Ok(mut s) = state.lock() {
                            s.mic_active = true;
                        }
                        let state_for_handler = Arc::clone(&state);
                        let state_for_cleanup = Arc::clone(&state);
                        let mut shutdown_clone = shutdown.resubscribe();
                        tokio::spawn(async move {
                            if let Err(e) = handle_audio_client(stream, state_for_handler, &mut shutdown_clone)
                                .await
                            {
                                error!("Reverse audio client error ({}): {}", peer, e);
                            }
                            if let Ok(mut s) = state_for_cleanup.lock() {
                                s.mic_active = false;
                            }
                        });
                    }
                    Err(e) => {
                        error!("Reverse audio accept error: {}", e);
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_audio_client(
    mut stream: TcpStream,
    state: Arc<Mutex<ReverseChannelState>>,
    shutdown: &mut broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let mut opus_decoder = opus::Decoder::new(48000, opus::Channels::Mono)?;
    let mut len_buf = [0u8; 4];
    let mut frame_buf = vec![0u8; 4096];
    let mut pcm_buf = vec![0i16; 480 * 120]; // 120ms of mono 48kHz PCM

    // Open named pipe for virtual audio device
    let pipe_path = r"\\.\pipe\lan_audio_reverse_mic";

    #[cfg(windows)]
    let mut named_pipe: Option<std::fs::File> = {
        match std::fs::OpenOptions::new().write(true).open(pipe_path) {
            Ok(file) => {
                info!("Opened named pipe for reverse mic: {}", pipe_path);
                if let Ok(mut s) = state.lock() {
                    s.virtual_device_detected = true;
                }
                Some(file)
            }
            Err(e) => {
                warn!(
                    "Could not open named pipe {}: {} (virtual audio device may not be connected)",
                    pipe_path, e
                );
                if let Ok(mut s) = state.lock() {
                    s.virtual_device_detected = false;
                    s.virtual_device_warning = Some(
                        "Mic input requires a virtual audio device (e.g. VB-Cable). Named pipe fallback is active — check your audio settings.".into()
                    );
                }
                None
            }
        }
    };

    #[cfg(not(windows))]
    #[allow(unused)]
    let named_pipe: Option<std::fs::File> = None;

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                break;
            }
            result = stream.read_exact(&mut len_buf) => {
                if let Err(e) = result {
                    warn!("Reverse audio read error (header): {}", e);
                    break;
                }
                let payload_len = u32::from_le_bytes(len_buf) as usize;
                if payload_len == 0 || payload_len > frame_buf.len() {
                    warn!("Invalid reverse audio frame length: {}", payload_len);
                    continue;
                }
                if let Err(e) = stream.read_exact(&mut frame_buf[..payload_len]).await {
                    warn!("Reverse audio read error (payload): {}", e);
                    break;
                }

                match opus_decoder.decode(&frame_buf[..payload_len], &mut pcm_buf, false) {
                    Ok(samples) => {
                        // Compute peak and RMS
                        let mut peak = 0.0f32;
                        let mut sum_sq = 0.0f64;
                        for &s in &pcm_buf[..samples] {
                            let f = s as f32 / 32768.0;
                            let abs = f.abs();
                            if abs > peak { peak = abs; }
                            sum_sq += (f as f64) * (f as f64);
                        }
                        let rms = if samples > 0 {
                            (sum_sq / samples as f64).sqrt() as f32
                        } else {
                            0.0
                        };
                        let peak_db = if peak > 0.0 { 20.0 * peak.log10() } else { -96.0 };
                        let rms_db = if rms > 0.0 { 20.0 * rms.log10() } else { -96.0 };

                        if let Ok(mut s) = state.lock() {
                            s.mic_peak_db = peak_db;
                            s.mic_rms_db = rms_db;
                        }

                        // Write to named pipe using bytemuck for safe transmute to bytes
                        let pcm_bytes: &[u8] = bytemuck::cast_slice(&pcm_buf[..samples]);

                        #[cfg(windows)]
                        if let Some(ref mut pipe) = named_pipe {
                            use std::io::Write;
                            match pipe.write_all(pcm_bytes) {
                                Ok(()) => {
                                    // Clear virtual device warning on first successful write
                                    if let Ok(mut s) = state.lock() {
                                        if s.virtual_device_warning.is_some() {
                                            s.virtual_device_detected = true;
                                            s.virtual_device_warning = None;
                                            info!("Named pipe consumer detected — virtual audio device warning cleared");
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to write to named pipe: {}", e);
                                }
                            }
                            let _ = pipe.flush();
                        }
                    }
                    Err(ref e) if e.code() == opus::ErrorCode::BufferTooSmall => {
                        warn!("Opus decode buffer too small for reverse channel");
                    }
                    Err(e) => {
                        warn!("Opus decode error on reverse channel: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn run_control_listener(
    listener: TcpListener,
    state: Arc<Mutex<ReverseChannelState>>,
    mut shutdown: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                info!("Reverse control listener shutting down");
                break;
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, peer)) => {
                        info!("Reverse control client connected from {}", peer);
                        let state_clone = Arc::clone(&state);
                        let mut shutdown_clone = shutdown.resubscribe();
                        tokio::spawn(async move {
                            if let Err(e) = handle_control_stream(stream, state_clone, &mut shutdown_clone).await {
                                warn!("Reverse control stream ended ({}): {}", peer, e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Reverse control accept error: {}", e);
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_control_stream(
    stream: TcpStream,
    state: Arc<Mutex<ReverseChannelState>>,
    shutdown: &mut broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let buf_reader = BufReader::new(stream);
    let mut lines = buf_reader.lines();

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                break;
            }
            line_result = lines.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<serde_json::Value>(trimmed) {
                            Ok(msg) => {
                                let msg_type = msg["type"].as_str().unwrap_or("");
                                match msg_type {
                                    "mic_gain" => {
                                        if let Some(gain) = msg["value"].as_f64() {
                                            debug!("Received mic_gain: {}", gain);
                                        }
                                    }
                                    "volume" => {
                                        if let Some(vol) = msg["value"].as_f64() {
                                            let pct = (vol.clamp(0.0, 1.0) * 100.0).round() as u8;
                                            debug!("Remote Android volume changed: {}%", pct);
                                            if let Ok(mut s) = state.lock() {
                                                s.android_volume_pct = pct;
                                            }
                                        }
                                    }
                                    "mic_device" => {
                                        if let Some(name) = msg["name"].as_str() {
                                            debug!("Android mic device: {}", name);
                                            if let Ok(mut s) = state.lock() {
                                                s.mic_device_name = name.to_string();
                                            }
                                        }
                                    }
                                    "mic_level" => {
                                        let peak = msg["peak_db"].as_f64().map(|v| v as f32).unwrap_or(-96.0);
                                        let rms = msg["rms_db"].as_f64().map(|v| v as f32).unwrap_or(-96.0);
                                        if let Ok(mut s) = state.lock() {
                                            s.mic_peak_db = peak;
                                            s.mic_rms_db = rms;
                                        }
                                    }
                                    "mic_active" => {
                                        let active = msg["active"].as_bool().unwrap_or(false);
                                        if let Ok(mut s) = state.lock() {
                                            s.mic_active = active;
                                        }
                                    }
                                    "volume_changed" => {
                                        if let Some(pct) = msg["volume_pct"].as_u64() {
                                            debug!("Android reported volume change: {}%", pct);
                                            if let Ok(mut s) = state.lock() {
                                                s.android_volume_pct = pct as u8;
                                            }
                                        }
                                    }
                                    "ping" => {
                                        // Keep-alive, no response needed
                                    }
                                    other => {
                                        debug!("Unknown control message type: {}", other);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Invalid control JSON: {} — raw: {}", trimmed, e);
                            }
                        }
                    }
                    Ok(None) => {
                        debug!("Control stream EOF");
                        break;
                    }
                    Err(e) => {
                        warn!("Control read error: {}", e);
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
