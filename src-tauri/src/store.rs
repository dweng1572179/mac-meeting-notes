use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use chrono::{DateTime, FixedOffset};

use crate::domain::{AppError, AppResult, Session};

pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn list(&self) -> AppResult<Vec<Session>> {
        let directory = self.sessions_dir();
        if !directory.exists() {
            return Ok(Vec::new());
        }

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
        self.read_session(self.sessions_dir().join(format!("{id}.json")))
    }

    pub fn save(&self, session: &Session) -> AppResult<()> {
        self.validate_id(&session.id)?;
        let directory = self.sessions_dir();
        fs::create_dir_all(&directory).map_err(io_error)?;
        let json = serde_json::to_vec_pretty(session).map_err(json_error)?;
        let path = directory.join(format!("{}.json", session.id));
        let temporary_path = directory.join(format!("{}.json.tmp", session.id));
        let mut temporary = File::create(&temporary_path).map_err(io_error)?;
        temporary.write_all(&json).map_err(io_error)?;
        temporary.sync_all().map_err(io_error)?;
        fs::rename(temporary_path, path).map_err(io_error)?;
        sync_directory(&directory)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let session = self.get(id)?;
        if let Some(audio_path) = session
            .audio_path
            .as_deref()
            .and_then(|path| self.owned_audio_path(path))
        {
            fs::remove_file(audio_path).map_err(io_error)?;
        }
        fs::remove_file(self.sessions_dir().join(format!("{id}.json"))).map_err(io_error)?;
        Ok(())
    }

    fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    fn owned_audio_path(&self, audio_path: &str) -> Option<PathBuf> {
        let audio_directory = self.root.join("audio").canonicalize().ok()?;
        let audio_path = PathBuf::from(audio_path).canonicalize().ok()?;
        audio_path
            .starts_with(audio_directory)
            .then_some(audio_path)
    }

    fn read_session(&self, path: PathBuf) -> AppResult<Session> {
        let json = fs::read(path).map_err(io_error)?;
        serde_json::from_slice(&json).map_err(json_error)
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

fn io_error(error: std::io::Error) -> AppError {
    AppError::new("storage_error", error.to_string())
}

fn json_error(error: serde_json::Error) -> AppError {
    AppError::new("serialization_error", error.to_string())
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
