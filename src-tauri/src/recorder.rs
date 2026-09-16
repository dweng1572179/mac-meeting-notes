use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::{SecondsFormat, Utc};

use crate::domain::{AppError, AppResult};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingInfo {
    pub session_id: String,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordingFiles {
    pub system: PathBuf,
    pub microphone: PathBuf,
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
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            slot: Mutex::new(RecordingSlot::default()),
        }
    }

    pub fn start(
        &self,
        session_id: &str,
        system_path: &Path,
        microphone_path: &Path,
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
        match native::NativeRecording::start(system_path, microphone_path) {
            Ok(recording) => slot.recording = Some(recording),
            Err(error) => {
                slot.session_id = None;
                return Err(error);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (system_path, microphone_path);
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

        let release_result = slot.release(session_id);
        match (result, release_result) {
            (Ok(path), Ok(())) => Ok(path),
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        }
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
mod native {
    use std::{
        ffi::{c_void, CStr},
        path::{Path, PathBuf},
        ptr::{self, NonNull},
        sync::atomic::{AtomicI32, AtomicUsize, Ordering},
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
        domain::{AppError, AppResult},
        recorder::RecordingFiles,
    };

    const NO_ERR: i32 = 0;
    const AAC_BIT_RATE: u32 = 24_000;
    const AUDIO_PERMISSION_DENIED: i32 = -66748;
    const CALLBACK_GATE_CLOSED: usize = 1 << (usize::BITS - 1);

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
    }

    // SAFETY: ownership moves only under Recorder's mutex. Core Audio accesses CallbackState
    // through its stable Box; cleanup closes its gate and drains admitted callbacks before dispose.
    unsafe impl Send for NativeRecording {}

    struct CallbackState {
        file: ExtAudioFileRef,
        bytes_per_frame: u32,
        gate: CallbackGate,
        write_status: AtomicI32,
    }

    // SAFETY: file and bytes_per_frame are immutable during capture; gate/write_status are
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
        pub(super) fn start(path: &Path, microphone_path: &Path) -> AppResult<Self> {
            let mut recording = Self {
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
            };

            // SAFETY: setup owns every returned resource and records it immediately so any later
            // error can run the same complete reverse-order teardown as stop.
            unsafe {
                if let Err(error) = recording.setup(path, microphone_path) {
                    return Err(recording.fail(error));
                }
            }
            Ok(recording)
        }

        pub(super) fn stop(mut self) -> AppResult<RecordingFiles> {
            // SAFETY: this value exclusively owns the registered IOProc and all handles below.
            let errors = unsafe { self.cleanup() };
            if errors.is_empty() {
                Ok(RecordingFiles {
                    system: self.path.clone(),
                    microphone: self.microphone_path.clone(),
                })
            } else {
                Err(combine_errors(errors))
            }
        }

        unsafe fn setup(&mut self, path: &Path, microphone_path: &Path) -> AppResult<()> {
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
            self.file = create_audio_file(path, &tap_format)?;
            set_client_format(self.file, &tap_format)?;
            set_bit_rate(self.file)?;
            check_status(
                "ExtAudioFileWriteAsync(prime)",
                ExtAudioFileWriteAsync(self.file, 0, ptr::null()),
            )?;

            self.callback = Some(Box::new(CallbackState {
                file: self.file,
                bytes_per_frame: tap_format.mBytesPerFrame,
                gate: CallbackGate::default(),
                write_status: AtomicI32::new(NO_ERR),
            }));
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

        unsafe fn setup_microphone(&mut self, path: &Path) -> AppResult<()> {
            let (device, input_format) = default_input_stream()?;
            self.microphone_device_id = device;
            self.microphone_file = create_audio_file(path, &input_format)?;
            set_client_format(self.microphone_file, &input_format)?;
            set_bit_rate(self.microphone_file)?;
            check_status(
                "ExtAudioFileWriteAsync(microphone prime)",
                ExtAudioFileWriteAsync(self.microphone_file, 0, ptr::null()),
            )?;

            self.microphone_callback = Some(Box::new(CallbackState {
                file: self.microphone_file,
                bytes_per_frame: input_format.mBytesPerFrame,
                gate: CallbackGate::default(),
                write_status: AtomicI32::new(NO_ERR),
            }));
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
            let mut errors = unsafe { self.cleanup() };
            errors.insert(0, error);
            combine_errors(errors)
        }

        unsafe fn cleanup(&mut self) -> Vec<AppError> {
            let mut errors = Vec::new();
            cleanup_stream(
                &mut errors,
                "microphone",
                self.microphone_device_id,
                &mut self.microphone_io_proc_id,
                &mut self.microphone_started,
                &mut self.microphone_file,
                &mut self.microphone_callback,
            );
            self.microphone_device_id = 0;
            cleanup_stream(
                &mut errors,
                "system audio",
                self.aggregate_id,
                &mut self.io_proc_id,
                &mut self.started,
                &mut self.file,
                &mut self.callback,
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
            errors
        }
    }

    unsafe fn cleanup_stream(
        errors: &mut Vec<AppError>,
        label: &str,
        device_id: AudioObjectID,
        io_proc_id: &mut AudioDeviceIOProcID,
        started: &mut bool,
        file: &mut ExtAudioFileRef,
        callback: &mut Option<Box<CallbackState>>,
    ) {
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
                callback.write_status.load(Ordering::Acquire),
            );
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
        let Some(_lease) = state.gate.try_enter() else {
            return NO_ERR;
        };

        let list = input.as_ref();
        let Some(first_buffer) = list.mBuffers.first() else {
            return NO_ERR;
        };
        if list.mNumberBuffers == 0 || first_buffer.mData.is_null() {
            return NO_ERR;
        }
        let frames = first_buffer.mDataByteSize / state.bytes_per_frame;
        if frames == 0 {
            return NO_ERR;
        }

        let status = ExtAudioFileWriteAsync(state.file, frames, input.as_ptr());
        if status != NO_ERR {
            let _ = state.write_status.compare_exchange(
                NO_ERR,
                status,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        NO_ERR
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

    fn create_audio_file(
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

    fn set_client_format(
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

    fn set_bit_rate(file: ExtAudioFileRef) -> AppResult<()> {
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
