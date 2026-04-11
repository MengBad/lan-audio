use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use anyhow::{anyhow, Context};
use futures_util::{SinkExt, StreamExt};
use lan_audio_protocol::{ClientControlMessage, ServerControlMessage};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::ServerConfig;
use crate::metrics::Metrics;
use crate::transport::UdpTransport;

#[derive(Clone)]
pub struct SessionServer {
    cfg: Arc<ServerConfig>,
    metrics: Arc<Metrics>,
    transport: UdpTransport,
}

impl SessionServer {
    pub fn new(cfg: Arc<ServerConfig>, metrics: Arc<Metrics>, transport: UdpTransport) -> Self {
        Self {
            cfg,
            metrics,
            transport,
        }
    }

    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) -> anyhow::Result<()> {
        let listener = TcpListener::bind(self.cfg.ws_bind)
            .await
            .with_context(|| format!("bind ws listener: {}", self.cfg.ws_bind))?;
        info!(bind = %self.cfg.ws_bind, "ws session server started");

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    info!("ws session server stopping");
                    break;
                }
                incoming = listener.accept() => {
                    let (stream, peer) = incoming?;
                    let cloned = self.clone();
                    let child_shutdown = shutdown.resubscribe();
                    tokio::spawn(async move {
                        if let Err(err) = cloned.handle_client(stream, peer, child_shutdown).await {
                            warn!(peer = %peer, error = %err, "session failed");
                        }
                    });
                }
            }
        }
        Ok(())
    }

    async fn handle_client(
        &self,
        stream: TcpStream,
        peer: SocketAddr,
        mut shutdown: broadcast::Receiver<()>,
    ) -> anyhow::Result<()> {
        let ws_stream = tokio_tungstenite::accept_async(stream).await?;
        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        let hello_msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws_rx.next())
            .await
            .context("hello timeout")?
            .ok_or_else(|| anyhow!("client disconnected before hello"))??;

        let client_hello = match hello_msg {
            Message::Text(text) => serde_json::from_str::<ClientControlMessage>(&text)
                .context("invalid hello json")?,
            _ => return Err(anyhow!("expected text hello message")),
        };

        let (client_name, udp_port, desired_sample_rate, channels) = match client_hello {
            ClientControlMessage::ClientHello {
                client_name,
                udp_port,
                desired_sample_rate,
                channels,
            } => (client_name, udp_port, desired_sample_rate, channels),
            _ => return Err(anyhow!("first message must be client_hello")),
        };

        let session_id = Uuid::new_v4();
        let target = SocketAddr::new(resolve_ip(peer.ip()), udp_port);
        self.metrics.inc_sessions();

        let welcome = ServerControlMessage::ServerWelcome {
            session_id,
            codec: "opus".to_string(),
            sample_rate: desired_sample_rate.max(8_000),
            channels,
            frames_per_packet: self.cfg.frames_per_packet,
        };
        ws_tx
            .send(Message::Text(serde_json::to_string(&welcome)?.into()))
            .await?;

        info!(
            session = %session_id,
            peer = %peer,
            client = %client_name,
            udp_target = %target,
            "session established"
        );

        let stream_task = match self
            .transport
            .spawn_stream(session_id, target, shutdown.resubscribe())
            .await
        {
            Ok(handle) => handle,
            Err(err) => {
                let err_msg = ServerControlMessage::ServerError {
                    code: "capture_init_failed".to_string(),
                    message: err.to_string(),
                };
                ws_tx
                    .send(Message::Text(serde_json::to_string(&err_msg)?.into()))
                    .await?;
                self.metrics.dec_sessions();
                return Err(err);
            }
        };

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    break;
                }
                msg = ws_rx.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(ClientControlMessage::ClientPing { seq, ts_unix_ms }) = serde_json::from_str::<ClientControlMessage>(&text) {
                                let pong = ServerControlMessage::ServerPong { seq, ts_unix_ms };
                                ws_tx.send(Message::Text(serde_json::to_string(&pong)?.into())).await?;
                                let snapshot = self.metrics.snapshot();
                                let metrics_msg = ServerControlMessage::ServerMetrics {
                                    tx_packets: snapshot.tx_packets,
                                    tx_bytes: snapshot.tx_bytes,
                                    sessions: snapshot.active_sessions,
                                };
                                ws_tx.send(Message::Text(serde_json::to_string(&metrics_msg)?.into())).await?;
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Ok(_)) => {}
                        Some(Err(err)) => return Err(anyhow!(err)),
                    }
                }
            }
        }

        stream_task.abort();
        self.metrics.dec_sessions();
        info!(session = %session_id, "session closed");
        Ok(())
    }
}

fn resolve_ip(ip: IpAddr) -> IpAddr {
    ip
}
