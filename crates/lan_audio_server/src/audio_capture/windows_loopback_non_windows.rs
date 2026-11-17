use async_trait::async_trait;

use super::{
    AudioCaptureSource, AudioFormat, AudioFrame, CaptureDebugDumpConfig, CaptureError,
    CaptureSourceState, DeviceChangeStatus,
};

/// Non-Windows placeholder implementation.
#[derive(Debug)]
pub struct WindowsLoopbackCapture {
    state: CaptureSourceState,
}

impl WindowsLoopbackCapture {
    pub fn new_default_output(
        _format: AudioFormat,
        _debug_cfg: CaptureDebugDumpConfig,
    ) -> Result<Self, CaptureError> {
        Err(CaptureError::UnsupportedPlatform(
            "windows_loopback requires Windows".to_string(),
        ))
    }
}

#[async_trait]
impl AudioCaptureSource for WindowsLoopbackCapture {
    async fn start(&mut self) -> Result<(), CaptureError> {
        self.state = CaptureSourceState::Failed;
        Err(CaptureError::UnsupportedPlatform(
            "windows_loopback requires Windows".to_string(),
        ))
    }

    async fn read_frame(&mut self) -> Result<AudioFrame, CaptureError> {
        Err(CaptureError::UnsupportedPlatform(
            "windows_loopback requires Windows".to_string(),
        ))
    }

    async fn stop(&mut self) -> Result<(), CaptureError> {
        self.state = CaptureSourceState::Stopped;
        Ok(())
    }

    fn format(&self) -> AudioFormat {
        AudioFormat::default()
    }

    fn state(&self) -> CaptureSourceState {
        self.state
    }

    fn source_name(&self) -> &'static str {
        "windows_loopback"
    }

    fn device_change_status(&self) -> DeviceChangeStatus {
        DeviceChangeStatus::None
    }

    fn clear_device_change_status(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_windows_loopback_returns_unsupported() {
        let err = WindowsLoopbackCapture::new_default_output(
            AudioFormat::default(),
            CaptureDebugDumpConfig {
                enabled: false,
                seconds: 1,
                output_dir: "x".to_string(),
            },
        )
        .expect_err("must be unsupported");
        assert!(matches!(err, CaptureError::UnsupportedPlatform(_)));
    }

    #[test]
    fn non_windows_hotplug_status_is_noop() {
        let source = WindowsLoopbackCapture {
            state: CaptureSourceState::Created,
        };
        assert_eq!(source.device_change_status(), DeviceChangeStatus::None);
        source.clear_device_change_status();
        assert_eq!(source.device_change_status(), DeviceChangeStatus::None);
    }
}
