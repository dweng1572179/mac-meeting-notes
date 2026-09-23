#[cfg(test)]
mod tests {
    use super::*;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("meeting-windows-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn packet(source: AudioSource, start_frame: u64, frames: usize) -> Packet {
        Packet {
            source,
            start_frame,
            captured_ms: start_frame * 1_000 / u64::from(SAMPLE_RATE),
            pcm: [1i16.to_le_bytes()].repeat(frames).into_iter().flatten().collect(),
        }
    }

    fn writer(directory: &Directory, shared: Arc<Shared>) -> RecordingWriter {
        RecordingWriter::new(
            directory.0.join("test.wav"),
            directory.0.join("test-mic.wav"),
            true,
            shared,
        )
    }

    #[test]
    fn automatic_rotation_keeps_every_frame_and_final_partial_section() {
        let directory = Directory::new();
        let shared = Arc::new(Shared::new());
        let mut writer = writer(&directory, shared.clone());
        writer.write_packet(packet(AudioSource::Microphone, 0, 61 * SAMPLE_RATE as usize));
        let first = shared.take_segments().unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].duration_seconds, 60.0);
        writer.finish();
        let tail = shared.take_segments().unwrap();
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].start_seconds, 60.0);
        assert_eq!(tail[0].duration_seconds, 1.0);
        assert_eq!(crate::audio::inspect(&first[0].path).unwrap().frames, 960_000);
        assert_eq!(crate::audio::inspect(&tail[0].path).unwrap().frames, 16_000);
        assert_eq!(shared.counters[1].written_frames.load(Ordering::Acquire), 976_000);
    }

    #[test]
    fn a_capture_gap_preserves_the_next_sections_true_time() {
        let directory = Directory::new();
        let shared = Arc::new(Shared::new());
        let mut writer = writer(&directory, shared.clone());
        writer.write_packet(packet(AudioSource::System, 0, SAMPLE_RATE as usize));
        writer.write_packet(packet(AudioSource::System, 5 * u64::from(SAMPLE_RATE), SAMPLE_RATE as usize));
        writer.finish();
        let sections = shared.take_segments().unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].start_seconds, 0.0);
        assert_eq!(sections[0].duration_seconds, 1.0);
        assert_eq!(sections[1].start_seconds, 5.0);
        assert_eq!(sections[1].duration_seconds, 1.0);
        assert!(shared.health(false).unwrap().warnings.iter().any(|warning| warning.contains("gap")));
    }

    #[test]
    fn queue_overflow_is_visible_and_is_not_counted_as_written_audio() {
        let shared = Shared::new();
        let (sender, _receiver) = mpsc::sync_channel(1);
        queue_packet(&sender, packet(AudioSource::System, 0, 16), &shared).unwrap();
        queue_packet(&sender, packet(AudioSource::System, 16, 16), &shared).unwrap();
        let health = shared.health(false).unwrap();
        assert_eq!(health.system.admitted_frames, 32);
        assert_eq!(health.system.written_frames, 0);
        assert!(health.warnings.iter().any(|warning| warning.contains("queue")));
    }

    #[test]
    fn a_failed_source_preserves_the_other_source_and_existing_file() {
        let directory = Directory::new();
        let existing = directory.0.join("test-system-segment-00000000.wav");
        std::fs::write(&existing, b"keep this file").unwrap();
        let shared = Arc::new(Shared::new());
        let mut writer = writer(&directory, shared.clone());
        writer.write_packet(packet(AudioSource::System, 0, 16));
        writer.write_packet(packet(AudioSource::Microphone, 0, 32));
        writer.finish();
        let sections = shared.take_segments().unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].source, AudioSource::Microphone);
        assert_eq!(std::fs::read(existing).unwrap(), b"keep this file");
        assert!(shared.health(true).unwrap().system.write_error.is_some());
    }

    #[test]
    fn writer_drains_packets_before_finalizing_on_stop() {
        let directory = Directory::new();
        let shared = Arc::new(Shared::new());
        let writer = writer(&directory, shared.clone());
        let (sender, receiver) = mpsc::sync_channel(2);
        queue_packet(&sender, packet(AudioSource::Microphone, 0, 100), &shared).unwrap();
        queue_packet(&sender, packet(AudioSource::Microphone, 100, 200), &shared).unwrap();
        drop(sender);
        writer.run(receiver);
        let sections = shared.take_segments().unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(crate::audio::inspect(&sections[0].path).unwrap().frames, 300);
        assert_eq!(shared.health(true).unwrap().microphone.written_frames, 300);
    }
}
//! WASAPI owns capture on one thread; this writer never holds a WASAPI buffer.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

