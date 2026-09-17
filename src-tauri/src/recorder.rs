use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicI32, AtomicU64, Ordering},
        Mutex,
    },
    time::SystemTime,
};

use chrono::{SecondsFormat, Utc};

use crate::domain::{AppError, AppResult, AudioSource};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingInfo {
    pub session_id: String,
    pub started_at: String,
}

fn segment_path(base: &Path, source: AudioSource, index: u64) -> AppResult<PathBuf> {
    let id = base
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|id| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
        .ok_or_else(|| AppError::new("invalid_audio_path", "Invalid capture session path"))?;
    if base
        .extension()
        .map_or(true, |extension| extension != "m4a")
    {
        return Err(AppError::new(
            "invalid_audio_path",
            "Invalid capture session path",
        ));
    }
    Ok(base.with_file_name(format!("{id}-{}-segment-{index:08}.m4a", source.filename())))
}

#[derive(Debug, Clone, PartialEq)]
pub struct CapturedSegment {
    pub source: AudioSource,
    pub index: u64,
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub path: PathBuf,
}

#[cfg(test)]
#[path = "recorder_rotation_tests.rs"]
mod rotation_tests;

#[derive(Debug, Clone, PartialEq)]
pub struct RecordingFiles {
    pub system: PathBuf,
    pub microphone: PathBuf,
    pub health: Option<RecordingHealth>,
    pub segments: Vec<CapturedSegment>,
    pub segmented: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingHealth {
    pub wall_seconds: f64,
    pub system: SourceHealth,
    pub microphone: SourceHealth,
    pub identity_changed: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceHealth {
    pub admitted_frames: u64,
    // Frames accepted by ExtAudioFileWriteAsync; stop also reports final flush errors.
    pub written_frames: u64,
    pub captured_seconds: f64,
    pub sample_rate: f64,
    pub last_callback_age_seconds: Option<f64>,
    pub write_error: Option<i32>,
    pub status: CaptureStatus,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStatus {
    Starting,
    Healthy,
    NoFrames,
    Stalled,
    WriteError,
    ShortCapture,
}

#[derive(Default)]
struct SourceCounters {
    admitted_frames: AtomicU64,
    written_frames: AtomicU64,
    // Zero means no callback; other values are elapsed milliseconds plus one.
    last_callback_ms: AtomicU64,
    last_written_frame_ms: AtomicU64,
    write_status: AtomicI32,
}

impl SourceCounters {
    fn callback(&self, elapsed_ms: u64) {
        self.last_callback_ms
            .store(elapsed_ms.saturating_add(1), Ordering::Release);
    }

    fn admit(&self, frames: u32) {
        self.admitted_frames
            .fetch_add(u64::from(frames), Ordering::Relaxed);
    }

    fn complete_write(&self, frames: u32, status: i32) {
        if status == 0 {
            if frames != 0 {
                self.last_written_frame_ms.store(
                    self.last_callback_ms.load(Ordering::Acquire),
                    Ordering::Release,
                );
            }
            self.written_frames
                .fetch_add(u64::from(frames), Ordering::Release);
        } else {
            let _ =
                self.write_status
                    .compare_exchange(0, status, Ordering::AcqRel, Ordering::Acquire);
        }
    }

    fn snapshot(&self, sample_rate: f64, wall_seconds: f64, stopped: bool) -> SourceHealth {
        let written_frames = self.written_frames.load(Ordering::Acquire);
        let admitted_frames = self.admitted_frames.load(Ordering::Acquire);
        let last_callback = self.last_callback_ms.load(Ordering::Acquire);
        let last_callback_age_seconds = last_callback
            .checked_sub(1)
            .map(|ms| (wall_seconds - ms as f64 / 1000.0).max(0.0));
        let last_frame_age_seconds = self
            .last_written_frame_ms
            .load(Ordering::Acquire)
            .checked_sub(1)
            .map(|ms| (wall_seconds - ms as f64 / 1000.0).max(0.0));
        let captured_seconds = written_frames as f64 / sample_rate;
        let write_status = self.write_status.load(Ordering::Acquire);
        let missing_seconds = wall_seconds - captured_seconds;
        let status = if write_status != 0 {
            CaptureStatus::WriteError
        } else if written_frames == 0 {
            if stopped || wall_seconds >= 10.0 {
                CaptureStatus::NoFrames
            } else {
                CaptureStatus::Starting
            }
        } else if !stopped && last_frame_age_seconds.is_some_and(|age| age >= 10.0) {
            CaptureStatus::Stalled
        } else if stopped && (missing_seconds > 5.0 || missing_seconds > wall_seconds * 0.05) {
            CaptureStatus::ShortCapture
        } else {
            CaptureStatus::Healthy
        };
        SourceHealth {
            admitted_frames,
            written_frames,
            captured_seconds,
            sample_rate,
            last_callback_age_seconds,
            write_error: (write_status != 0).then_some(write_status),
            status,
        }
    }
}

impl RecordingHealth {
    fn from_sources(wall_seconds: f64, system: SourceHealth, microphone: SourceHealth) -> Self {
        let mut warnings = Vec::new();
        for (label, source) in [("System audio", &system), ("Microphone", &microphone)] {
            let detail = match source.status {
                CaptureStatus::NoFrames => Some("has produced no audio frames".to_owned()),
                CaptureStatus::Stalled => Some(
                    "has not delivered writable audio frames for at least 10 seconds".to_owned(),
                ),
                CaptureStatus::WriteError => Some(format!(
                    "could not write some audio (OSStatus {})",
                    source.write_error.unwrap_or_default()
                )),
                CaptureStatus::ShortCapture => Some(format!(
                    "captured {:.1} seconds of a {:.1}-second recording",
                    source.captured_seconds, wall_seconds
                )),
                CaptureStatus::Starting | CaptureStatus::Healthy => None,
            };
            if let Some(detail) = detail {
                warnings.push(format!("{label} {detail}."));
            }
            // A write failure can coexist with a substantial gap; preserve the duration evidence.
            if source.status == CaptureStatus::WriteError {
                warnings.push(format!(
                    "{label} accepted {:.1} seconds of audio during {:.1} seconds of recording.",
                    source.captured_seconds, wall_seconds
                ));
            }
        }
        Self {
            wall_seconds,
            system,
            microphone,
            identity_changed: false,
            warnings,
        }
    }

    fn set_identity_changed(&mut self, changed: bool) {
        self.identity_changed = changed;
        if changed {
            self.warnings.push("The app executable changed while this process was running. macOS privacy permissions may have changed and capture may be incomplete. Avoid replacing the app during recording; stable Developer ID signing is needed to prevent ad-hoc identity changes.".to_owned());
        }
    }
}

#[derive(PartialEq)]
struct ExecutableFingerprint {
    length: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl ExecutableFingerprint {
    fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Some(Self {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        })
    }
}

struct ExecutableIdentity {
    path: PathBuf,
    baseline: Option<ExecutableFingerprint>,
}

impl ExecutableIdentity {
    fn read(path: PathBuf) -> Self {
        Self {
            baseline: ExecutableFingerprint::read(&path),
            path,
        }
    }

    fn changed(&self) -> bool {
        self.baseline != ExecutableFingerprint::read(&self.path)
    }
}

#[derive(Default)]
struct RecordingSlot {
    session_id: Option<String>,
    #[cfg(target_os = "macos")]
    recording: Option<native::NativeRecording>,
}

impl RecordingSlot {
    fn reserve(&mut self, session_id: &str) -> AppResult<()> {
        if self.session_id.is_some() {
            return Err(AppError::new(
                "already_recording",
                "A recording is already in progress",
            ));
        }
        self.session_id = Some(session_id.to_owned());
        Ok(())
    }

    fn release(&mut self, session_id: &str) -> AppResult<()> {
        if self.session_id.as_deref() != Some(session_id) {
            return Err(AppError::new(
                "recording_session_mismatch",
                "Recording belongs to a different session",
            ));
        }
        self.session_id = None;
        Ok(())
    }
}

pub struct Recorder {
    // ponytail: one global recording matches the single-window v1; move to per-session recorders only if concurrent capture becomes a real requirement.
    slot: Mutex<RecordingSlot>,
    executable: Option<ExecutableIdentity>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            slot: Mutex::new(RecordingSlot::default()),
            executable: std::env::current_exe().ok().map(ExecutableIdentity::read),
        }
    }

    pub fn start(
        &self,
        session_id: &str,
        system_path: &Path,
        microphone_path: &Path,
    ) -> AppResult<RecordingInfo> {
        self.start_mode(session_id, system_path, microphone_path, false)
    }

    pub fn start_segmented(
        &self,
        session_id: &str,
        system_path: &Path,
        microphone_path: &Path,
    ) -> AppResult<RecordingInfo> {
        self.start_mode(session_id, system_path, microphone_path, true)
    }

    fn start_mode(
        &self,
        session_id: &str,
        system_path: &Path,
        microphone_path: &Path,
        segmented: bool,
    ) -> AppResult<RecordingInfo> {
        if session_id.is_empty() {
            return Err(AppError::new(
                "invalid_session_id",
                "Session ID is required",
            ));
        }

        let mut slot = self.lock_slot()?;
        slot.reserve(session_id)?;

        #[cfg(target_os = "macos")]
        match native::NativeRecording::start(system_path, microphone_path, segmented) {
            Ok(recording) => slot.recording = Some(recording),
            Err(error) => {
                slot.session_id = None;
                return Err(error);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (system_path, microphone_path, segmented);
            slot.session_id = None;
            return Err(AppError::new(
                "audio_capture_unsupported",
                "System audio capture requires macOS 14.2 or newer",
            ));
        }

        Ok(RecordingInfo {
            session_id: session_id.to_owned(),
            started_at: Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true),
        })
    }

    /// Run on a control worker. Capture continues while retired files finalize.
    pub fn rotate(&self, session_id: &str) -> AppResult<Vec<CapturedSegment>> {
        let mut slot = self.lock_slot()?;
        if slot.session_id.as_deref() != Some(session_id) {
            return Err(AppError::new(
                "recording_session_mismatch",
                "Recording belongs to a different session",
            ));
        }
        #[cfg(target_os = "macos")]
        {
            slot.recording
                .as_mut()
                .ok_or_else(|| AppError::new("recorder_state", "Recording resources are missing"))?
                .rotate()
        }
        #[cfg(not(target_os = "macos"))]
        Err(AppError::new(
            "audio_capture_unsupported",
            "System audio capture requires macOS",
        ))
    }

    pub fn stop(&self, session_id: &str) -> AppResult<RecordingFiles> {
        let mut slot = self.lock_slot()?;
        if slot.session_id.as_deref() != Some(session_id) {
            return Err(AppError::new(
                "recording_session_mismatch",
                "Recording belongs to a different session",
            ));
        }

        #[cfg(target_os = "macos")]
        let result = match slot.recording.take() {
            Some(recording) => recording.stop(),
            None => Err(AppError::new(
                "recorder_state",
                "Recording resources are missing",
            )),
        };

        #[cfg(not(target_os = "macos"))]
        let result = Err(AppError::new(
            "audio_capture_unsupported",
            "System audio capture requires macOS 14.2 or newer",
        ));

        let result = result.map(|mut files: RecordingFiles| {
            if let Some(health) = files.health.as_mut() {
                health.set_identity_changed(self.identity_changed());
            }
            files
        });
        let release_result = slot.release(session_id);
        match (result, release_result) {
            (Ok(path), Ok(())) => Ok(path),
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        }
    }

    pub fn health(&self, session_id: &str) -> AppResult<RecordingHealth> {
        let slot = self.lock_slot()?;
        if slot.session_id.as_deref() != Some(session_id) {
            return Err(AppError::new(
                "recording_session_mismatch",
                "Recording belongs to a different session",
            ));
        }
        #[cfg(target_os = "macos")]
        {
            let recording = slot.recording.as_ref().ok_or_else(|| {
                AppError::new("recorder_state", "Recording resources are missing")
            })?;
            let mut health = recording.health()?;
            health.set_identity_changed(self.identity_changed());
            Ok(health)
        }
        #[cfg(not(target_os = "macos"))]
        Err(AppError::new(
            "audio_capture_unsupported",
            "System audio capture requires macOS 14.2 or newer",
        ))
    }

    fn identity_changed(&self) -> bool {
        self.executable
            .as_ref()
            .is_some_and(ExecutableIdentity::changed)
    }

    pub fn is_recording(&self) -> bool {
        self.slot
            .lock()
            .map(|slot| slot.session_id.is_some())
            .unwrap_or(true)
    }

    fn lock_slot(&self) -> AppResult<std::sync::MutexGuard<'_, RecordingSlot>> {
        self.slot
            .lock()
            .map_err(|_| AppError::new("recorder_state", "Recorder state is unavailable"))
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
pub(crate) mod native {
    use std::{
        ffi::{c_void, CStr},
        path::{Path, PathBuf},
        ptr::{self, NonNull},
        sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering},
        thread,
        time::Duration,
    };

    use objc2::{rc::Retained, AnyThread};
    use objc2_audio_toolbox::{
        kAudioConverterApplicableEncodeBitRates, kAudioConverterEncodeBitRate, kAudioFileM4AType,
        kExtAudioFileProperty_AudioConverter, kExtAudioFileProperty_ClientDataFormat,
        AudioConverterGetProperty, AudioConverterGetPropertyInfo, AudioConverterRef,
        AudioConverterSetProperty, AudioFileFlags, ExtAudioFileCreateWithURL, ExtAudioFileDispose,
        ExtAudioFileGetProperty, ExtAudioFileRef, ExtAudioFileSetProperty, ExtAudioFileWriteAsync,
    };
    use objc2_core_audio::{
        kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
        kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
        kAudioAggregateDeviceUIDKey, kAudioDevicePropertyDeviceIsAlive,
        kAudioDevicePropertyStreams, kAudioHardwarePropertyDefaultInputDevice,
        kAudioHardwarePropertyTranslatePIDToProcessObject, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeInput, kAudioObjectSystemObject,
        kAudioStreamPropertyVirtualFormat, kAudioSubTapUIDKey, kAudioTapPropertyFormat,
        AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID,
        AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
        AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
        AudioHardwareDestroyProcessTap, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize,
        AudioObjectID, AudioObjectPropertyAddress, CATapDescription, CATapMuteBehavior,
    };
    use objc2_core_audio_types::{
        kAudioFormatLinearPCM, kAudioFormatMPEG4AAC, AudioBufferList, AudioStreamBasicDescription,
        AudioTimeStamp, AudioValueRange,
    };
    use objc2_core_foundation::{CFDictionary, CFURL};
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSString};

