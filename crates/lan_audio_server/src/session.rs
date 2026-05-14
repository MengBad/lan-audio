use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use futures_util::{SinkExt, StreamExt};
use lan_audio_protocol::{
    audio_mode_profile, AudioMode, AudioModeChanged, ClientControlMessage, ClientInfo,
    ClientJoined, ClientLeft, ClientList, ClientListEntry, ConnectionState, ConnectionStateMachine,
    ControlMessageV2, ErrorMessage, FailureCode, Hello, HelloAck, ProtocolCapabilities,
    ServerControlMessage, ServerInfo, SetAudioMode, TransportType, PROTOCOL_VERSION_V2,
};
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::{timeout, Duration, Instant};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::{CodecSelection, DataPlaneFormat, ServerConfig, TransportMode};
use crate::data_plane::DataPlaneRouter;
use crate::metrics::Metrics;

pub const MAX_CLIENTS: usize = 4;
const USB_PENDING_TIMEOUT_SECS: u64 = 60;

#[derive(Clone)]
pub struct SessionServer {
    cfg: Arc<ServerConfig>,
    metrics: Arc<Metrics>,
    registry: ClientRegistry,
    data_plane_router: Arc<std::sync::Mutex<DataPlaneRouter>>,
    current_audio_mode: Arc<std::sync::Mutex<AudioMode>>,
}

/// Phase 3 watermark slot. Cloneable across the WS handler (writer), the
/// watchdog task (reader/drain), and the test mocks. Single-slot
/// "last-writer-wins" semantics keep contention low.
pub type WatermarkSlot = Arc<std::sync::Mutex<Option<lan_audio_protocol::WatermarkReport>>>;

#[derive(Clone)]
pub struct ClientRegistry {
    inner: Arc<Mutex<ClientRegistryInner>>,
    metrics: Arc<Metrics>,
    /// Phase 3 watermark feed. The TCP control handler stores the latest
    /// per-client observation here; the adaptive watchdog drains it on
    /// every tick. Using a single-slot std Mutex (cheap, lock-free reads on
    /// the watchdog thread are not a goal — drains happen at ~2 Hz).
    latest_watermark: WatermarkSlot,
}

struct ClientRegistryInner {
    clients: HashMap<Uuid, ClientHandle>,
    pending_usb_clients: VecDeque<(Uuid, Instant)>,
    pending_usb_streams: VecDeque<Arc<Mutex<OwnedWriteHalf>>>,
}

struct ClientHandle {
    id: Uuid,
    name: String,
    client_key: String,
    control_tx: mpsc::UnboundedSender<String>,
    transport: Option<ClientTransportTarget>,
    prefers_usb_transport: bool,
    data_plane: DataPlaneFormat,
    codec: CodecSelection,
    audio_mode: AudioMode,
    preferred_sample_rate: u32,
    pending_first_packet: bool,
    pending_mode_sync: bool,
    supports_v2_events: bool,
}

#[derive(Clone)]
enum ClientTransportTarget {
    Wifi(SocketAddr),
    Usb(Arc<Mutex<OwnedWriteHalf>>),
}

#[derive(Clone)]
pub enum ClientTransportSnapshot {
    Wifi(SocketAddr),
    Usb(Arc<Mutex<OwnedWriteHalf>>),
}

#[derive(Clone)]
pub struct BroadcastClient {
    pub id: Uuid,
    pub name: String,
    pub data_plane: DataPlaneFormat,
    pub codec: CodecSelection,
    pub audio_mode: AudioMode,
    pub preferred_sample_rate: u32,
    pub transport: ClientTransportSnapshot,
    pub first_packet: bool,
    pub mode_changed: bool,
}

#[derive(Clone)]
enum ClientTransportKind {
    Wifi(SocketAddr),
    Usb { serial: String },
}

struct RegisterClientRequest {
    id: Uuid,
    name: String,
    client_key: String,
    control_tx: mpsc::UnboundedSender<String>,
    transport_kind: ClientTransportKind,
    data_plane: DataPlaneFormat,
    codec: CodecSelection,
    audio_mode: AudioMode,
    preferred_sample_rate: u32,
    supports_v2_events: bool,
}

impl SessionServer {
    pub fn new(
        cfg: Arc<ServerConfig>,
        metrics: Arc<Metrics>,
        registry: ClientRegistry,
        data_plane_router: Arc<std::sync::Mutex<DataPlaneRouter>>,
        current_audio_mode: Arc<std::sync::Mutex<AudioMode>>,
    ) -> Self {
        Self {
            cfg,
            metrics,
            registry,
            data_plane_router,
            current_audio_mode,
        }
    }

    fn current_audio_mode(&self) -> AudioMode {
        *self.current_audio_mode.lock().unwrap()
    }