use super::{segment_path, CapturedSegment, RecordingHealth, SourceCounters};
use crate::{audio::wav::Writer, domain::{AppError, AppResult, AudioSource}};
use std::{
    path::PathBuf,
    sync::{atomic::{AtomicBool, Ordering}, mpsc::{self, Receiver, SyncSender, TrySendError}, Arc, Mutex},
    time::{Duration, Instant},
};

const SAMPLE_RATE: u32 = 16_000;
const SECTION_FRAMES: u64 = SAMPLE_RATE as u64 * 60;
const QUEUE_PACKETS: usize = 128;
const TIMESTAMP_TOLERANCE: u64 = SAMPLE_RATE as u64 / 1_000;

fn source_index(source: AudioSource) -> usize {
    match source { AudioSource::System => 0, AudioSource::Microphone => 1 }
}

struct Shared {
    started: Instant,
    stopping: AtomicBool,
    counters: [SourceCounters; 2],
    warnings: Mutex<Vec<String>>,
    segments: Mutex<Vec<CapturedSegment>>,
}

impl Shared {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            stopping: AtomicBool::new(false),
            counters: std::array::from_fn(|_| SourceCounters::default()),
            warnings: Mutex::new(Vec::new()),
            segments: Mutex::new(Vec::new()),
        }
    }

    fn warn(&self, warning: String) {
        let mut warnings = self.warnings.lock().unwrap_or_else(|error| error.into_inner());
        if !warnings.contains(&warning) { warnings.push(warning); }
    }

    fn take_segments(&self) -> AppResult<Vec<CapturedSegment>> {
        Ok(std::mem::take(&mut *self.segments.lock().unwrap_or_else(|error| error.into_inner())))
    }

    fn health(&self, stopped: bool) -> AppResult<RecordingHealth> {
        let wall = self.started.elapsed().as_secs_f64();
        let mut health = RecordingHealth::from_sources(
            wall,
            self.counters[0].snapshot(f64::from(SAMPLE_RATE), wall, stopped),
            self.counters[1].snapshot(f64::from(SAMPLE_RATE), wall, stopped),
        );
        health.warnings.extend(self.warnings.lock().unwrap_or_else(|error| error.into_inner()).iter().cloned());
        Ok(health)
    }
}

struct Packet {
    source: AudioSource,
    start_frame: u64,
    captured_ms: u64,
    pcm: Vec<u8>,
}

fn queue_packet(sender: &SyncSender<Packet>, packet: Packet, shared: &Shared) -> AppResult<()> {
    let source = packet.source;
    let counter = &shared.counters[source_index(source)];
    counter.callback(packet.captured_ms);
    counter.admit((packet.pcm.len() / 2) as u32);
    match sender.try_send(packet) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => {
            shared.warn(format!("{} capture queue filled; some audio was lost. Available sections were kept.", source.label()));
            Ok(())
        }
        Err(TrySendError::Disconnected(_)) => Err(AppError::new("audio_capture", "Audio file writer stopped unexpectedly")),
    }
}

struct SourceWriter {
    base: PathBuf,
    source: AudioSource,
    segmented: bool,
    index: u64,
    start_frame: u64,
    writer: Option<Writer>,
    path: PathBuf,
    failed: bool,
}

impl SourceWriter {
    fn new(base: PathBuf, source: AudioSource, segmented: bool) -> Self {
        Self { path: base.clone(), base, source, segmented, index: 0, start_frame: 0, writer: None, failed: false }
    }

    fn open(&mut self, start_frame: u64) -> AppResult<()> {
        self.path = if self.segmented { segment_path(&self.base, self.source, self.index)? } else { self.base.clone() };
        self.writer = Some(Writer::create(&self.path, SAMPLE_RATE)?);
        self.start_frame = start_frame;
        Ok(())
    }

