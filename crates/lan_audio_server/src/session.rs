use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::{anyhow, Context};
use futures_util::{SinkExt, StreamExt};
use lan_audio_protocol::{
    audio_mode_profile, AudioMode, AudioModeChanged, ClientControlMessage, ClientInfo,
    ControlMessageV2, ErrorMessage, Hello, HelloAck, ProtocolCapabilities, ServerControlMessage,
    ServerInfo, SetAudioMode, PROTOCOL_VERSION_V2,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::{CodecSelection, DataPlaneFormat, ServerConfig};
use crate::metrics::Metrics;
use crate::transport::UdpTransport;

#[derive(Clone)]
pub struct SessionServer {
    cfg: Arc<ServerConfig>,
    metrics: Arc<Metrics>,
    transport: UdpTransport,
    active_streams: Arc<Mutex<HashMap<IpAddr, ActiveStream>>>,
    current_audio_mode: Arc<StdMutex<AudioMode>>,
}

struct ActiveStream {
    session_id: Uuid,
    abort: tokio::task::AbortHandle,
}

impl SessionServer {
    const UDP_STREAM_GRACE_AFTER_WS_CLOSE_SECS: u64 = 30;
    pub fn new(
        cfg: Arc<ServerConfig>,
        metrics: Arc<Metrics>,
        transport: UdpTransport,
        current_audio_mode: Arc<StdMutex<AudioMode>>,
    ) -> Self {
        Self {
            cfg,
            metrics,
            transport,
            active_streams: Arc::new(Mutex::new(HashMap::new())),
            current_audio_mode,
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

        let hello_text = match hello_msg {
            Message::Text(text) => text.to_string(),
            _ => return Err(anyhow!("expected text hello message")),
        };

        let mut current_audio_mode = self.read_current_audio_mode();
        let session_id = Uuid::new_v4();
        let (
            client_name,
            udp_port,
            desired_sample_rate,
            channels,
            v2_session,
            client_id,
            client_capabilities,
        ) = match parse_session_hello(&hello_text)? {
            SessionHello::Legacy {
                client_name,
                udp_port,
                desired_sample_rate,
                channels,
            } => (
                client_name,
                udp_port,
                desired_sample_rate,
                channels,
                false,
                "legacy-client".to_string(),
                legacy_client_capabilities(),
            ),
            SessionHello::V2(hello) => {
                info!(
                    session = %session_id,
                    client_id = %hello.client_id,
                    device_name = %hello.device_name,
                    protocol_version = hello.protocol_version,
                    preferred_audio_mode = ?hello.preferred_audio_mode,
                    capabilities = ?hello.capabilities,
                    "protocol v2 hello received"
                );
                (
                    hello.device_name,
                    hello.udp_port,
                    hello.desired_sample_rate,
                    hello.channels,
                    true,
                    hello.client_id,
                    hello.capabilities,
                )
            }
        };
        let negotiated_path = negotiate_session_path(&self.cfg, v2_session, &client_capabilities);
        let selected_data_plane = negotiated_path.data_plane;
        let effective_codec = negotiated_path.codec;
        self.metrics
            .set_negotiated_session_path(selected_data_plane.as_str(), effective_codec.as_str());

        if v2_session {
            let hello_ack = ControlMessageV2::HelloAck(HelloAck {
                protocol_version: PROTOCOL_VERSION_V2,
                accepted: true,
                session_id,
                current_audio_mode,
                mode_profile: audio_mode_profile(current_audio_mode),
                capabilities: default_server_capabilities(),
                message: "hello_ack".to_string(),
            });
            ws_tx
                .send(Message::Text(serde_json::to_string(&hello_ack)?.into()))
                .await?;

            let server_info = ControlMessageV2::ServerInfo(ServerInfo {
                server_id: session_id,
                server_name: self.cfg.server_name.clone(),
                platform: "windows".to_string(),
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                ws_port: self.cfg.ws_bind.port(),
                udp_port: self.cfg.udp_bind.port(),
                protocol_version: PROTOCOL_VERSION_V2,
                current_audio_mode,
                mode_profile: audio_mode_profile(current_audio_mode),
                codec: effective_codec.as_protocol_preference(),
                data_plane: selected_data_plane.as_str().to_string(),
                gray_mode: selected_data_plane != crate::config::DataPlaneFormat::LegacyLas1,
                recommended_connection: "usb_tethering_or_5ghz_wifi".to_string(),
            });
            ws_tx
                .send(Message::Text(serde_json::to_string(&server_info)?.into()))
                .await?;
        };

        let target = SocketAddr::new(resolve_ip(peer.ip()), udp_port);
        self.metrics
            .note_client_connected(&client_name, &peer.ip().to_string());
        self.metrics.inc_sessions();
        if self.cfg.data_plane_format != selected_data_plane {
            warn!(
                session = %session_id,
                requested_data_plane = %self.cfg.data_plane_format.as_str(),
                negotiated_data_plane = %selected_data_plane.as_str(),
                v2_session,
                "requested data plane is not active for this session; using negotiated data plane"
            );
        }
        if self.cfg.codec_selection != effective_codec {
            warn!(
                session = %session_id,
                requested_codec = %self.cfg.codec_selection.as_str(),
                effective_codec = %effective_codec.as_str(),
                selected_data_plane = %selected_data_plane.as_str(),
                client_supports_opus_experimental = client_capabilities.supports_opus_experimental,
                "requested codec is not active for this session; using negotiated codec"
            );
        }

        let welcome = ServerControlMessage::ServerWelcome {
            session_id,
            codec: effective_codec.as_str().to_string(),
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
            client_id = %client_id,
            udp_target = %target,
            "session established"
        );

        let stream_task = match self
            .transport
            .spawn_stream(
                session_id,
                target,
                selected_data_plane,
                effective_codec,
                shutdown.resubscribe(),
            )
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
        if let Some(previous) = self
            .replace_active_stream(peer.ip(), session_id, stream_task.abort_handle())
            .await
        {
            warn!(
                peer_ip = %peer.ip(),
                old_session = %previous.session_id,
                new_session = %session_id,
                "replaced previous active stream for same client ip"
            );
            previous.abort.abort();
        }

        let mut ws_stream_closed = false;
        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    break;
                }
                msg = ws_rx.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(ClientControlMessage::ClientPing { seq, ts_unix_ms }) =
                                serde_json::from_str::<ClientControlMessage>(&text)
                            {
                                let pong = ServerControlMessage::ServerPong { seq, ts_unix_ms };
                                ws_tx
                                    .send(Message::Text(serde_json::to_string(&pong)?.into()))
                                    .await?;
                                let snapshot = self.metrics.snapshot();
                                let metrics_msg = ServerControlMessage::ServerMetrics {
                                    tx_packets: snapshot.tx_packets,
                                    tx_bytes: snapshot.tx_bytes,
                                    sessions: snapshot.active_sessions,
                                };
                                ws_tx
                                    .send(Message::Text(serde_json::to_string(&metrics_msg)?.into()))
                                    .await?;
                                continue;
                            }

                            if v2_session {
                                match serde_json::from_str::<ControlMessageV2>(&text) {
                                    Ok(v2_msg) => {
                                        match v2_msg {
                                            ControlMessageV2::SetAudioMode(SetAudioMode { mode, reason }) => {
                                                current_audio_mode = mode;
                                                self.write_current_audio_mode(mode);
                                                let changed = ControlMessageV2::AudioModeChanged(AudioModeChanged {
                                                    mode,
                                                    applied: true,
                                                    reason,
                                                    mode_profile: audio_mode_profile(mode),
                                                });
                                                ws_tx.send(Message::Text(serde_json::to_string(&changed)?.into())).await?;
                                                info!(session = %session_id, mode = ?current_audio_mode, "audio mode updated by client");
                                            }
                                            ControlMessageV2::ClientInfo(ClientInfo { client_name, platform, app_version, udp_port }) => {
                                                info!(
                                                    session = %session_id,
                                                    client = %client_name,
                                                    platform = %platform,
                                                    app_version = %app_version,
                                                    udp_port,
                                                    "protocol v2 client info received"
                                                );
                                            }
                                            _ => {}
                                        }
                                    }
                                    Err(err) => {
                                        warn!(
                                            session = %session_id,
                                            error = %err,
                                            payload = %text,
                                            "failed to parse protocol v2 control message"
                                        );
                                        let error = ControlMessageV2::Error(ErrorMessage {
                                            code: "bad_control_message".to_string(),
                                            message: format!("invalid v2 message: {err}"),
                                            recoverable: true,
                                        });
                                        ws_tx.send(Message::Text(serde_json::to_string(&error)?.into())).await?;
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            ws_stream_closed = true;
                            break;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(err)) => return Err(anyhow!(err)),
                    }
                }
            }
        }

        if ws_stream_closed {
            warn!(
                session = %session_id,
                grace_secs = Self::UDP_STREAM_GRACE_AFTER_WS_CLOSE_SECS,
                "ws control channel closed; keep udp stream alive for grace period"
            );
            tokio::select! {
                _ = shutdown.recv() => {}
                _ = tokio::time::sleep(Duration::from_secs(Self::UDP_STREAM_GRACE_AFTER_WS_CLOSE_SECS)) => {}
            }
        }

        stream_task.abort();
        self.remove_active_stream_if_owner(peer.ip(), session_id)
            .await;
        self.metrics.dec_sessions();
        info!(session = %session_id, "session closed");
        Ok(())
    }

    async fn replace_active_stream(
        &self,
        peer_ip: IpAddr,
        session_id: Uuid,
        new_abort: tokio::task::AbortHandle,
    ) -> Option<ActiveStream> {
        let mut guard = self.active_streams.lock().await;
        guard.insert(
            peer_ip,
            ActiveStream {
                session_id,
                abort: new_abort,
            },
        )
    }

    async fn remove_active_stream_if_owner(&self, peer_ip: IpAddr, session_id: Uuid) {
        let mut guard = self.active_streams.lock().await;
        let should_remove = guard
            .get(&peer_ip)
            .map(|active| active.session_id == session_id)
            .unwrap_or(false);
        if should_remove {
            guard.remove(&peer_ip);
        }
    }
}

