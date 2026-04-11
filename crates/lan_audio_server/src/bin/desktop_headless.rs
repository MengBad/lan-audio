use lan_audio_server::config::ServerConfig;
use lan_audio_server::service::LanAudioService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,lan_audio_server=debug".into()),
        )
        .init();

    let mut cfg = ServerConfig::default();
    cfg.apply_args(std::env::args().skip(1))?;

    let service = LanAudioService::new(cfg).await?;
    service.run_until_shutdown().await
}
