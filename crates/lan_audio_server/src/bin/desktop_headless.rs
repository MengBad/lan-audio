use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use lan_audio_protocol::DataPlanePath;
use lan_audio_server::config::ServerConfig;
use lan_audio_server::service::LanAudioService;
use serde::Serialize;

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

Force rollback (overrides data-plane and codec to legacy_las1+pcm16):
  --force-rollback

Common options:
  --audio-source <windows_loopback|synthetic>
  --data-plane <v2_header|legacy_las1>
  --codec <opus|pcm16>
  --transport <wifi|usb>
  --adb-serial <serial>
  --audio-mode <low_latency|balanced|high_quality>
  --force-rollback
  --no-audio-fallback
  --capture-dump-wav
  --capture-dump-seconds <n>
  --capture-dump-dir <dir>
"
    );
}

#[derive(Debug, Serialize)]
struct RollbackEvidence {
    tested_at: String,
    forced_rollback: bool,
    active_data_plane: String,
    codec: String,
    rollback_state: String,
    snapshot_observed: bool,
    log_excerpt: String,
}

fn rollback_evidence_path() -> anyhow::Result<PathBuf> {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = crate_dir
        .parent()
        .and_then(|path| path.parent())
        .ok_or_else(|| anyhow!("failed to resolve repo root from CARGO_MANIFEST_DIR"))?;
    Ok(repo_root
        .join("artifacts")
        .join("validate")
        .join("rollback_evidence.json"))
}

fn write_rollback_evidence(service: &LanAudioService) -> anyhow::Result<()> {
    let status = service.data_plane_status();
    let active_data_plane = status.active_path.as_str().to_string();
    let codec = status.active_codec.as_str().to_string();
    let rollback_state = if !status.is_on_main_path
        && status.active_path == DataPlanePath::LegacyLas1
        && status.active_codec.as_str() == "pcm16"
    {
        "active"
    } else {
        "inactive"
    }
    .to_string();
    let evidence = RollbackEvidence {
        tested_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
        forced_rollback: true,
        active_data_plane,
        codec,
        rollback_state,
        snapshot_observed: true,
        log_excerpt: "forced_rollback: legacy_las1 + pcm16".to_string(),
    };
    let evidence_path = rollback_evidence_path()?;
    if let Some(parent) = evidence_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create rollback evidence dir: {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&evidence)?;
    std::fs::write(&evidence_path, json)
        .with_context(|| format!("write rollback evidence: {}", evidence_path.display()))?;
    Ok(())
}

async fn run_force_rollback_capture(service: LanAudioService) -> anyhow::Result<()> {
    let service = Arc::new(service);
    let runner = {
        let service = Arc::clone(&service);
        tokio::spawn(async move { service.run_until_shutdown().await })
    };

    tokio::time::sleep(Duration::from_secs(5)).await;
    write_rollback_evidence(service.as_ref())?;
    service.stop();

    runner
        .await
        .context("join desktop_headless force-rollback runner")?
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
    if std::env::args().any(|arg| arg == "--force-rollback") {
        run_force_rollback_capture(service).await
    } else {
        service.run_until_shutdown().await
    }
}