impl SessionServer {
    fn read_current_audio_mode(&self) -> AudioMode {
        *self
            .current_audio_mode
            .lock()
            .expect("current_audio_mode lock")
    }

    fn write_current_audio_mode(&self, mode: AudioMode) {
        *self
            .current_audio_mode
            .lock()
            .expect("current_audio_mode lock") = mode;
    }
}

fn resolve_ip(ip: IpAddr) -> IpAddr {
    ip
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NegotiatedSessionPath {
    data_plane: DataPlaneFormat,
    codec: CodecSelection,
}

fn negotiate_session_path(
    cfg: &ServerConfig,
    v2_session: bool,
    client_capabilities: &ProtocolCapabilities,
) -> NegotiatedSessionPath {
    if !v2_session {
        return NegotiatedSessionPath {
            data_plane: DataPlaneFormat::LegacyLas1,
            codec: CodecSelection::Pcm16,
        };
    }

    let data_plane = cfg.selected_data_plane_format();
    let codec = match (cfg.codec_selection, data_plane) {
        (CodecSelection::OpusExperimental, DataPlaneFormat::V2Header)
            if client_capabilities.supports_opus_experimental =>
        {
            CodecSelection::OpusExperimental
        }
        _ => CodecSelection::Pcm16,
    };

    NegotiatedSessionPath { data_plane, codec }
}

fn legacy_client_capabilities() -> ProtocolCapabilities {
    ProtocolCapabilities {
        supports_pcm16: true,
        supports_f32: false,
        supports_modes: false,
        supports_metrics: false,
        supports_opus_future: false,
        supports_opus_experimental: false,
        supports_low_latency: false,
        supports_high_quality: false,
        supports_native_audio_track: false,
        supports_fast_path: false,
        supports_stable_audio_track: false,
        supports_usb_tethering: false,
        supports_usb_direct_future: false,
    }
}

#[derive(Debug)]
enum SessionHello {
    Legacy {
        client_name: String,
        udp_port: u16,
        desired_sample_rate: u32,
        channels: u8,
    },
    V2(Hello),
}

fn parse_session_hello(text: &str) -> anyhow::Result<SessionHello> {
    if let Ok(ControlMessageV2::Hello(hello)) = serde_json::from_str::<ControlMessageV2>(text) {
        if hello.protocol_version != PROTOCOL_VERSION_V2 {
            return Err(anyhow!(
                "unsupported protocol version: {}",
                hello.protocol_version
            ));
        }
        return Ok(SessionHello::V2(hello));
    }

    let legacy =
        serde_json::from_str::<ClientControlMessage>(text).context("invalid hello json")?;
    match legacy {
        ClientControlMessage::ClientHello {
            client_name,
            udp_port,
            desired_sample_rate,
            channels,
        } => Ok(SessionHello::Legacy {
            client_name,
            udp_port,
            desired_sample_rate,
            channels,
        }),
        _ => Err(anyhow!("first message must be client_hello or hello")),
    }
}

pub(crate) fn default_server_capabilities() -> ProtocolCapabilities {
    ProtocolCapabilities {
        supports_pcm16: true,
        supports_f32: false,
        supports_modes: true,
        supports_metrics: true,
        supports_opus_future: true,
        supports_opus_experimental: true,
        supports_low_latency: true,
        supports_high_quality: true,
        supports_native_audio_track: true,
        supports_fast_path: false,
        supports_stable_audio_track: true,
        supports_usb_tethering: true,
        supports_usb_direct_future: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AudioSourceKind;
    use lan_audio_protocol::{AudioMode, ControlMessageV2, Hello, PROTOCOL_VERSION_V2};

    #[test]
    fn parse_session_hello_accepts_legacy_hello() {
        let json = r#"{
            "type":"client_hello",
            "client_name":"pixel",
            "udp_port":54000,
            "desired_sample_rate":48000,
            "channels":2
        }"#;

        let parsed = parse_session_hello(json).expect("parse legacy");
        match parsed {
            SessionHello::Legacy {
                client_name,
                udp_port,
                desired_sample_rate,
                channels,
            } => {
                assert_eq!(client_name, "pixel");
                assert_eq!(udp_port, 54000);
                assert_eq!(desired_sample_rate, 48000);
                assert_eq!(channels, 2);
            }
            SessionHello::V2(_) => panic!("expected legacy hello"),
        }
    }

    #[test]
    fn parse_session_hello_accepts_v2_hello() {
        let msg = ControlMessageV2::Hello(Hello {
            protocol_version: PROTOCOL_VERSION_V2,
            device_name: "pixel-8".to_string(),
            client_id: "android-1".to_string(),
            udp_port: 55000,
            desired_sample_rate: 48_000,
            channels: 2,
            capabilities: default_server_capabilities(),
            preferred_audio_mode: AudioMode::Balanced,
        });
        let json = serde_json::to_string(&msg).expect("serialize");

        let parsed = parse_session_hello(&json).expect("parse v2");
        match parsed {
            SessionHello::V2(hello) => {
                assert_eq!(hello.udp_port, 55000);
                assert_eq!(hello.preferred_audio_mode, AudioMode::Balanced);
                assert!(hello.capabilities.supports_modes);
                assert!(hello.capabilities.supports_native_audio_track);
            }
            SessionHello::Legacy { .. } => panic!("expected v2 hello"),
        }
    }

    #[test]
    fn parse_session_hello_rejects_unsupported_v2_version() {
        let msg = ControlMessageV2::Hello(Hello {
            protocol_version: 99,
            device_name: "pixel-8".to_string(),
            client_id: "android-1".to_string(),
            udp_port: 55000,
            desired_sample_rate: 48_000,
            channels: 2,
            capabilities: default_server_capabilities(),
            preferred_audio_mode: AudioMode::Balanced,
        });
        let json = serde_json::to_string(&msg).expect("serialize");
        let err = parse_session_hello(&json).expect_err("must reject unsupported version");
        assert!(err.to_string().contains("unsupported protocol version"));
    }

    #[test]
    fn negotiate_session_path_forces_legacy_clients_to_safe_pcm_v1() {
        let cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            codec_selection: CodecSelection::OpusExperimental,
            audio_source: AudioSourceKind::Synthetic,
            ..ServerConfig::default()
        };

        let negotiated = negotiate_session_path(&cfg, false, &default_server_capabilities());

        assert_eq!(negotiated.data_plane, DataPlaneFormat::LegacyLas1);
        assert_eq!(negotiated.codec, CodecSelection::Pcm16);
    }

    #[test]
    fn negotiate_session_path_falls_back_without_client_opus_capability() {
        let cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            codec_selection: CodecSelection::OpusExperimental,
            audio_source: AudioSourceKind::Synthetic,
            ..ServerConfig::default()
        };
        let mut caps = default_server_capabilities();
        caps.supports_opus_experimental = false;

        let negotiated = negotiate_session_path(&cfg, true, &caps);

        assert_eq!(negotiated.data_plane, DataPlaneFormat::V2Header);
        assert_eq!(negotiated.codec, CodecSelection::Pcm16);
    }

    #[test]
    fn negotiate_session_path_allows_opus_only_when_v2_and_client_supports_it() {
        let cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            codec_selection: CodecSelection::OpusExperimental,
            audio_source: AudioSourceKind::Synthetic,
            ..ServerConfig::default()
        };

        let negotiated = negotiate_session_path(&cfg, true, &default_server_capabilities());

        assert_eq!(negotiated.data_plane, DataPlaneFormat::V2Header);
        assert_eq!(negotiated.codec, CodecSelection::OpusExperimental);
    }

    #[test]
    fn negotiate_session_path_keeps_loopback_v2_behind_explicit_gray_flag() {
        let cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            codec_selection: CodecSelection::OpusExperimental,
            audio_source: AudioSourceKind::WindowsLoopback,
            allow_loopback_v2_header_gray: false,
            ..ServerConfig::default()
        };

        let negotiated = negotiate_session_path(&cfg, true, &default_server_capabilities());

        assert_eq!(negotiated.data_plane, DataPlaneFormat::LegacyLas1);
        assert_eq!(negotiated.codec, CodecSelection::Pcm16);
    }
}