    fn set_current_audio_mode(&self, mode: AudioMode) {
        let mut guard = self.current_audio_mode.lock().unwrap();
        *guard = mode;
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

        let mut conn_state = ConnectionStateMachine::default();
        conn_state
            .transition(ConnectionState::Handshaking)
            .expect("disconnected -> handshaking is always valid");

        let hello_msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws_rx.next())
            .await
            .map_err(|_| {
                let _ = conn_state.fail(ConnectionState::Closed, FailureCode::HandshakeTimeout);
                warn!(peer = %peer, failure_code = ?FailureCode::HandshakeTimeout, "hello timeout");
                anyhow!("hello timeout")
            })?
            .ok_or_else(|| {
                let _ = conn_state.fail(ConnectionState::Closed, FailureCode::HandshakeTimeout);
                anyhow!("client disconnected before hello")
            })??;
        let hello_text = match hello_msg {
            Message::Text(text) => text.to_string(),
            _ => return Err(anyhow!("expected text hello message")),
        };

        let session_id = Uuid::new_v4();
        let (
            client_name,
            client_key,
            udp_port,
            desired_sample_rate,
            channels,
            v2_session,
            client_capabilities,
            _preferred_audio_mode,
        ) = match parse_session_hello(&hello_text)? {
            SessionHello::Legacy {
                client_name,
                udp_port,
                desired_sample_rate,
                channels,
            } => {
                let client_key = format!("legacy-{}-{}", client_name, peer.ip());
                (
                    client_name,
                    client_key,
                    udp_port,
                    desired_sample_rate,
                    channels,
                    false,
                    legacy_client_capabilities(),
                    self.cfg.current_audio_mode,
                )
            }
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
                    hello.client_id,
                    hello.udp_port,
                    hello.desired_sample_rate,
                    hello.channels,
                    true,
                    hello.capabilities,
                    hello.preferred_audio_mode,
                )
            }
        };
        let preferred_sample_rate = normalize_preferred_sample_rate(desired_sample_rate);
        let server_audio_mode = self.current_audio_mode();

        let negotiated_path = {
            let router = self.data_plane_router.lock().unwrap();
            negotiate_session_path(&self.cfg, &router, v2_session, &client_capabilities)
        };
        self.metrics.set_negotiated_session_path(
            negotiated_path.data_plane.as_str(),
            negotiated_path.codec.as_str(),
        );
        if let Err(e) = conn_state.transition(ConnectionState::Negotiated) {
            warn!(session = %session_id, error = %e, "state transition to negotiated failed");
        }

        let transport_kind = match &self.cfg.transport_mode {
            TransportMode::WiFi => ClientTransportKind::Wifi(SocketAddr::new(peer.ip(), udp_port)),
            TransportMode::Usb { serial } => ClientTransportKind::Usb {
                serial: serial.clone(),
            },
        };
        let transport_type = match &self.cfg.transport_mode {
            TransportMode::WiFi => TransportType::Wifi,
            TransportMode::Usb { .. } => TransportType::Usb,
        };

        let (control_tx, mut control_rx) = mpsc::unbounded_channel::<String>();
        let writer_task = tokio::spawn(async move {
            while let Some(text) = control_rx.recv().await {
                if ws_tx.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
        });

        let register_result = self
            .registry
            .register_client(RegisterClientRequest {
                id: session_id,
                name: client_name.clone(),
                client_key: client_key.clone(),
                control_tx: control_tx.clone(),
                transport_kind,
                data_plane: negotiated_path.data_plane,
                codec: negotiated_path.codec,
                audio_mode: server_audio_mode,
                preferred_sample_rate,
                supports_v2_events: v2_session,
            })
            .await;

        if let Err(err) = register_result {
            let _ = conn_state.fail(ConnectionState::Closed, FailureCode::NegotiationMismatch);
            warn!(
                session = %session_id,
                failure_code = ?FailureCode::NegotiationMismatch,
                error = %err,
                "client registration failed"
            );
            if v2_session {
                let hello_ack = ControlMessageV2::HelloAck(HelloAck {
                    protocol_version: PROTOCOL_VERSION_V2,
                    accepted: false,
                    session_id,
                    current_audio_mode: server_audio_mode,
                    transport_type,
                    mode_profile: audio_mode_profile(server_audio_mode),
                    capabilities: default_server_capabilities(),
                    message: err.to_string(),
                });
                let _ = control_tx.send(serde_json::to_string(&hello_ack)?);
            } else {
                let _ =
                    control_tx.send(serde_json::to_string(&ServerControlMessage::ServerError {
                        code: "register_client_failed".to_string(),
                        message: err.to_string(),
                    })?);
            }
            drop(control_tx);
            let _ = writer_task.await;
            return Ok(());
        }

        if v2_session {
            let _ = control_tx.send(serde_json::to_string(&ControlMessageV2::HelloAck(
                HelloAck {
                    protocol_version: PROTOCOL_VERSION_V2,
                    accepted: true,
                    session_id,
                    current_audio_mode: server_audio_mode,
                    transport_type,
                    mode_profile: audio_mode_profile(server_audio_mode),
                    capabilities: default_server_capabilities(),
                    message: "hello_ack".to_string(),
                },
            ))?);

            let _ = control_tx.send(serde_json::to_string(&ControlMessageV2::ServerInfo(
                ServerInfo {
                    server_id: session_id,
                    server_name: self.cfg.server_name.clone(),
                    platform: "windows".to_string(),
                    app_version: env!("CARGO_PKG_VERSION").to_string(),
                    ws_port: self.cfg.ws_bind.port(),
                    udp_port: self.cfg.udp_bind.port(),
                    protocol_version: PROTOCOL_VERSION_V2,
                    current_audio_mode: server_audio_mode,
                    mode_profile: audio_mode_profile(server_audio_mode),
                    codec: negotiated_path.codec.as_protocol_preference(),
                    data_plane: negotiated_path.data_plane.as_str().to_string(),
                    gray_mode: negotiated_path.data_plane != DataPlaneFormat::LegacyLas1,
                    recommended_connection: match self.cfg.transport_mode {
                        TransportMode::WiFi => "usb_tethering_or_5ghz_wifi".to_string(),
                        TransportMode::Usb { .. } => "usb".to_string(),
                    },
                },
            ))?);
        }

        let _ = control_tx.send(serde_json::to_string(
            &ServerControlMessage::ServerWelcome {
                session_id,
                codec: negotiated_path.codec.as_str().to_string(),
                sample_rate: preferred_sample_rate,
                channels,
                frames_per_packet: self.cfg.frames_per_packet,
            },
        )?);

        if let Err(e) = conn_state.transition(ConnectionState::Streaming) {
            warn!(session = %session_id, error = %e, "state transition to streaming failed");
        }

        info!(
            session = %session_id,
            peer = %peer,
            client = %client_name,
            client_key = %client_key,
            transport = %self.cfg.transport_mode.as_str(),
            conn_state = ?conn_state.state(),
            "session established"
        );

        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                msg = timeout(Duration::from_secs(30), ws_rx.next()) => {
                    let msg = match msg {
                        Ok(inner) => inner,
                        Err(_) => {
                            warn!(session = %session_id, "websocket read timeout (30s), closing stale session");
                            break;
                        }
                    };
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            let text = text.to_string();
                            if let Ok(ClientControlMessage::ClientPing { seq, ts_unix_ms }) =
                                serde_json::from_str::<ClientControlMessage>(&text)
                            {
                                let _ = control_tx.send(serde_json::to_string(&ServerControlMessage::ServerPong {
                                    seq,
                                    ts_unix_ms,
                                })?);
                                let snapshot = self.metrics.snapshot();
                                let _ = control_tx.send(serde_json::to_string(&ServerControlMessage::ServerMetrics {
                                    tx_packets: snapshot.tx_packets,
                                    tx_bytes: snapshot.tx_bytes,
                                    sessions: snapshot.active_sessions,
                                })?);
                                continue;
                            }

                            // Phase 3: client buffer-level reports feed the
                            // adaptive sync engine. We accept the message
                            // even on legacy v1 sessions — the variant is
                            // additive over the existing JSON tag space.
                            if let Ok(ClientControlMessage::ClientWatermark(report)) =
                                serde_json::from_str::<ClientControlMessage>(&text)
                            {
                                self.registry.note_client_watermark(report);
                                continue;
                            }

                            if v2_session {
                                match serde_json::from_str::<ControlMessageV2>(&text) {
                                    Ok(ControlMessageV2::SetAudioMode(SetAudioMode { mode, reason, preferred_sample_rate })) => {
                                        let normalized_preferred_sample_rate =
                                            preferred_sample_rate.map(normalize_preferred_sample_rate);
                                        self.set_current_audio_mode(mode);
                                        self.registry
                                            .set_client_mode(session_id, mode, normalized_preferred_sample_rate)
                                            .await;
                                        let _ = control_tx.send(serde_json::to_string(&ControlMessageV2::AudioModeChanged(
                                            AudioModeChanged {
                                                mode,
                                                applied: true,
                                                reason,
                                                mode_profile: audio_mode_profile(mode),
                                            }
                                        ))?);
                                        info!(
                                            session = %session_id,
                                            mode = ?mode,
                                            preferred_sample_rate = normalized_preferred_sample_rate.unwrap_or(preferred_sample_rate_default()),
                                            "audio mode updated by client"
                                        );
                                    }
                                    Ok(ControlMessageV2::ClientInfo(ClientInfo { client_name, platform, app_version, udp_port })) => {
                                        info!(
                                            session = %session_id,
                                            client = %client_name,
                                            platform = %platform,
                                            app_version = %app_version,
                                            udp_port,
                                            "protocol v2 client info received"
                                        );
                                    }
                                    Ok(_) => {}
                                    Err(err) => {
                                        warn!(
                                            session = %session_id,
                                            error = %err,
                                            payload = %text,
                                            "failed to parse protocol v2 control message"
                                        );
                                        let _ = control_tx.send(serde_json::to_string(&ControlMessageV2::Error(
                                            ErrorMessage {
                                                code: "bad_control_message".to_string(),
                                                message: format!("invalid v2 message: {err}"),
                                                recoverable: true,
                                                failure_code: None,
                                            }
                                        ))?);
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Ok(_)) => {}
                        Some(Err(err)) => return Err(anyhow!(err)),
                    }
                }
            }
        }

        self.registry.remove_client(session_id).await;
        let _ = conn_state.transition(ConnectionState::Closed);
        drop(control_tx);
        let _ = writer_task.await;
        info!(session = %session_id, conn_state = ?conn_state.state(), "session closed");
        Ok(())
    }
}

