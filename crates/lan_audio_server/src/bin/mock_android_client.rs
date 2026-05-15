use std::net::SocketAddr;
use std::time::{Duration, Instant};

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use lan_audio_protocol::{
    detect_data_plane_packet_kind, AudioMode, ClientControlMessage, ClientInfo, ControlMessageV2,
    DataPlanePacketKind, DiscoveryBeacon, Hello, ProtocolCapabilities, SetAudioMode,
    UdpAudioCodecV2, UdpAudioPacket, UdpAudioPacketV2, DISCOVERY_PORT, PROTOCOL_VERSION_V2,
    UDP_FLAG_V2_CONFIG_CHANGED, UDP_FLAG_V2_DISCONTINUITY,
};
use tokio::net::UdpSocket;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,mock_android_client=debug".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    let supports_opus = parse_supports_opus_from_args(&args);
    let target = parse_target_from_args(&args);
    let (server_host, ws_port) = if let Some(target) = target {
        println!("using direct target {}:{}", target.host, target.ws_port);
        (target.host, target.ws_port)
    } else {
        let (beacon, server_addr) = wait_for_beacon().await?;
        println!(
            "discovered server {} ({})",
            beacon.server_name, beacon.server_id
        );
        (server_addr.ip().to_string(), beacon.ws_port)
    };

    let udp_socket = UdpSocket::bind("0.0.0.0:0").await?;
    let udp_port = udp_socket.local_addr()?.port();

    let ws_url = format!("ws://{}:{}/", server_host, ws_port);
    let (ws_stream, _) = connect_async(&ws_url).await.context("connect ws")?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    let v2_hello = ControlMessageV2::Hello(Hello {
        protocol_version: PROTOCOL_VERSION_V2,
        device_name: "mock-android".to_string(),
        client_id: format!("mock-{}", now_ms()),
        udp_port,
        desired_sample_rate: 48_000,
        channels: 2,
        capabilities: ProtocolCapabilities {
            supports_pcm16: true,
            supports_f32: false,
            supports_modes: true,
            supports_metrics: true,
            supports_opus_future: supports_opus,
            supports_opus,
            supports_opus_experimental: supports_opus,
            supports_low_latency: true,
            supports_high_quality: true,
            supports_native_audio_track: true,
            supports_fast_path: true,
            supports_stable_audio_track: true,
            supports_usb_tethering: true,
            supports_usb_direct_future: false,
            supports_reverse_channel: false,
            supports_hires_pcm24: false,
        },
        preferred_audio_mode: AudioMode::Balanced,
    });
    ws_tx
        .send(Message::Text(serde_json::to_string(&v2_hello)?))
        .await?;
    ws_tx
        .send(Message::Text(serde_json::to_string(
            &ControlMessageV2::ClientInfo(ClientInfo {
                client_name: "mock-android".to_string(),
                platform: "mock".to_string(),
                app_version: "local".to_string(),
                udp_port,
            }),
        )?))
        .await?;

    let mut ping_seq = 0_u64;
    let mut ping_interval = tokio::time::interval(Duration::from_secs(1));

    let mut buf = vec![0_u8; 4096];
    let mut rx_packets = 0_u64;
    let mut rx_bytes = 0_u64;
    let mut last_seq: Option<u32> = None;
    let mut losses = 0_u64;
    let mut rx_v1 = 0_u64;
    let mut rx_v2 = 0_u64;
    let mut rx_pcm16 = 0_u64;
    let mut rx_opus = 0_u64;
    let mut rx_config_changed = 0_u64;
    let mut rx_discontinuity = 0_u64;
    let mut window_start = Instant::now();
    let mut mode_switch_step = 0_u8;

    loop {
        tokio::select! {
            ws = ws_rx.next() => {
                if let Some(Ok(Message::Text(text))) = ws {
                    println!("ws: {text}");
                    if let Ok(ControlMessageV2::HelloAck(ack)) = serde_json::from_str::<ControlMessageV2>(&text) {
                        println!(
                            "hello_ack: protocol_version={} mode={:?} capabilities={:?}",
                            ack.protocol_version, ack.current_audio_mode, ack.capabilities
                        );
                    }
                }
            }
            udp = udp_socket.recv_from(&mut buf) => {
                let (n, _from): (usize, SocketAddr) = udp?;
                rx_packets += 1;
                rx_bytes += n as u64;

                match detect_data_plane_packet_kind(&buf[..n]) {
                    DataPlanePacketKind::LegacyLas1 => {
                        if let Ok(packet) = UdpAudioPacket::decode(&buf[..n]) {
                            rx_v1 += 1;
                            rx_pcm16 += 1;
                            if let Some(prev) = last_seq {
                                if packet.sequence != prev.wrapping_add(1) {
                                    losses += packet.sequence.wrapping_sub(prev.wrapping_add(1)) as u64;
                                }
                            }
                            last_seq = Some(packet.sequence);
                        }
                    }
                    DataPlanePacketKind::V2Lav2 => {
                        if let Ok(packet) = UdpAudioPacketV2::decode(&buf[..n]) {
                            rx_v2 += 1;
                            match packet.header.codec {
                                UdpAudioCodecV2::Opus => rx_opus += 1,
                                UdpAudioCodecV2::Pcm16 => rx_pcm16 += 1,
                                UdpAudioCodecV2::F32 => {}
                                UdpAudioCodecV2::Pcm24 => {}
                            }
                            if packet.header.flags & UDP_FLAG_V2_CONFIG_CHANGED != 0 {
                                rx_config_changed += 1;
                            }
                            if packet.header.flags & UDP_FLAG_V2_DISCONTINUITY != 0 {
                                rx_discontinuity += 1;
                            }
                            if let Some(prev) = last_seq {
                                if packet.header.sequence != prev.wrapping_add(1) {
                                    losses += packet.header.sequence.wrapping_sub(prev.wrapping_add(1)) as u64;
                                }
                            }
                            last_seq = Some(packet.header.sequence);
                        }
                    }
                    DataPlanePacketKind::Unknown => {}
                }
            }
            _ = ping_interval.tick() => {
                let ping = ClientControlMessage::ClientPing {
                    seq: ping_seq,
                    ts_unix_ms: now_ms(),
                };
                if let Ok(msg) = serde_json::to_string(&ping) {
                    let _ = ws_tx.send(Message::Text(msg)).await;
                }
                ping_seq = ping_seq.wrapping_add(1);
            }
        }

        if window_start.elapsed() >= Duration::from_secs(1) {
            println!(
                "udp stats: packets={} bytes={} v1={} v2={} pcm16={} opus={} losses={} cfg_changed={} discontinuity={} last_seq={:?}",
                rx_packets, rx_bytes, rx_v1, rx_v2, rx_pcm16, rx_opus, losses, rx_config_changed, rx_discontinuity, last_seq
            );
            mode_switch_step = mode_switch_step.saturating_add(1);
            if mode_switch_step == 3 {
                let _ = ws_tx
                    .send(Message::Text(serde_json::to_string(
                        &ControlMessageV2::SetAudioMode(SetAudioMode {
                            mode: AudioMode::LowLatency,
                            reason: "mock_validation".to_string(),
                            preferred_sample_rate: Some(48_000),
                            preferred_codec: None,
                        }),
                    )?))
                    .await;
                println!("sent set_audio_mode=low_latency");
            } else if mode_switch_step == 6 {
                let _ = ws_tx
                    .send(Message::Text(serde_json::to_string(
                        &ControlMessageV2::SetAudioMode(SetAudioMode {
                            mode: AudioMode::HighQuality,
                            reason: "mock_validation".to_string(),
                            preferred_sample_rate: Some(48_000),
                            preferred_codec: None,
                        }),
                    )?))
                    .await;
                println!("sent set_audio_mode=high_quality");
            } else if mode_switch_step == 9 {
                let _ = ws_tx
                    .send(Message::Text(serde_json::to_string(
                        &ControlMessageV2::SetAudioMode(SetAudioMode {
                            mode: AudioMode::Balanced,
                            reason: "mock_validation".to_string(),
                            preferred_sample_rate: Some(48_000),
                            preferred_codec: None,
                        }),
                    )?))
                    .await;
                println!("sent set_audio_mode=balanced");
            } else if mode_switch_step >= 12 {
                break;
            }
            window_start = Instant::now();
        }
    }

    Ok(())
}

#[derive(Debug)]
struct DirectTarget {
    host: String,
    ws_port: u16,
}

fn parse_target_from_args(args: &[String]) -> Option<DirectTarget> {
    let mut host: Option<String> = None;
    let mut ws_port: u16 = 39991;
    let mut idx = 1;
    while idx < args.len() {
        match args[idx].as_str() {
            "--host" => {
                if idx + 1 < args.len() {
                    host = Some(args[idx + 1].clone());
                    idx += 1;
                }
            }
            "--ws-port" => {
                if idx + 1 < args.len() {
                    if let Ok(port) = args[idx + 1].parse::<u16>() {
                        ws_port = port;
                    }
                    idx += 1;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    host.map(|h| DirectTarget { host: h, ws_port })
}

fn parse_supports_opus_from_args(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--supports-opus")
}

async fn wait_for_beacon() -> anyhow::Result<(DiscoveryBeacon, SocketAddr)> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{DISCOVERY_PORT}")).await?;
    let mut buf = vec![0_u8; 2048];
    loop {
        let (n, from) = socket.recv_from(&mut buf).await?;
        if let Ok(beacon) = serde_json::from_slice::<DiscoveryBeacon>(&buf[..n]) {
            return Ok((beacon, from));
        }
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
