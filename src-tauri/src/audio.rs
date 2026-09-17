use std::path::Path;

use crate::domain::{AppError, AppResult};

#[derive(Debug, Clone, Copy)]
pub struct AudioInfo {
    pub frames: u64,
    pub sample_rate: f64,
}

pub fn inspect(path: &Path) -> AppResult<AudioInfo> {
    #[cfg(target_os = "macos")]
    {
        let file = native::AudioFile::open(path)?;
        let info = file.info()?;
        file.close()?;
        Ok(info)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(AppError::new(
            "audio_chunk",
            "Audio decoding requires macOS",
        ))
    }
}

/// Create one independently finalized, at most five-minute M4A. Existing outputs
/// are refused, including aliases of the source; callers own stale scratch cleanup.
pub fn write_chunk(
    input: &Path,
    output: &Path,
    start_seconds: f64,
    duration_seconds: f64,
) -> AppResult<()> {
    if !start_seconds.is_finite()
        || start_seconds < 0.0
        || !duration_seconds.is_finite()
        || duration_seconds <= 0.0
        || duration_seconds > 300.0
    {
        return Err(AppError::new(
            "audio_chunk",
            "Invalid audio chunk time range",
        ));
    }
    #[cfg(target_os = "macos")]
    {
        native::write_chunk(input, output, start_seconds, duration_seconds)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (input, output);
        Err(AppError::new(
            "audio_chunk",
            "Audio decoding requires macOS",
        ))
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use crate::recorder::native::{create_audio_file, set_bit_rate, set_client_format};
    use objc2_audio_toolbox::{
        kExtAudioFileProperty_FileDataFormat, kExtAudioFileProperty_FileLengthFrames,
        ExtAudioFileDispose, ExtAudioFileGetProperty, ExtAudioFileOpenURL, ExtAudioFileRead,
        ExtAudioFileRef, ExtAudioFileSeek, ExtAudioFileWrite,
    };
    use objc2_core_audio_types::{
        kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatLinearPCM, AudioBuffer,
        AudioBufferList, AudioStreamBasicDescription,
    };
    use objc2_core_foundation::CFURL;
    use std::{
        fs,
        ptr::{self, NonNull},
    };

    const BLOCK_FRAMES: usize = 8192;
    const MAX_CHUNK_BYTES: u64 = 24_000_000;

    pub(super) struct AudioFile(ExtAudioFileRef);

    impl AudioFile {
        pub(super) fn open(path: &Path) -> AppResult<Self> {
            let url = CFURL::from_file_path(path)
                .ok_or_else(|| AppError::new("audio_chunk", "Invalid audio file path"))?;
            let mut file = Self(ptr::null_mut());
            // SAFETY: URL and handle output live through the call; RAII owns any returned handle.
            check("ExtAudioFileOpenURL", unsafe {
                ExtAudioFileOpenURL(&url, NonNull::from(&mut file.0))
            })?;
            if file.0.is_null() {
                return Err(AppError::new("audio_chunk", "Invalid audio file handle"));
            }
            Ok(file)
        }

        fn property<T>(&self, id: u32) -> AppResult<T> {
            let mut value = std::mem::MaybeUninit::<T>::uninit();
            let mut size = size_of::<T>() as u32;
            // SAFETY: the two callers use the exact POD type for their property; size bounds writes.
            check("ExtAudioFileGetProperty", unsafe {
                ExtAudioFileGetProperty(
                    self.0,
                    id,
                    NonNull::from(&mut size),
                    NonNull::new_unchecked(value.as_mut_ptr().cast()),
                )
            })?;
            if size != size_of::<T>() as u32 {
                return Err(AppError::new("audio_chunk", "Invalid audio property size"));
            }
            // SAFETY: a successful exact-sized read initialized this property’s POD value.
            Ok(unsafe { value.assume_init() })
        }

        pub(super) fn info(&self) -> AppResult<AudioInfo> {
            let format: AudioStreamBasicDescription =
                self.property(kExtAudioFileProperty_FileDataFormat)?;
            let frames: i64 = self.property(kExtAudioFileProperty_FileLengthFrames)?;
            if frames < 0
                || !format.mSampleRate.is_finite()
                || format.mSampleRate <= 0.0
                || format.mChannelsPerFrame == 0
            {
                return Err(AppError::new(
                    "audio_chunk",
                    "Invalid audio duration or sample rate",
                ));
            }
            Ok(AudioInfo {
                frames: frames as u64,
                sample_rate: format.mSampleRate,
            })
        }

        pub(super) fn close(mut self) -> AppResult<()> {
            let handle = std::mem::replace(&mut self.0, ptr::null_mut());
            // SAFETY: exclusive ownership, and the handle is removed before disposal to avoid double-close.
            check("ExtAudioFileDispose", unsafe {
                ExtAudioFileDispose(handle)
            })
        }
    }

    impl Drop for AudioFile {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: fallback teardown after an error; successful paths explicitly check close().
                unsafe {
                    ExtAudioFileDispose(self.0);
                }
            }
        }
    }

    struct PendingOutput<'a> {
        path: &'a Path,
        keep: bool,
    }
    impl Drop for PendingOutput<'_> {
        fn drop(&mut self) {
            if !self.keep {
                let _ = fs::remove_file(self.path);
            }
        }
    }

    pub(super) fn write_chunk(
        input: &Path,
        output: &Path,
        start: f64,
        duration: f64,
    ) -> AppResult<()> {
        let source = AudioFile::open(input)?;
        let info = source.info()?;
        let start_frame = (start * info.sample_rate).round();
        let frame_count = (duration * info.sample_rate).round();
        if !start_frame.is_finite()
            || !frame_count.is_finite()
            || start_frame >= info.frames as f64
            || frame_count < 1.0
            || start_frame >= i64::MAX as f64
            || frame_count >= i64::MAX as f64
        {
            return Err(AppError::new(
                "audio_chunk",
                "Audio chunk is outside the source duration",
            ));
        }
        let start_frame = start_frame as u64;
        let available = info.frames - start_frame;
        let mut remaining = frame_count as u64;
        // Allow one frame of rounding when metadata seconds were derived from source frames.
        if remaining > available.saturating_add(1) {
            return Err(AppError::new(
                "audio_chunk",
                "Audio chunk extends beyond the source duration",
            ));
        }
        remaining = remaining.min(available);
        let format = AudioStreamBasicDescription {
            mSampleRate: info.sample_rate,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: 4,
            mFramesPerPacket: 1,
            mBytesPerFrame: 4,
            mChannelsPerFrame: 1,
            mBitsPerChannel: 32,
            mReserved: 0,
        };
        set_client_format(source.0, &format)?;
        // SAFETY: source is live; seek uses source frames and client sample rate is unchanged.
        check("ExtAudioFileSeek", unsafe {
            ExtAudioFileSeek(source.0, start_frame as i64)
        })?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|error| {
                AppError::new("audio_chunk", format!("Cannot create audio chunk: {error}"))
            })?;
        let mut pending = PendingOutput {
            path: output,
            keep: false,
        };
        let destination = AudioFile(create_audio_file(output, &format)?);
        set_client_format(destination.0, &format)?;
        set_bit_rate(destination.0)?;
        let mut samples = [0f32; BLOCK_FRAMES];
        while remaining > 0 {
            let requested = remaining.min(BLOCK_FRAMES as u64) as u32;
            let mut count = requested;
            let mut buffers = AudioBufferList {
                mNumberBuffers: 1,
                mBuffers: [AudioBuffer {
                    mNumberChannels: 1,
                    mDataByteSize: requested * 4,
                    mData: samples.as_mut_ptr().cast(),
                }],
            };
            // SAFETY: one mono f32 buffer has room for exactly requested frames; handles are live.
            check("ExtAudioFileRead", unsafe {
                ExtAudioFileRead(
                    source.0,
                    NonNull::from(&mut count),
                    NonNull::from(&mut buffers),
                )
            })?;
            if count == 0 || count > requested || buffers.mBuffers[0].mDataByteSize != count * 4 {
                return Err(AppError::new(
                    "audio_chunk",
                    "Audio decoding ended before the requested chunk was complete",
                ));
            }
            // SAFETY: decode filled count frames in the buffer and destination accepts the same format.
            check("ExtAudioFileWrite", unsafe {
                ExtAudioFileWrite(destination.0, count, NonNull::from(&mut buffers))
            })?;
            remaining -= count as u64;
        }
        destination.close()?;
        source.close()?;
        let bytes = fs::metadata(output)
            .map_err(|error| AppError::new("audio_chunk", error.to_string()))?
            .len();
        if bytes == 0 || bytes >= MAX_CHUNK_BYTES {
            return Err(AppError::new(
                "audio_chunk_size",
                "Audio chunk exceeds the safe upload size or is empty",
            ));
        }
        pending.keep = true;
        Ok(())
    }

    fn check(operation: &str, status: i32) -> AppResult<()> {
        if status == 0 {
            Ok(())
        } else {
            Err(AppError::new(
                "audio_chunk",
                format!("{operation} failed with OSStatus {status}"),
            ))
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) mod tests {
    use super::*;
    use objc2_audio_toolbox::{ExtAudioFileDispose, ExtAudioFileOpenURL, ExtAudioFileRead};
    use objc2_core_audio_types::{
        kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatLinearPCM, AudioBuffer,
        AudioBufferList, AudioStreamBasicDescription,
    };
    use objc2_core_foundation::CFURL;
    use std::{
        fs,
        io::{BufWriter, Write},
        path::PathBuf,
        ptr::{self, NonNull},
    };

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let directory = Self(
                std::env::temp_dir().join(format!("meeting-notes-chunks-{}", uuid::Uuid::new_v4())),
            );
            fs::create_dir(&directory.0).unwrap();
            directory
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // A real PCM WAV with three distinct tones catches incorrect seeking, lost final
    // partial chunks, empty/silent encodes, and chunks that contain the wrong source.
    fn write_wave(path: &Path, frames: u32) {
        let mut file = BufWriter::new(fs::File::create(path).unwrap());
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36 + frames * 2).to_le_bytes()).unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&16_000u32.to_le_bytes()).unwrap();
        file.write_all(&32_000u32.to_le_bytes()).unwrap();
        file.write_all(&2u16.to_le_bytes()).unwrap();
        file.write_all(&16u16.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&(frames * 2).to_le_bytes()).unwrap();
        for frame in 0..frames {
            let frequency = match frame / 4_800_000 {
                0 => 300.0,
                1 => 600.0,
                _ => 900.0,
            };
            let sample = (12_000.0
                * (std::f64::consts::TAU * frequency * frame as f64 / 16_000.0).sin())
                as i16;
            file.write_all(&sample.to_le_bytes()).unwrap();
        }
        file.flush().unwrap();
    }

    pub(crate) fn decoded_tone(path: &Path) -> (u64, f64, f64) {
        let url = CFURL::from_file_path(path).unwrap();
        let mut raw = ptr::null_mut();
        assert_eq!(
            unsafe { ExtAudioFileOpenURL(&url, NonNull::from(&mut raw)) },
            0
        );
        struct Handle(objc2_audio_toolbox::ExtAudioFileRef);
        impl Drop for Handle {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe {
                        ExtAudioFileDispose(self.0);
                    }
                }
            }
        }
        let mut file = Handle(raw);
        let format = AudioStreamBasicDescription {
            mSampleRate: 16_000.0,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: 4,
            mFramesPerPacket: 1,
            mBytesPerFrame: 4,
            mChannelsPerFrame: 1,
            mBitsPerChannel: 32,
            mReserved: 0,
        };
        crate::recorder::native::set_client_format(file.0, &format).unwrap();
        let mut samples = [0f32; 4096];
        let mut frames = 0u64;
        let mut energy = 0.0;
        let mut crossings = 0u64;
        let mut previous = 0.0;
        loop {
            let mut count = samples.len() as u32;
            let mut buffers = AudioBufferList {
                mNumberBuffers: 1,
                mBuffers: [AudioBuffer {
                    mNumberChannels: 1,
                    mDataByteSize: count * 4,
                    mData: samples.as_mut_ptr().cast(),
                }],
            };
            assert_eq!(
                unsafe {
                    ExtAudioFileRead(
                        file.0,
                        NonNull::from(&mut count),
                        NonNull::from(&mut buffers),
                    )
                },
                0
            );
            if count == 0 {
                break;
            }
            for &sample in &samples[..count as usize] {
                crossings += u64::from(previous < 0.0 && sample >= 0.0);
                energy += f64::from(sample).powi(2);
                previous = sample;
            }
            frames += count as u64;
        }
        let status = unsafe { ExtAudioFileDispose(file.0) };
        file.0 = ptr::null_mut();
        assert_eq!(status, 0);
        (
            frames,
            crossings as f64 * 16_000.0 / frames as f64,
            (energy / frames as f64).sqrt(),
        )
    }

    #[test]
    fn chunks_over_ten_minutes_into_independently_decodable_timed_audio() {
        let directory = TestDirectory::new();
        let input = directory.0.join("source.wav");
        write_wave(&input, 9_684_000); // 605.25 seconds, with a non-integral final chunk.
        let original = fs::metadata(&input).unwrap().len();
        let info = inspect(&input).unwrap();
        assert_eq!(info.frames, 9_684_000);
        assert_eq!(info.sample_rate, 16_000.0);
        for (index, start, duration, frequency) in [
            (0, 0.0, 300.0, 300.0),
            (1, 300.0, 300.0, 600.0),
            (2, 600.0, 5.25, 900.0),
        ] {
            let output = directory.0.join(format!("chunk-{index}.m4a"));
            write_chunk(&input, &output, start, duration).unwrap();
            let info = inspect(&output).unwrap();
            assert!((info.frames as f64 / info.sample_rate - duration).abs() < 0.1);
            let (frames, measured_frequency, rms) = decoded_tone(&output);
            assert!((frames as f64 / 16_000.0 - duration).abs() < 0.1);
            assert!(
                (measured_frequency - frequency).abs() < 3.0,
                "{measured_frequency} != {frequency}"
            );
            assert!(rms > 0.15 && rms < 0.4, "unexpected RMS: {rms}");
            assert!(fs::metadata(&output).unwrap().len() < 24_000_000);
        }
        assert_eq!(fs::metadata(&input).unwrap().len(), original);
        assert_eq!(inspect(&input).unwrap().frames, 9_684_000);
    }

    #[test]
    fn empty_audio_is_distinct_from_corrupt_audio() {
        let directory = TestDirectory::new();
        let empty = directory.0.join("empty.wav");
        write_wave(&empty, 0);
        assert_eq!(inspect(&empty).unwrap().frames, 0);
        let corrupt = directory.0.join("corrupt.m4a");
        fs::write(&corrupt, b"not an audio file").unwrap();
        assert!(inspect(&corrupt).is_err());
        let output = directory.0.join("chunk.m4a");
        assert!(write_chunk(&empty, &output, 0.0, 1.0).is_err());
        assert!(!output.exists());
    }

    #[test]
    fn truncated_audio_fails_without_publishing_a_chunk() {
        let directory = TestDirectory::new();
        let input = directory.0.join("truncated.wav");
        write_wave(&input, 16_000);
        fs::OpenOptions::new()
            .write(true)
            .open(&input)
            .unwrap()
            .set_len(1044)
            .unwrap();
        let original = fs::read(&input).unwrap();
        let output = directory.0.join("chunk.m4a");
        assert!(write_chunk(&input, &output, 0.0, 1.0).is_err());
        assert!(!output.exists());
        assert_eq!(fs::read(&input).unwrap(), original);
    }

    #[test]
    fn invalid_ranges_and_existing_outputs_preserve_originals() {
        let directory = TestDirectory::new();
        let input = directory.0.join("source.wav");
        write_wave(&input, 16_000);
        let original = fs::read(&input).unwrap();
        for (start, duration) in [
            (0.0, 0.0),
            (-1.0, 1.0),
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
            (0.0, 301.0),
            (0.5, 1.0),
        ] {
            let output = directory.0.join("invalid.m4a");
            assert!(write_chunk(&input, &output, start, duration).is_err());
            assert!(!output.exists());
        }
        assert!(write_chunk(&input, &input, 0.0, 1.0).is_err());
        let linked = directory.0.join("linked.wav");
        fs::hard_link(&input, &linked).unwrap();
        assert!(write_chunk(&input, &linked, 0.0, 1.0).is_err());
        assert_eq!(fs::read(&input).unwrap(), original);
    }
}
