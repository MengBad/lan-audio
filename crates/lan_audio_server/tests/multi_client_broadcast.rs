use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use lan_audio_protocol::{
    AudioMode, ControlMessageV2, Hello, HelloAck, ProtocolCapabilities, PROTOCOL_VERSION_V2,
};
use lan_audio_server::config::{AudioSourceKind, ServerConfig, TransportMode};
use lan_audio_server::service::LanAudioService;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream};
use uuid::Uuid;

type WsStream = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct TestClient {
    ws: WsStream,
    udp: UdpSocket,
}

#[tokio::test]
async fn multi_client_broadcast_handles_disconnect_and_rejects_over_limit() {
    let mut cfg = ServerConfig::default();
    cfg.audio_source = AudioSourceKind::Synthetic;
    cfg.transport_mode = TransportMode::WiFi;
    cfg.ws_bind = format!("127.0.0.1:{}", pick_free_tcp_port())
        .parse()
        .expect("ws bind");
    cfg.udp_bind = format!("127.0.0.1:{}", pick_free_udp_port())
        .parse()
        .expect("udp bind");
    cfg.discovery_bind = "127.0.0.1:0".parse().expect("discovery bind");
    cfg.discovery_broadcast = "127.0.0.1:39990".parse().expect("discovery broadcast");

    let service = LanAudioService::new(cfg.clone())
        .await
        .expect("create service");
    let service = std::sync::Arc::new(service);
    let runner = {
        let service = std::sync::Arc::clone(&service);
        tokio::spawn(async move { service.run_until_shutdown().await })
    };

    let mut c1 = connect_client(cfg.ws_bind.port(), "c1").await;
    let c2 = connect_client(cfg.ws_bind.port(), "c2").await;
    let c3 = connect_client(cfg.ws_bind.port(), "c3").await;

    assert_receives_frame(&c1.udp).await;
    assert_receives_frame(&c2.udp).await;
    assert_receives_frame(&c3.udp).await;

    c1.ws
        .close(None)
        .await
        .expect("close first client websocket");
    tokio::time::sleep(Duration::from_millis(150)).await;

    assert_receives_frame(&c2.udp).await;
    assert_receives_frame(&c3.udp).await;

    let mut _c1r = connect_client(cfg.ws_bind.port(), "c1r").await;
    let mut _c4 = connect_client(cfg.ws_bind.port(), "c4").await;

    let mut c5 = connect_raw_client(cfg.ws_bind.port(), "c5").await;
    let ack = wait_for_hello_ack(&mut c5).await;
    assert!(!ack.accepted, "the 5th client must be rejected");
    assert_eq!(ack.message, "too_many_clients");

    service.stop();
    let joined = timeout(Duration::from_secs(5), runner)
        .await
        .expect("service should stop in time");
    let run_result = joined.expect("join ok");
    run_result.expect("service exits cleanly");
}

async fn connect_client(ws_port: u16, name: &str) -> TestClient {
    let udp = UdpSocket::bind("127.0.0.1:0").await.expect("bind udp");
    let udp_port = udp.local_addr().expect("udp addr").port();
    let ws = connect_raw(ws_port)
        .await
        .expect("connect websocket for client");
    let mut ws = ws;
    send_v2_hello(&mut ws, name, udp_port).await;
    let ack = wait_for_hello_ack(&mut ws).await;
    assert!(ack.accepted, "hello_ack should accept client {name}");
    TestClient { ws, udp }
}

async fn connect_raw_client(ws_port: u16, name: &str) -> WsStream {
    let mut ws = connect_raw(ws_port)
        .await
        .expect("connect websocket for raw client");
    send_v2_hello(&mut ws, name, 0).await;
    ws
}

async fn connect_raw(ws_port: u16) -> anyhow::Result<WsStream> {
    let url = format!("ws://127.0.0.1:{ws_port}/");
    for _ in 0..30 {
        match connect_async(&url).await {
            Ok((ws, _)) => return Ok(ws),
            Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
    Err(anyhow::anyhow!("failed to connect websocket: {url}"))
}

async fn send_v2_hello(ws: &mut WsStream, name: &str, udp_port: u16) {
    let hello = ControlMessageV2::Hello(Hello {
        protocol_version: PROTOCOL_VERSION_V2,
        device_name: name.to_string(),
        client_id: format!("{name}-{}", Uuid::new_v4()),
        udp_port,
        desired_sample_rate: 48_000,
        channels: 2,
        capabilities: test_capabilities(),
        preferred_audio_mode: AudioMode::Balanced,
    });
    ws.send(Message::Text(
        serde_json::to_string(&hello)
            .expect("serialize hello")
            .into(),
    ))
    .await
    .expect("send hello");
}

async fn wait_for_hello_ack(ws: &mut WsStream) -> HelloAck {
    let mut attempts = 0;
    while attempts < 20 {
        attempts += 1;
        let msg = timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("timed out waiting hello_ack")
            .expect("websocket stream ended")
            .expect("websocket message error");
        if let Message::Text(text) = msg {
            if let Ok(ControlMessageV2::HelloAck(ack)) =
                serde_json::from_str::<ControlMessageV2>(text.as_str())
            {
                return ack;
            }
        }
    }
    panic!("did not receive hello_ack in time");
}

async fn assert_receives_frame(udp: &UdpSocket) {
    let mut buf = vec![0_u8; 4096];
    let n = timeout(Duration::from_secs(3), udp.recv(&mut buf))
        .await
        .expect("timed out waiting broadcast frame")
        .expect("udp receive failed");
    assert!(n > 0, "received frame should not be empty");
}

fn pick_free_tcp_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind tcp port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn pick_free_udp_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .expect("bind udp port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn test_capabilities() -> ProtocolCapabilities {
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
        supports_fast_path: true,
        supports_stable_audio_track: true,
        supports_usb_tethering: true,
        supports_usb_direct_future: false,
        supports_reverse_channel: false,
    }
}
