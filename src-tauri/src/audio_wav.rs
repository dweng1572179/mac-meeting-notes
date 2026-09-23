//! The small PCM format produced by the Windows recorder. No general media decoder.
use super::*;
use std::{fs::{File, OpenOptions}, io::{Cursor, Read, Seek, SeekFrom, Write}};

fn invalid(message: impl Into<String>) -> AppError { AppError::new("audio_chunk", message) }
fn io_error(error: std::io::Error) -> AppError { invalid(error.to_string()) }

pub(crate) struct Writer {
    file: File,
    sample_rate: u32,
    bytes: u32,
}

impl Writer {
    pub(crate) fn create(path: &Path, sample_rate: u32) -> AppResult<Self> {
        if !(8_000..=192_000).contains(&sample_rate) { return Err(invalid("Unsupported PCM sample rate")); }
        let mut file = OpenOptions::new().write(true).create_new(true).open(path).map_err(io_error)?;
        // An unfinished header must never look like a successfully finalized empty recording.
        file.write_all(&[0; 44]).map_err(io_error)?;
        Ok(Self { file, sample_rate, bytes: 0 })
    }

    #[cfg(any(test, target_os = "windows"))]
    pub(crate) fn frames(&self) -> u64 { u64::from(self.bytes / 2) }

    pub(crate) fn write_pcm(&mut self, pcm: &[u8]) -> AppResult<()> {
        let size = u32::try_from(pcm.len()).ok().and_then(|size| self.bytes.checked_add(size))
            .filter(|size| *size <= u32::MAX - 36);
        if pcm.len() % 2 != 0 || size.is_none() { return Err(invalid("Invalid or oversized PCM recording")); }
        self.file.write_all(pcm).map_err(io_error)?;
        self.bytes = size.unwrap();
        Ok(())
    }

    pub(crate) fn finish(mut self) -> AppResult<()> {
        let mut header = Vec::with_capacity(44);
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&(36 + self.bytes).to_le_bytes());
        header.extend_from_slice(b"WAVEfmt ");
        header.extend_from_slice(&16u32.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&self.sample_rate.to_le_bytes());
        header.extend_from_slice(&(self.sample_rate * 2).to_le_bytes());
        header.extend_from_slice(&2u16.to_le_bytes());
        header.extend_from_slice(&16u16.to_le_bytes());
        header.extend_from_slice(b"data");
        header.extend_from_slice(&self.bytes.to_le_bytes());
        self.file.seek(SeekFrom::Start(0)).map_err(io_error)?;
        self.file.write_all(&header).map_err(io_error)?;
        self.file.sync_all().map_err(io_error)
    }
}

struct PcmInfo { info: AudioInfo, data_start: u64 }

fn parse(reader: &mut (impl Read + Seek)) -> AppResult<PcmInfo> {
    let length = reader.seek(SeekFrom::End(0)).map_err(io_error)?;
    reader.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let mut header = [0; 12];
    reader.read_exact(&mut header).map_err(io_error)?;
    let declared = u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap())) + 8;
    if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" || declared != length {
        return Err(invalid("Incomplete or invalid WAV recording"));
    }
    let mut sample_rate = None;
    let mut data = None;
    let mut position = 12u64;
    while position < length {
        if length - position < 8 { return Err(invalid("Truncated WAV chunk")); }
        reader.seek(SeekFrom::Start(position)).map_err(io_error)?;
        let mut chunk = [0; 8];
        reader.read_exact(&mut chunk).map_err(io_error)?;
        let size = u64::from(u32::from_le_bytes(chunk[4..].try_into().unwrap()));
        let start = position + 8;
        let end = start + size;
        if end > length || end + size % 2 > length { return Err(invalid("Truncated WAV audio")); }
        match &chunk[..4] {
            b"fmt " => {
                if sample_rate.is_some() || size != 16 { return Err(invalid("Unsupported WAV format")); }
                let mut format = [0; 16];
                reader.read_exact(&mut format).map_err(io_error)?;
                let rate = u32::from_le_bytes(format[4..8].try_into().unwrap());
                if format[..4] != [1, 0, 1, 0] || format[12..] != [2, 0, 16, 0]
                    || !(8_000..=192_000).contains(&rate)
                    || u32::from_le_bytes(format[8..12].try_into().unwrap()) != rate * 2 {
                    return Err(invalid("Expected mono 16-bit PCM audio"));
                }
                sample_rate = Some(rate);
            }
            b"data" => {
                if data.is_some() || size % 2 != 0 { return Err(invalid("Invalid WAV sample data")); }
                data = Some((start, size / 2));
            }
            _ => {}
        }
        position = end + size % 2;
    }
    let sample_rate = sample_rate.ok_or_else(|| invalid("Missing WAV format"))?;
    let (data_start, frames) = data.ok_or_else(|| invalid("Missing WAV samples"))?;
    Ok(PcmInfo { info: AudioInfo { frames, sample_rate: f64::from(sample_rate) }, data_start })
}

pub(crate) fn inspect(path: &Path) -> AppResult<AudioInfo> {
    parse(&mut File::open(path).map_err(io_error)?).map(|pcm| pcm.info)
}

pub(crate) fn inspect_bytes(bytes: &[u8]) -> AppResult<AudioInfo> {
    parse(&mut Cursor::new(bytes)).map(|pcm| pcm.info)
}