    use crate::{
        domain::{AppError, AppResult, AudioSource},
        recorder::{
            segment_path, CapturedSegment, RecordingFiles, RecordingHealth, SourceCounters,
            SourceHealth,
        },
    };

    const NO_ERR: i32 = 0;
    const AAC_BIT_RATE: u32 = 24_000;
    const AUDIO_PERMISSION_DENIED: i32 = -66748;
    const CALLBACK_GATE_CLOSED: usize = 1 << (usize::BITS - 1);

    #[derive(Clone, Copy)]
    #[repr(C)]
    struct MachTimebase {
        numer: u32,
        denom: u32,
    }

    unsafe extern "C" {
        fn mach_timebase_info(info: *mut MachTimebase) -> i32;
        fn mach_continuous_time() -> u64;
    }

    #[derive(Clone, Copy)]
    struct ContinuousClock {
        started_ticks: u64,
        timebase: MachTimebase,
    }

    impl ContinuousClock {
        fn new() -> AppResult<Self> {
            let mut timebase = MachTimebase { numer: 0, denom: 0 };
            // SAFETY: timebase has the layout and writable size required by Mach.
            check_status("mach_timebase_info", unsafe {
                mach_timebase_info(&mut timebase)
            })?;
            if timebase.numer == 0 || timebase.denom == 0 {
                return Err(AppError::new(
                    "audio_capture",
                    "macOS returned an invalid capture clock timebase",
                ));
            }
            // SAFETY: this clock read takes no pointers and is supported on macOS 10.12+.
            Ok(Self {
                started_ticks: unsafe { mach_continuous_time() },
                timebase,
            })
        }

        fn elapsed(&self) -> Duration {
            // Rust Instant uses CLOCK_UPTIME_RAW on macOS and excludes system sleep.
            // Continuous Mach time includes sleep, preserving gaps in wall duration and
            // callback age. Cache the timebase before capture so callbacks only read time.
            // SAFETY: this clock read takes no pointers; it does not allocate or take locks.
            self.elapsed_at(unsafe { mach_continuous_time() })
        }

        fn elapsed_at(&self, ticks: u64) -> Duration {
            let nanos = u128::from(ticks.saturating_sub(self.started_ticks))
                * u128::from(self.timebase.numer)
                / u128::from(self.timebase.denom);
            Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
        }
    }

    struct SegmentSink {
        file: AtomicPtr<objc2_audio_toolbox::OpaqueExtAudioFile>,
        gate: CallbackGate,
        written_frames: AtomicU64,
        path: PathBuf,
    }

