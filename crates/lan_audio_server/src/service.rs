use std::sync::Arc;
use lan_audio_protocol::AudioMode;
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::ServerConfig;
use crate::discovery::{run_discovery_broadcast, DiscoveryConfig};
use crate::metrics::{Metrics, MetricsSnapshot};
use crate::session::{ClientRegistry, SessionServer};
use crate::transport::BroadcastTransport;
use crate::usb_transport::{setup_adb_forward, AdbForwardManager};

pub struct LanAudioService {
    cfg: Arc<ServerConfig>,
    metrics: Arc<Metrics>,
    shutdown_tx: broadcast::Sender<()>,
    adb_forward_manager: AdbForwardManager,
}

impl LanAudioService {
    pub async fn new(cfg: ServerConfig) -> anyhow::Result<Self> {
        let cfg = Arc::new(cfg);
        let metrics = Metrics::new_shared();
        metrics.set_current_audio_source(cfg.audio_source.as_str());
        metrics.set_capture_source_state("created");
        metrics.set_capture_device_name("n/a");
        metrics.set_capture_format(cfg.sample_rate, cfg.channels as u16);
        let (shutdown_tx, _) = broadcast::channel(16);
        let adb_forward_manager = AdbForwardManager::default();
        if let Some(serial) = cfg.transport_mode.adb_serial() {
            setup_adb_forward(serial, cfg.ws_bind.port(), cfg.ws_bind.port())?;
            adb_forward_manager.track_forward(serial.to_string(), cfg.ws_bind.port());
            setup_adb_forward(serial, cfg.udp_bind.port(), cfg.udp_bind.port())?;
            adb_forward_manager.track_forward(serial.to_string(), cfg.udp_bind.port());
        }
        Ok(Self {
            cfg,
            metrics,
            shutdown_tx,
            adb_forward_manager,
        })
    }

    pub fn metrics_snapshot(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }

    pub fn current_audio_mode(&self) -> AudioMode {
        self.cfg.current_audio_mode
    }

    pub fn set_current_audio_mode(&self, mode: AudioMode) {
        let _ = mode;
    }

    pub async fn run_until_shutdown(&self) -> anyhow::Result<()> {
        let registry = ClientRegistry::new(Arc::clone(&self.metrics));
        let transport =
            BroadcastTransport::new(Arc::clone(&self.cfg), Arc::clone(&self.metrics), registry.clone())
                .await?;
        let session_server =
            SessionServer::new(Arc::clone(&self.cfg), Arc::clone(&self.metrics), registry);

        let discovery_cfg = DiscoveryConfig {
            server_id: Uuid::new_v4(),
            server_name: self.cfg.server_name.clone(),
            bind_addr: self.cfg.discovery_bind,
            broadcast_addr: self.cfg.discovery_broadcast,
            ws_port: self.cfg.ws_bind.port(),
            udp_port: self.cfg.udp_bind.port(),
        };

        let mut handles = Vec::new();
        if self.cfg.transport_mode.as_str() == "wifi" {
            let rx = self.shutdown_tx.subscribe();
            handles.push(tokio::spawn(async move {
                run_discovery_broadcast(discovery_cfg, rx).await
            }));
        }
        {
            let transport = transport.clone();
            let rx = self.shutdown_tx.subscribe();
            handles.push(tokio::spawn(async move { transport.run(rx).await }));
        }
        {
            let rx = self.shutdown_tx.subscribe();
            handles.push(tokio::spawn(async move { session_server.run(rx).await }));
        }
        {
            let metrics = Arc::clone(&self.metrics);
            let mut rx = self.shutdown_tx.subscribe();
            handles.push(tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = rx.recv() => break,
                        _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                            let s = metrics.snapshot();
                            info!(
                                tx_packets = s.tx_packets,
                                tx_bytes = s.tx_bytes,
                                active_sessions = s.active_sessions,
                                capture_frames_produced = s.capture_frames_produced,
                                capture_read_errors = s.capture_read_errors,
                                capture_underruns = s.capture_underruns,
                                capture_start_attempts = s.capture_start_attempts,
                                capture_start_failures = s.capture_start_failures,
                                capture_silent_frames = s.capture_silent_frames,
                                capture_non_silent_frames = s.capture_non_silent_frames,
                                capture_no_packet_count = s.capture_no_packet_count,
                                current_audio_source = %s.current_audio_source,
                                capture_source_state = %s.capture_source_state,
                                capture_device_name = %s.capture_device_name,
                                capture_sample_rate = s.capture_sample_rate,
                                capture_channels = s.capture_channels,
                                capture_buffer_frames = s.capture_buffer_frames,
                                capture_last_peak = s.capture_last_peak,
                                capture_last_rms = s.capture_last_rms,
                                last_capture_pts_ms = s.last_capture_pts_ms,
                                "metrics snapshot"
                            );
                        }
                    }
                }
                Ok(())
            }));
        }

        info!("lan audio service started");

        let mut external_stop_rx = self.shutdown_tx.subscribe();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("ctrl_c received, shutting down");
            }
            _ = external_stop_rx.recv() => {
                info!("received stop signal, shutting down");
            }
        }
        let _ = self.shutdown_tx.send(());

        for h in handles {
            match h.await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => warn!(error = %err, "background task failed"),
                Err(err) => warn!(error = %err, "join error"),
            }
        }
        Ok(())
    }

    pub fn stop(&self) {
        let _ = &self.adb_forward_manager;
        let _ = self.shutdown_tx.send(());
    }
}
