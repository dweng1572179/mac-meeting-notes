use super::*;
use objc2_core_audio_types::{kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, AudioBuffer};
use std::fs;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("meeting-rotation-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn format() -> AudioStreamBasicDescription {
    AudioStreamBasicDescription {
        mSampleRate: 16_000.0,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
        mBytesPerPacket: 4,
        mFramesPerPacket: 1,
        mBytesPerFrame: 4,
        mChannelsPerFrame: 1,
        mBitsPerChannel: 32,
        mReserved: 0,
    }
}
fn recording(directory: &Directory) -> NativeRecording {
    let mut recording = NativeRecording::unstarted(
        &directory.0.join("test.m4a"),
        &directory.0.join("test-mic.m4a"),
        true,
    )
    .unwrap();
    recording
        .prepare_stream(AudioSource::System, &format())
        .unwrap();
    recording
        .prepare_stream(AudioSource::Microphone, &format())
        .unwrap();
    recording
}
fn feed(callback: &CallbackState, frames: usize, frequency: f64) {
    let mut samples = [0f32; 1024];
    let mut offset = 0;
    while offset < frames {
        let count = samples.len().min(frames - offset);
        for (index, sample) in samples[..count].iter_mut().enumerate() {
            *sample = (std::f64::consts::TAU * frequency * (offset + index) as f64 / 16_000.0).sin()
                as f32
                * 0.3;
        }
        let buffers = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: count as u32 * 4,
                mData: samples.as_mut_ptr().cast(),
            }],
        };
        // SAFETY: buffers and samples are alive and match the fixture client format.
        unsafe {
            write_input(callback, &buffers);
        }
        offset += count;
    }
}

#[test]
fn rotation_finalizes_each_source_and_stop_publishes_tail_once() {
    let directory = Directory::new();
    let mut recording = recording(&directory);
    feed(recording.callback.as_ref().unwrap(), 16_000, 300.0);
    feed(
        recording.microphone_callback.as_ref().unwrap(),
        8_000,
        600.0,
    );
    let first = recording.rotate().unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].source, AudioSource::System);
    assert_eq!(first[1].source, AudioSource::Microphone);
    assert_eq!(first[0].index, 0);
    assert_eq!(first[0].start_seconds, 0.0);
    assert!((first[0].duration_seconds - 1.0).abs() < 0.001);
    assert!((first[1].duration_seconds - 0.5).abs() < 0.001);
    for (segment, expected_frequency) in first.iter().zip([300.0, 600.0]) {
        assert!(crate::audio::inspect(&segment.path).unwrap().frames > 0);
        let (_, frequency, rms) = crate::audio::tests::decoded_tone(&segment.path);
        assert!((frequency - expected_frequency).abs() < 5.0);
        assert!(rms > 0.1);
    }
    assert!(recording.rotate().unwrap().is_empty());
    feed(recording.callback.as_ref().unwrap(), 4_000, 900.0);
    feed(
        recording.microphone_callback.as_ref().unwrap(),
        12_000,
        1200.0,
    );
    let files = recording.stop().unwrap();
    assert!(files.segmented);
    assert_eq!(files.segments.len(), 2);
    assert_eq!(files.segments[0].index, 1);
    assert_eq!(files.segments[0].start_seconds, first[0].duration_seconds);
    assert_eq!(files.segments[1].start_seconds, first[1].duration_seconds);
    for (segment, expected_frequency) in files.segments.iter().zip([900.0, 1200.0]) {
        let (_, frequency, rms) = crate::audio::tests::decoded_tone(&segment.path);
        assert!((frequency - expected_frequency).abs() < 10.0);
        assert!(rms > 0.1);
    }
    let health = files.health.unwrap();
    assert_eq!(health.system.written_frames, 20_000);
    assert_eq!(health.microphone.written_frames, 20_000);
    assert!((first[0].duration_seconds + files.segments[0].duration_seconds - 1.25).abs() < 0.001);
    assert!((first[1].duration_seconds + files.segments[1].duration_seconds - 1.25).abs() < 0.001);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 4);
}

#[test]
fn failed_replacement_preserves_active_capture_and_rotates_healthy_source() {
    let directory = Directory::new();
    let mut recording = recording(&directory);
    feed(recording.callback.as_ref().unwrap(), 16_000, 300.0);
    feed(
        recording.microphone_callback.as_ref().unwrap(),
        16_000,
        600.0,
    );
    let collision = directory.0.join("test-system-segment-00000001.m4a");
    fs::write(&collision, b"preserve existing recording").unwrap();
    let segments = recording.rotate().unwrap();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].source, AudioSource::Microphone);
    assert!(!recording.health().unwrap().warnings.is_empty());
    feed(recording.callback.as_ref().unwrap(), 16_000, 300.0);
    let files = recording.stop().unwrap();
    let system = files
        .segments
        .iter()
        .find(|part| part.source == AudioSource::System)
        .unwrap();
    assert!((system.duration_seconds - 2.0).abs() < 0.001);
    assert_eq!(fs::read(collision).unwrap(), b"preserve existing recording");
    assert_eq!(files.health.unwrap().system.written_frames, 32_000);
}

