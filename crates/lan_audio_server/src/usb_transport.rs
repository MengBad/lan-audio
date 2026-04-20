use std::process::Command;
use std::sync::{Arc, Mutex};

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

pub fn setup_adb_forward(serial: &str, host_port: u16, device_port: u16) -> Result<()> {
    run_adb(&[
        "-s",
        serial,
        "forward",
        &format!("tcp:{host_port}"),
        &format!("tcp:{device_port}"),
    ])
    .with_context(|| {
        format!("setup adb forward serial={serial} host_port={host_port} device_port={device_port}")
    })?;
    info!(serial, host_port, device_port, "adb forward configured");
    Ok(())
}

pub fn teardown_adb_forward(serial: &str, host_port: u16) -> Result<()> {
    run_adb(&[
        "-s",
        serial,
        "forward",
        "--remove",
        &format!("tcp:{host_port}"),
    ])
    .with_context(|| format!("teardown adb forward serial={serial} host_port={host_port}"))?;
    info!(serial, host_port, "adb forward teardown completed");
    Ok(())
}

#[derive(Debug, Default, Clone)]
pub struct AdbForwardManager {
    inner: Arc<Mutex<Vec<(String, u16)>>>,
}

impl AdbForwardManager {
    pub fn track_forward(&self, serial: impl Into<String>, host_port: u16) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.push((serial.into(), host_port));
        }
    }
}

impl Drop for AdbForwardManager {
    fn drop(&mut self) {
        let tracked = if let Ok(mut guard) = self.inner.lock() {
            std::mem::take(&mut *guard)
        } else {
            Vec::new()
        };
        for (serial, host_port) in tracked.into_iter().rev() {
            if let Err(err) = teardown_adb_forward(&serial, host_port) {
                warn!(serial, host_port, error = %err, "adb forward teardown failed");
            }
        }
    }
}

fn run_adb(args: &[&str]) -> Result<String> {
    let output = Command::new("adb")
        .args(args)
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
