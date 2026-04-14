use std::fs::{self, File};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::ptr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::task::yield_now;
use tracing::{debug, info, warn};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
};

use super::pcm_accumulator::{PacketKind, PcmFrameAccumulator};
use super::{
    AudioCaptureSource, AudioFormat, AudioFrame, CaptureDebugDumpConfig, CaptureError,
    CaptureSourceState, SampleFormat,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceSampleKind {
    F32,
    I16,
}

#[derive(Debug, Clone)]
struct DeviceFormatInfo {
    sample_rate_hz: u32,
    channels: u16,
    sample_kind: DeviceSampleKind,
    bits_per_sample: u16,
    format_tag: u16,
    sub_format: Option<String>,
}

#[derive(Debug)]
struct CaptureStatsWindow {
    started_at: Instant,
    last_callback_at: Option<Instant>,
    callbacks: u64,
    silent_callbacks: u64,
    non_silent_callbacks: u64,
    total_callback_frames: u64,
    emitted_frames: u64,
    no_packet_polls: u64,
    total_samples: u64,
    non_zero_samples: u64,
    sum_sq: f64,
    peak: f32,
    interval_count: u64,
    interval_total_ms: f64,
    interval_max_ms: f64,
    last_accumulator_buffered_samples: usize,
}

impl CaptureStatsWindow {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            last_callback_at: None,
            callbacks: 0,
            silent_callbacks: 0,
            non_silent_callbacks: 0,
            total_callback_frames: 0,
            emitted_frames: 0,
            no_packet_polls: 0,
            total_samples: 0,
            non_zero_samples: 0,
            sum_sq: 0.0,
            peak: 0.0,
            interval_count: 0,
            interval_total_ms: 0.0,
            interval_max_ms: 0.0,
            last_accumulator_buffered_samples: 0,
        }
    }

    fn record_silent_callback(
        &mut self,
        frames: u32,
        sample_count: usize,
        accumulator_buffered_samples: usize,
    ) {
        self.record_callback(
            frames,
            sample_count as u64,
            0,
            0.0,
            0.0,
            true,
            accumulator_buffered_samples,
        );
    }

    fn record_pcm_callback(
        &mut self,
        frames: u32,
        samples: &[f32],
        accumulator_buffered_samples: usize,
    ) {
        let mut peak = 0.0_f32;
        let mut sum_sq = 0.0_f64;
        let mut non_zero_samples = 0_u64;

        for sample in samples {
            let abs = sample.abs();
            if abs > peak {
                peak = abs;
            }
            if abs > 1e-6 {
                non_zero_samples += 1;
            }
            let sample64 = *sample as f64;
            sum_sq += sample64 * sample64;
        }

        self.record_callback(
            frames,
            samples.len() as u64,
            non_zero_samples,
            peak,
            sum_sq,
            false,
            accumulator_buffered_samples,
        );
    }

    fn record_no_packet_poll(&mut self, accumulator_buffered_samples: usize) {
        self.no_packet_polls += 1;
        self.last_accumulator_buffered_samples = accumulator_buffered_samples;
    }

    fn record_emitted_frame(&mut self, accumulator_buffered_samples: usize) {
        self.emitted_frames += 1;
        self.last_accumulator_buffered_samples = accumulator_buffered_samples;
    }

    fn maybe_log(&mut self, source_buffer_frames: u32) {
        let elapsed = self.started_at.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }

        let elapsed_secs = elapsed.as_secs_f64().max(f64::EPSILON);
        let callbacks_per_sec = self.callbacks as f64 / elapsed_secs;
        let avg_frames_per_callback = if self.callbacks == 0 {
            0.0
        } else {
            self.total_callback_frames as f64 / self.callbacks as f64
        };
        let frames_per_sec = self.total_callback_frames as f64 / elapsed_secs;
        let emitted_frames_per_sec = self.emitted_frames as f64 / elapsed_secs;
        let rms = if self.total_samples == 0 {
            0.0
        } else {
            (self.sum_sq / self.total_samples as f64).sqrt()
        };
        let non_zero_sample_ratio = if self.total_samples == 0 {
            0.0
        } else {
            self.non_zero_samples as f64 / self.total_samples as f64
        };
        let callback_interval_avg_ms = if self.interval_count == 0 {
            0.0
        } else {
            self.interval_total_ms / self.interval_count as f64
        };

        info!(
            callbacks_per_sec,
            avg_frames_per_callback,
            frames_per_sec,
            emitted_frames_per_sec,
            peak = self.peak,
            rms,
            non_zero_sample_ratio,
            callback_interval_avg_ms,
            callback_interval_max_ms = self.interval_max_ms,
            no_packet_polls = self.no_packet_polls,
            accumulator_buffered_samples = self.last_accumulator_buffered_samples as u64,
            source_buffer_frames,
            silent_callbacks = self.silent_callbacks,
            non_silent_callbacks = self.non_silent_callbacks,
            "capture summary"
        );

        self.reset_window();
    }

    fn record_callback(
        &mut self,
        frames: u32,
        sample_count: u64,
        non_zero_samples: u64,
        peak: f32,
        sum_sq: f64,
        is_silence: bool,
        accumulator_buffered_samples: usize,
    ) {
        let now = Instant::now();
        if let Some(prev) = self.last_callback_at {
            let interval_ms = now.duration_since(prev).as_secs_f64() * 1000.0;
            self.interval_count += 1;
            self.interval_total_ms += interval_ms;
            if interval_ms > self.interval_max_ms {
                self.interval_max_ms = interval_ms;
            }
        }
        self.last_callback_at = Some(now);

        self.callbacks += 1;
        if is_silence {
            self.silent_callbacks += 1;
        } else {
            self.non_silent_callbacks += 1;
        }
        self.total_callback_frames += u64::from(frames);
        self.total_samples += sample_count;
        self.non_zero_samples += non_zero_samples;
        self.sum_sq += sum_sq;
        if peak > self.peak {
            self.peak = peak;
        }
        self.last_accumulator_buffered_samples = accumulator_buffered_samples;
    }

    fn reset_window(&mut self) {
        let last_callback_at = self.last_callback_at;
        let last_accumulator_buffered_samples = self.last_accumulator_buffered_samples;
        *self = Self::new();
        self.last_callback_at = last_callback_at;
        self.last_accumulator_buffered_samples = last_accumulator_buffered_samples;
    }
}