    fn finish(&mut self, shared: &Shared) -> AppResult<()> {
        let Some(writer) = self.writer.take() else { return Ok(()); };
        let frames = writer.frames();
        writer.finish()?;
        if frames > 0 && self.segmented {
            shared.segments.lock().unwrap_or_else(|error| error.into_inner()).push(CapturedSegment {
                source: self.source,
                index: self.index,
                start_seconds: self.start_frame as f64 / f64::from(SAMPLE_RATE),
                duration_seconds: frames as f64 / f64::from(SAMPLE_RATE),
                path: self.path.clone(),
            });
        }
        self.index += 1;
        Ok(())
    }

    fn write(&mut self, packet: Packet, shared: &Shared) -> AppResult<()> {
        if self.failed || packet.pcm.is_empty() { return Ok(()); }
        if packet.pcm.len() % 2 != 0 {
            return Err(AppError::new("audio_capture", "An audio packet ended inside a PCM frame"));
        }
        let mut start = packet.start_frame;
        let mut pcm = packet.pcm.as_slice();
        if let Some(writer) = &self.writer {
            let expected = self.start_frame + writer.frames();
            if start.abs_diff(expected) <= TIMESTAMP_TOLERANCE {
                start = expected;
            } else if start > expected {
                shared.warn(format!("{} capture contained a gap. Section timestamps preserve the missing interval.", self.source.label()));
                if self.segmented {
                    self.finish(shared)?;
                } else {
                    // Legacy full-file capture must preserve time too; use bounded silence writes.
                    let mut missing = start - expected;
                    let silence = [0u8; 8_192];
                    while missing > 0 {
                        let frames = missing.min((silence.len() / 2) as u64);
                        self.writer.as_mut().expect("writer installed").write_pcm(&silence[..frames as usize * 2])?;
                        missing -= frames;
                    }
                }
            } else {
                shared.warn(format!("{} capture timestamps overlapped. Repeated frames were omitted.", self.source.label()));
                let overlap = (expected - start).min((pcm.len() / 2) as u64) as usize;
                pcm = &pcm[overlap * 2..];
                start = expected;
            }
        }
        while !pcm.is_empty() {
            if self.writer.is_none() { self.open(start)?; }
            let writer = self.writer.as_mut().expect("writer installed");
            let available = if self.segmented { SECTION_FRAMES - writer.frames() } else { u64::MAX };
            let frames = available.min((pcm.len() / 2) as u64) as usize;
            writer.write_pcm(&pcm[..frames * 2])?;
            let counter = &shared.counters[source_index(self.source)];
            counter.complete_write(frames as u32, 0);
            counter.last_written_frame_ms.store(packet.captured_ms.saturating_add(1), Ordering::Release);
            pcm = &pcm[frames * 2..];
            start += frames as u64;
            if self.segmented && writer.frames() == SECTION_FRAMES { self.finish(shared)?; }
        }
        Ok(())
    }

    fn fail(&mut self, shared: &Shared, error: AppError) {
        self.failed = true;
        shared.counters[source_index(self.source)].complete_write(0, -2_147_467_259); // E_FAIL
        shared.warn(format!("{} could not save an audio section: {}. Captured files were kept.", self.source.label(), error.message));
    }
}

struct RecordingWriter {
    sources: [SourceWriter; 2],
    shared: Arc<Shared>,
}

impl RecordingWriter {
    fn new(system: PathBuf, microphone: PathBuf, segmented: bool, shared: Arc<Shared>) -> Self {
        Self {
            sources: [
                SourceWriter::new(system.clone(), AudioSource::System, segmented),
                SourceWriter::new(if segmented { system } else { microphone }, AudioSource::Microphone, segmented),
            ],
            shared,
        }
    }

    fn write_packet(&mut self, packet: Packet) {
        let source = &mut self.sources[source_index(packet.source)];
        if let Err(error) = source.write(packet, &self.shared) { source.fail(&self.shared, error); }
    }

    fn rotate_due(&mut self) {
        let now = (self.shared.started.elapsed().as_secs_f64() * f64::from(SAMPLE_RATE)) as u64;
        for source in &mut self.sources {
            if source.segmented && source.writer.is_some() && now.saturating_sub(source.start_frame) >= SECTION_FRAMES {
                if let Err(error) = source.finish(&self.shared) { source.fail(&self.shared, error); }
            }
        }
    }