impl ClientRegistry {
    pub fn new(metrics: Arc<Metrics>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ClientRegistryInner {
                clients: HashMap::new(),
                pending_usb_clients: VecDeque::new(),
                pending_usb_streams: VecDeque::new(),
            })),
            metrics,
            latest_watermark: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Phase 3: store the most recently received watermark observation. The
    /// adaptive watchdog drains this on every tick. We keep a single-slot
    /// "last writer wins" semantic — the engine cares about freshness, not
    /// history.
    pub fn note_client_watermark(&self, report: lan_audio_protocol::WatermarkReport) {
        if let Ok(mut guard) = self.latest_watermark.lock() {
            *guard = Some(report);
        }
    }

    /// Phase 3: drain the latest watermark observation if any. Returns
    /// `None` if no new report has arrived since the previous drain.
    pub fn take_latest_watermark(&self) -> Option<lan_audio_protocol::WatermarkReport> {
        self.latest_watermark.lock().ok().and_then(|mut g| g.take())
    }

    /// Phase 3: hand out a clone of the watermark slot so the transport
    /// layer's watchdog task can drain reports without holding a registry
    /// reference.
    pub fn watermark_slot(&self) -> WatermarkSlot {
        Arc::clone(&self.latest_watermark)
    }

    pub async fn client_count(&self) -> usize {
        self.inner.lock().await.clients.len()
    }

    async fn register_client(&self, request: RegisterClientRequest) -> anyhow::Result<()> {
        let mut broadcasts: Vec<(mpsc::UnboundedSender<String>, String)> = Vec::new();
        let replaced_client = {
            let mut guard = self.inner.lock().await;

            let mut replaced = None;
            // Replace any existing session with the same client_key (same device reconnecting)
            let existing_id = guard
                .clients
                .values()
                .find(|client| client.client_key == request.client_key)
                .map(|client| client.id);
            if let Some(existing_id) = existing_id {
                replaced = remove_client_locked(&mut guard, existing_id);
            } else if matches!(request.transport_kind, ClientTransportKind::Usb { .. }) {
                // Also replace any existing USB client (only one USB session allowed)
                let existing_usb_id = guard
                    .clients
                    .values()
                    .find(|client| matches!(client.transport, Some(ClientTransportTarget::Usb(_))))
                    .map(|client| client.id);
                if let Some(existing_usb_id) = existing_usb_id {
                    replaced = remove_client_locked(&mut guard, existing_usb_id);
                }
            } else if let ClientTransportKind::Wifi(new_addr) = &request.transport_kind {
                // For WiFi: replace existing session from the same IP only when
                // the device name matches (same device reconnecting with a new client_id).
                let new_ip = new_addr.ip();
                let existing_same_device = guard
                    .clients
                    .values()
                    .find(|client| {
                        client.name == request.name
                            && if let Some(ClientTransportTarget::Wifi(addr)) = &client.transport {
                                addr.ip() == new_ip
                            } else {
                                false
                            }
                    })
                    .map(|client| client.id);
                if let Some(existing_id) = existing_same_device {
                    replaced = remove_client_locked(&mut guard, existing_id);
                }
            }

            // Check capacity AFTER replacement — a reconnecting client may have freed its own slot
            if guard.clients.len() >= MAX_CLIENTS {
                return Err(anyhow!("too_many_clients"));
            }
            if !guard.clients.is_empty()
                && replaced.is_none()
                && !check_multi_client_allowed_with_guard(&guard)
            {
                return Err(anyhow!("multi_client_upgrade_required"));
            }

            let mut client = ClientHandle {
                id: request.id,
                name: request.name.clone(),
                client_key: request.client_key,
                control_tx: request.control_tx.clone(),
                transport: None,
                prefers_usb_transport: false,
                data_plane: request.data_plane,
                codec: request.codec,
                audio_mode: request.audio_mode,
                preferred_sample_rate: request.preferred_sample_rate,
                pending_first_packet: true,
                pending_mode_sync: false,
                supports_v2_events: request.supports_v2_events,
            };
            match request.transport_kind {
                ClientTransportKind::Wifi(target) => {
                    client.transport = Some(ClientTransportTarget::Wifi(target));
                }
                ClientTransportKind::Usb { serial } => {
                    client.prefers_usb_transport = true;
                    if let Some(stream) = guard.pending_usb_streams.pop_front() {
                        client.transport = Some(ClientTransportTarget::Usb(stream));
                    } else {
                        info!(serial, client = %client.name, "usb client waiting for forwarded tcp data stream");
                        guard
                            .pending_usb_clients
                            .push_back((client.id, Instant::now()));
                    }
                }
            }

            self.metrics
                .note_client_connected(&client.name, &client.client_key);
            self.metrics.inc_sessions();
            guard.clients.insert(client.id, client);

            let client_list_json = build_client_list_json(&guard.clients);
            if let Some(ref json) = client_list_json {
                for client in guard.clients.values() {
                    if client.supports_v2_events {
                        broadcasts.push((client.control_tx.clone(), json.clone()));
                    }
                }
            }

            if let Some(joined_json) = build_client_joined_json(request.id, &request.name) {
                for client in guard.clients.values() {
                    if client.id != request.id && client.supports_v2_events {
                        broadcasts.push((client.control_tx.clone(), joined_json.clone()));
                    }
                }
            }
            replaced
        };

        if let Some(removed) = replaced_client {
            self.metrics.dec_sessions();
            if let Some(left_json) = build_client_left_json(removed.id, &removed.name) {
                self.broadcast_control_json(left_json).await;
            }
        }

        for (tx, json) in broadcasts {
            let _ = tx.send(json);
        }
        Ok(())
    }

    pub async fn remove_client(&self, client_id: Uuid) -> Option<String> {
        let removed = {
            let mut guard = self.inner.lock().await;
            remove_client_locked(&mut guard, client_id)
        }?;
        self.metrics.dec_sessions();
        if let Some(json) = build_client_left_json(removed.id, &removed.name) {
            self.broadcast_control_json(json).await;
        }
        Some(removed.name)
    }

    pub async fn set_client_mode(
        &self,
        client_id: Uuid,
        mode: AudioMode,
        preferred_sample_rate: Option<u32>,
    ) {
        let mut broadcasts: Vec<(mpsc::UnboundedSender<String>, String)> = Vec::new();
        {
            let mut guard = self.inner.lock().await;
            let Some(client) = guard.clients.get_mut(&client_id) else {
                return;
            };
            client.audio_mode = mode;
            if let Some(sample_rate) = preferred_sample_rate {
                client.preferred_sample_rate = sample_rate;
            }
            client.pending_mode_sync = true;
            let Some(json) = build_client_list_json(&guard.clients) else {
                return;
            };
            for client in guard.clients.values() {
                if client.supports_v2_events {
                    broadcasts.push((client.control_tx.clone(), json.clone()));
                }
            }
        }
        for (tx, json) in broadcasts {
            let _ = tx.send(json);
        }
    }

    pub async fn attach_usb_stream(&self, write_half: OwnedWriteHalf) {
        let stream = Arc::new(Mutex::new(write_half));
        let attached_client = {
            let mut guard = self.inner.lock().await;
            // Clean up expired pending USB clients
            let now = Instant::now();
            let mut expired = Vec::new();
            guard.pending_usb_clients.retain(|(id, ts)| {
                if now.duration_since(*ts) > Duration::from_secs(USB_PENDING_TIMEOUT_SECS) {
                    expired.push(*id);
                    false
                } else {
                    true
                }
            });
            for id in expired {
                warn!(client_id = %id, "removing expired pending USB client");
                remove_client_locked(&mut guard, id);
            }
            if let Some((client_id, _)) = guard.pending_usb_clients.pop_front() {
                if let Some(client) = guard.clients.get_mut(&client_id) {
                    client.transport = Some(ClientTransportTarget::Usb(Arc::clone(&stream)));
                    Some(client.name.clone())
                } else {
                    None
                }
            } else if let Some((_, client)) = guard
                .clients
                .iter_mut()
                .find(|(_, client)| client.prefers_usb_transport && client.transport.is_none())
            {
                client.transport = Some(ClientTransportTarget::Usb(Arc::clone(&stream)));
                Some(client.name.clone())
            } else {
                guard.pending_usb_streams.push_back(stream);
                None
            }
        };
        if let Some(name) = attached_client {
            info!(client = %name, "attached forwarded tcp stream to pending usb client");
        } else {
            info!("queued forwarded tcp stream while waiting for usb websocket hello");
        }
    }

    pub async fn mark_usb_transport_lost(&self, client_id: Uuid) -> Option<String> {
        let mut guard = self.inner.lock().await;
        let (name, prefers_usb_transport) = {
            let client = guard.clients.get_mut(&client_id)?;
            client.transport = None;
            (client.name.clone(), client.prefers_usb_transport)
        };
        if !prefers_usb_transport {
            return None;
        }
        if !guard
            .pending_usb_clients
            .iter()
            .any(|(id, _)| *id == client_id)
        {
            guard
                .pending_usb_clients
                .push_back((client_id, Instant::now()));
        }
        Some(name)
    }

    pub async fn take_broadcast_clients(&self) -> Vec<BroadcastClient> {
        let mut guard = self.inner.lock().await;
        let mut clients = Vec::new();
        for client in guard.clients.values_mut() {
            let Some(transport) = client.transport.clone() else {
                continue;
            };
            let transport = match transport {
                ClientTransportTarget::Wifi(addr) => ClientTransportSnapshot::Wifi(addr),
                ClientTransportTarget::Usb(writer) => ClientTransportSnapshot::Usb(writer),
            };
            clients.push(BroadcastClient {
                id: client.id,
                name: client.name.clone(),
                data_plane: client.data_plane,
                codec: client.codec,
                audio_mode: client.audio_mode,
                preferred_sample_rate: client.preferred_sample_rate,
                transport,
                first_packet: client.pending_first_packet,
                mode_changed: client.pending_mode_sync,
            });
            client.pending_first_packet = false;
            client.pending_mode_sync = false;
        }
        clients
    }

    async fn broadcast_control_json(&self, json: String) {
        let recipients = {
            let guard = self.inner.lock().await;
            guard
                .clients
                .values()
                .filter(|client| client.supports_v2_events)
                .map(|client| client.control_tx.clone())
                .collect::<Vec<_>>()
        };
        for tx in recipients {
            let _ = tx.send(json.clone());
        }
    }
}

