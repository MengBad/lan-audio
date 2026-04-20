use lan_audio_server::config::ServerConfig;
use lan_audio_server::service::LanAudioService;

fn print_help() {
    println!(
        "\
desktop_headless

Recommended default path:
  --audio-source windows_loopback --data-plane v2_header --codec opus

USB mode:
  --transport usb --adb-serial <serial>

Rollback path:
  --audio-source windows_loopback --data-plane legacy_las1 --codec pcm16

Common options:
  --audio-source <windows_loopback|synthetic>
  --data-plane <v2_header|legacy_las1>
  --codec <opus|pcm16>
  --transport <wifi|usb>
  --adb-serial <serial>
  --audio-mode <low_latency|balanced|high_quality>
  --no-audio-fallback
  --capture-dump-wav
  --capture-dump-seconds <n>
  --capture-dump-dir <dir>
"
    );
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,lan_audio_server=debug".into()),
        )
        .init();

    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    let mut cfg = ServerConfig::default();
    cfg.apply_args(std::env::args().skip(1))?;

    let service = LanAudioService::new(cfg).await?;
    service.run_until_shutdown().await
}
