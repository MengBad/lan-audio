use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use lan_audio_protocol::DiscoveryBeacon;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub server_id: Uuid,
    pub server_name: String,
    pub bind_addr: SocketAddr,
    pub broadcast_addr: SocketAddr,
    pub ws_port: u16,
    pub udp_port: u16,
}

pub async fn run_discovery_broadcast(
    cfg: DiscoveryConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let socket = UdpSocket::bind(cfg.bind_addr)
        .await
        .with_context(|| format!("bind discovery socket: {}", cfg.bind_addr))?;
    socket.set_broadcast(true)?;

    info!(
        bind = %cfg.bind_addr,
        broadcast = %cfg.broadcast_addr,
        "discovery broadcaster started"
    );

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                info!("discovery broadcaster stopping");
                return Ok(());
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                let beacon = DiscoveryBeacon {
                    kind: "lan_audio_discovery_v1".to_string(),
                    server_id: cfg.server_id,
                    server_name: cfg.server_name.clone(),
                    ws_port: cfg.ws_port,
                    udp_port: cfg.udp_port,
                    ts_unix_ms: now_ms(),
                };
                match serde_json::to_vec(&beacon) {
                    Ok(payload) => {
                        if let Err(err) = socket.send_to(&payload, cfg.broadcast_addr).await {
                            warn!(error = %err, "failed sending discovery beacon");
                        }
                    }
                    Err(err) => warn!(error = %err, "failed serialize discovery beacon"),
                }
            }
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub type SharedDiscoveryConfig = Arc<DiscoveryConfig>;
