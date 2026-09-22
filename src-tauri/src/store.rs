use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use chrono::{DateTime, FixedOffset};

use crate::domain::{AppError, AppResult, AudioSource, Session, TranscriptionSettings};

pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn list(&self) -> AppResult<Vec<Session>> {
        let directory = self.sessions_dir()?;
        if !directory.try_exists().map_err(io_error)? {
            return Ok(Vec::new());
        }
        self.cleanup_deletions();

        let mut sessions = Vec::new();
        for entry in fs::read_dir(directory).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                sessions.push(self.read_session(path)?);
            }
        }
        let mut sessions = sessions
            .into_iter()
            .map(|session| Ok((parse_started_at(&session.started_at)?, session)))
            .collect::<AppResult<Vec<_>>>()?;
        sessions.sort_by(|(left, _), (right, _)| right.cmp(left));
        Ok(sessions.into_iter().map(|(_, session)| session).collect())
    }

    pub fn get(&self, id: &str) -> AppResult<Session> {
        self.validate_id(id)?;
        self.read_session(self.sessions_dir()?.join(format!("{id}.json")))
    }

    pub fn save(&self, session: &Session) -> AppResult<()> {
        self.validate_id(&session.id)?;
        validate_transcription(session)?;
        if let Some(path) = session.audio_path.as_deref() {
            self.validate_audio_path(&session.id, path)?;
        }
        if let Some(path) = session.microphone_audio_path.as_deref() {
            self.validate_microphone_audio_path(&session.id, path)?;
        }
        let directory = self.sessions_dir()?;
        let json = serde_json::to_vec_pretty(session).map_err(json_error)?;
        let path = directory.join(format!("{}.json", session.id));
        atomic_write(&path, &json)
    }

    pub fn settings(&self) -> AppResult<TranscriptionSettings> {
        validate_directory(&self.root)?;
        let path = self.root.join("settings.json");
        if !validate_file(&path)? {
            return Ok(TranscriptionSettings::new_recording_default());
        }
        let settings: TranscriptionSettings =
            serde_json::from_slice(&fs::read(path).map_err(io_error)?).map_err(json_error)?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &TranscriptionSettings) -> AppResult<()> {
        settings.validate()?;
        validate_directory(&self.root)?;
        atomic_write(
            &self.root.join("settings.json"),
            &serde_json::to_vec_pretty(settings).map_err(json_error)?,
        )
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let session = self.get(id)?;
        if let Some(path) = session.audio_path.as_deref() {
            self.validate_audio_path(id, path)?;
        }
        if let Some(path) = session.microphone_audio_path.as_deref() {
            self.validate_microphone_audio_path(id, path)?;
        }

        let sessions_directory = self.sessions_dir()?;
        let session_path = sessions_directory.join(format!("{id}.json"));
        let tombstone_path = sessions_directory.join(format!("{id}.json.deleting"));
        if validate_file(&tombstone_path)? {
            return Err(AppError::new(
                "storage_error",
                "A prior meeting deletion still needs cleanup",
            ));
        }
        fs::rename(session_path, &tombstone_path).map_err(io_error)?;

        if sync_directory(&sessions_directory).is_ok() {
            let _ = self.cleanup_deletion(id, &tombstone_path);
        }
        Ok(())
    }

    fn sessions_dir(&self) -> AppResult<PathBuf> {
        validate_directory(&self.root)?;
        let directory = self.root.join("sessions");
        validate_directory(&directory)?;
        Ok(directory)
    }

    pub(crate) fn audio_path(&self, id: &str) -> AppResult<PathBuf> {
        let path = self.root.join("audio").join(format!("{id}.m4a"));
        self.validate_audio_path(id, &path.to_string_lossy())
    }

    pub(crate) fn validate_audio_path(&self, id: &str, audio_path: &str) -> AppResult<PathBuf> {
        self.validate_named_audio_path(id, audio_path, &format!("{id}.m4a"))
    }

    pub(crate) fn validate_microphone_audio_path(
        &self,
        id: &str,
        audio_path: &str,
    ) -> AppResult<PathBuf> {
        self.validate_named_audio_path(id, audio_path, &format!("{id}-mic.m4a"))
    }

    pub(crate) fn chunk_path(&self, id: &str, source: AudioSource) -> AppResult<PathBuf> {
        let filename = format!("{id}-{}-chunk.m4a", source.filename());
        let path = self.root.join("audio").join(&filename);
        self.validate_named_audio_path(id, &path.to_string_lossy(), &filename)
    }

    pub(crate) fn segment_path(
        &self,
        id: &str,
        source: AudioSource,
        index: u64,
    ) -> AppResult<PathBuf> {
        if index > 99_999_999 {
            return Err(invalid_audio_path());
        }
        let filename = format!("{id}-{}-segment-{index:08}.m4a", source.filename());
        let path = self.root.join("audio").join(&filename);
        self.validate_named_audio_path(id, &path.to_string_lossy(), &filename)
    }

    // Only canonical names belong to this meeting. Never follow links during recovery or deletion.
    pub(crate) fn segment_files(&self, id: &str) -> AppResult<Vec<(AudioSource, u64, PathBuf)>> {
        self.validate_id(id)?;
        validate_directory(&self.root)?;
        let directory = self.root.join("audio");
        validate_directory(&directory)?;
        if !directory.try_exists().map_err(io_error)? {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(directory).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            for source in [AudioSource::System, AudioSource::Microphone] {
                let prefix = format!("{id}-{}-segment-", source.filename());
                let Some(index) = name
                    .strip_prefix(&prefix)
                    .and_then(|name| name.strip_suffix(".m4a"))
                else {
                    continue;
                };
                if index.len() != 8 || !index.bytes().all(|byte| byte.is_ascii_digit()) {
                    continue;
                }
                let index: u64 = index.parse().map_err(|_| invalid_audio_path())?;
                files.push((source, index, self.segment_path(id, source, index)?));
            }
        }
        files.sort_by_key(|(source, index, _)| (source.filename(), *index));
        Ok(files)
    }

    fn validate_named_audio_path(
        &self,
        id: &str,
        audio_path: &str,
        filename: &str,
    ) -> AppResult<PathBuf> {
        self.validate_id(id)?;
        let audio_directory = self.root.join("audio");
        let expected = audio_directory.join(filename);
        let audio_path = PathBuf::from(audio_path);
        if audio_path != expected {
            return Err(invalid_audio_path());
        }

        validate_directory(&self.root).map_err(|_| invalid_audio_path())?;
        validate_directory(&audio_directory).map_err(|_| invalid_audio_path())?;
        validate_file(&audio_path).map_err(|_| invalid_audio_path())?;
        Ok(audio_path)
    }

    fn cleanup_deletions(&self) {
        let Ok(directory) = self.sessions_dir() else {
            return;
        };
        let Ok(entries) = fs::read_dir(&directory) else {
            return;
        };
        if sync_directory(&directory).is_err() {
            return;
        }
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".json.deleting"))
            else {
                continue;
            };
            let _ = self.cleanup_deletion(id, &path);
        }
    }

    fn cleanup_deletion(&self, id: &str, tombstone_path: &Path) -> AppResult<()> {
        self.validate_id(id)?;
        let canonical = self.sessions_dir()?.join(format!("{id}.json"));
        if validate_file(&canonical)? {
            return Err(AppError::new(
                "storage_error",
                "Meeting deletion has not committed",
            ));
        }
        let session = self.read_session(tombstone_path.to_owned())?;
        if session.id != id {
            return Err(AppError::new(
                "invalid_session_id",
                "Deletion marker does not match its session",
            ));
        }
        let audio_paths = [
            session
                .audio_path
                .as_deref()
                .map(|path| self.validate_audio_path(id, path)),
            session
                .microphone_audio_path
                .as_deref()
                .map(|path| self.validate_microphone_audio_path(id, path)),
        ];
        let mut removed_audio = false;
        for audio_path in audio_paths
            .into_iter()
            .flatten()
            .chain([
                self.chunk_path(id, AudioSource::System),
                self.chunk_path(id, AudioSource::Microphone),
            ])
            .chain(
                self.segment_files(id)?
                    .into_iter()
                    .map(|(_, _, path)| Ok(path)),
            )
        {
            let audio_path = audio_path?;
            match fs::remove_file(audio_path) {
                Ok(()) => removed_audio = true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error)),
            }
        }
        if removed_audio {
            sync_directory(&self.root.join("audio"))?;
        }
        fs::remove_file(tombstone_path).map_err(io_error)?;
        let _ = sync_directory(&self.sessions_dir()?);
        Ok(())
    }

    fn read_session(&self, path: PathBuf) -> AppResult<Session> {
        self.sessions_dir()?;
        validate_file(&path)?;
        let json = fs::read(path).map_err(io_error)?;
        let session: Session = serde_json::from_slice(&json).map_err(json_error)?;
        validate_transcription(&session)?;
        Ok(session)
    }

    fn validate_id(&self, id: &str) -> AppResult<()> {
        if !id.is_empty()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            Ok(())
        } else {
            Err(AppError::new("invalid_session_id", "Session ID is invalid"))
        }
    }
}

