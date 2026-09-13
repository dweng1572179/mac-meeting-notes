use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionInput {
    pub title: String,
    pub context: String,
    pub attendees: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionInput {
    pub id: String,
    pub title: String,
    pub context: String,
    pub attendees: Vec<String>,
    pub original_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub context: String,
    pub attendees: Vec<String>,
    pub original_notes: String,
    pub transcript: Option<String>,
    pub enriched_notes: Option<String>,
    pub status: SessionStatus,
    pub error: Option<AppError>,
    pub audio_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    Draft,
    Recording,
    Processing,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppError {
    pub code: String,
    pub message: String,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl Session {
    pub fn new(input: CreateSessionInput) -> Self {
        let title = if input.title.trim().is_empty() {
            "Untitled meeting".into()
        } else {
            input.title
        };

        Self {
            id: Uuid::new_v4().to_string(),
            title,
            started_at: Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true),
            ended_at: None,
            context: input.context,
            attendees: input.attendees,
            original_notes: String::new(),
            transcript: None,
            enriched_notes: None,
            status: SessionStatus::Draft,
            error: None,
            audio_path: None,
        }
    }

    pub fn apply(&mut self, input: UpdateSessionInput) -> AppResult<()> {
        if input.id != self.id {
            return Err(AppError::new("session_id_mismatch", "Session ID does not match"));
        }
        self.title = input.title;
        self.context = input.context;
        self.attendees = input.attendees;
        self.original_notes = input.original_notes;
        Ok(())
    }
}
