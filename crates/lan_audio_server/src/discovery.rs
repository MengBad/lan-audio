use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use if_addrs::{get_if_addrs, IfAddr};
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
    let targets = discovery_targets(cfg.broadcast_addr);
    let bind = socket.local_addr().unwrap_or(cfg.bind_addr);
    let targets_text = targets
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    info!(
        bind = %bind,
        broadcast = %cfg.broadcast_addr,
        targets = %targets_text,
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
                        for target in &targets {
                            if let Err(err) = socket.send_to(&payload, target).await {
                                warn!(error = %err, target = %target, "failed sending discovery beacon");
                            }
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

fn discovery_targets(default_target: SocketAddr) -> Vec<SocketAddr> {
    let mut targets = BTreeSet::new();
    targets.insert(default_target);

    if let Ok(interfaces) = get_if_addrs() {
        for iface in interfaces {
            let IfAddr::V4(v4) = iface.addr else {
                continue;
            };
            if v4.ip.is_loopback() {
                continue;
            }
            let broadcast = v4
                .broadcast
                .unwrap_or_else(|| directed_broadcast(v4.ip, v4.netmask));
            targets.insert(SocketAddr::new(
                IpAddr::V4(broadcast),
                default_target.port(),
            ));
        }
    }

    targets.into_iter().collect()
}

fn directed_broadcast(ip: Ipv4Addr, netmask: Ipv4Addr) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(ip) | !u32::from(netmask))
}

#[cfg(test)]
mod tests {
    use super::directed_broadcast;
    use std::net::Ipv4Addr;

    #[test]
    fn directed_broadcast_uses_ip_and_mask() {
        assert_eq!(
            directed_broadcast(
                Ipv4Addr::new(192, 168, 31, 44),
                Ipv4Addr::new(255, 255, 255, 0)
            ),
            Ipv4Addr::new(192, 168, 31, 255)
        );
        assert_eq!(
            directed_broadcast(
                Ipv4Addr::new(10, 0, 0, 18),
                Ipv4Addr::new(255, 255, 0, 0)
            ),
            Ipv4Addr::new(10, 0, 255, 255)
        );
    }
}
