use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Debug, Default)]
pub struct Metrics {
    tx_packets: AtomicU64,
    tx_bytes: AtomicU64,
    active_sessions: AtomicU64,
    capture_frames_produced: AtomicU64,
    capture_read_errors: AtomicU64,
    capture_underruns: AtomicU64,
    capture_start_attempts: AtomicU64,
    capture_start_failures: AtomicU64,
    capture_silent_frames: AtomicU64,
    capture_non_silent_frames: AtomicU64,
    capture_no_packet_count: AtomicU64,
    last_capture_pts_ms: AtomicU64,
    capture_sample_rate: AtomicU64,
    capture_channels: AtomicU64,
    capture_buffer_frames: AtomicU64,
    capture_last_peak: RwLock<f32>,
    capture_last_rms: RwLock<f32>,
    current_audio_source: RwLock<String>,
    capture_source_state: RwLock<String>,
    capture_device_name: RwLock<String>,
    recent_clients: RwLock<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricsSnapshot {
    pub tx_packets: u64,
    pub tx_bytes: u64,
    pub active_sessions: u64,
    pub capture_frames_produced: u64,
    pub capture_read_errors: u64,
    pub capture_underruns: u64,
    pub capture_start_attempts: u64,
    pub capture_start_failures: u64,
    pub capture_silent_frames: u64,
    pub capture_non_silent_frames: u64,
    pub capture_no_packet_count: u64,
    pub current_audio_source: String,
    pub capture_source_state: String,
    pub capture_device_name: String,
    pub capture_sample_rate: u64,
    pub capture_channels: u64,
    pub capture_buffer_frames: u64,
    pub capture_last_peak: f32,
    pub capture_last_rms: f32,
    pub last_capture_pts_ms: u64,
    pub recent_clients: Vec<String>,
}

impl Metrics {
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn inc_packets(&self, packet_bytes: usize) {
        self.tx_packets.fetch_add(1, Ordering::Relaxed);
        self.tx_bytes
            .fetch_add(packet_bytes as u64, Ordering::Relaxed);
    }