pub struct WindowsLoopbackCapture {
    target_format: AudioFormat,
    state: CaptureSourceState,
    device_name: Option<String>,
    device_format: Option<DeviceFormatInfo>,
    frame_duration_ms: u16,
    accumulator: PcmFrameAccumulator,
    recent_error: Option<CaptureError>,
    audio_client: Option<IAudioClient>,
    capture_client: Option<IAudioCaptureClient>,
    mix_format_ptr: Option<*mut WAVEFORMATEX>,
    source_buffer_frames: u32,
    debug_wav: Option<PcmDebugWavWriter>,
    capture_stats: CaptureStatsWindow,
}

unsafe impl Send for WindowsLoopbackCapture {}

impl WindowsLoopbackCapture {
    pub fn new_default_output(
        format: AudioFormat,
        debug_cfg: CaptureDebugDumpConfig,
    ) -> Result<Self, CaptureError> {
        let debug_wav = if debug_cfg.enabled {
            Some(PcmDebugWavWriter::new(
                format.sample_rate_hz,
                format.channels,
                debug_cfg.seconds,
                &debug_cfg.output_dir,
            )?)
        } else {
            None
        };

        Ok(Self {
            target_format: format,
            state: CaptureSourceState::Created,
            device_name: None,
            device_format: None,
            frame_duration_ms: format.frame_duration_ms,
            accumulator: PcmFrameAccumulator::new(format),
            recent_error: None,
            audio_client: None,
            capture_client: None,
            mix_format_ptr: None,
            source_buffer_frames: 0,
            debug_wav,
            capture_stats: CaptureStatsWindow::new(),
        })
    }