#[test]
fn retiring_a_segment_waits_for_its_admitted_write_before_disposal() {
    let directory = Directory::new();
    let sink = SegmentSink::create(&directory.0.join("sink.m4a"), &format()).unwrap();
    let lease = sink.gate.try_enter().unwrap();
    sink.gate.disable();
    assert!(!sink.gate.is_idle());
    assert!(!sink.file.load(Ordering::Acquire).is_null());
    assert!(sink.gate.try_enter().is_none());
    std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let sink_ref = &sink;
        scope.spawn(move || {
            sink_ref.close().unwrap();
            sender.send(()).unwrap();
        });
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(10)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        assert!(!sink.file.load(Ordering::Acquire).is_null());
        drop(lease);
        receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    assert!(sink.gate.is_idle());
    assert!(sink.file.load(Ordering::Acquire).is_null());
    sink.close().unwrap(); // duplicate teardown does not dispose twice.
}

#[test]
fn callbacks_crossing_rotation_boundaries_keep_every_accepted_frame() {
    let directory = Directory::new();
    let mut recording = recording(&directory);
    feed(recording.callback.as_ref().unwrap(), 16_000, 500.0);
    let mut segments = Vec::new();
    let callback = recording.callback.as_ref().unwrap();
    let writer = recording.system_segments.as_mut().unwrap();
    std::thread::scope(|scope| {
        let producer = scope.spawn(|| {
            for _ in 0..100 {
                feed(callback, 512, 500.0);
                std::thread::sleep(Duration::from_micros(100));
            }
        });
        for _ in 0..10 {
            if let Some(segment) = writer.rotate(callback).unwrap() {
                segments.push(segment);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        producer.join().unwrap();
    });
    let files = recording.stop().unwrap();
    segments.extend(files.segments);
    assert!(segments.len() > 1);
    let total = segments
        .iter()
        .map(|segment| crate::audio::inspect(&segment.path).unwrap().frames)
        .sum::<u64>();
    assert_eq!(total, 67_200);
    let health = files.health.unwrap();
    assert_eq!(health.system.admitted_frames, 67_200);
    assert_eq!(health.system.written_frames, 67_200);
    assert_eq!(health.system.write_error, None);
    let mut end = 0.0;
    for segment in segments {
        assert!((segment.start_seconds - end).abs() < 0.000_001);
        end += segment.duration_seconds;
    }
    assert!((end - 4.2).abs() < 0.000_001);
}

#[test]
fn concurrent_stop_and_rotation_publish_each_source_frame_once() {
    let directory = Directory::new();
    let recording = recording(&directory);
    feed(recording.callback.as_ref().unwrap(), 16_000, 300.0);
    let recorder = crate::recorder::Recorder::new();
    {
        let mut slot = recorder.slot.lock().unwrap();
        slot.reserve("test").unwrap();
        slot.recording = Some(recording);
    }
    let mut segments = std::thread::scope(|scope| {
        let rotation = scope.spawn(|| recorder.rotate("test"));
        let stopped = recorder.stop("test").unwrap();
        let mut segments = stopped.segments;
        match rotation.join().unwrap() {
            Ok(rotated) => segments.extend(rotated),
            Err(error) => assert_eq!(error.code, "recording_session_mismatch"),
        }
        segments
    });
    segments.sort_by_key(|segment| segment.index);
    assert_eq!(segments.len(), 1);
    assert_eq!(
        crate::audio::inspect(&segments[0].path).unwrap().frames,
        16_000
    );
    assert!(!recorder.is_recording());
}

#[test]
fn truncated_finalization_is_not_published_as_complete_capture() {
    let directory = Directory::new();
    let mut recording = recording(&directory);
    feed(recording.callback.as_ref().unwrap(), 16_000, 300.0);
    // Model a queued-write flush that loses accepted frames: actual encoded audio is
    // one second, while the async writer reported accepting two. Native decode is real.
    recording.system_segments.as_ref().unwrap().sinks[0]
        .written_frames
        .fetch_add(16_000, Ordering::Release);
    let finalized = recording.rotate().unwrap();
    assert!(
        finalized.is_empty(),
        "a short finalized file must not be published as complete"
    );
    assert!(!recording.health().unwrap().warnings.is_empty());
    let retained = directory.0.join("test-system-segment-00000000.m4a");
    assert_eq!(crate::audio::inspect(&retained).unwrap().frames, 16_000);
    feed(recording.callback.as_ref().unwrap(), 16_000, 600.0);
    let files = recording.stop().unwrap();
    assert_eq!(files.segments.len(), 1);
    assert_eq!(files.segments[0].index, 1);
    assert_eq!(files.segments[0].start_seconds, 2.0);
    assert!(retained.exists());
}

#[test]
fn failed_preparation_does_not_poison_the_next_rotation() {
    let directory = Directory::new();
    let mut recording = recording(&directory);
    feed(recording.callback.as_ref().unwrap(), 16_000, 300.0);
    // Fail after the next path has been reserved but before any callback can use it.
    // The real native client-format setter rejects this unknown format identifier.
    recording.system_segments.as_mut().unwrap().format.mFormatID = 0;
    assert!(recording.rotate().unwrap().is_empty());
    assert!(!recording.health().unwrap().warnings.is_empty());
    recording.system_segments.as_mut().unwrap().format = format();
    feed(recording.callback.as_ref().unwrap(), 16_000, 300.0);
    let recovered = recording.rotate().unwrap();
    assert_eq!(
        recovered.len(),
        1,
        "an owned, unwritten preparation file must not block retry"
    );
    assert_eq!(recovered[0].index, 0);
    assert!((recovered[0].duration_seconds - 2.0).abs() < 0.001);
    assert_eq!(recording.health().unwrap().system.written_frames, 32_000);
}

#[test]
#[ignore = "Offline native AAC stress: encodes 110 accelerated minutes on each source; run explicitly"]
fn accelerated_110_minute_two_source_rotation_preserves_every_frame() {
    const MINUTES: u64 = 110;
    const FRAMES_PER_MINUTE: u64 = 16_000 * 60;
    let directory = Directory::new();
    let mut recording = recording(&directory);
    // Reuse one second of PCM per source instead of computing 211 million sine samples.
    let mut tones = [300.0, 600.0].map(|frequency| {
        (0..16_000)
            .map(|frame| {
                (std::f64::consts::TAU * frequency * frame as f64 / 16_000.0).sin() as f32 * 0.3
            })
            .collect::<Vec<_>>()
    });
    let mut section_counts = [0u64; 2];
    let mut finalized_frames = [0u64; 2];
    for minute in 0..MINUTES {
        for _ in 0..60 {
            for offset in (0..16_000).step_by(1024) {
                for (callback, samples) in [
                    recording.callback.as_ref().unwrap(),
                    recording.microphone_callback.as_ref().unwrap(),
                ]
                .into_iter()
                .zip(tones.iter_mut())
                {
                    let samples = &mut samples[offset..(offset + 1024).min(16_000)];
                    let buffers = AudioBufferList {
                        mNumberBuffers: 1,
                        mBuffers: [AudioBuffer {
                            mNumberChannels: 1,
                            mDataByteSize: samples.len() as u32 * 4,
                            mData: samples.as_mut_ptr().cast(),
                        }],
                    };
                    // SAFETY: reusable PCM remains alive and matches the fixture's mono float format.
                    unsafe { write_input(callback, &buffers) };
                }
            }
            // Give native async encoders time to drain while remaining roughly 200x accelerated.
            std::thread::sleep(Duration::from_millis(5));
        }
        let sections = recording.rotate().unwrap();
        assert_eq!(
            sections.len(),
            2,
            "minute {minute}: both sources must finalize; health: {:?}",
            recording.health().unwrap()
        );
        for section in sections {
            let source = match section.source {
                AudioSource::System => 0,
                AudioSource::Microphone => 1,
            };
            assert_eq!(section.index, minute);
            assert!((section.start_seconds - minute as f64 * 60.0).abs() < 0.000_001);
            assert!((section.duration_seconds - 60.0).abs() < 0.000_001);
            let info = crate::audio::inspect(&section.path).unwrap();
            assert_eq!(info.sample_rate, 16_000.0);
            assert_eq!(info.frames, FRAMES_PER_MINUTE);
            if matches!(minute, 0 | 54 | 109) {
                let (frames, frequency, rms) = crate::audio::tests::decoded_tone(&section.path);
                assert_eq!(frames, FRAMES_PER_MINUTE);
                assert!((frequency - [300.0, 600.0][source]).abs() < 5.0);
                assert!(rms > 0.1 && rms < 0.4);
            }
            section_counts[source] += 1;
            finalized_frames[source] += info.frames;
            // Keep only the active files; Directory also cleans up on any assertion failure.
            fs::remove_file(section.path).unwrap();
        }
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    }
    let files = recording.stop().unwrap();
    assert!(files.segmented);
    assert!(
        files.segments.is_empty(),
        "stop must not republish finalized sections or empty tails"
    );
    assert_eq!(section_counts, [MINUTES; 2]);
    assert_eq!(finalized_frames, [MINUTES * FRAMES_PER_MINUTE; 2]);
    let health = files.health.unwrap();
    for source in [health.system, health.microphone] {
        assert_eq!(source.admitted_frames, MINUTES * FRAMES_PER_MINUTE);
        assert_eq!(source.written_frames, MINUTES * FRAMES_PER_MINUTE);
        assert_eq!(source.write_error, None);
        assert!((source.captured_seconds - 6_600.0).abs() < 0.000_001);
    }
    for entry in fs::read_dir(&directory.0).unwrap() {
        let path = entry.unwrap().path();
        assert_eq!(crate::audio::inspect(&path).unwrap().frames, 0);
        fs::remove_file(path).unwrap();
    }
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}
