use std::fs::{self, File};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::ptr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::time::sleep;
use tracing::{debug, info, warn};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
    MMDeviceEnumerator, WAVEFORMATEX,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED, CLSCTX_ALL,
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
                .map_err(|e| self.set_failed(CaptureError::DeviceActivationFailed(e.to_string())))?;

            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| self.set_failed(CaptureError::DeviceActivationFailed(e.to_string())))?;

            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|_| self.set_failed(CaptureError::DefaultDeviceNotFound))?;

            let id = device
                .GetId()
                .map_err(|e| self.set_failed(CaptureError::DeviceActivationFailed(e.to_string())))?;
            let id_text = id
                .to_string()
                .map_err(|e| self.set_failed(CaptureError::DeviceActivationFailed(e.to_string())))?;
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
                .map_err(|e| self.set_failed(CaptureError::DeviceActivationFailed(e.to_string())))?;

            let mix_format_ptr = audio_client
                .GetMixFormat()
                .map_err(|e| self.set_failed(CaptureError::AudioClientInitFailed(e.to_string())))?;

            if mix_format_ptr.is_null() {
                return Err(self.set_failed(CaptureError::AudioClientInitFailed(
                    "GetMixFormat returned null".to_string(),
                )));
            }

            let mix = *mix_format_ptr;
            let sample_kind = match mix.wFormatTag as u32 {
                3 => DeviceSampleKind::F32,
                1 => DeviceSampleKind::I16,
                other => {
                    return Err(self.set_failed(CaptureError::UnsupportedMixFormat(format!(
                        "unsupported wFormatTag={other}; subformat=n/a"
                    ))));
                }
            };

            let format_info = DeviceFormatInfo {
                sample_rate_hz: mix.nSamplesPerSec,
                channels: mix.nChannels,
                sample_kind,
                bits_per_sample: mix.wBitsPerSample,
                format_tag: mix.wFormatTag,
                sub_format: None,
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
                .map_err(|e| self.set_failed(CaptureError::CaptureClientInitFailed(e.to_string())))?;

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
                writer.set_format(self.target_format.sample_rate_hz, self.target_format.channels);
            }
            self.audio_client = Some(audio_client);
            self.capture_client = Some(capture_client);
            self.mix_format_ptr = Some(mix_format_ptr);
            self.state = CaptureSourceState::ClientInitialized;
            Ok(())
        }
    }

    fn do_start(&mut self) -> Result<(), CaptureError> {
        let client = self
            .audio_client
            .clone()
            .ok_or_else(|| self.set_failed(CaptureError::StartFailed("audio client missing".to_string())))?;
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
        let capture = self
            .capture_client
            .clone()
            .ok_or_else(|| self.set_failed(CaptureError::ReadBufferFailed("capture client missing".to_string())))?;
        let fmt = self
            .device_format
            .clone()
            .ok_or_else(|| self.set_failed(CaptureError::UnsupportedMixFormat("device format missing".to_string())))?;

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
                    .GetBuffer(
                        &mut data_ptr,
                        &mut num_frames,
                        &mut flags,
                        None,
                        None,
                    )
                    .map_err(|e| self.set_failed(CaptureError::ReadBufferFailed(e.to_string())))?;

                let is_silence = (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
                let channels = fmt.channels as usize;
                let sample_count = num_frames as usize * channels;

                if is_silence {
                    saw_silent_packet = true;
                    self.accumulator.push_silence_samples(sample_count);
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
                            let src = std::slice::from_raw_parts(data_ptr as *const f32, sample_count);
                            self.accumulator.push_samples(src, false);
                        }
                        DeviceSampleKind::I16 => {
                            let src = std::slice::from_raw_parts(data_ptr as *const i16, sample_count);
                            let mut tmp = Vec::with_capacity(sample_count);
                            tmp.extend(src.iter().map(|v| (*v as f32) / i16::MAX as f32));
                            self.accumulator.push_samples(&tmp, false);
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
                    return Ok(PacketReadStatus::Frame(frame));
                }
            }
        }

        if !saw_packet {
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

        let mut no_packet_observed = false;
        for _ in 0..10 {
            match self.fill_accumulator_from_packets()? {
                PacketReadStatus::Frame(frame) => {
                    debug!(
                        packet_kind = ?frame.packet_kind,
                        peak = frame.peak,
                        rms = frame.rms,
                        "loopback produced frame"
                    );
                    return Ok(frame);
                }
                PacketReadStatus::NoPacket => {
                    no_packet_observed = true;
                    sleep(Duration::from_millis(2)).await;
                }
                PacketReadStatus::PacketButNotEnough => {
                    sleep(Duration::from_millis(1)).await;
                }
            }
        }

        let mut frame = AudioFrame::silence(now_ms(), self.target_format);
        frame.packet_kind = if no_packet_observed {
            PacketKind::NoPacket
        } else {
            PacketKind::SilentPacket
        };
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
    fn new(sample_rate: u32, channels: u16, seconds: u32, output_dir: &str) -> Result<Self, CaptureError> {
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
        self.max_samples = (sample_rate as usize) * (channels as usize) * (self.max_seconds as usize);
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
            self.file
                .write_all(&s.to_le_bytes())
                .map_err(|e| CaptureError::ReadBufferFailed(format!("write dump wav failed: {e}")))?;
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