fn remove_client_locked(guard: &mut ClientRegistryInner, client_id: Uuid) -> Option<ClientHandle> {
    guard.pending_usb_clients.retain(|(id, _)| *id != client_id);
    guard.clients.remove(&client_id)
}

fn preferred_sample_rate_default() -> u32 {
    48_000
}

fn normalize_preferred_sample_rate(sample_rate: u32) -> u32 {
    match sample_rate {
        8_000 | 12_000 | 16_000 | 24_000 | 48_000 => sample_rate,
        _ => preferred_sample_rate_default(),
    }
}

fn build_client_list_json(clients: &HashMap<Uuid, ClientHandle>) -> Option<String> {
    // Sort by client UUID so the broadcast order is deterministic across
    // calls. Without this the HashMap iteration order changes every tick
    // and the desktop / Android UI keeps reshuffling the device list.
    let mut entries: Vec<&ClientHandle> = clients
        .values()
        .filter(|client| client.supports_v2_events)
        .collect();
    entries.sort_by_key(|client| client.id);
    let list = ControlMessageV2::ClientList(ClientList {
        clients: entries
            .into_iter()
            .map(|client| ClientListEntry {
                id: client.id,
                name: client.name.clone(),
                mode: client.audio_mode,
            })
            .collect(),
    });
    serde_json::to_string(&list).ok()
}