fn validate_transcription(session: &Session) -> AppResult<()> {
    session.transcription_settings.validate()?;
    let invalid = || {
        AppError::new(
            "invalid_transcription",
            "Saved transcription progress is invalid. Your meeting files were kept.",
        )
    };
    for (index, segment) in session.capture_segments.iter().enumerate() {
        if !session.segmented_capture
            || segment.index > 99_999_999
            || !segment.start_seconds.is_finite()
            || segment.start_seconds < 0.0
            || !segment.duration_seconds.is_finite()
            || segment.duration_seconds <= 0.0
            || !(segment.start_seconds + segment.duration_seconds).is_finite()
            || session.capture_segments[..index].iter().any(|other| {
                other.source == segment.source
                    && (other.index == segment.index
                        || (other.start_seconds
                            < segment.start_seconds + segment.duration_seconds - 0.000_001
                            && segment.start_seconds
                                < other.start_seconds + other.duration_seconds - 0.000_001))
            })
        {
            return Err(invalid());
        }
    }
    for (index, track) in session.transcription.iter().enumerate() {
        if session.transcription[..index]
            .iter()
            .any(|prior| prior.source == track.source)
        {
            return Err(invalid());
        }
        let mut end = 0.0;
        for chunk in &track.chunks {
            if !chunk.start_seconds.is_finite()
                || !chunk.duration_seconds.is_finite()
                || chunk.duration_seconds <= 0.0
                || chunk.duration_seconds > 300.0
                || chunk.start_seconds < 0.0
                || if session.segmented_capture {
                    chunk.start_seconds < end - 0.000_001
                } else {
                    (chunk.start_seconds - end).abs() > 0.000_001
                }
            {
                return Err(invalid());
            }
            if let Some(index) = chunk.segment_index {
                let Some(segment) = session
                    .capture_segments
                    .iter()
                    .find(|segment| segment.source == track.source && segment.index == index)
                else {
                    return Err(invalid());
                };
                if chunk.start_seconds < segment.start_seconds - 0.000_001
                    || chunk.start_seconds + chunk.duration_seconds
                        > segment.start_seconds + segment.duration_seconds + 0.000_001
                {
                    return Err(invalid());
                }
            } else if session.segmented_capture {
                return Err(invalid());
            }
            if chunk.error.is_some() && chunk.transcript.is_some() {
                return Err(invalid());
            }
            if !chunk.segments.is_empty() {
                if chunk.transcript.is_none() {
                    return Err(invalid());
                }
                crate::domain::validate_transcript_segments(
                    &chunk.segments,
                    chunk.duration_seconds,
                )?;
            }
            end = chunk.start_seconds + chunk.duration_seconds;
        }
    }
    for segment in &session.capture_segments {
        let mut end = segment.start_seconds;
        for chunk in session
            .transcription
            .iter()
            .filter(|track| track.source == segment.source)
            .flat_map(|track| &track.chunks)
            .filter(|chunk| chunk.segment_index == Some(segment.index))
        {
            if (chunk.start_seconds - end).abs() > 0.000_001 {
                return Err(invalid());
            }
            end = chunk.start_seconds + chunk.duration_seconds;
        }
        if (end - segment.start_seconds - segment.duration_seconds).abs() > 0.000_001 {
            return Err(invalid());
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, json: &[u8]) -> AppResult<()> {
    let directory = path.parent().expect("app data file parent");
    validate_directory(directory)?;
    let temporary_path = path.with_extension("json.tmp");
    validate_file(path)?;
    let temporary_exists = validate_file(&temporary_path)?;
    fs::create_dir_all(directory).map_err(io_error)?;
    if temporary_exists {
        fs::remove_file(&temporary_path).map_err(io_error)?;
    }
    let mut temporary = File::options()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .map_err(io_error)?;
    temporary.write_all(json).map_err(io_error)?;
    temporary.sync_all().map_err(io_error)?;
    fs::rename(temporary_path, path).map_err(io_error)?;
    sync_directory(directory)
}

fn validate_directory(path: &Path) -> AppResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
        _ => Err(AppError::new(
            "storage_error",
            "App data directory must not be a symlink or file",
        )),
    }
}

fn validate_file(path: &Path) -> AppResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
        _ => Err(AppError::new(
            "storage_error",
            "App data file must not be a symlink or directory",
        )),
    }
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::new("storage_error", error.to_string())
}

fn json_error(error: serde_json::Error) -> AppError {
    AppError::new("serialization_error", error.to_string())
}

fn invalid_audio_path() -> AppError {
    AppError::new("invalid_audio_path", "Recorded audio path is invalid")
}

fn parse_started_at(started_at: &str) -> AppResult<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(started_at)
        .map_err(|error| AppError::new("invalid_session_timestamp", error.to_string()))
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> AppResult<()> {
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(io_error)
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn sync_directory_succeeds_after_atomic_rename() {
        let directory =
            std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let temporary = directory.join("session.json.tmp");
        let saved = directory.join("session.json");
        fs::write(&temporary, "{}").unwrap();
        fs::rename(temporary, saved).unwrap();

        super::sync_directory(&directory).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }
}
