use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

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
        sessions.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        Ok(sessions)
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
        fs::rename(temporary_path, path).map_err(io_error)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let session = self.get(id)?;
        fs::remove_file(self.sessions_dir().join(format!("{id}.json"))).map_err(io_error)?;
        if let Some(audio_path) = session.audio_path {
            let audio_path = PathBuf::from(audio_path);
            if audio_path.exists() {
                fs::remove_file(audio_path).map_err(io_error)?;
            }
        }
        Ok(())
    }

    fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    fn read_session(&self, path: PathBuf) -> AppResult<Session> {
        let json = fs::read(path).map_err(io_error)?;
        serde_json::from_slice(&json).map_err(json_error)
    }

    fn validate_id(&self, id: &str) -> AppResult<()> {
        if !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
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
