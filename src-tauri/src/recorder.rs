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

    pub fn start(&self, session_id: &str, path: &Path) -> AppResult<RecordingInfo> {
        if session_id.is_empty() {
            return Err(AppError::new(
                "invalid_session_id",
                "Session ID is required",
            ));
        }

        let mut slot = self.lock_slot()?;
        slot.reserve(session_id)?;

        #[cfg(target_os = "macos")]
        match native::NativeRecording::start(path) {
            Ok(recording) => slot.recording = Some(recording),
            Err(error) => {
                slot.session_id = None;
                return Err(error);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
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

    pub fn stop(&self, session_id: &str) -> AppResult<PathBuf> {
        let mut slot = self.lock_slot()?;
        if slot.session_id.as_deref() != Some(session_id) {
            return Err(AppError::new(
                "recording_session_mismatch",
                "Recording belongs to a different session",
            ));
        }

        #[cfg(target_os = "macos")]
        let result = slot
            .recording
            .take()
            .ok_or_else(|| AppError::new("recorder_state", "Recording resources are missing"))?
            .stop();

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
        sync::atomic::{AtomicBool, AtomicI32, Ordering},
    };

    use objc2::{rc::Retained, AnyThread};
    use objc2_audio_toolbox::{
        kAudioConverterEncodeBitRate, kAudioFileM4AType, kExtAudioFileProperty_AudioConverter,
        kExtAudioFileProperty_ClientDataFormat, AudioConverterRef, AudioConverterSetProperty,
        AudioFileFlags, ExtAudioFileCreateWithURL, ExtAudioFileDispose, ExtAudioFileGetProperty,
        ExtAudioFileRef, ExtAudioFileSetProperty, ExtAudioFileWriteAsync,
    };
    use objc2_core_audio::{
        kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
        kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
        kAudioAggregateDeviceUIDKey, kAudioDevicePropertyDeviceUID, kAudioDevicePropertyStreams,
        kAudioHardwarePropertyDefaultOutputDevice,
        kAudioHardwarePropertyTranslatePIDToProcessObject, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
        kAudioSubTapUIDKey, kAudioTapPropertyFormat, AudioDeviceCreateIOProcID,
        AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop,
        AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap,
        AudioHardwareDestroyAggregateDevice, AudioHardwareDestroyProcessTap,
        AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
        AudioObjectPropertyAddress, CATapDescription, CATapMuteBehavior,
    };
    use objc2_core_audio_types::{
        kAudioFormatMPEG4AAC, AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp,
    };
    use objc2_core_foundation::{CFDictionary, CFRetained, CFString, CFURL};
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSString};

    use crate::domain::{AppError, AppResult};

    const NO_ERR: i32 = 0;
    const AAC_BIT_RATE: u32 = 48_000;

    pub(super) struct NativeRecording {
        path: PathBuf,
        tap_id: AudioObjectID,
        aggregate_id: AudioObjectID,
        io_proc_id: AudioDeviceIOProcID,
        file: ExtAudioFileRef,
        callback: Option<Box<CallbackState>>,
        started: bool,
    }

    // SAFETY: ownership moves only under Recorder's mutex. Core Audio accesses CallbackState
    // through its stable Box; cleanup disables that callback before touching owned resources.
    unsafe impl Send for NativeRecording {}

    struct CallbackState {
        file: ExtAudioFileRef,
        bytes_per_frame: u32,
        active: AtomicBool,
        write_status: AtomicI32,
    }

    // SAFETY: file and bytes_per_frame are immutable during capture; active/write_status are
    // atomic, and ExtAudioFileWriteAsync is explicitly supported from real-time callbacks.
    unsafe impl Send for CallbackState {}
    unsafe impl Sync for CallbackState {}

    impl NativeRecording {
        pub(super) fn start(path: &Path) -> AppResult<Self> {
            let mut recording = Self {
                path: path.to_owned(),
                tap_id: 0,
                aggregate_id: 0,
                io_proc_id: None,
                file: ptr::null_mut(),
                callback: None,
                started: false,
            };

            // SAFETY: setup owns every returned resource and records it immediately so any later
            // error can run the same complete reverse-order teardown as stop.
            unsafe {
                if let Err(error) = recording.setup(path) {
                    return Err(recording.fail(error));
                }
            }
            Ok(recording)
        }

        pub(super) fn stop(mut self) -> AppResult<PathBuf> {
            // SAFETY: this value exclusively owns the registered IOProc and all handles below.
            let errors = unsafe { self.cleanup() };
            if errors.is_empty() {
                Ok(self.path.clone())
            } else {
                Err(combine_errors(errors))
            }
        }

        unsafe fn setup(&mut self, path: &Path) -> AppResult<()> {
            let (device_uid, stream_index) = default_output_stream()?;
            let process_id = process_audio_object()?;
            let processes = object_ids_to_nsarray(&[process_id]);
            let device_uid = NSString::from_str(&device_uid);
            let description = CATapDescription::initExcludingProcesses_andDeviceUID_withStream(
                CATapDescription::alloc(),
                &processes,
                &device_uid,
                stream_index,
            );
            description.setName(&NSString::from_str("Meeting Notes system audio"));
            description.setPrivate(true);
            description.setMuteBehavior(CATapMuteBehavior::Unmuted);
            description.setExclusive(true);
            description.setMixdown(true);
            description.setMono(true);
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
                active: AtomicBool::new(true),
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
            if let Some(callback) = &self.callback {
                callback.active.store(false, Ordering::Release);
            }

            if self.started {
                collect_status(
                    &mut errors,
                    "AudioDeviceStop",
                    AudioDeviceStop(self.aggregate_id, self.io_proc_id),
                );
                self.started = false;
            }

            let mut callback_may_run = false;
            if self.io_proc_id.is_some() {
                let status = AudioDeviceDestroyIOProcID(self.aggregate_id, self.io_proc_id);
                callback_may_run = status != NO_ERR;
                collect_status(&mut errors, "AudioDeviceDestroyIOProcID", status);
                self.io_proc_id = None;
            }
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

            if let Some(callback) = &self.callback {
                let status = callback.write_status.load(Ordering::Acquire);
                collect_status(&mut errors, "ExtAudioFileWriteAsync(callback)", status);
            }
            if !self.file.is_null() {
                collect_status(
                    &mut errors,
                    "ExtAudioFileDispose",
                    ExtAudioFileDispose(self.file),
                );
                self.file = ptr::null_mut();
            }

            if callback_may_run {
                if let Some(callback) = self.callback.take() {
                    let _ = Box::leak(callback);
                }
            } else {
                self.callback = None;
            }
            errors
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
        if !state.active.load(Ordering::Acquire) {
            return NO_ERR;
        }

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

    fn default_output_stream() -> AppResult<(String, isize)> {
        let device = read_scalar::<AudioObjectID>(
            kAudioObjectSystemObject as AudioObjectID,
            property_address(
                kAudioHardwarePropertyDefaultOutputDevice,
                kAudioObjectPropertyScopeGlobal,
            ),
            "AudioObjectGetPropertyData(default output device)",
            None,
        )?;
        if device == 0 {
            return Err(AppError::new(
                "audio_capture",
                "Core Audio returned no default output device",
            ));
        }

        let stream_address =
            property_address(kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput);
        let mut stream_bytes = 0;
        // SAFETY: address and byte-count output are valid for the duration of the call.
        check_status(
            "AudioObjectGetPropertyDataSize(default output streams)",
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
                "Default output device has no output stream",
            ));
        }
        let mut streams = vec![0; stream_bytes as usize / size_of::<AudioObjectID>()];
        // SAFETY: streams has exactly the aligned writable capacity reported by Core Audio.
        check_status(
            "AudioObjectGetPropertyData(default output streams)",
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
        if streams.first().copied().unwrap_or(0) == 0 {
            return Err(AppError::new(
                "audio_capture",
                "Default output device returned an invalid output stream",
            ));
        }

        Ok((read_device_uid(device)?, 0))
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

    fn read_device_uid(device: AudioObjectID) -> AppResult<String> {
        let address = property_address(
            kAudioDevicePropertyDeviceUID,
            kAudioObjectPropertyScopeGlobal,
        );
        let mut uid = ptr::null::<CFString>();
        let mut size = size_of::<*const CFString>() as u32;
        // SAFETY: uid receives one retained CFString pointer; all other pointers are valid locals.
        check_status(
            "AudioObjectGetPropertyData(default output device UID)",
            unsafe {
                AudioObjectGetPropertyData(
                    device,
                    NonNull::from(&address),
                    0,
                    ptr::null(),
                    NonNull::from(&mut size),
                    NonNull::new_unchecked((&mut uid as *mut *const CFString).cast()),
                )
            },
        )?;
        let uid = NonNull::new(uid.cast_mut()).ok_or_else(|| {
            AppError::new(
                "audio_capture",
                "Default output device returned no device UID",
            )
        })?;
        // SAFETY: Core Audio's CFString property follows the Create/Copy ownership convention.
        Ok(unsafe { CFRetained::from_raw(uid) }.to_string())
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
        let mut bit_rate = AAC_BIT_RATE;
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

    fn status_error(operation: &str, status: i32) -> AppError {
        AppError::new(
            "audio_capture",
            format!("{operation} failed with OSStatus {status}"),
        )
    }

    fn combine_errors(errors: Vec<AppError>) -> AppError {
        AppError::new(
            "audio_capture",
            errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