    fn set_failed(&mut self, err: CaptureError) -> CaptureError {
        self.state = CaptureSourceState::Failed;
        self.recent_error = Some(err.clone());
        err
    }

    fn resolve_default_output_device(&mut self) -> Result<IMMDevice, CaptureError> {
        info!("resolving default output endpoint");
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|e| {
                    self.set_failed(CaptureError::DeviceActivationFailed(e.to_string()))
                })?;

            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| {
                    self.set_failed(CaptureError::DeviceActivationFailed(e.to_string()))
                })?;

            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|_| self.set_failed(CaptureError::DefaultDeviceNotFound))?;

            let id = device.GetId().map_err(|e| {
                self.set_failed(CaptureError::DeviceActivationFailed(e.to_string()))
            })?;
            let id_text = id.to_string().map_err(|e| {
                self.set_failed(CaptureError::DeviceActivationFailed(e.to_string()))
            })?;
            self.device_name = Some(id_text.clone());
            // TODO(wasapi): query friendly display name via property store.

            CoTaskMemFree(Some(id.0 as _));

            self.state = CaptureSourceState::DeviceResolved;
            info!(device = %id_text, "default output endpoint resolved");
            Ok(device)
        }
    }

    fn init_audio_clients(&mut self, device: &IMMDevice) -> Result<(), CaptureError> {
        info!("initializing WASAPI audio client (loopback)");
        unsafe {
            let audio_client: IAudioClient = device
                .Activate::<IAudioClient>(CLSCTX_ALL, None)
                .map_err(|e| {
                    self.set_failed(CaptureError::DeviceActivationFailed(e.to_string()))
                })?;

            let mix_format_ptr = audio_client
                .GetMixFormat()
                .map_err(|e| self.set_failed(CaptureError::AudioClientInitFailed(e.to_string())))?;

            if mix_format_ptr.is_null() {
                return Err(self.set_failed(CaptureError::AudioClientInitFailed(
                    "GetMixFormat returned null".to_string(),
                )));
            }

            let mix = *mix_format_ptr;
            let sample_rate_hz = mix.nSamplesPerSec;
            let channels = mix.nChannels;
            let bits_per_sample = mix.wBitsPerSample;
            let format_tag = mix.wFormatTag;
            let cb_size = mix.cbSize;
            let mut sub_format = None;
            let sample_kind = match format_tag as u32 {
                3 => {
                    if bits_per_sample != 32 {
                        return Err(self.set_failed(CaptureError::UnsupportedMixFormat(format!(
                            "unsupported float bits_per_sample={}; expected 32",
                            bits_per_sample
                        ))));
                    }
                    DeviceSampleKind::F32
                }
                1 => {
                    if bits_per_sample != 16 {
                        return Err(self.set_failed(CaptureError::UnsupportedMixFormat(format!(
                            "unsupported PCM bits_per_sample={}; expected 16 for wFormatTag=1",
                            bits_per_sample
                        ))));
                    }
                    DeviceSampleKind::I16
                }
                65534 => {
                    if cb_size < 22 {
                        return Err(self.set_failed(CaptureError::UnsupportedMixFormat(format!(
                            "WAVE_FORMAT_EXTENSIBLE cbSize too small: {}",
                            cb_size
                        ))));
                    }

                    // Parse common WAVE_FORMAT_EXTENSIBLE subformats:
                    // SubFormat.Data1 == 3 => IEEE float
                    // SubFormat.Data1 == 1 => PCM integer
                    // TODO(wasapi): compare full SubFormat GUID for stricter validation.
                    let ext_ptr = mix_format_ptr as *const u8;
                    let sub_guid_ptr =
                        ext_ptr.add(std::mem::size_of::<WAVEFORMATEX>() + 2 + 4) as *const [u8; 16];
                    let sub_guid = std::ptr::read_unaligned(sub_guid_ptr);
                    sub_format = Some(format!("{:02X?}", sub_guid));
                    let sub_data1 =
                        u32::from_le_bytes([sub_guid[0], sub_guid[1], sub_guid[2], sub_guid[3]]);

                    match sub_data1 {
                        3 => {
                            if bits_per_sample != 32 {
                                return Err(self.set_failed(CaptureError::UnsupportedMixFormat(
                                    format!(
                                    "extensible float bits_per_sample={} unsupported; expected 32",
                                    bits_per_sample
                                ),
                                )));
                            }
                            DeviceSampleKind::F32
                        }
                        1 => {
                            if bits_per_sample != 16 {
                                return Err(self.set_failed(CaptureError::UnsupportedMixFormat(
                                    format!(
                                    "extensible PCM bits_per_sample={} unsupported; expected 16",
                                    bits_per_sample
                                ),
                                )));
                            }
                            DeviceSampleKind::I16
                        }
                        other => {
                            return Err(self.set_failed(CaptureError::UnsupportedMixFormat(format!(
                                "unsupported WAVE_FORMAT_EXTENSIBLE SubFormat.Data1={other}; subformat={}",
                                sub_format.clone().unwrap_or_else(|| "n/a".to_string())
                            ))));
                        }
                    }
                }
                other => {
                    return Err(self.set_failed(CaptureError::UnsupportedMixFormat(format!(
                        "unsupported wFormatTag={other}; subformat=n/a"
                    ))));
                }
            };

            let format_info = DeviceFormatInfo {
                sample_rate_hz,
                channels,
                sample_kind,
                bits_per_sample,
                format_tag,
                sub_format,
            };

            info!(
                sample_rate = format_info.sample_rate_hz,
                channels = format_info.channels,
                bits_per_sample = format_info.bits_per_sample,
                format_tag = format_info.format_tag,
                sub_format = %format_info.sub_format.clone().unwrap_or_else(|| "n/a".to_string()),
                "WASAPI mix format"
            );

            audio_client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_LOOPBACK,
                    (self.frame_duration_ms as i64) * 10_000,
                    0,
                    mix_format_ptr,
                    None,
                )
                .map_err(|e| self.set_failed(CaptureError::AudioClientInitFailed(e.to_string())))?;

            self.source_buffer_frames = audio_client
                .GetBufferSize()
                .map_err(|e| self.set_failed(CaptureError::AudioClientInitFailed(e.to_string())))?;

            let capture_client: IAudioCaptureClient = audio_client
                .GetService::<IAudioCaptureClient>()
                .map_err(|e| {
                    self.set_failed(CaptureError::CaptureClientInitFailed(e.to_string()))
                })?;

            let sample_rate_hz = format_info.sample_rate_hz;
            let channels = format_info.channels;
            self.device_format = Some(format_info);
            self.target_format = AudioFormat {
                sample_rate_hz,
                channels,
                sample_format: SampleFormat::F32,
                frame_duration_ms: self.frame_duration_ms,
            };
            self.accumulator = PcmFrameAccumulator::new(self.target_format);
            if let Some(writer) = &mut self.debug_wav {
                writer.set_format(
                    self.target_format.sample_rate_hz,
                    self.target_format.channels,
                );
            }
            self.audio_client = Some(audio_client);
            self.capture_client = Some(capture_client);
            self.mix_format_ptr = Some(mix_format_ptr);
            self.state = CaptureSourceState::ClientInitialized;
            Ok(())
        }
    }

    fn do_start(&mut self) -> Result<(), CaptureError> {
        let client = self.audio_client.clone().ok_or_else(|| {
            self.set_failed(CaptureError::StartFailed(
                "audio client missing".to_string(),
            ))
        })?;
        unsafe {
            client
                .Start()
                .map_err(|e| self.set_failed(CaptureError::StartFailed(e.to_string())))?;
        }
        self.state = CaptureSourceState::Started;
        info!(
            device = %self.device_name.clone().unwrap_or_else(|| "unknown".to_string()),
            "WASAPI loopback started"
        );
        Ok(())
    }

    fn fill_accumulator_from_packets(&mut self) -> Result<PacketReadStatus, CaptureError> {
        let capture = self.capture_client.clone().ok_or_else(|| {
            self.set_failed(CaptureError::ReadBufferFailed(
                "capture client missing".to_string(),
            ))
        })?;
        let fmt = self.device_format.clone().ok_or_else(|| {
            self.set_failed(CaptureError::UnsupportedMixFormat(
                "device format missing".to_string(),
            ))
        })?;

        let mut saw_packet = false;
        let mut saw_silent_packet = false;
        let mut saw_non_silent_packet = false;

        unsafe {
            loop {
                let packet_frames = capture
                    .GetNextPacketSize()
                    .map_err(|e| self.set_failed(CaptureError::ReadBufferFailed(e.to_string())))?;

                if packet_frames == 0 {
                    break;
                }
                saw_packet = true;

                let mut data_ptr: *mut u8 = ptr::null_mut();
                let mut num_frames = 0u32;
                let mut flags = 0u32;

                capture
                    .GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None)
                    .map_err(|e| self.set_failed(CaptureError::ReadBufferFailed(e.to_string())))?;

                let is_silence = (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
                let channels = fmt.channels as usize;
                let sample_count = num_frames as usize * channels;

                if is_silence {
                    saw_silent_packet = true;
                    self.accumulator.push_silence_samples(sample_count);
                    self.capture_stats.record_silent_callback(
                        num_frames,
                        sample_count,
                        self.accumulator.buffered_samples(),
                    );
                } else {
                    saw_non_silent_packet = true;
                    if data_ptr.is_null() {
                        let _ = capture.ReleaseBuffer(num_frames);
                        return Err(self.set_failed(CaptureError::ReadBufferFailed(
                            "GetBuffer data pointer is null".to_string(),
                        )));
                    }

                    match fmt.sample_kind {
                        DeviceSampleKind::F32 => {
                            let src =
                                std::slice::from_raw_parts(data_ptr as *const f32, sample_count);
                            self.accumulator.push_samples(src, false);
                            self.capture_stats.record_pcm_callback(
                                num_frames,
                                src,
                                self.accumulator.buffered_samples(),
                            );
                        }
                        DeviceSampleKind::I16 => {
                            let src =
                                std::slice::from_raw_parts(data_ptr as *const i16, sample_count);
                            let mut tmp = Vec::with_capacity(sample_count);
                            tmp.extend(src.iter().map(|v| (*v as f32) / i16::MAX as f32));
                            self.accumulator.push_samples(&tmp, false);
                            self.capture_stats.record_pcm_callback(
                                num_frames,
                                &tmp,
                                self.accumulator.buffered_samples(),
                            );
                        }
                    }
                }

                capture
                    .ReleaseBuffer(num_frames)
                    .map_err(|e| self.set_failed(CaptureError::ReadBufferFailed(e.to_string())))?;

                if let Some(acc_frame) = self.accumulator.pop_frame() {
                    let mut frame = self
                        .accumulator
                        .into_audio_frame(now_ms(), acc_frame, Some(self.source_buffer_frames))
                        .map_err(|e| self.set_failed(e))?;
                    frame.packet_kind = if saw_non_silent_packet && saw_silent_packet {
                        PacketKind::Mixed
                    } else if saw_non_silent_packet {
                        PacketKind::AudioPacket
                    } else {
                        PacketKind::SilentPacket
                    };
                    if let Some(writer) = &mut self.debug_wav {
                        if let Err(err) = writer.write_samples(&frame.samples_f32) {
                            warn!(error = %err, "capture debug wav write failed");
                        }
                    }
                    self.capture_stats
                        .record_emitted_frame(self.accumulator.buffered_samples());
                    return Ok(PacketReadStatus::Frame(frame));
                }
            }
        }

        if !saw_packet {
            self.capture_stats
                .record_no_packet_poll(self.accumulator.buffered_samples());
            return Ok(PacketReadStatus::NoPacket);
        }
        Ok(PacketReadStatus::PacketButNotEnough)
    }

    fn do_stop(&mut self) {
        if let Some(client) = &self.audio_client {
            unsafe {
                if let Err(err) = client.Stop() {
                    warn!(error = %err, "WASAPI stop returned error");
                }
            }
        }
        if let Some(ptr) = self.mix_format_ptr.take() {
            unsafe {
                CoTaskMemFree(Some(ptr as _));
            }
        }
        if let Some(writer) = &mut self.debug_wav {
            if let Err(err) = writer.finalize() {
                warn!(error = %err, "finalize debug wav failed");
            }
        }
        self.audio_client = None;
        self.capture_client = None;
        self.state = CaptureSourceState::Stopped;
    }
}

