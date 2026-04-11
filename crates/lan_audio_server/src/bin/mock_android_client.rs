use std::net::SocketAddr;
use std::time::{Duration, Instant};

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use lan_audio_protocol::{ClientControlMessage, DiscoveryBeacon, UdpAudioPacket, DISCOVERY_PORT};
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

    let (beacon, server_addr) = wait_for_beacon().await?;
    println!("discovered server {} ({})", beacon.server_name, beacon.server_id);

    let udp_socket = UdpSocket::bind("0.0.0.0:0").await?;
    let udp_port = udp_socket.local_addr()?.port();

    let ws_url = format!("ws://{}:{}/", server_addr.ip(), beacon.ws_port);
    let (ws_stream, _) = connect_async(&ws_url).await.context("connect ws")?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    let hello = ClientControlMessage::ClientHello {
        client_name: "mock-android".to_string(),
        udp_port,
        desired_sample_rate: 48_000,
        channels: 2,
    };
    ws_tx
        .send(Message::Text(serde_json::to_string(&hello)?.into()))
        .await?;

    if let Some(Ok(Message::Text(text))) = ws_rx.next().await {
        println!("welcome: {text}");
    }

    let ping_task = tokio::spawn(async move {
        let mut seq = 0_u64;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let ping = ClientControlMessage::ClientPing {
                seq,
                ts_unix_ms: now_ms(),
            };
            match serde_json::to_string(&ping) {
                Ok(msg) => {
                    if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    eprintln!("serialize ping failed: {err}");
                    break;
                }
            }
            seq = seq.wrapping_add(1);
        }
    });

    let mut buf = vec![0_u8; 1500];
    let mut rx_packets = 0_u64;
    let mut rx_bytes = 0_u64;
    let mut last_seq = None;
    let mut losses = 0_u64;
    let mut window_start = Instant::now();

    loop {
        let (n, _from): (usize, SocketAddr) = udp_socket.recv_from(&mut buf).await?;
        if let Ok(packet) = UdpAudioPacket::decode(&buf[..n]) {
            rx_packets += 1;
            rx_bytes += n as u64;
            if let Some(prev) = last_seq {
                if packet.sequence != prev.wrapping_add(1) {
                    losses += packet.sequence.wrapping_sub(prev.wrapping_add(1)) as u64;
                }
            }
            last_seq = Some(packet.sequence);
        }

        if window_start.elapsed() >= Duration::from_secs(1) {
            println!(
                "udp stats: packets={} bytes={} losses={} last_seq={:?}",
                rx_packets, rx_bytes, losses, last_seq
            );
            window_start = Instant::now();
        }

        if ping_task.is_finished() {
            break;
        }
    }

    Ok(())
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
