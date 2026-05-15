//! Process-level CPU sampler used by the watchdog.
//!
//! The Windows implementation reads the current process's kernel + user CPU
//! time via `GetProcessTimes` and divides by elapsed wall-clock time scaled by
//! the logical processor count. On non-Windows hosts (CI, tests) the sampler
//! returns 0 so the watchdog stays in Green tier and downstream code can be
//! exercised without platform-specific gating.

use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct CpuSample {
    pub percent: f64,
    pub sampled_at: Instant,
}

#[cfg(windows)]
mod imp {
    use super::CpuSample;
    use std::sync::Mutex;
    use std::time::Instant;
    use windows::Win32::Foundation::{FILETIME, HANDLE};
    use windows::Win32::System::SystemInformation::GetSystemInfo;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    pub struct ProcessCpuSampler {
        last: Mutex<Option<Snapshot>>,
        logical_processors: u32,
    }

    #[derive(Clone, Copy)]
    struct Snapshot {
        wall_at: Instant,
        cpu_100ns: u64,
    }

    impl ProcessCpuSampler {
        pub fn new() -> Self {
            let mut info = unsafe { std::mem::zeroed() };
            unsafe { GetSystemInfo(&mut info) };
            let logical_processors = info.dwNumberOfProcessors.max(1);
            Self {
                last: Mutex::new(None),
                logical_processors,
            }
        }

        pub fn sample(&self) -> Option<CpuSample> {
            let snapshot = current_snapshot()?;
            let mut guard = self.last.lock().ok()?;
            let result = match *guard {
                None => CpuSample {
                    percent: 0.0,
                    sampled_at: snapshot.wall_at,
                },
                Some(prev) => {
                    let wall_delta = snapshot
                        .wall_at
                        .saturating_duration_since(prev.wall_at)
                        .as_secs_f64();
                    if wall_delta <= 0.0 {
                        return None;
                    }
                    let cpu_seconds =
                        (snapshot.cpu_100ns.saturating_sub(prev.cpu_100ns)) as f64 / 10_000_000.0;
                    let logical = self.logical_processors as f64;
                    let percent = ((cpu_seconds / wall_delta) / logical) * 100.0;
                    CpuSample {
                        percent: percent.clamp(0.0, 100.0),
                        sampled_at: snapshot.wall_at,
                    }
                }
            };
            *guard = Some(snapshot);
            Some(result)
        }
    }

    impl Default for ProcessCpuSampler {
        fn default() -> Self {
            Self::new()
        }
    }

    fn filetime_to_100ns(ft: FILETIME) -> u64 {
        ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
    }

    fn current_snapshot() -> Option<Snapshot> {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let process: HANDLE = unsafe { GetCurrentProcess() };
        let ok =
            unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) };
        if ok.is_err() {
            return None;
        }
        let cpu_100ns = filetime_to_100ns(kernel).saturating_add(filetime_to_100ns(user));
        Some(Snapshot {
            wall_at: Instant::now(),
            cpu_100ns,
        })
    }
}

#[cfg(not(windows))]
mod imp {
    use super::CpuSample;
    use std::time::Instant;

    pub struct ProcessCpuSampler;

    impl ProcessCpuSampler {
        pub fn new() -> Self {
            Self
        }

        pub fn sample(&self) -> Option<CpuSample> {
            Some(CpuSample {
                percent: 0.0,
                sampled_at: Instant::now(),
            })
        }
    }

    impl Default for ProcessCpuSampler {
        fn default() -> Self {
            Self::new()
        }
    }
}

pub use imp::ProcessCpuSampler;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_returns_finite_percent() {
        let sampler = ProcessCpuSampler::new();
        let _first = sampler.sample();
        // Burn a tiny bit of CPU so the second sample has a non-zero delta.
        let mut acc: u64 = 0;
        for i in 0..5_000 {
            acc = acc.wrapping_add(i);
        }
        std::hint::black_box(acc);
        if let Some(sample) = sampler.sample() {
            assert!(sample.percent.is_finite());
            assert!((0.0..=100.0).contains(&sample.percent));
        }
    }
}