#[derive(Debug)]
enum PacketReadStatus {
    NoPacket,
    PacketButNotEnough,
    Frame(AudioFrame),
}

#[async_trait]
impl AudioCaptureSource for WindowsLoopbackCapture {
    async fn start(&mut self) -> Result<(), CaptureError> {
        let device = self.resolve_default_output_device()?;
        debug!(state = %self.state.as_str(), "default output device resolved");
        self.init_audio_clients(&device)?;
        debug!(state = %self.state.as_str(), "audio client initialized");
        self.do_start()?;
        Ok(())
    }

    async fn read_frame(&mut self) -> Result<AudioFrame, CaptureError> {
        if self.state != CaptureSourceState::Started {
            return Err(CaptureError::NotStarted);
        }

        // On Windows the default timer granularity is often about 15.6ms, so
        // `sleep(1ms)`/`sleep(2ms)` can stretch a 10-iteration poll loop into
        // roughly 150ms and collapse playout to ~6fps. Keep polling within one
        // frame-sized deadline and yield the runtime instead of sleeping.
        let deadline = Instant::now()
            + Duration::from_millis(u64::from(self.frame_duration_ms).saturating_add(2));
        let mut no_packet_observed = false;
        loop {
            match self.fill_accumulator_from_packets()? {
                PacketReadStatus::Frame(frame) => {
                    debug!(
                        packet_kind = ?frame.packet_kind,
                        peak = frame.peak,
                        rms = frame.rms,
                        "loopback produced frame"
                    );
                    self.capture_stats.maybe_log(self.source_buffer_frames);
                    return Ok(frame);
                }
                PacketReadStatus::NoPacket => {
                    no_packet_observed = true;
                    if Instant::now() >= deadline {
                        break;
                    }
                    yield_now().await;
                }
                PacketReadStatus::PacketButNotEnough => {
                    if Instant::now() >= deadline {
                        break;
                    }
                    yield_now().await;
                }
            }
        }

        let mut frame = AudioFrame::silence(now_ms(), self.target_format);
        frame.packet_kind = if no_packet_observed {
            PacketKind::NoPacket
        } else {
            PacketKind::SilentPacket
        };
        self.capture_stats.maybe_log(self.source_buffer_frames);
        Ok(frame)
    }

