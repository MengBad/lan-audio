//! Thin wrapper around Windows MMCSS (Multimedia Class Scheduler Service).
//!
//! On Windows, audio-critical threads should be registered with MMCSS so the
//! scheduler boosts them above background work and protects against the
//! scheduler-noise that causes occasional XRuns. This module provides a small
//! RAII handle that registers a thread with one of the standard task names
//! and unregisters it on drop.
//!
//! On non-Windows targets (used during tests on Linux/macOS CI) the wrapper
//! is a no-op so the higher-level call sites can stay platform-agnostic.

/// Standard MMCSS task names. Choose the one that best matches the workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmcssTask {
    /// Sample-accurate, lowest tolerance for jitter. Use for the audio
    /// capture / playback thread.
    ProAudio,
    /// Real-time audio that can tolerate slightly more jitter than ProAudio.
    /// Use for the encoding thread.
    Audio,
    /// Real-time but with slightly looser scheduling guarantees. Use for the
    /// network sender thread.
    Games,
}

impl MmcssTask {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProAudio => "Pro Audio",
            Self::Audio => "Audio",
            Self::Games => "Games",
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::MmcssTask;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::{
        AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW,
    };

    pub struct ThreadPriorityHandle {
        handle: HANDLE,
    }

    impl ThreadPriorityHandle {
        pub fn register(task: MmcssTask) -> std::io::Result<Self> {
            let wide: Vec<u16> = OsStr::new(task.as_str())
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut task_index: u32 = 0;
            let handle =
                unsafe { AvSetMmThreadCharacteristicsW(PCWSTR(wide.as_ptr()), &mut task_index) }
                    .map_err(|err| {
                        std::io::Error::other(format!("AvSetMmThreadCharacteristicsW: {err}"))
                    })?;
            if handle.is_invalid() {
                return Err(std::io::Error::other(
                    "AvSetMmThreadCharacteristicsW returned invalid handle",
                ));
            }
            Ok(Self { handle })
        }
    }

    impl Drop for ThreadPriorityHandle {
        fn drop(&mut self) {
            if !self.handle.is_invalid() {
                unsafe {
                    let _ = AvRevertMmThreadCharacteristics(self.handle);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::MmcssTask;

    pub struct ThreadPriorityHandle;

    impl ThreadPriorityHandle {
        pub fn register(_task: MmcssTask) -> std::io::Result<Self> {
            // Non-Windows: no-op so call sites can remain platform-agnostic.
            Ok(Self)
        }
    }
}

pub use imp::ThreadPriorityHandle;

/// Convenience helper: register the current thread for `task`. The returned
/// handle keeps the registration alive for as long as it is in scope. Errors
/// are intentionally swallowed and turned into None — losing the priority
/// boost is preferable to crashing the audio thread.
pub fn boost_current_thread(task: MmcssTask) -> Option<ThreadPriorityHandle> {
    match ThreadPriorityHandle::register(task) {
        Ok(handle) => Some(handle),
        Err(err) => {
            tracing::warn!(
                target: "lan_audio_server::thread_priority",
                task = task.as_str(),
                error = %err,
                "MMCSS registration failed; continuing at default priority"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_name_strings_are_stable() {
        assert_eq!(MmcssTask::ProAudio.as_str(), "Pro Audio");
        assert_eq!(MmcssTask::Audio.as_str(), "Audio");
        assert_eq!(MmcssTask::Games.as_str(), "Games");
    }

    #[test]
    fn boost_returns_handle_on_supported_platform() {
        // On non-Windows this is a no-op success.
        // On Windows test runners this may fail if MMCSS service isn't
        // available, so we don't assert on the inner value.
        let _h = boost_current_thread(MmcssTask::Audio);
    }
}