    pub fn inc_sessions(&self) {
        self.active_sessions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_sessions(&self) {
        self.active_sessions.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn inc_capture_frames_produced(&self) {
        self.capture_frames_produced.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_capture_read_errors(&self) {
        self.capture_read_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_capture_underruns(&self) {
        self.capture_underruns.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_capture_start_attempts(&self) {
        self.capture_start_attempts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_capture_start_failures(&self) {
        self.capture_start_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_capture_silent_frames(&self) {
        self.capture_silent_frames.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_capture_non_silent_frames(&self) {
        self.capture_non_silent_frames
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_capture_no_packet_count(&self) {
        self.capture_no_packet_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_current_audio_source(&self, source: impl Into<String>) {
        if let Ok(mut lock) = self.current_audio_source.write() {
            *lock = source.into();
        }
    }

    pub fn set_capture_source_state(&self, state: impl Into<String>) {
        if let Ok(mut lock) = self.capture_source_state.write() {
            *lock = state.into();
        }
    }

    pub fn set_capture_device_name(&self, name: impl Into<String>) {
        if let Ok(mut lock) = self.capture_device_name.write() {
            *lock = name.into();
        }
    }

    pub fn set_capture_format(&self, sample_rate: u32, channels: u16) {
        self.capture_sample_rate
            .store(sample_rate as u64, Ordering::Relaxed);
        self.capture_channels
            .store(channels as u64, Ordering::Relaxed);
    }

    pub fn set_capture_buffer_frames(&self, frames: u32) {
        self.capture_buffer_frames
            .store(frames as u64, Ordering::Relaxed);
    }

    pub fn set_capture_level(&self, peak: f32, rms: f32) {
        if let Ok(mut p) = self.capture_last_peak.write() {
            *p = peak;
        }
        if let Ok(mut r) = self.capture_last_rms.write() {
            *r = rms;
        }
    }

    pub fn set_last_capture_pts_ms(&self, pts_ms: u64) {
        self.last_capture_pts_ms.store(pts_ms, Ordering::Relaxed);
    }

    pub fn note_client_connected(&self, client_name: &str, client_ip: &str) {
        let normalized_name = if client_name.trim().is_empty() {
            "Unknown Client"
        } else {
            client_name.trim()
        };
        let normalized_ip = client_ip.trim();
        let label = if normalized_ip.is_empty() {
            normalized_name.to_string()
        } else {
            format!("{normalized_name} ({normalized_ip})")
        };
        if let Ok(mut lock) = self.recent_clients.write() {
            lock.retain(|v| v != &label);
            lock.insert(0, label);
            lock.truncate(8);
        }
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let current_audio_source = self
            .current_audio_source
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "unknown".to_string());
        let capture_source_state = self
            .capture_source_state
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "unknown".to_string());
        let capture_device_name = self
            .capture_device_name
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "unknown".to_string());
        let capture_last_peak = self.capture_last_peak.read().map(|v| *v).unwrap_or(0.0);
        let capture_last_rms = self.capture_last_rms.read().map(|v| *v).unwrap_or(0.0);
        let recent_clients = self
            .recent_clients
            .read()
            .map(|v| v.clone())
            .unwrap_or_default();

        MetricsSnapshot {
            tx_packets: self.tx_packets.load(Ordering::Relaxed),
            tx_bytes: self.tx_bytes.load(Ordering::Relaxed),
            active_sessions: self.active_sessions.load(Ordering::Relaxed),
            capture_frames_produced: self.capture_frames_produced.load(Ordering::Relaxed),
            capture_read_errors: self.capture_read_errors.load(Ordering::Relaxed),
            capture_underruns: self.capture_underruns.load(Ordering::Relaxed),
            capture_start_attempts: self.capture_start_attempts.load(Ordering::Relaxed),
            capture_start_failures: self.capture_start_failures.load(Ordering::Relaxed),
            capture_silent_frames: self.capture_silent_frames.load(Ordering::Relaxed),
            capture_non_silent_frames: self.capture_non_silent_frames.load(Ordering::Relaxed),
            capture_no_packet_count: self.capture_no_packet_count.load(Ordering::Relaxed),
            current_audio_source,
            capture_source_state,
            capture_device_name,
            capture_sample_rate: self.capture_sample_rate.load(Ordering::Relaxed),
            capture_channels: self.capture_channels.load(Ordering::Relaxed),
            capture_buffer_frames: self.capture_buffer_frames.load(Ordering::Relaxed),
            capture_last_peak,
            capture_last_rms,
            last_capture_pts_ms: self.last_capture_pts_ms.load(Ordering::Relaxed),
            recent_clients,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_snapshot_changes() {
        let m = Metrics::default();
        m.set_current_audio_source("synthetic");
        m.set_capture_source_state("started");
        m.set_capture_device_name("device-id");
        m.set_capture_format(48_000, 2);
        m.set_capture_buffer_frames(960);
        m.set_capture_level(0.8, 0.2);
        m.inc_sessions();
        m.inc_packets(100);
        m.inc_packets(200);
        m.inc_capture_frames_produced();
        m.inc_capture_read_errors();
        m.inc_capture_underruns();
        m.inc_capture_start_attempts();
        m.inc_capture_start_failures();
        m.inc_capture_silent_frames();
        m.inc_capture_non_silent_frames();
        m.inc_capture_no_packet_count();
        m.set_last_capture_pts_ms(1234);
        m.note_client_connected("Pixel 8", "192.168.1.10");
        let s = m.snapshot();
        assert_eq!(s.active_sessions, 1);
        assert_eq!(s.tx_packets, 2);
        assert_eq!(s.tx_bytes, 300);
        assert_eq!(s.capture_frames_produced, 1);
        assert_eq!(s.capture_read_errors, 1);
        assert_eq!(s.capture_underruns, 1);
        assert_eq!(s.capture_start_attempts, 1);
        assert_eq!(s.capture_start_failures, 1);
        assert_eq!(s.capture_silent_frames, 1);
        assert_eq!(s.capture_non_silent_frames, 1);
        assert_eq!(s.capture_no_packet_count, 1);
        assert_eq!(s.current_audio_source, "synthetic");
        assert_eq!(s.capture_source_state, "started");
        assert_eq!(s.capture_device_name, "device-id");
        assert_eq!(s.capture_sample_rate, 48_000);
        assert_eq!(s.capture_channels, 2);
        assert_eq!(s.capture_buffer_frames, 960);
        assert!((s.capture_last_peak - 0.8).abs() < 1e-6);
        assert!((s.capture_last_rms - 0.2).abs() < 1e-6);
        assert_eq!(s.last_capture_pts_ms, 1234);
        assert_eq!(s.recent_clients, vec!["Pixel 8 (192.168.1.10)"]);
    }
}