    async fn stop(&mut self) -> Result<(), CaptureError> {
        self.do_stop();
        Ok(())
    }

    fn format(&self) -> AudioFormat {
        if let Some(df) = &self.device_format {
            AudioFormat {
                sample_rate_hz: df.sample_rate_hz,
                channels: df.channels,
                sample_format: SampleFormat::F32,
                frame_duration_ms: self.frame_duration_ms,
            }
        } else {
            self.target_format
        }
    }

    fn state(&self) -> CaptureSourceState {
        self.state
    }

    fn source_name(&self) -> &'static str {
        "windows_loopback"
    }

    fn device_name(&self) -> Option<String> {
        self.device_name.clone()
    }
}

impl Drop for WindowsLoopbackCapture {
    fn drop(&mut self) {
        self.do_stop();
    }
}

struct PcmDebugWavWriter {
    file: File,
    channels: u16,
    sample_rate: u32,
    max_seconds: u32,
    max_samples: usize,
    written_samples: usize,
    finalized: bool,
    path: PathBuf,
}

impl PcmDebugWavWriter {
    fn new(
        sample_rate: u32,
        channels: u16,
        seconds: u32,
        output_dir: &str,
    ) -> Result<Self, CaptureError> {
        let out_dir = PathBuf::from(output_dir);
        fs::create_dir_all(&out_dir)
            .map_err(|e| CaptureError::StartFailed(format!("create dump dir failed: {e}")))?;

        let name = format!("wasapi_loopback_{}.wav", now_ms());
        let path = out_dir.join(name);
        let mut file = File::create(&path)
            .map_err(|e| CaptureError::StartFailed(format!("create dump wav failed: {e}")))?;

        file.write_all(&[0u8; 44])
            .map_err(|e| CaptureError::StartFailed(format!("init wav header failed: {e}")))?;

        info!(path = %path.display(), seconds, "capture debug wav enabled");

        Ok(Self {
            file,
            channels,
            sample_rate,
            max_seconds: seconds,
            max_samples: (sample_rate as usize) * (seconds as usize) * (channels as usize),
            written_samples: 0,
            finalized: false,
            path,
        })
    }