pub(crate) fn write_chunk(input: &Path, output: &Path, start: f64, duration: f64) -> AppResult<()> {
    if !start.is_finite() || start < 0.0 || !duration.is_finite() || duration <= 0.0 || duration > 300.0 {
        return Err(invalid("Invalid audio chunk time range"));
    }
    let mut source = File::open(input).map_err(io_error)?;
    let pcm = parse(&mut source)?;
    let first = (start * pcm.info.sample_rate).round();
    let frames = (duration * pcm.info.sample_rate).round();
    if first >= pcm.info.frames as f64 || !first.is_finite() || frames < 1.0
        || first + frames > pcm.info.frames as f64 + 1.0 {
        return Err(invalid("Audio chunk extends beyond the source duration"));
    }
    let frames = (frames as u64).min(pcm.info.frames - first as u64);
    if frames * 2 + 44 >= 24_000_000 {
        return Err(AppError::new("audio_chunk_size", "Audio chunk exceeds the safe upload size"));
    }
    source.seek(SeekFrom::Start(pcm.data_start + first as u64 * 2)).map_err(io_error)?;
    let mut writer = Writer::create(output, pcm.info.sample_rate as u32)?;
    let result = (|| {
        let mut remaining = frames * 2;
        let mut buffer = [0; 16_384];
        while remaining > 0 {
            let count = remaining.min(buffer.len() as u64) as usize;
            source.read_exact(&mut buffer[..count]).map_err(io_error)?;
            writer.write_pcm(&buffer[..count])?;
            remaining -= count as u64;
        }
        writer.finish()
    })();
    if result.is_err() { let _ = std::fs::remove_file(output); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf, sync::atomic::{AtomicU64, Ordering}};

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!("meeting-notes-wave-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn durable_wave_chunks_preserve_exact_samples_and_tail() {
        let directory = Directory::new();
        let input = directory.0.join("source.wav");
        let samples: Vec<u8> = (0..32_123).flat_map(|frame| ((frame % 30_000) as i16).to_le_bytes()).collect();
        let mut writer = Writer::create(&input, 16_000).unwrap();
        writer.write_pcm(&samples).unwrap();
        assert_eq!(writer.frames(), 32_123);
        writer.finish().unwrap();
        let info = inspect_bytes(&fs::read(&input).unwrap()).unwrap();
        assert_eq!(info.frames, 32_123);
        assert_eq!(info.sample_rate, 16_000.0);
        let output = directory.0.join("tail.wav");
        write_chunk(&input, &output, 1.0, 16_123.0 / 16_000.0).unwrap();
        let result = fs::read(&output).unwrap();
        assert_eq!(inspect_bytes(&result).unwrap().frames, 16_123);
        assert_eq!(&result[44..], &samples[32_000..]);
        assert!(write_chunk(&input, &input, 0.0, 1.0).is_err());
        assert!(write_chunk(&input, &output, 0.0, 1.0).is_err());
        assert_eq!(&fs::read(&input).unwrap()[44..], &samples);
    }

    #[test]
    fn rejects_truncation_malformed_format_and_unfinished_audio() {
        let directory = Directory::new();
        let source = directory.0.join("source.wav");
        let mut writer = Writer::create(&source, 16_000).unwrap();
        writer.write_pcm(&[0, 0, 1, 0]).unwrap();
        writer.finish().unwrap();
        let valid = fs::read(&source).unwrap();
        for length in 0..valid.len() {
            assert!(inspect_bytes(&valid[..length]).is_err(), "accepted truncated length {length}");
        }
        for offset in [0, 4, 8, 12, 16, 20, 22, 24, 28, 32, 34, 36, 40] {
            let mut malformed = valid.clone();
            malformed[offset] ^= 0xff;
            assert!(inspect_bytes(&malformed).is_err(), "accepted corrupt field {offset}");
        }
        let unfinished = directory.0.join("unfinished.wav");
        let mut writer = Writer::create(&unfinished, 16_000).unwrap();
        writer.write_pcm(&[1, 0]).unwrap();
        drop(writer);
        assert!(inspect_bytes(&fs::read(unfinished).unwrap()).is_err());
        let empty = directory.0.join("empty.wav");
        Writer::create(&empty, 16_000).unwrap().finish().unwrap();
        assert_eq!(inspect_bytes(&fs::read(empty).unwrap()).unwrap().frames, 0);
    }

    #[test]
    fn bounds_ranges_and_refuses_oversized_upload_without_output() {
        let directory = Directory::new();
        let input = directory.0.join("source.wav");
        let output = directory.0.join("chunk.wav");
        let mut writer = Writer::create(&input, 48_000).unwrap();
        assert!(writer.write_pcm(&[1]).is_err());
        // Sparse source is unnecessary: one repeated 96 KB block keeps test memory bounded.
        for _ in 0..300 { writer.write_pcm(&vec![0; 96_000]).unwrap(); }
        writer.finish().unwrap();
        assert!(write_chunk(&input, &output, 0.0, 300.0).is_err());
        assert!(!output.exists());
        for (start, duration) in [(f64::NAN, 1.0), (-1.0, 1.0), (0.0, 301.0), (300.0, 1.0), (299.0, 2.0)] {
            assert!(write_chunk(&input, &output, start, duration).is_err());
            assert!(!output.exists());
        }
        assert!(Writer::create(&output, 0).is_err());
        assert!(!output.exists());
    }
}
