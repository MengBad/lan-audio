# Protocol Spec (MVP)

## 1) Discovery (UDP Broadcast)

端口：`39990`（默认，可配置）

JSON (UTF-8):

```json
{
  "type": "lan_audio_discovery_v1",
  "server_id": "uuid",
  "server_name": "windows-desktop",
  "ws_port": 39991,
  "udp_port": 39992,
  "ts_unix_ms": 1710000000000
}
```

## 2) Control Session (WebSocket JSON)

### Client -> Server

- `client_hello`
- `client_ping`

### Server -> Client

- `server_welcome`
- `server_pong`
- `server_metrics`
- `server_error`

示例：

```json
{
  "type": "client_hello",
  "client_name": "pixel-7",
  "udp_port": 54000,
  "desired_sample_rate": 48000,
  "channels": 2
}
```

```json
{
  "type": "server_welcome",
  "session_id": "uuid",
  "codec": "opus",
  "sample_rate": 48000,
  "channels": 2,
  "frames_per_packet": 480
}
```

## 3) Audio Transport (UDP Binary)

`LAS1` 头格式（小端）：

- magic: 4 bytes = `LAS1`
- version: u8
- flags: u8
- sequence: u32
- timestamp_ms: u64
- sample_rate: u32
- channels: u8
- frames_per_packet: u16
- payload_len: u16
- payload: `[u8; payload_len]`

> MVP 目前 payload 为 passthrough 调试数据（`TODO`: 接真实 Opus 编码输出）。