    fn set_format(&mut self, sample_rate: u32, channels: u16) {
        self.sample_rate = sample_rate;
        self.channels = channels;
        self.max_samples =
            (sample_rate as usize) * (channels as usize) * (self.max_seconds as usize);
    }

    fn write_samples(&mut self, samples: &[f32]) -> Result<(), CaptureError> {
        if self.written_samples >= self.max_samples {
            return Ok(());
        }
        let remain = self.max_samples - self.written_samples;
        let take = remain.min(samples.len());
        for sample in &samples[..take] {
            let v = sample.clamp(-1.0, 1.0);
            let s = (v * i16::MAX as f32) as i16;
            self.file.write_all(&s.to_le_bytes()).map_err(|e| {
                CaptureError::ReadBufferFailed(format!("write dump wav failed: {e}"))
            })?;
        }
        self.written_samples += take;
        if self.written_samples >= self.max_samples {
            self.finalize()?;
        }
        Ok(())
    }

    fn finalize(&mut self) -> Result<(), CaptureError> {
        if self.finalized {
            return Ok(());
        }
        let data_bytes = (self.written_samples * 2) as u32;
        let riff_size = 36 + data_bytes;
        let byte_rate = self.sample_rate * (self.channels as u32) * 2;
        let block_align = self.channels * 2;

        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| CaptureError::ReadBufferFailed(format!("seek dump wav failed: {e}")))?;

        let mut header = Vec::with_capacity(44);
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&riff_size.to_le_bytes());
        header.extend_from_slice(b"WAVE");
        header.extend_from_slice(b"fmt ");
        header.extend_from_slice(&16u32.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&self.channels.to_le_bytes());
        header.extend_from_slice(&self.sample_rate.to_le_bytes());
        header.extend_from_slice(&byte_rate.to_le_bytes());
        header.extend_from_slice(&block_align.to_le_bytes());
        header.extend_from_slice(&16u16.to_le_bytes());
        header.extend_from_slice(b"data");
        header.extend_from_slice(&data_bytes.to_le_bytes());

        self.file
            .write_all(&header)
            .map_err(|e| CaptureError::ReadBufferFailed(format!("write wav header failed: {e}")))?;
        self.file
            .flush()
            .map_err(|e| CaptureError::ReadBufferFailed(format!("flush wav failed: {e}")))?;

        self.finalized = true;
        info!(path = %self.path.display(), samples = self.written_samples, "capture debug wav finalized");
        Ok(())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