fn build_client_joined_json(id: Uuid, name: &str) -> Option<String> {
    serde_json::to_string(&ControlMessageV2::ClientJoined(ClientJoined {
        id,
        name: name.to_string(),
    }))
    .ok()
}

fn build_client_left_json(id: Uuid, name: &str) -> Option<String> {
    serde_json::to_string(&ControlMessageV2::ClientLeft(ClientLeft {
        id,
        name: name.to_string(),
    }))
    .ok()
}

pub fn check_multi_client_allowed(registry: &ClientRegistry) -> bool {
    let _ = registry;
    // TODO(phase-5): insert license key validation here when client_count > 1.
    true
}

fn check_multi_client_allowed_with_guard(_registry: &ClientRegistryInner) -> bool {
    // TODO(phase-5): insert license key validation here when client_count > 1.
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NegotiatedSessionPath {
    data_plane: DataPlaneFormat,
    codec: CodecSelection,
}

fn negotiate_session_path(
    _cfg: &ServerConfig,
    router: &DataPlaneRouter,
    v2_session: bool,
    client_capabilities: &ProtocolCapabilities,
) -> NegotiatedSessionPath {
    if !v2_session {
        return NegotiatedSessionPath {
            data_plane: DataPlaneFormat::LegacyLas1,
            codec: CodecSelection::Pcm16,
        };
    }

    let data_plane = router.active_format();
    let codec = match (router.active_codec(), data_plane) {
        (CodecSelection::Opus, DataPlaneFormat::V2Header)
            if client_capabilities.supports_opus
                || client_capabilities.supports_opus_experimental =>
        {
            CodecSelection::Opus
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
        supports_opus: false,
        supports_opus_experimental: false,
        supports_low_latency: false,
        supports_high_quality: false,
        supports_native_audio_track: false,
        supports_fast_path: false,
        supports_stable_audio_track: false,
        supports_usb_tethering: false,
        supports_usb_direct_future: false,
        supports_reverse_channel: false,
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
        supports_opus: true,
        supports_opus_experimental: true,
        supports_low_latency: true,
        supports_high_quality: true,
        supports_native_audio_track: true,
        supports_fast_path: false,
        supports_stable_audio_track: true,
        supports_usb_tethering: true,
        supports_usb_direct_future: false,
        supports_reverse_channel: false,
    }
}

pub async fn write_length_prefixed_frame(
    writer: &Arc<Mutex<OwnedWriteHalf>>,
    payload: &[u8],
) -> anyhow::Result<()> {
    let mut guard = writer.lock().await;
    timeout(USB_DIRECT_WRITE_TIMEOUT, async {
        guard
            .write_all(&(payload.len() as u32).to_be_bytes())
            .await?;
        guard.write_all(payload).await
    })
    .await
    .context("usb direct frame write timeout")??;
    Ok(())
}

const USB_DIRECT_WRITE_TIMEOUT: Duration = Duration::from_millis(250);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AudioSourceKind;
    use crate::data_plane::DataPlaneRouter;
    use lan_audio_protocol::{
        AudioMode, ControlMessageV2, DataPlanePath, Hello, ServiceMetricsSnapshot, ServiceSnapshot,
        TransportType, PROTOCOL_VERSION_V2,
    };
    use serde_json::json;

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
            codec_selection: CodecSelection::Opus,
            audio_source: AudioSourceKind::Synthetic,
            ..ServerConfig::default()
        };

        let router = DataPlaneRouter::from_config(&cfg);
        let negotiated =
            negotiate_session_path(&cfg, &router, false, &default_server_capabilities());

        assert_eq!(negotiated.data_plane, DataPlaneFormat::LegacyLas1);
        assert_eq!(negotiated.codec, CodecSelection::Pcm16);
    }

    #[test]
    fn negotiate_session_path_falls_back_without_client_opus_capability() {
        let cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            codec_selection: CodecSelection::Opus,
            audio_source: AudioSourceKind::Synthetic,
            ..ServerConfig::default()
        };
        let mut caps = default_server_capabilities();
        caps.supports_opus = false;
        caps.supports_opus_experimental = false;

        let router = DataPlaneRouter::from_config(&cfg);
        let negotiated = negotiate_session_path(&cfg, &router, true, &caps);

        assert_eq!(negotiated.data_plane, DataPlaneFormat::V2Header);
        assert_eq!(negotiated.codec, CodecSelection::Pcm16);
    }

    #[test]
    fn negotiate_session_path_allows_opus_only_when_v2_and_client_supports_it() {
        let cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            codec_selection: CodecSelection::Opus,
            audio_source: AudioSourceKind::Synthetic,
            ..ServerConfig::default()
        };

        let router = DataPlaneRouter::from_config(&cfg);
        let negotiated =
            negotiate_session_path(&cfg, &router, true, &default_server_capabilities());

        assert_eq!(negotiated.data_plane, DataPlaneFormat::V2Header);
        assert_eq!(negotiated.codec, CodecSelection::Opus);
    }

    #[test]
    fn negotiate_session_path_recommends_loopback_v2_by_default() {
        let cfg = ServerConfig {
            data_plane_format: DataPlaneFormat::V2Header,
            codec_selection: CodecSelection::Opus,
            audio_source: AudioSourceKind::WindowsLoopback,
            allow_loopback_v2_header_gray: false,
            ..ServerConfig::default()
        };

        let router = DataPlaneRouter::from_config(&cfg);
        let negotiated =
            negotiate_session_path(&cfg, &router, true, &default_server_capabilities());

        assert_eq!(negotiated.data_plane, DataPlaneFormat::V2Header);
        assert_eq!(negotiated.codec, CodecSelection::Opus);
    }

    #[test]
    fn connection_state_machine_tracks_session_lifecycle() {
        let mut csm = ConnectionStateMachine::default();
        assert_eq!(csm.state(), ConnectionState::Disconnected);

        csm.transition(ConnectionState::Handshaking).unwrap();
        assert_eq!(csm.state(), ConnectionState::Handshaking);

        csm.transition(ConnectionState::Negotiated).unwrap();
        assert_eq!(csm.state(), ConnectionState::Negotiated);

        csm.transition(ConnectionState::Streaming).unwrap();
        assert_eq!(csm.state(), ConnectionState::Streaming);

        csm.transition(ConnectionState::Closed).unwrap();
        assert_eq!(csm.state(), ConnectionState::Closed);
        assert!(csm.failure_code().is_none());
    }

    #[test]
    fn connection_state_machine_records_failure_code_on_fail() {
        let mut csm = ConnectionStateMachine::default();
        csm.transition(ConnectionState::Handshaking).unwrap();

        csm.fail(ConnectionState::Closed, FailureCode::HandshakeTimeout)
            .unwrap();
        assert_eq!(csm.state(), ConnectionState::Closed);
        assert_eq!(csm.failure_code(), Some(FailureCode::HandshakeTimeout));
    }

    #[test]
    fn error_message_serializes_failure_code_when_present() {
        let msg = ErrorMessage {
            code: "test".to_string(),
            message: "test error".to_string(),
            recoverable: true,
            failure_code: Some(FailureCode::NegotiationMismatch),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("NEGOTIATION_MISMATCH"));

        let msg_none = ErrorMessage {
            code: "test".to_string(),
            message: "test error".to_string(),
            recoverable: true,
            failure_code: None,
        };
        let json_none = serde_json::to_string(&msg_none).unwrap();
        assert!(!json_none.contains("failure_code"));
    }

    #[test]
    fn snapshot_includes_active_data_plane() {
        let snapshot = ServiceSnapshot {
            transport: TransportType::Usb,
            mode: AudioMode::Balanced,
            data_plane: DataPlanePath::V2Header,
            active_data_plane: DataPlanePath::UsbDirect,
            rollback_available: true,
            codec: lan_audio_protocol::AudioCodecPreference::Opus,
            effective_codec: lan_audio_protocol::AudioCodecPreference::Opus,
            state: ConnectionState::Streaming,
            rollback_state: lan_audio_protocol::RollbackState::MainPathActive,
            metrics: ServiceMetricsSnapshot::default(),
            last_error: None,
        };

        assert_eq!(
            serde_json::to_value(snapshot).unwrap(),
            json!({
                "transport": "usb",
                "mode": "balanced",
                "data_plane": "v2_header",
                "active_data_plane": "usb_direct",
                "rollback_available": true,
                "codec": "opus",
                "effective_codec": "opus",
                "state": "streaming",
                "rollback_state": "main_path_active",
                "metrics": {
                    "buffered_ms": 0,
                    "underrun": 0,
                    "late_packets": 0,
                    "dropped_packets": 0,
                    "rtt_ms": 0,
                    "reconnect_count": 0,
                    "decode_errors": 0,
                    "sink_write_gap_ms_p95": 0
                }
            })
        );
    }

    #[test]
    fn watermark_slot_round_trips_latest_observation() {
        let metrics = Metrics::new_shared();
        let registry = ClientRegistry::new(metrics);

        // No reports yet — drain returns None.
        assert!(registry.take_latest_watermark().is_none());

        let report = lan_audio_protocol::WatermarkReport {
            ts_unix_ms: 1_700_000_000_500,
            jitter_buf_ms: 95,
            ring_buf_ms: 30,
            silence_fill_delta: 2,
            underrun_delta: 0,
            jitter_p95_us: 1_800,
        };
        registry.note_client_watermark(report);

        let drained = registry
            .take_latest_watermark()
            .expect("watermark must be present after note");
        assert_eq!(drained.jitter_buf_ms, 95);
        assert_eq!(drained.ring_buf_ms, 30);
        assert_eq!(drained.silence_fill_delta, 2);
        assert_eq!(drained.jitter_p95_us, 1_800);

        // Drain consumes the slot — second drain returns None.
        assert!(registry.take_latest_watermark().is_none());
    }

    #[test]
    fn watermark_slot_uses_last_writer_wins_semantics() {
        let metrics = Metrics::new_shared();
        let registry = ClientRegistry::new(metrics);

        let first = lan_audio_protocol::WatermarkReport {
            ts_unix_ms: 1_000,
            jitter_buf_ms: 50,
            ring_buf_ms: 20,
            silence_fill_delta: 0,
            underrun_delta: 0,
            jitter_p95_us: 1_000,
        };
        let second = lan_audio_protocol::WatermarkReport {
            ts_unix_ms: 2_000,
            jitter_buf_ms: 100,
            ring_buf_ms: 40,
            silence_fill_delta: 1,
            underrun_delta: 1,
            jitter_p95_us: 2_000,
        };
        registry.note_client_watermark(first);
        registry.note_client_watermark(second);

        let drained = registry
            .take_latest_watermark()
            .expect("watermark must be present");
        assert_eq!(drained.ts_unix_ms, 2_000, "last writer must win");
        assert_eq!(drained.jitter_buf_ms, 100);
    }

    /// Build a small `clients` map with two v2 sessions and verify that
    /// `build_client_list_json` emits them sorted by client UUID. Without
    /// the explicit sort, the broadcast order would track HashMap iteration
    /// order and the desktop / Android UI kept reshuffling the device list.
    #[test]
    fn client_list_json_is_deterministically_ordered_by_id() {
        use mpsc::unbounded_channel;
        // Pick two UUIDs whose ordering is unambiguous so the test is not
        // flaky.
        let a_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let b_id = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();

        let (tx_a, _rx_a) = unbounded_channel::<String>();
        let (tx_b, _rx_b) = unbounded_channel::<String>();
        let make_handle = |id: Uuid, name: &str, tx: mpsc::UnboundedSender<String>| ClientHandle {
            id,
            name: name.to_string(),
            client_key: name.to_string(),
            control_tx: tx,
            transport: None,
            prefers_usb_transport: false,
            data_plane: DataPlaneFormat::V2Header,
            codec: CodecSelection::Opus,
            audio_mode: AudioMode::Balanced,
            preferred_sample_rate: 48_000,
            pending_first_packet: true,
            pending_mode_sync: false,
            supports_v2_events: true,
        };

        let mut clients: HashMap<Uuid, ClientHandle> = HashMap::new();
        // Insert in reverse-id order to make sure HashMap order can't
        // accidentally match the desired sort.
        clients.insert(b_id, make_handle(b_id, "device-b", tx_b));
        clients.insert(a_id, make_handle(a_id, "device-a", tx_a));

        let json = build_client_list_json(&clients).expect("client list json");
        // device-a (lower UUID) must appear before device-b (higher UUID).
        let pos_a = json.find("device-a").expect("device-a in payload");
        let pos_b = json.find("device-b").expect("device-b in payload");
        assert!(
            pos_a < pos_b,
            "list must be sorted by id, got: {}",
            json
        );
    }
}