    impl SegmentSink {
        fn create(path: &Path, format: &AudioStreamBasicDescription) -> AppResult<Box<Self>> {
            // A numbered recording is never overwritten, even through a symlink or hard link.
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| {
                    AppError::new(
                        "audio_capture",
                        format!("Cannot create recording segment: {error}"),
                    )
                })?;
            let sink = Box::new(Self {
                file: AtomicPtr::new(ptr::null_mut()),
                gate: CallbackGate::default(),
                written_frames: AtomicU64::new(0),
                path: path.to_owned(),
            });
            let prepared = (|| {
                let file = create_audio_file(path, format)?;
                sink.file.store(file, Ordering::Release);
                set_client_format(file, format)?;
                set_bit_rate(file)?;
                // SAFETY: the sink exclusively owns this configured file; no callback sees it yet.
                check_status("ExtAudioFileWriteAsync(segment prime)", unsafe {
                    ExtAudioFileWriteAsync(file, 0, ptr::null())
                })
            })();
            if let Err(error) = prepared {
                drop(sink);
                // This call reserved the path and no callback has seen it: no captured audio
                // exists here. Keep the numbered name available for a later rotation retry.
                let _ = std::fs::remove_file(path);
                return Err(error);
            }
            Ok(sink)
        }

        fn close(&self) -> AppResult<()> {
            self.gate.disable();
            self.gate.wait_for_idle();
            let file = self.file.swap(ptr::null_mut(), Ordering::AcqRel);
            if file.is_null() {
                return Ok(());
            }
            // SAFETY: no admitted callback can still use this handle. Dispose flushes queued async writes.
            check_status("ExtAudioFileDispose(segment)", unsafe {
                ExtAudioFileDispose(file)
            })
        }
    }

    impl Drop for SegmentSink {
        fn drop(&mut self) {
            // Only setup failure or recorder teardown drops a sink; success paths check close explicitly.
            let _ = self.close();
        }
    }

    struct SegmentWriter {
        base: PathBuf,
        source: AudioSource,
        format: AudioStreamBasicDescription,
        // ponytail: retain small closed sink boxes until stop to avoid callback pointer reclamation/ABA;
        // only add reclamation if multi-day captures make their metadata memory measurable.
        #[allow(clippy::vec_box)]
        sinks: Vec<Box<SegmentSink>>,
        start_seconds: f64,
        finished: bool,
    }

    impl SegmentWriter {
        fn new(
            base: &Path,
            source: AudioSource,
            format: &AudioStreamBasicDescription,
        ) -> AppResult<Self> {
            let first = SegmentSink::create(&segment_path(base, source, 0)?, format)?;
            Ok(Self {
                base: base.to_owned(),
                source,
                format: *format,
                sinks: vec![first],
                start_seconds: 0.0,
                finished: false,
            })
        }

        fn active_ptr(&self) -> *mut SegmentSink {
            self.sinks
                .last()
                .expect("writer has an active sink")
                .as_ref() as *const SegmentSink as *mut SegmentSink
        }

        fn rotate(&mut self, callback: &CallbackState) -> AppResult<Option<CapturedSegment>> {
            if self.finished
                || self
                    .sinks
                    .last()
                    .expect("active sink")
                    .written_frames
                    .load(Ordering::Acquire)
                    == 0
            {
                return Ok(None);
            }
            let index = self.sinks.len() as u64;
            let next =
                SegmentSink::create(&segment_path(&self.base, self.source, index)?, &self.format)?;
            self.sinks.push(next);
            // Publish the fully prepared replacement before closing the old gate. A callback that
            // already loaded the old pointer either acquires its lease or reloads the new pointer.
            callback
                .active_sink
                .store(self.active_ptr(), Ordering::Release);
            self.finalize(index - 1)
        }

        fn finish(&mut self) -> AppResult<Option<CapturedSegment>> {
            if self.finished {
                return Ok(None);
            }
            self.finished = true;
            self.finalize(self.sinks.len() as u64 - 1)
        }

        fn finalize(&mut self, index: u64) -> AppResult<Option<CapturedSegment>> {
            let sink = &self.sinks[index as usize];
            // Drain before reading frame totals. Capture is already routed to the new sink, or stopped.
            let close = sink.close();
            let accepted_frames = sink.written_frames.load(Ordering::Acquire);
            let start_seconds = self.start_seconds;
            // A damaged segment still occupies its captured time. Its file stays for recovery.
            self.start_seconds += accepted_frames as f64 / self.format.mSampleRate;
            close?;
            std::fs::File::open(&sink.path)
                .and_then(|file| file.sync_all())
                .map_err(|error| {
                    AppError::new(
                        "audio_capture",
                        format!("Cannot save finalized segment: {error}"),
                    )
                })?;
            if let Some(parent) = sink.path.parent() {
                std::fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|error| {
                        AppError::new(
                            "audio_capture",
                            format!("Cannot save segment directory: {error}"),
                        )
                    })?;
            }
            let info = crate::audio::inspect(&sink.path)?;
            if info.frames == 0 {
                if accepted_frames > 0 {
                    return Err(AppError::new(
                        "audio_capture",
                        "Finalized segment lost accepted audio frames",
                    ));
                }
                return Ok(None);
            }
            let duration_seconds = info.frames as f64 / info.sample_rate;
            if duration_seconds + 1.0 / info.sample_rate
                < accepted_frames as f64 / self.format.mSampleRate
            {
                return Err(AppError::new(
                    "audio_capture",
                    "Finalized segment is shorter than the audio accepted during capture",
                ));
            }
            self.start_seconds = start_seconds + duration_seconds;
            Ok(Some(CapturedSegment {
                source: self.source,
                index,
                start_seconds,
                duration_seconds,
                path: sink.path.clone(),
            }))
        }
    }

    #[cfg(test)]
    mod rotation_tests {
        include!("recorder_native_rotation_tests.rs");
    }

    pub(super) struct NativeRecording {
        path: PathBuf,
        microphone_path: PathBuf,
        tap_id: AudioObjectID,
        aggregate_id: AudioObjectID,
        io_proc_id: AudioDeviceIOProcID,
        file: ExtAudioFileRef,
        callback: Option<Box<CallbackState>>,
        started: bool,
        microphone_device_id: AudioObjectID,
        microphone_io_proc_id: AudioDeviceIOProcID,
        microphone_file: ExtAudioFileRef,
        microphone_callback: Option<Box<CallbackState>>,
        microphone_started: bool,
        started_at: ContinuousClock,
        segmented: bool,
        system_segments: Option<SegmentWriter>,
        microphone_segments: Option<SegmentWriter>,
        rotation_warnings: Vec<String>,
        finalized_segments: Vec<CapturedSegment>,
    }

    // SAFETY: ownership moves only under Recorder's mutex. Core Audio accesses CallbackState
    // through its stable Box; cleanup closes its gate and drains admitted callbacks before dispose.
    unsafe impl Send for NativeRecording {}

    struct CallbackState {
        file: ExtAudioFileRef,
        active_sink: AtomicPtr<SegmentSink>,
        bytes_per_frame: u32,
        gate: CallbackGate,
        counters: SourceCounters,
        sample_rate: f64,
        started_at: ContinuousClock,
    }

    // SAFETY: file, format, and start time are immutable during capture; gate/counters are
    // atomic, and ExtAudioFileWriteAsync is explicitly supported from real-time callbacks.
    unsafe impl Send for CallbackState {}
    unsafe impl Sync for CallbackState {}

    #[derive(Default)]
    pub(super) struct CallbackGate {
        state: AtomicUsize,
    }

    pub(super) struct CallbackLease<'a> {
        gate: &'a CallbackGate,
    }

    impl CallbackGate {
        pub(super) fn try_enter(&self) -> Option<CallbackLease<'_>> {
            let mut state = self.state.load(Ordering::Acquire);
            loop {
                if state & CALLBACK_GATE_CLOSED != 0 || state == CALLBACK_GATE_CLOSED - 1 {
                    return None;
                }
                match self.state.compare_exchange_weak(
                    state,
                    state + 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => return Some(CallbackLease { gate: self }),
                    Err(current) => state = current,
                }
            }
        }

        pub(super) fn disable(&self) {
            self.state.fetch_or(CALLBACK_GATE_CLOSED, Ordering::AcqRel);
        }

        pub(super) fn is_idle(&self) -> bool {
            self.state.load(Ordering::Acquire) & !CALLBACK_GATE_CLOSED == 0
        }

        fn wait_for_idle(&self) {
            while !self.is_idle() {
                std::thread::yield_now();
            }
        }
    }

    impl Drop for CallbackLease<'_> {
        fn drop(&mut self) {
            self.gate.state.fetch_sub(1, Ordering::Release);
        }
    }

    impl NativeRecording {
        pub(super) fn start(
            path: &Path,
            microphone_path: &Path,
            segmented: bool,
        ) -> AppResult<Self> {
            let mut recording = Self::unstarted(path, microphone_path, segmented)?;
            // SAFETY: setup records owned resources before any later operation can fail.
            unsafe {
                if let Err(error) = recording.setup(path, microphone_path) {
                    return Err(recording.fail(error));
                }
            }
            Ok(recording)
        }

        fn unstarted(path: &Path, microphone_path: &Path, segmented: bool) -> AppResult<Self> {
            Ok(Self {
                path: path.to_owned(),
                microphone_path: microphone_path.to_owned(),
                tap_id: 0,
                aggregate_id: 0,
                io_proc_id: None,
                file: ptr::null_mut(),
                callback: None,
                started: false,
                microphone_device_id: 0,
                microphone_io_proc_id: None,
                microphone_file: ptr::null_mut(),
                microphone_callback: None,
                microphone_started: false,
                started_at: ContinuousClock::new()?,
                segmented,
                system_segments: None,
                microphone_segments: None,
                rotation_warnings: Vec::new(),
                finalized_segments: Vec::new(),
            })
        }

        pub(super) fn stop(mut self) -> AppResult<RecordingFiles> {
            // SAFETY: this value exclusively owns the registered IOProc and all handles below.
            let (errors, mut health) = unsafe { self.cleanup() };
            if let Some(health) = health.as_mut() {
                health
                    .warnings
                    .extend(errors.into_iter().map(|error| error.message));
            }
            // A failed source or teardown must not discard the other source's usable file.
            // Post-stop decoding determines which paths contain readable audio.
            Ok(RecordingFiles {
                system: self.path.clone(),
                microphone: self.microphone_path.clone(),
                health,
                segments: std::mem::take(&mut self.finalized_segments),
                segmented: self.segmented,
            })
        }

        pub(super) fn health(&self) -> AppResult<RecordingHealth> {
            let wall_seconds = self.started_at.elapsed().as_secs_f64();
            let (Some(system), Some(microphone)) = (&self.callback, &self.microphone_callback)
            else {
                return Err(AppError::new(
                    "recorder_state",
                    "Recording resources are missing",
                ));
            };
            let mut health = RecordingHealth::from_sources(
                wall_seconds,
                system
                    .counters
                    .snapshot(system.sample_rate, wall_seconds, false),
                microphone
                    .counters
                    .snapshot(microphone.sample_rate, wall_seconds, false),
            );
            health
                .warnings
                .extend(self.rotation_warnings.iter().cloned());
            Ok(health)
        }

        pub(super) fn rotate(&mut self) -> AppResult<Vec<CapturedSegment>> {
            let mut finalized = Vec::new();
            for (writer, callback) in [
                (&mut self.system_segments, &self.callback),
                (&mut self.microphone_segments, &self.microphone_callback),
            ] {
                if let (Some(writer), Some(callback)) = (writer, callback) {
                    match writer.rotate(callback) {
                        Ok(Some(segment)) => finalized.push(segment),
                        Ok(None) => {}
                        Err(error) => {
                            let warning = format!(
                                "{} segment finalization: {}. Captured files were kept.",
                                writer.source.label(),
                                error.message
                            );
                            if !self.rotation_warnings.contains(&warning) {
                                self.rotation_warnings.push(warning);
                            }
                        }
                    }
                }
            }
            Ok(finalized)
        }

        fn prepare_stream(
            &mut self,
            source: AudioSource,
            format: &AudioStreamBasicDescription,
        ) -> AppResult<()> {
            let (file, writer, callback, path) = match source {
                AudioSource::System => (
                    &mut self.file,
                    &mut self.system_segments,
                    &mut self.callback,
                    &self.path,
                ),
                AudioSource::Microphone => (
                    &mut self.microphone_file,
                    &mut self.microphone_segments,
                    &mut self.microphone_callback,
                    &self.microphone_path,
                ),
            };
            let active_sink = if self.segmented {
                *writer = Some(SegmentWriter::new(&self.path, source, format)?);
                writer.as_mut().expect("writer installed").active_ptr()
            } else {
                *file = create_audio_file(path, format)?;
                set_client_format(*file, format)?;
                set_bit_rate(*file)?;
                // SAFETY: file and client format are installed before capture starts.
                check_status("ExtAudioFileWriteAsync(prime)", unsafe {
                    ExtAudioFileWriteAsync(*file, 0, ptr::null())
                })?;
                ptr::null_mut()
            };
            *callback = Some(Box::new(CallbackState {
                file: *file,
                active_sink: AtomicPtr::new(active_sink),
                bytes_per_frame: format.mBytesPerFrame,
                gate: CallbackGate::default(),
                counters: SourceCounters::default(),
                sample_rate: format.mSampleRate,
                started_at: self.started_at,
            }));
            Ok(())
        }

        unsafe fn setup(&mut self, _path: &Path, microphone_path: &Path) -> AppResult<()> {
            let description = system_tap_description(process_audio_object()?);
            let tap_uid = description.UUID().UUIDString();

            check_status(
                "AudioHardwareCreateProcessTap",
                AudioHardwareCreateProcessTap(Some(&description), &mut self.tap_id),
            )?;
            if self.tap_id == 0 {
                return Err(AppError::new(
                    "audio_capture",
                    "AudioHardwareCreateProcessTap returned an invalid tap ID",
                ));
            }

            let tap_format = read_tap_format(self.tap_id)?;
            if tap_format.mChannelsPerFrame != 1 || tap_format.mBytesPerFrame == 0 {
                return Err(AppError::new(
                    "audio_capture",
                    "Core Audio tap did not provide a valid mono PCM format",
                ));
            }

            self.aggregate_id = create_aggregate_device(&tap_uid)?;
            wait_for_aggregate_device(self.aggregate_id)?;
            self.prepare_stream(AudioSource::System, &tap_format)?;
            let callback = self.callback.as_mut().expect("callback was just set");
            check_status(
                "AudioDeviceCreateIOProcID",
                AudioDeviceCreateIOProcID(
                    self.aggregate_id,
                    Some(audio_io_proc),
                    (&mut **callback as *mut CallbackState).cast(),
                    NonNull::from(&mut self.io_proc_id),
                ),
            )?;
            if self.io_proc_id.is_none() {
                return Err(AppError::new(
                    "audio_capture",
                    "AudioDeviceCreateIOProcID returned an invalid IO proc",
                ));
            }

            check_status(
                "AudioDeviceStart",
                AudioDeviceStart(self.aggregate_id, self.io_proc_id),
            )?;
            self.started = true;
            self.setup_microphone(microphone_path)
                .map_err(microphone_error)?;
            Ok(())
        }

        unsafe fn setup_microphone(&mut self, _path: &Path) -> AppResult<()> {
            let (device, input_format) = default_input_stream()?;
            self.microphone_device_id = device;
            self.prepare_stream(AudioSource::Microphone, &input_format)?;
            let callback = self
                .microphone_callback
                .as_mut()
                .expect("microphone callback was just set");
            check_status(
                "AudioDeviceCreateIOProcID(microphone)",
                AudioDeviceCreateIOProcID(
                    self.microphone_device_id,
                    Some(audio_io_proc),
                    (&mut **callback as *mut CallbackState).cast(),
                    NonNull::from(&mut self.microphone_io_proc_id),
                ),
            )?;
            if self.microphone_io_proc_id.is_none() {
                return Err(AppError::new(
                    "audio_capture",
                    "AudioDeviceCreateIOProcID returned an invalid microphone IO proc",
                ));
            }
            check_status(
                "AudioDeviceStart(microphone)",
                AudioDeviceStart(self.microphone_device_id, self.microphone_io_proc_id),
            )?;
            self.microphone_started = true;
            Ok(())
        }

        fn fail(&mut self, error: AppError) -> AppError {
            // SAFETY: setup has stopped and this value still exclusively owns every recorded handle.
            let (mut errors, _) = unsafe { self.cleanup() };
            errors.insert(0, error);
            combine_errors(errors)
        }

        unsafe fn cleanup(&mut self) -> (Vec<AppError>, Option<RecordingHealth>) {
            let wall_seconds = self.started_at.elapsed().as_secs_f64();
            // Close both gates together so sequential teardown cannot keep one source recording.
            for callback in [&self.callback, &self.microphone_callback]
                .into_iter()
                .flatten()
            {
                callback.gate.disable();
            }
            let mut errors = Vec::new();
            let microphone_health = cleanup_stream(
                &mut errors,
                "microphone",
                self.microphone_device_id,
                &mut self.microphone_io_proc_id,
                &mut self.microphone_started,
                &mut self.microphone_file,
                &mut self.microphone_callback,
                &mut self.microphone_segments,
                &mut self.finalized_segments,
                wall_seconds,
            );
            self.microphone_device_id = 0;
            let system_health = cleanup_stream(
                &mut errors,
                "system audio",
                self.aggregate_id,
                &mut self.io_proc_id,
                &mut self.started,
                &mut self.file,
                &mut self.callback,
                &mut self.system_segments,
                &mut self.finalized_segments,
                wall_seconds,
            );
            if self.aggregate_id != 0 {
                collect_status(
                    &mut errors,
                    "AudioHardwareDestroyAggregateDevice",
                    AudioHardwareDestroyAggregateDevice(self.aggregate_id),
                );
                self.aggregate_id = 0;
            }
            if self.tap_id != 0 {
                collect_status(
                    &mut errors,
                    "AudioHardwareDestroyProcessTap",
                    AudioHardwareDestroyProcessTap(self.tap_id),
                );
                self.tap_id = 0;
            }
            self.finalized_segments
                .sort_by_key(|segment| match segment.source {
                    AudioSource::System => 0,
                    AudioSource::Microphone => 1,
                });
            let health = system_health
                .zip(microphone_health)
                .map(|(system, microphone)| {
                    let mut health =
                        RecordingHealth::from_sources(wall_seconds, system, microphone);
                    health
                        .warnings
                        .extend(self.rotation_warnings.iter().cloned());
                    health
                });
            (errors, health)
        }
    }

    // Native handles remain in their owning recorder; grouping them just for teardown obscures ownership.
    #[allow(clippy::too_many_arguments)]
    unsafe fn cleanup_stream(
        errors: &mut Vec<AppError>,
        label: &str,
        device_id: AudioObjectID,
        io_proc_id: &mut AudioDeviceIOProcID,
        started: &mut bool,
        file: &mut ExtAudioFileRef,
        callback: &mut Option<Box<CallbackState>>,
        segments: &mut Option<SegmentWriter>,
        finalized: &mut Vec<CapturedSegment>,
        wall_seconds: f64,
    ) -> Option<SourceHealth> {
        if let Some(callback) = callback.as_ref() {
            callback.gate.disable();
        }
        if *started {
            collect_status(
                errors,
                &format!("AudioDeviceStop({label})"),
                AudioDeviceStop(device_id, *io_proc_id),
            );
            *started = false;
        }
        let mut callback_may_run = false;
        if io_proc_id.is_some() {
            let status = AudioDeviceDestroyIOProcID(device_id, *io_proc_id);
            callback_may_run = status != NO_ERR;
            collect_status(
                errors,
                &format!("AudioDeviceDestroyIOProcID({label})"),
                status,
            );
            *io_proc_id = None;
        }
        if let Some(callback) = callback.as_ref() {
            callback.gate.wait_for_idle();
            collect_status(
                errors,
                &format!("ExtAudioFileWriteAsync({label} callback)"),
                callback.counters.write_status.load(Ordering::Acquire),
            );
        }
        // Snapshot only after all admitted callbacks have left, before releasing their state.
        let health = callback.as_ref().map(|callback| {
            callback
                .counters
                .snapshot(callback.sample_rate, wall_seconds, true)
        });
        if let Some(mut writer) = segments.take() {
            match writer.finish() {
                Ok(Some(segment)) => finalized.push(segment),
                Ok(None) => {}
                Err(error) => errors.push(error),
            }
        }
        if !file.is_null() {
            collect_status(
                errors,
                &format!("ExtAudioFileDispose({label})"),
                ExtAudioFileDispose(*file),
            );
            *file = ptr::null_mut();
        }
        if callback_may_run {
            if let Some(callback) = callback.take() {
                let _ = Box::leak(callback);
            }
        } else {
            *callback = None;
        }
        health
    }

    impl Drop for NativeRecording {
        fn drop(&mut self) {
            // SAFETY: Drop is the final exclusive owner; cleanup is idempotent after stop/failure.
            let _ = unsafe { self.cleanup() };
        }
    }

    unsafe extern "C-unwind" fn audio_io_proc(
        _device: AudioObjectID,
        _now: NonNull<AudioTimeStamp>,
        input: NonNull<AudioBufferList>,
        _input_time: NonNull<AudioTimeStamp>,
        _output: NonNull<AudioBufferList>,
        _output_time: NonNull<AudioTimeStamp>,
        client_data: *mut c_void,
    ) -> i32 {
        let Some(state) = client_data.cast::<CallbackState>().as_ref() else {
            return NO_ERR;
        };
        write_input(state, input.as_ref());
        NO_ERR
    }

    unsafe fn write_input(state: &CallbackState, list: &AudioBufferList) {
        let Some(_lease) = state.gate.try_enter() else {
            return;
        };

        state
            .counters
            .callback(state.started_at.elapsed().as_millis() as u64);
        let Some(first_buffer) = list.mBuffers.first() else {
            return;
        };
        if list.mNumberBuffers == 0 || first_buffer.mData.is_null() {
            return;
        }
        let frames = first_buffer.mDataByteSize / state.bytes_per_frame;
        if frames == 0 {
            return;
        }

        state.counters.admit(frames);
        let status = if state.active_sink.load(Ordering::Acquire).is_null() {
            ExtAudioFileWriteAsync(state.file, frames, list)
        } else {
            loop {
                // Sinks have stable addresses until the outer callback gate is drained at stop.
                let sink = &*state.active_sink.load(Ordering::Acquire);
                let Some(_sink_lease) = sink.gate.try_enter() else {
                    continue;
                };
                let status =
                    ExtAudioFileWriteAsync(sink.file.load(Ordering::Acquire), frames, list);
                if status == NO_ERR {
                    sink.written_frames
                        .fetch_add(u64::from(frames), Ordering::Release);
                }
                break status;
            }
        };
        state.counters.complete_write(frames, status);
    }

    pub(super) fn system_tap_description(process_id: AudioObjectID) -> Retained<CATapDescription> {
        let processes = object_ids_to_nsarray(&[process_id]);
        // SAFETY: the retained NSArray contains valid Core Audio process object IDs.
        let description = unsafe {
            CATapDescription::initMonoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &processes,
            )
        };
        unsafe {
            description.setName(&NSString::from_str("Meeting Notes system audio"));
            description.setPrivate(true);
            description.setMuteBehavior(CATapMuteBehavior::Unmuted);
        }
        description
    }

    fn default_input_stream() -> AppResult<(AudioObjectID, AudioStreamBasicDescription)> {
        let device = read_scalar::<AudioObjectID>(
            kAudioObjectSystemObject as AudioObjectID,
            property_address(
                kAudioHardwarePropertyDefaultInputDevice,
                kAudioObjectPropertyScopeGlobal,
            ),
            "AudioObjectGetPropertyData(default input device)",
            None,
        )?;
        if device == 0 {
            return Err(AppError::new(
                "audio_capture",
                "Core Audio returned no default microphone",
            ));
        }
        let stream = device_streams(device, kAudioObjectPropertyScopeInput, "input")?
            .first()
            .copied()
            .filter(|stream| *stream != 0)
            .ok_or_else(|| {
                AppError::new(
                    "audio_capture",
                    "Default microphone returned no input stream",
                )
            })?;
        let format = read_scalar::<AudioStreamBasicDescription>(
            stream,
            property_address(
                kAudioStreamPropertyVirtualFormat,
                kAudioObjectPropertyScopeGlobal,
            ),
            "AudioObjectGetPropertyData(default microphone format)",
            None,
        )?;
        if format.mFormatID != kAudioFormatLinearPCM
            || format.mChannelsPerFrame == 0
            || format.mBytesPerFrame == 0
        {
            return Err(AppError::new(
                "audio_capture",
                "Default microphone did not provide a valid PCM format",
            ));
        }
        Ok((device, format))
    }

    fn device_streams(
        device: AudioObjectID,
        scope: u32,
        label: &str,
    ) -> AppResult<Vec<AudioObjectID>> {
        let stream_address = property_address(kAudioDevicePropertyStreams, scope);
        let mut stream_bytes = 0;
        // SAFETY: address and byte-count output are valid for the duration of the call.
        check_status(
            &format!("AudioObjectGetPropertyDataSize(default {label} streams)"),
            unsafe {
                AudioObjectGetPropertyDataSize(
                    device,
                    NonNull::from(&stream_address),
                    0,
                    ptr::null(),
                    NonNull::from(&mut stream_bytes),
                )
            },
        )?;
        if stream_bytes < size_of::<AudioObjectID>() as u32 {
            return Err(AppError::new(
                "audio_capture",
                format!("Default {label} device has no {label} stream"),
            ));
        }
        let mut streams = vec![0; stream_bytes as usize / size_of::<AudioObjectID>()];
        // SAFETY: streams has exactly the aligned writable capacity reported by Core Audio.
        check_status(
            &format!("AudioObjectGetPropertyData(default {label} streams)"),
            unsafe {
                AudioObjectGetPropertyData(
                    device,
                    NonNull::from(&stream_address),
                    0,
                    ptr::null(),
                    NonNull::from(&mut stream_bytes),
                    NonNull::new_unchecked(streams.as_mut_ptr().cast()),
                )
            },
        )?;
        Ok(streams)
    }

    fn process_audio_object() -> AppResult<AudioObjectID> {
        let pid = std::process::id() as i32;
        let object = read_scalar::<AudioObjectID>(
            kAudioObjectSystemObject as AudioObjectID,
            property_address(
                kAudioHardwarePropertyTranslatePIDToProcessObject,
                kAudioObjectPropertyScopeGlobal,
            ),
            "AudioObjectGetPropertyData(current process)",
            Some(&pid),
        )?;
        if object == 0 {
            Err(AppError::new(
                "audio_capture",
                "Core Audio returned no audio object for Meeting Notes",
            ))
        } else {
            Ok(object)
        }
    }

    fn read_tap_format(tap_id: AudioObjectID) -> AppResult<AudioStreamBasicDescription> {
        read_scalar(
            tap_id,
            property_address(kAudioTapPropertyFormat, kAudioObjectPropertyScopeGlobal),
            "AudioObjectGetPropertyData(tap format)",
            None,
        )
    }

    fn read_scalar<T>(
        object: AudioObjectID,
        address: AudioObjectPropertyAddress,
        operation: &str,
        qualifier: Option<&i32>,
    ) -> AppResult<T> {
        let mut value = std::mem::MaybeUninit::<T>::uninit();
        let mut size = size_of::<T>() as u32;
        let (qualifier_size, qualifier_ptr) = qualifier.map_or((0, ptr::null()), |value| {
            (size_of::<i32>() as u32, (value as *const i32).cast())
        });
        // SAFETY: value is writable for size_of::<T>(); Core Audio initializes it on success.
        check_status(operation, unsafe {
            AudioObjectGetPropertyData(
                object,
                NonNull::from(&address),
                qualifier_size,
                qualifier_ptr,
                NonNull::from(&mut size),
                NonNull::new_unchecked(value.as_mut_ptr().cast()),
            )
        })?;
        if size != size_of::<T>() as u32 {
            return Err(AppError::new(
                "audio_capture",
                format!(
                    "{operation} returned {size} bytes instead of {}",
                    size_of::<T>()
                ),
            ));
        }
        // SAFETY: a successful exact-size Core Audio property read initialized the value.
        Ok(unsafe { value.assume_init() })
    }

    fn create_aggregate_device(tap_uid: &NSString) -> AppResult<AudioObjectID> {
        let sub_tap: Retained<NSDictionary<NSString, NSObject>> =
            NSDictionary::from_slices::<NSString>(
                &[&cstr_key(kAudioSubTapUIDKey)],
                &[tap_uid.as_ref()],
            );
        let tap_list: Retained<NSArray<NSObject>> =
            NSArray::from_retained_slice(&[Retained::into_super(sub_tap)]);
        let name = NSString::from_str("Meeting Notes capture");
        let uid = NSString::from_str(&format!(
            "com.dweng.meetingnotes.tap.{}",
            uuid::Uuid::new_v4()
        ));
        let yes = NSNumber::numberWithBool(true);
        let keys = [
            cstr_key(kAudioAggregateDeviceNameKey),
            cstr_key(kAudioAggregateDeviceUIDKey),
            cstr_key(kAudioAggregateDeviceIsPrivateKey),
            cstr_key(kAudioAggregateDeviceTapAutoStartKey),
            cstr_key(kAudioAggregateDeviceTapListKey),
        ];
        let values: [&NSObject; 5] = [
            name.as_ref(),
            uid.as_ref(),
            yes.as_ref(),
            yes.as_ref(),
            tap_list.as_ref(),
        ];
        let key_refs = keys.iter().map(|key| key.as_ref()).collect::<Vec<_>>();
        let dictionary: Retained<NSDictionary<NSString, NSObject>> =
            NSDictionary::from_slices::<NSString>(&key_refs, &values);
        // SAFETY: NSDictionary and CFDictionary are toll-free bridged, and dictionary lives
        // through the call.
        let dictionary = unsafe { &*(Retained::as_ptr(&dictionary) as *const CFDictionary) };
        let mut aggregate_id = 0;
        // SAFETY: dictionary is valid and aggregate_id is a writable local.
        check_status("AudioHardwareCreateAggregateDevice", unsafe {
            AudioHardwareCreateAggregateDevice(dictionary, NonNull::from(&mut aggregate_id))
        })?;
        if aggregate_id == 0 {
            Err(AppError::new(
                "audio_capture",
                "AudioHardwareCreateAggregateDevice returned an invalid device ID",
            ))
        } else {
            Ok(aggregate_id)
        }
    }

    pub(crate) fn create_audio_file(
        path: &Path,
        tap_format: &AudioStreamBasicDescription,
    ) -> AppResult<ExtAudioFileRef> {
        let url = CFURL::from_file_path(path).ok_or_else(|| {
            AppError::new(
                "invalid_audio_path",
                "Recording path is not a valid file path",
            )
        })?;
        let mut file_format = AudioStreamBasicDescription {
            mSampleRate: tap_format.mSampleRate,
            mFormatID: kAudioFormatMPEG4AAC,
            mFormatFlags: 0,
            mBytesPerPacket: 0,
            mFramesPerPacket: 0,
            mBytesPerFrame: 0,
            mChannelsPerFrame: 1,
            mBitsPerChannel: 0,
            mReserved: 0,
        };
        let mut file = ptr::null_mut();
        // SAFETY: URL/format/output pointers remain valid for the call; no channel layout is used.
        check_status("ExtAudioFileCreateWithURL", unsafe {
            ExtAudioFileCreateWithURL(
                &url,
                kAudioFileM4AType,
                NonNull::from(&mut file_format),
                ptr::null(),
                AudioFileFlags::EraseFile.bits(),
                NonNull::from(&mut file),
            )
        })?;
        if file.is_null() {
            Err(AppError::new(
                "audio_capture",
                "ExtAudioFileCreateWithURL returned an invalid audio file",
            ))
        } else {
            Ok(file)
        }
    }

    fn wait_for_aggregate_device(device: AudioObjectID) -> AppResult<()> {
        wait_until_aggregate_ready(
            || {
                read_scalar::<u32>(
                    device,
                    property_address(
                        kAudioDevicePropertyDeviceIsAlive,
                        kAudioObjectPropertyScopeGlobal,
                    ),
                    "AudioObjectGetPropertyData(aggregate device alive)",
                    None,
                )
            },
            Duration::from_millis(100),
        )
    }

    pub(super) fn wait_until_aggregate_ready<F>(
        mut read_alive: F,
        retry_delay: Duration,
    ) -> AppResult<()>
    where
        F: FnMut() -> AppResult<u32>,
    {
        for _ in 0..30 {
            if matches!(read_alive(), Ok(alive) if alive != 0) {
                return Ok(());
            }
            thread::sleep(retry_delay);
        }
        Err(AppError::new(
            "audio_capture",
            "Core Audio capture device did not become ready",
        ))
    }

    pub(crate) fn set_client_format(
        file: ExtAudioFileRef,
        tap_format: &AudioStreamBasicDescription,
    ) -> AppResult<()> {
        // SAFETY: file is live and tap_format points to an exact-size ASBD.
        check_status("ExtAudioFileSetProperty(client format)", unsafe {
            ExtAudioFileSetProperty(
                file,
                kExtAudioFileProperty_ClientDataFormat,
                size_of::<AudioStreamBasicDescription>() as u32,
                NonNull::new_unchecked(
                    (tap_format as *const AudioStreamBasicDescription)
                        .cast_mut()
                        .cast(),
                ),
            )
        })
    }

    pub(crate) fn set_bit_rate(file: ExtAudioFileRef) -> AppResult<()> {
        let mut converter: AudioConverterRef = ptr::null_mut();
        let mut size = size_of::<AudioConverterRef>() as u32;
        // SAFETY: file is live and converter/size are valid outputs.
        check_status("ExtAudioFileGetProperty(audio converter)", unsafe {
            ExtAudioFileGetProperty(
                file,
                kExtAudioFileProperty_AudioConverter,
                NonNull::from(&mut size),
                NonNull::new_unchecked((&mut converter as *mut AudioConverterRef).cast()),
            )
        })?;
        if converter.is_null() {
            return Err(AppError::new(
                "audio_capture",
                "ExtAudioFileGetProperty returned an invalid audio converter",
            ));
        }
        let mut ranges_size = 0;
        // SAFETY: converter is live and ranges_size is a valid output.
        check_status(
            "AudioConverterGetPropertyInfo(applicable bit rates)",
            unsafe {
                AudioConverterGetPropertyInfo(
                    converter,
                    kAudioConverterApplicableEncodeBitRates,
                    &mut ranges_size,
                    ptr::null_mut(),
                )
            },
        )?;
        let range_size = size_of::<AudioValueRange>() as u32;
        if ranges_size == 0 || ranges_size % range_size != 0 {
            return Err(AppError::new(
                "audio_capture",
                "Audio converter returned invalid AAC bit-rate ranges",
            ));
        }
        let mut ranges = vec![
            AudioValueRange {
                mMinimum: 0.0,
                mMaximum: 0.0,
            };
            (ranges_size / range_size) as usize
        ];
        // SAFETY: ranges is sized from Core Audio's property info and both outputs are valid.
        check_status("AudioConverterGetProperty(applicable bit rates)", unsafe {
            AudioConverterGetProperty(
                converter,
                kAudioConverterApplicableEncodeBitRates,
                NonNull::from(&mut ranges_size),
                NonNull::new_unchecked(ranges.as_mut_ptr().cast()),
            )
        })?;
        if ranges_size % range_size != 0 {
            return Err(AppError::new(
                "audio_capture",
                "Audio converter returned invalid AAC bit-rate ranges",
            ));
        }
        ranges.truncate((ranges_size / range_size) as usize);
        let mut bit_rate = closest_bit_rate(&ranges, AAC_BIT_RATE).ok_or_else(|| {
            AppError::new(
                "audio_capture",
                "Audio converter did not report a usable AAC bit rate",
            )
        })?;
        // SAFETY: the converter is owned by the live ExtAudioFile; bit_rate is an exact-size value.
        check_status("AudioConverterSetProperty(encode bit rate)", unsafe {
            AudioConverterSetProperty(
                converter,
                kAudioConverterEncodeBitRate,
                size_of::<u32>() as u32,
                NonNull::from(&mut bit_rate).cast(),
            )
        })
    }

    pub(super) fn closest_bit_rate(ranges: &[AudioValueRange], target: u32) -> Option<u32> {
        ranges
            .iter()
            .filter(|range| {
                range.mMinimum.is_finite()
                    && range.mMaximum.is_finite()
                    && range.mMinimum >= 0.0
                    && range.mMinimum <= range.mMaximum
                    && range.mMaximum <= u32::MAX as f64
            })
            .map(|range| {
                (target as f64)
                    .clamp(range.mMinimum, range.mMaximum)
                    .round() as u32
            })
            .min_by_key(|candidate| (candidate.abs_diff(target), *candidate))
    }

    fn property_address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: kAudioObjectPropertyElementMain,
        }
    }

    fn object_ids_to_nsarray(ids: &[AudioObjectID]) -> Retained<NSArray<NSNumber>> {
        let ids = ids
            .iter()
            .map(|id| NSNumber::numberWithUnsignedInt(*id))
            .collect::<Vec<_>>();
        NSArray::from_retained_slice(&ids)
    }

    fn cstr_key(key: &CStr) -> Retained<NSString> {
        NSString::from_str(key.to_str().expect("Core Audio keys are ASCII"))
    }

    fn check_status(operation: &str, status: i32) -> AppResult<()> {
        if status == NO_ERR {
            Ok(())
        } else {
            Err(status_error(operation, status))
        }
    }

    fn collect_status(errors: &mut Vec<AppError>, operation: &str, status: i32) {
        if status != NO_ERR {
            errors.push(status_error(operation, status));
        }
    }

    pub(super) fn status_error(operation: &str, status: i32) -> AppError {
        AppError::new(
            if status == AUDIO_PERMISSION_DENIED {
                "audio_permission"
            } else {
                "audio_capture"
            },
            format!("{operation} failed with OSStatus {status}"),
        )
    }

    pub(super) fn microphone_error(error: AppError) -> AppError {
        AppError::new(
            if error.code == "audio_permission" {
                "microphone_permission"
            } else {
                "microphone_capture"
            },
            error.message,
        )
    }

    pub(super) fn combine_errors(errors: Vec<AppError>) -> AppError {
        let code = errors
            .first()
            .map(|error| error.code.clone())
            .unwrap_or_else(|| "audio_capture".into());
        AppError::new(
            code,
            errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use objc2_core_audio_types::{kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked};

        #[test]
        fn continuous_capture_clock_counts_sleep_in_wall_duration() {
            let clock = ContinuousClock {
                started_ticks: 120,
                timebase: MachTimebase {
                    numer: 125,
                    denom: 3,
                },
            };
            // Sixty seconds of active capture plus sixty seconds asleep: continuous ticks
            // advance by 120 seconds even though the active/uptime clock advances by only 60.
            let wall = clock.elapsed_at(2_880_000_120);
            assert_eq!(wall, Duration::from_secs(120));
            assert_eq!(clock.elapsed_at(123), Duration::from_nanos(125));
            assert_eq!(clock.elapsed_at(119), Duration::ZERO);
            let counters = SourceCounters::default();
            counters.callback(60_000);
            counters.admit(60 * 48_000);
            counters.complete_write(60 * 48_000, 0);
            let live = counters.snapshot(48_000.0, wall.as_secs_f64(), false);
            assert_eq!(live.status, crate::recorder::CaptureStatus::Stalled);
            assert_eq!(live.last_callback_age_seconds, Some(60.0));
            assert_eq!(
                counters.snapshot(48_000.0, wall.as_secs_f64(), true).status,
                crate::recorder::CaptureStatus::ShortCapture
            );
        }

        #[test]
        fn continuous_capture_clock_reads_monotonic_native_time() {
            let clock = ContinuousClock::new().unwrap();
            assert!(clock.started_ticks > 0);
            let first = clock.elapsed();
            let second = clock.elapsed();
            assert!(second >= first);
        }

        #[test]
        fn stop_preserves_healthy_source_when_other_source_failed_to_write() {
            let started_at = ContinuousClock::new().unwrap();
            let callback = |status| {
                let counters = SourceCounters::default();
                counters.callback(0);
                counters.admit(48_000);
                counters.complete_write(48_000, status);
                Some(Box::new(CallbackState {
                    file: ptr::null_mut(),
                    active_sink: AtomicPtr::new(ptr::null_mut()),
                    bytes_per_frame: 4,
                    gate: CallbackGate::default(),
                    counters,
                    sample_rate: 48_000.0,
                    started_at,
                }))
            };
            // No native handles are installed: this exercises final accounting and the error
            // path without opening a device or recording microphone/system audio.
            let recording = NativeRecording {
                path: PathBuf::from("system.m4a"),
                microphone_path: PathBuf::from("microphone.m4a"),
                tap_id: 0,
                aggregate_id: 0,
                io_proc_id: None,
                file: ptr::null_mut(),
                callback: callback(-50),
                started: false,
                microphone_device_id: 0,
                microphone_io_proc_id: None,
                microphone_file: ptr::null_mut(),
                microphone_callback: callback(0),
                microphone_started: false,
                started_at,
                segmented: false,
                system_segments: None,
                microphone_segments: None,
                rotation_warnings: Vec::new(),
                finalized_segments: Vec::new(),
            };
            let files = recording.stop().unwrap();
            assert_eq!(files.microphone, PathBuf::from("microphone.m4a"));
            let health = files.health.unwrap();
            assert_eq!(health.system.written_frames, 0);
            assert_eq!(health.microphone.written_frames, 48_000);
            assert_eq!(health.system.write_error, Some(-50));
            assert!(health
                .warnings
                .iter()
                .any(|warning| warning.contains("OSStatus -50")));
        }

        #[test]
        fn configures_a_supported_aac_bit_rate_on_this_mac() {
            let path = std::env::temp_dir().join(format!(
                "meeting-notes-aac-compat-{}.m4a",
                std::process::id()
            ));
            let client_format = AudioStreamBasicDescription {
                mSampleRate: 48_000.0,
                mFormatID: kAudioFormatLinearPCM,
                mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
                mBytesPerPacket: 4,
                mFramesPerPacket: 1,
                mBytesPerFrame: 4,
                mChannelsPerFrame: 1,
                mBitsPerChannel: 32,
                mReserved: 0,
            };
            let file = create_audio_file(&path, &client_format).unwrap();
            set_client_format(file, &client_format).unwrap();

            let result = set_bit_rate(file);
            // SAFETY: file is live and no callback can access this test-only file.
            let dispose_status = unsafe { ExtAudioFileDispose(file) };
            let _ = std::fs::remove_file(path);

            result.unwrap();
            assert_eq!(dispose_status, NO_ERR);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_health_counts_only_successfully_written_frames() {
        let counters = SourceCounters::default();
        counters.callback(1_000);
        counters.admit(48_000);
        counters.complete_write(48_000, 0);
        counters.admit(24_000);
        counters.complete_write(24_000, -50);
        counters.complete_write(0, -51);
        let health = counters.snapshot(48_000.0, 2.0, false);
        assert_eq!(health.admitted_frames, 72_000);
        assert_eq!(health.written_frames, 48_000);
        assert_eq!(health.captured_seconds, 1.0);
        assert_eq!(health.last_callback_age_seconds, Some(1.0));
        assert_eq!(health.write_error, Some(-50));
        assert_eq!(health.status, CaptureStatus::WriteError);
    }

    #[test]
    fn capture_health_warns_for_no_frames_and_stalls_then_recovers() {
        let counters = SourceCounters::default();
        assert_eq!(
            counters.snapshot(48_000.0, 2.0, false).status,
            CaptureStatus::Starting
        );
        assert_eq!(
            counters.snapshot(48_000.0, 11.0, false).status,
            CaptureStatus::NoFrames
        );
        counters.callback(12_000);
        counters.admit(48_000);
        counters.complete_write(48_000, 0);
        assert_eq!(
            counters.snapshot(48_000.0, 12.0, false).status,
            CaptureStatus::Healthy
        );
        assert_eq!(
            counters.snapshot(48_000.0, 23.0, false).status,
            CaptureStatus::Stalled
        );
        counters.callback(24_000);
        counters.admit(48_000);
        counters.complete_write(48_000, 0);
        assert_eq!(
            counters.snapshot(48_000.0, 24.0, false).status,
            CaptureStatus::Healthy
        );
    }

    #[test]
    fn capture_health_empty_callbacks_do_not_hide_a_stalled_source() {
        let counters = SourceCounters::default();
        counters.callback(1_000);
        counters.admit(48_000);
        counters.complete_write(48_000, 0);
        counters.callback(12_000);
        counters.complete_write(0, 0);
        let health = counters.snapshot(48_000.0, 12.0, false);
        assert_eq!(health.last_callback_age_seconds, Some(0.0));
        assert_eq!(health.status, CaptureStatus::Stalled);
    }

    #[test]
    fn capture_health_keeps_sources_independent_and_warns_about_short_capture() {
        let system = SourceCounters::default();
        let microphone = SourceCounters::default();
        microphone.callback(120_000);
        microphone.admit(60 * 48_000);
        microphone.complete_write(60 * 48_000, 0);
        let health = RecordingHealth::from_sources(
            120.0,
            system.snapshot(48_000.0, 120.0, true),
            microphone.snapshot(48_000.0, 120.0, true),
        );
        assert_eq!(health.system.status, CaptureStatus::NoFrames);
        assert_eq!(health.microphone.status, CaptureStatus::ShortCapture);
        assert_eq!(health.microphone.captured_seconds, 60.0);
        assert!(health
            .warnings
            .iter()
            .any(|warning| warning.contains("System audio")));
        assert!(health
            .warnings
            .iter()
            .any(|warning| warning.contains("Microphone")));
    }

    #[test]
    fn capture_health_does_not_confuse_silent_frames_with_missing_capture() {
        let counters = SourceCounters::default();
        counters.callback(60_000);
        counters.admit(60 * 48_000);
        counters.complete_write(60 * 48_000, 0);
        assert_eq!(
            counters.snapshot(48_000.0, 60.0, true).status,
            CaptureStatus::Healthy
        );
        assert_eq!(
            SourceCounters::default()
                .snapshot(48_000.0, 0.1, true)
                .status,
            CaptureStatus::NoFrames
        );
    }

    #[test]
    fn capture_health_checks_real_duration_without_rounding_away_loss() {
        let counters = SourceCounters::default();
        counters.callback(4_000);
        counters.admit(48_000);
        counters.complete_write(48_000, 0);
        assert_eq!(
            counters.snapshot(48_000.0, 4.0, true).status,
            CaptureStatus::ShortCapture
        );
        assert_eq!(
            counters.snapshot(48_000.0, 1.01, true).status,
            CaptureStatus::Healthy
        );
    }

    #[test]
    fn executable_identity_detects_replacement_and_disappearance() {
        let directory =
            std::env::temp_dir().join(format!("capture-identity-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("app");
        std::fs::write(&path, b"old").unwrap();
        let identity = ExecutableIdentity::read(path.clone());
        assert!(!identity.changed());
        let replacement = directory.join("new-app");
        std::fs::write(&replacement, b"new").unwrap();
        std::fs::rename(replacement, &path).unwrap();
        assert!(identity.changed());
        std::fs::remove_file(&path).unwrap();
        assert!(identity.changed());
        std::fs::remove_dir(directory).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn system_audio_tap_is_not_pinned_to_one_output_device() {
        let description = native::system_tap_description(42);

        assert!(unsafe { description.deviceUID() }.is_none());
    }

    #[test]
    fn a_second_session_cannot_steal_the_recorder() {
        let mut slot = RecordingSlot::default();
        slot.reserve("first").unwrap();
        let error = slot.reserve("second").unwrap_err();
        assert_eq!(error.code, "already_recording");
    }

    #[test]
    fn a_different_session_cannot_release_the_recorder() {
        let mut slot = RecordingSlot::default();
        slot.reserve("first").unwrap();
        let error = slot.release("second").unwrap_err();
        assert_eq!(error.code, "recording_session_mismatch");
        assert_eq!(slot.session_id.as_deref(), Some("first"));
    }

    #[test]
    fn a_failed_stop_releases_the_recorder_slot() {
        let recorder = Recorder::new();
        recorder.slot.lock().unwrap().reserve("first").unwrap();

        let error = recorder.stop("first").unwrap_err();

        assert_eq!(error.code, "recorder_state");
        assert!(!recorder.is_recording());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn disabled_callback_gate_rejects_new_work() {
        let gate = native::CallbackGate::default();

        gate.disable();

        assert!(gate.try_enter().is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn callback_gate_tracks_work_that_entered_before_disable() {
        let gate = native::CallbackGate::default();
        let lease = gate.try_enter().unwrap();

        gate.disable();

        assert!(!gate.is_idle());
        drop(lease);
        assert!(gate.is_idle());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn microphone_start_failure_keeps_its_error_code() {
        let error = native::combine_errors(vec![AppError::new(
            "microphone_capture",
            "Microphone permission was denied",
        )]);

        assert_eq!(error.code, "microphone_capture");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn microphone_errors_only_use_permission_guidance_for_permission_denials() {
        assert_eq!(
            native::microphone_error(AppError::new("audio_permission", "denied")).code,
            "microphone_permission"
        );
        assert_eq!(
            native::microphone_error(AppError::new("audio_capture", "codec failed")).code,
            "microphone_capture"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn unsupported_target_uses_the_nearest_applicable_aac_bit_rate() {
        use objc2_core_audio_types::AudioValueRange;

        let ranges = [
            AudioValueRange {
                mMinimum: 32_000.0,
                mMaximum: 32_000.0,
            },
            AudioValueRange {
                mMinimum: 40_000.0,
                mMaximum: 40_000.0,
            },
        ];

        assert_eq!(native::closest_bit_rate(&ranges, 24_000), Some(32_000));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn only_the_core_audio_permission_status_is_labeled_as_permission_failure() {
        assert_eq!(
            native::status_error("capture", -66748).code,
            "audio_permission"
        );
        assert_eq!(
            native::status_error("capture", 560226676).code,
            "audio_capture"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn capture_waits_until_the_aggregate_device_reports_alive() {
        let mut reads = 0;

        native::wait_until_aggregate_ready(
            || {
                reads += 1;
                Ok(u32::from(reads == 3))
            },
            std::time::Duration::ZERO,
        )
        .unwrap();

        assert_eq!(reads, 3);
    }
}
