use std::process::Command;
use std::sync::{Arc, Mutex};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AdbDevice {
    pub serial: String,
    pub model: String,
    pub transport_id: String,
}

pub fn adb_devices() -> Result<Vec<AdbDevice>> {
    let output = run_adb(&["devices", "-l"])?;
    let mut devices = Vec::new();
    for line in output.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(serial) = parts.next() else {
            continue;
        };
        let Some(state) = parts.next() else {
            continue;
        };
        if state.eq_ignore_ascii_case("offline") {
            continue;
        }
        let mut model = String::new();
        let mut transport_id = String::new();
        for part in parts {
            if let Some(value) = part.strip_prefix("model:") {
                model = value.to_string();
            } else if let Some(value) = part.strip_prefix("transport_id:") {
                transport_id = value.to_string();
            }
        }
        devices.push(AdbDevice {
            serial: serial.to_string(),
            model,
            transport_id,
        });
    }
    Ok(devices)
}

pub fn setup_adb_reverse(serial: &str, device_port: u16, host_port: u16) -> Result<()> {
    run_adb(&[
        "-s",
        serial,
        "reverse",
        &format!("tcp:{device_port}"),
        &format!("tcp:{host_port}"),
    ])
    .with_context(|| {
        format!("setup adb reverse serial={serial} device_port={device_port} host_port={host_port}")
    })?;
    info!(serial, device_port, host_port, "adb reverse configured");
    Ok(())
}

pub fn teardown_adb_reverse(serial: &str, device_port: u16) -> Result<()> {
    run_adb(&[
        "-s",
        serial,
        "reverse",
        "--remove",
        &format!("tcp:{device_port}"),
    ])
    .with_context(|| format!("teardown adb reverse serial={serial} device_port={device_port}"))?;
    info!(serial, device_port, "adb reverse teardown completed");
    Ok(())
}

#[derive(Debug, Default, Clone)]
pub struct AdbReverseManager {
    inner: Arc<Mutex<Vec<(String, u16)>>>,
}

impl AdbReverseManager {
    pub fn track_reverse(&self, serial: impl Into<String>, device_port: u16) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.push((serial.into(), device_port));
        }
    }
}

impl Drop for AdbReverseManager {
    fn drop(&mut self) {
        let tracked = if let Ok(mut guard) = self.inner.lock() {
            std::mem::take(&mut *guard)
        } else {
            Vec::new()
        };
        for (serial, device_port) in tracked.into_iter().rev() {
            if let Err(err) = teardown_adb_reverse(&serial, device_port) {
                warn!(serial, device_port, error = %err, "adb reverse teardown failed");
            }
        }
    }
}

fn run_adb(args: &[&str]) -> Result<String> {
    let output = adb_command(args)
        .output()
        .with_context(|| format!("spawn adb {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow!(
            "adb command failed (status={}): {} {}",
            output.status,
            stdout.trim(),
            stderr.trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn adb_command(args: &[&str]) -> Command {
    let mut command = Command::new("adb");
    command.args(args);
    #[cfg(windows)]
    {
        // Keep adb from spawning a visible console window when invoked by the Tauri GUI app.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_adb_devices_filters_offline() {
        let sample = "\
List of devices attached
5391d451               device usb:1-4 product:foo model:Pixel_8 transport_id:7
deadbeef               offline usb:1-2 product:bar model:Old transport_id:3
";
        let mut devices = Vec::new();
        for line in sample.lines().skip(1) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut parts = trimmed.split_whitespace();
            let serial = parts.next().unwrap_or_default();
            let state = parts.next().unwrap_or_default();
            if state.eq_ignore_ascii_case("offline") {
                continue;
            }
            let mut model = String::new();
            let mut transport_id = String::new();
            for part in parts {
                if let Some(value) = part.strip_prefix("model:") {
                    model = value.to_string();
                } else if let Some(value) = part.strip_prefix("transport_id:") {
                    transport_id = value.to_string();
                }
            }
            devices.push(AdbDevice {
                serial: serial.to_string(),
                model,
                transport_id,
            });
        }

        assert_eq!(
            devices,
            vec![AdbDevice {
                serial: "5391d451".to_string(),
                model: "Pixel_8".to_string(),
                transport_id: "7".to_string(),
            }]
        );
    }
}