    fn finish(&mut self) {
        for source in &mut self.sources {
            if let Err(error) = source.finish(&self.shared) { source.fail(&self.shared, error); }
        }
    }

    fn run(mut self, receiver: Receiver<Packet>) {
        loop {
            match receiver.recv_timeout(Duration::from_millis(200)) {
                Ok(packet) => self.write_packet(packet),
                Err(mpsc::RecvTimeoutError::Timeout) => {},
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            self.rotate_due();
        }
        self.finish();
    }
}

#[cfg(target_os = "windows")]
pub(super) use platform::NativeRecording;

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use crate::recorder::RecordingFiles;
    use std::{path::Path, ptr, thread::{self, JoinHandle}};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{CloseHandle, HANDLE, WAIT_FAILED},
            Media::Audio::{
                eCapture, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
                MMDeviceEnumerator, WAVEFORMATEX, AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
                AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR,
            },
            System::{
                Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED},
                Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
                Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED},
                Threading::{CreateEventW, WaitForMultipleObjects},
            },
        },
    };

    fn windows_error(error: windows::core::Error) -> AppError {
        AppError::new("audio_capture", format!("Windows audio error 0x{:08X}", error.code().0 as u32))
    }

    struct Apartment;
    impl Apartment {
        fn new() -> AppResult<Self> {
            // SAFETY: this dedicated thread owns every COM interface until before this guard drops.
            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok().map_err(windows_error)?; }
            Ok(Self)
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) { unsafe { CoUninitialize(); } }
    }

    struct Awake;
    impl Awake {
        fn new(shared: &Shared) -> Self {
            // The writer owns the assertion through the final durable flush. Display sleep is allowed.
            if unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) }.0 == 0 {
                shared.warn("Windows could not prevent idle sleep. Keep the computer awake while recording.".into());
            }
            Self
        }
    }
    impl Drop for Awake {
        fn drop(&mut self) { unsafe { SetThreadExecutionState(ES_CONTINUOUS); } }
    }

    struct Event(HANDLE);
    impl Event {
        fn new() -> AppResult<Self> {
            // SAFETY: no security descriptor or name is supplied; this thread owns the returned event.
            unsafe { CreateEventW(None, false, false, PCWSTR::null()).map(Self).map_err(windows_error) }
        }
    }
    impl Drop for Event {
        fn drop(&mut self) { let _ = unsafe { CloseHandle(self.0) }; }
    }

    fn qpc_time() -> AppResult<u64> {
        let (mut ticks, mut frequency) = (0, 0);
        // SAFETY: both APIs write one live i64; the resulting time matches GetBuffer's 100 ns units.
        unsafe {
            QueryPerformanceCounter(&mut ticks).map_err(windows_error)?;
            QueryPerformanceFrequency(&mut frequency).map_err(windows_error)?;
        }
        if ticks < 0 || frequency <= 0 {
            return Err(AppError::new("audio_capture", "Windows returned an invalid capture clock"));
        }
        Ok((ticks as u128 * 10_000_000 / frequency as u128) as u64)
    }

    struct Stream {
        capture: IAudioCaptureClient,
        client: IAudioClient,
        event: Event,
        source: AudioSource,
        started: bool,
        received_packet: bool,
    }

    impl Stream {
        fn open(enumerator: &IMMDeviceEnumerator, source: AudioSource) -> AppResult<Self> {
            let direction = if source == AudioSource::System { eRender } else { eCapture };
            let format = WAVEFORMATEX {
                wFormatTag: 1, // WAVE_FORMAT_PCM
                nChannels: 1,
                nSamplesPerSec: SAMPLE_RATE,
                nAvgBytesPerSec: SAMPLE_RATE * 2,
                nBlockAlign: 2,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            let mut flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
            if source == AudioSource::System { flags |= AUDCLNT_STREAMFLAGS_LOOPBACK; }
            // SAFETY: interfaces are created and retained exclusively on this COM-initialized thread.
            unsafe {
                let device = enumerator.GetDefaultAudioEndpoint(direction, eConsole).map_err(windows_error)?;
                let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(windows_error)?;
                let event = Event::new()?;
                client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 1_000_000, 0, &format, None).map_err(windows_error)?;
                client.SetEventHandle(event.0).map_err(windows_error)?;
                let capture = client.GetService().map_err(windows_error)?;
                Ok(Self { capture, client, event, source, started: false, received_packet: false })
            }
        }

        fn start(&mut self) -> AppResult<()> {
            unsafe { self.client.Start().map_err(windows_error)?; }
            self.started = true;
            Ok(())
        }

        fn stop(&mut self) -> AppResult<()> {
            if self.started {
                self.started = false;
                unsafe { self.client.Stop().map_err(windows_error)?; }
            }
            Ok(())
        }

        fn drain(&mut self, sender: &SyncSender<Packet>, shared: &Shared, origin: u64) -> AppResult<()> {
            // Bound each pass so an active device cannot starve the other source or the stop request.
            for _ in 0..32 {
                let size = unsafe { self.capture.GetNextPacketSize().map_err(windows_error)? };
                if size == 0 { break; }
                let (mut data, mut frames, mut flags, mut timestamp) = (ptr::null_mut(), 0, 0, 0);
                // SAFETY: outputs are initialized locals. Buffer access and release stay on this thread.
                unsafe {
                    self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut timestamp)).map_err(windows_error)?;
                }
                if frames == 0 { break; }
                let pcm = if frames > SAMPLE_RATE * 2 {
                    Err(AppError::new("audio_capture", "Windows returned an oversized audio packet"))
                } else if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                    Ok(vec![0; frames as usize * 2])
                } else if data.is_null() {
                    Err(AppError::new("audio_capture", "Windows returned an invalid audio buffer"))
                } else {
                    // SAFETY: WASAPI owns exactly frames * block-align bytes until ReleaseBuffer.
                    Ok(unsafe { std::slice::from_raw_parts(data, frames as usize * 2) }.to_vec())
                };
                // Release even when validating/copying failed, and before any queue or filesystem work.
                let released = unsafe { self.capture.ReleaseBuffer(frames).map_err(windows_error) };
                let pcm = pcm?;
                released?;
                if self.received_packet && flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                    shared.warn(format!("{} reported an audio discontinuity; some speech may be missing.", self.source.label()));
                }
                let captured_ms = shared.started.elapsed().as_millis() as u64;
                let start_frame = if flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 != 0 || timestamp < origin {
                    shared.warn(format!("{} reported an unreliable audio timestamp; section timing may be approximate.", self.source.label()));
                    (captured_ms * u64::from(SAMPLE_RATE) / 1_000).saturating_sub(u64::from(frames))
                } else {
                    ((timestamp - origin) as u128 * u128::from(SAMPLE_RATE) / 10_000_000) as u64
                };
                queue_packet(sender, Packet { source: self.source, start_frame, captured_ms, pcm }, shared)?;
                self.received_packet = true;
            }
            Ok(())
        }
    }
    impl Drop for Stream {
        fn drop(&mut self) { let _ = self.stop(); }
    }

    fn run_capture(shared: Arc<Shared>, sender: SyncSender<Packet>, ready: mpsc::Sender<AppResult<()>>, origin: u64) {
        let setup = || -> AppResult<(Apartment, Vec<Stream>)> {
            let apartment = Apartment::new()?;
            let enumerator: IMMDeviceEnumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(windows_error)? };
            let mut streams = Vec::new();
            for source in [AudioSource::System, AudioSource::Microphone] {
                match Stream::open(&enumerator, source) {
                    Ok(stream) => streams.push(stream),
                    Err(error) => shared.warn(format!("{} could not start: {}. Check the selected Windows device and microphone privacy settings.", source.label(), error.message)),
                }
            }
            streams.retain_mut(|stream| match stream.start() {
                Ok(()) => true,
                Err(error) => {
                    shared.warn(format!("{} could not start: {}.", stream.source.label(), error.message));
                    false
                }
            });
            if streams.is_empty() { return Err(AppError::new("audio_capture", "Neither Windows audio source could start. Check audio devices and microphone permissions.")); }
            Ok((apartment, streams))
        };
        let (_apartment, mut streams) = match setup() {
            Ok(value) => value,
            Err(error) => { let _ = ready.send(Err(error)); return; }
        };
        if ready.send(Ok(())).is_err() { return; }
        while !shared.stopping.load(Ordering::Acquire) && !streams.is_empty() {
            let events: Vec<_> = streams.iter().map(|stream| stream.event.0).collect();
            // Event handles remain live throughout the wait. A quiet endpoint is not a fatal timeout.
            if unsafe { WaitForMultipleObjects(&events, false, 100) } == WAIT_FAILED {
                shared.warn("Windows audio event wait failed. Available sections were kept; stop and start a new recording.".into());
                break;
            }
            streams.retain_mut(|stream| match stream.drain(&sender, &shared, origin) {
                Ok(()) => true,
                Err(error) => {
                    shared.warn(format!("{} stopped: {}. Its endpoint may be disconnected or unavailable. Available audio was kept; start a new recording to select devices again.", stream.source.label(), error.message));
                    false
                }
            });
        }
        for stream in &mut streams {
            if let Err(error) = stream.stop().and_then(|_| stream.drain(&sender, &shared, origin)) {
                shared.warn(format!("{} could not finish capture: {}. Available audio was kept.", stream.source.label(), error.message));
            }
        }
        // Streams drop before the COM apartment; sender then closes so the writer drains and finalizes.
    }

    pub(crate) struct NativeRecording {
        system: PathBuf,
        microphone: PathBuf,
        segmented: bool,
        shared: Arc<Shared>,
        capture: Option<JoinHandle<()>>,
        writer: Option<JoinHandle<()>>,
    }

    impl NativeRecording {
        pub(crate) fn start(system: &Path, microphone: &Path, segmented: bool) -> AppResult<Self> {
            if system.extension().is_none_or(|extension| extension != "wav")
                || microphone.extension().is_none_or(|extension| extension != "wav") {
                return Err(AppError::new("invalid_audio_path", "Windows capture requires WAV paths"));
            }
            let shared = Arc::new(Shared::new());
            let origin = qpc_time()?;
            let (sender, receiver) = mpsc::sync_channel(QUEUE_PACKETS);
            let mut recording = Self { system: system.to_owned(), microphone: microphone.to_owned(), segmented, shared: shared.clone(), capture: None, writer: None };
            let writer = RecordingWriter::new(system.to_owned(), microphone.to_owned(), segmented, shared.clone());
            let writer_shared = shared.clone();
            recording.writer = Some(thread::Builder::new().name("audio-files".into()).spawn(move || {
                let _awake = Awake::new(&writer_shared);
                writer.run(receiver);
            }).map_err(|error| AppError::new("audio_capture", error.to_string()))?);
            let (ready_sender, ready_receiver) = mpsc::channel();
            recording.capture = match thread::Builder::new().name("wasapi-capture".into()).spawn(move || run_capture(shared, sender, ready_sender, origin)) {
                Ok(thread) => Some(thread),
                Err(error) => return Err(AppError::new("audio_capture", error.to_string())),
            };
            ready_receiver.recv().map_err(|_| AppError::new("audio_capture", "Windows capture stopped during startup"))??;
            Ok(recording)
        }

        pub(crate) fn rotate(&mut self) -> AppResult<Vec<CapturedSegment>> { self.shared.take_segments() }
        pub(crate) fn health(&self) -> AppResult<RecordingHealth> { self.shared.health(false) }

        fn shutdown(&mut self) {
            self.shared.stopping.store(true, Ordering::Release);
            for worker in [&mut self.capture, &mut self.writer] {
                if let Some(worker) = worker.take() {
                    if worker.join().is_err() { self.shared.warn("An audio worker stopped unexpectedly. Available files were kept.".into()); }
                }
            }
        }

        pub(crate) fn stop(mut self) -> AppResult<RecordingFiles> {
            self.shutdown();
            Ok(RecordingFiles { system: self.system.clone(), microphone: self.microphone.clone(), health: Some(self.shared.health(true)?), segments: self.shared.take_segments()?, segmented: self.segmented })
        }
    }
    impl Drop for NativeRecording {
        fn drop(&mut self) { self.shutdown(); }
    }
}
