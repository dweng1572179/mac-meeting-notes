use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionSettings {
    pub language: String,
    pub vocabulary: String,
    pub model: String,
}

impl Default for TranscriptionSettings {
    fn default() -> Self {
        Self {
            language: String::new(),
            vocabulary: String::new(),
            model: "gpt-4o-mini-transcribe".into(),
        }
    }
}

impl TranscriptionSettings {
    pub fn new_recording_default() -> Self {
        Self {
            model: "gpt-4o-transcribe-diarize".into(),
            ..Self::default()
        }
    }

    pub fn validate(&self) -> AppResult<()> {
        if !(self.language.is_empty()
            || (self.language.len() == 2
                && self.language.bytes().all(|byte| byte.is_ascii_lowercase())))
        {
            return Err(AppError::new("invalid_transcription_settings", "Language must be empty for automatic detection or a two-letter lowercase language code."));
        }
        if self.vocabulary.chars().count() > 2000 {
            return Err(AppError::new(
                "invalid_transcription_settings",
                "Vocabulary must contain at most 2000 characters.",
            ));
        }
        if !matches!(
            self.model.as_str(),
            "gpt-4o-mini-transcribe" | "gpt-4o-transcribe" | "gpt-4o-transcribe-diarize"
        ) {
            return Err(AppError::new(
                "invalid_transcription_settings",
                "Choose a supported transcription model.",
            ));
        }
        Ok(())
    }
}

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
    pub folder: String,
    pub original_notes: String,
    #[serde(default)]
    pub notes: Option<String>,
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
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub notes: Option<String>,
    pub original_notes: String,
    pub transcript: Option<String>,
    pub enriched_notes: Option<String>,
    pub status: SessionStatus,
    pub error: Option<AppError>,
    pub audio_path: Option<String>,
    #[serde(default)]
    pub microphone_audio_path: Option<String>,
    #[serde(default)]
    pub transcription: Vec<SourceTranscript>,
    #[serde(default)]
    pub transcription_settings: TranscriptionSettings,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub capture_health: Option<crate::recorder::RecordingHealth>,
    #[serde(default)]
    pub capture_segments: Vec<CaptureSegment>,
    #[serde(default)]
    pub segmented_capture: bool,
    #[serde(default)]
    pub live_transcription_error: Option<AppError>,
    #[serde(default)]
    pub ai_suggestions: Option<AiSuggestions>,
    #[serde(default)]
    pub edited_enriched_notes: Option<String>,
    #[serde(default)]
    pub dismissed_suggestions: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AudioSource {
    System,
    Microphone,
}

impl AudioSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System audio",
            Self::Microphone => "Microphone",
        }
    }
    pub fn filename(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Microphone => "mic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptChunk {
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub transcript: Option<String>,
    #[serde(default)]
    pub segment_index: Option<u64>,
    #[serde(default)]
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceTranscript {
    pub source: AudioSource,
    pub chunks: Vec<TranscriptChunk>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MeetingCitation {
    pub session_id: String,
    pub title: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeetingAnswer {
    pub answer: String,
    pub citations: Vec<MeetingCitation>,
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
            folder: String::new(),
            original_notes: String::new(),
            notes: None,
            transcript: None,
            enriched_notes: None,
            status: SessionStatus::Draft,
            error: None,
            audio_path: None,
            microphone_audio_path: None,
            transcription: Vec::new(),
            transcription_settings: TranscriptionSettings::default(),
            warnings: Vec::new(),
            capture_health: None,
            capture_segments: Vec::new(),
            segmented_capture: false,
            live_transcription_error: None,
            ai_suggestions: None,
            edited_enriched_notes: None,
            dismissed_suggestions: Vec::new(),
        }
    }

    pub fn notes(&self) -> String {
        if let Some(notes) = &self.notes {
            return notes.clone();
        }
        let summary = self
            .edited_enriched_notes
            .as_deref()
            .or(self.enriched_notes.as_deref())
            .unwrap_or_default();
        let original = self.original_notes.trim();
        if summary.trim().is_empty() {
            return self.original_notes.clone();
        }
        // Older versions kept manual notes separately; never hide text absent from their summary.
        if !original.is_empty() && !summary.contains(original) {
            format!("{summary}\n\n{}", self.original_notes)
        } else {
            summary.to_owned()
        }
    }

    pub fn update_enriched_notes(&mut self, notes: String, source_notes: &str) {
        if notes.trim().is_empty() {
            return;
        }
        // A response must never replace edits made while its request was in flight.
        let current = self.notes();
        self.notes = Some(if current == source_notes {
            notes.clone()
        } else {
            current
        });
        self.enriched_notes = Some(notes);
    }

    pub fn apply(&mut self, input: UpdateSessionInput) -> AppResult<()> {
        if input.id != self.id {
            return Err(AppError::new(
                "session_id_mismatch",
                "Session ID does not match",
            ));
        }
        if input
            .notes
            .as_ref()
            .is_some_and(|notes| notes.len() > 4 * 1024 * 1024)
        {
            return Err(AppError::new(
                "invalid_notes",
                "Notes must be at most 4 MiB",
            ));
        }
        if let Some(notes) = input.notes {
            self.notes = Some(notes);
        }
        self.title = input.title;
        self.context = input.context;
        self.attendees = input.attendees;
        self.folder = input.folder.trim().to_owned();
        self.original_notes = input.original_notes;
        Ok(())
    }
}

pub fn transition_to_processing(
    mut session: Session,
    audio_path: impl Into<String>,
    microphone_audio_path: Option<String>,
) -> AppResult<Session> {
    if session.status != SessionStatus::Recording {
        return Err(AppError::new(
            "invalid_session_status",
            "Only a recording session can be processed",
        ));
    }
    session.ended_at = Some(Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true));
    session.audio_path = Some(audio_path.into());
    session.microphone_audio_path = microphone_audio_path;
    session.status = SessionStatus::Processing;
    session.error = None;
    Ok(session)
}

pub fn transition_to_failed(mut session: Session, error: AppError) -> Session {
    session.status = SessionStatus::Failed;
    session.error = Some(error);
    session
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSegment {
    pub source: AudioSource,
    pub index: u64,
    pub start_seconds: f64,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub id: String,
    pub speaker: String,
    #[serde(alias = "start")]
    pub start_seconds: f64,
    #[serde(alias = "end")]
    pub end_seconds: f64,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TranscriptionResult {
    pub omitted_speakers: bool,
    pub text: String,
    pub segments: Vec<TranscriptSegment>,
}

impl TranscriptionResult {
    pub fn retain_valid_speakers(&mut self, duration_seconds: f64) -> AppResult<()> {
        if let Err(error) = validate_transcript_segments(&self.segments, duration_seconds) {
            if self.text.trim().is_empty() {
                return Err(error);
            }
            // Speaker annotations are optional; keep usable words without inventing timing.
            self.segments.clear();
            self.omitted_speakers = true;
        }
        Ok(())
    }
}

pub fn validate_transcript_segments(
    segments: &[TranscriptSegment],
    duration_seconds: f64,
) -> AppResult<()> {
    let invalid = || {
        AppError::new(
            "invalid_transcription",
            "Speaker timestamps are invalid. Your audio and saved progress were kept.",
        )
    };
    if !duration_seconds.is_finite() || duration_seconds < 0.0 {
        return Err(invalid());
    }
    let mut ids = std::collections::HashSet::new();
    let mut previous_start = 0.0;
    for segment in segments {
        if segment.id.trim().is_empty()
            || segment.id.len() > 256
            || segment.speaker.trim().is_empty()
            || segment.speaker.len() > 256
            || segment.text.trim().is_empty()
            || !ids.insert(&segment.id)
            || !segment.start_seconds.is_finite()
            || !segment.end_seconds.is_finite()
            || segment.start_seconds < previous_start
            || segment.end_seconds <= segment.start_seconds
            || segment.end_seconds > duration_seconds + 1.0
        {
            return Err(invalid());
        }
        previous_start = segment.start_seconds;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub source_id: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub value: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantSuggestion {
    pub name: String,
    pub speaker_key: Option<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopicSuggestion {
    pub title: String,
    pub starts_at_turn_id: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiSuggestions {
    pub title: Option<Suggestion>,
    pub context: Option<Suggestion>,
    pub category: Option<Suggestion>,
    pub participants: Vec<ParticipantSuggestion>,
    pub topics: Vec<TopicSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptTurn {
    pub id: String,
    pub speaker_key: Option<String>,
    pub source: AudioSource,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeetingSource {
    pub id: String,
    pub text: String,
}

pub fn chunk_key(source: AudioSource, chunk: &TranscriptChunk) -> String {
    let source = match source {
        AudioSource::System => "system",
        AudioSource::Microphone => "microphone",
    };
    let offset = format!("offset-{:.0}", (chunk.start_seconds * 1000.0).round());
    match chunk.segment_index {
        Some(index) => format!("{source}:segment-{index}:{offset}"),
        None => format!("{source}:{offset}"),
    }
}

pub fn transcript_turns(session: &Session) -> Vec<TranscriptTurn> {
    let mut turns = Vec::new();
    for track in &session.transcription {
        for chunk in &track.chunks {
            let Some(text) = chunk
                .transcript
                .as_ref()
                .filter(|text| !text.trim().is_empty())
            else {
                continue;
            };
            let key = chunk_key(track.source, chunk);
            if chunk.segments.is_empty() {
                turns.push(TranscriptTurn {
                    id: format!("{key}:text"),
                    speaker_key: None,
                    source: track.source,
                    start_seconds: chunk.start_seconds,
                    end_seconds: chunk.start_seconds + chunk.duration_seconds,
                    text: text.clone(),
                });
            } else {
                turns.extend(chunk.segments.iter().map(|segment| TranscriptTurn {
                    id: format!("{key}:{}", segment.id),
                    speaker_key: Some(format!("{key}:{}", segment.speaker)),
                    source: track.source,
                    start_seconds: chunk.start_seconds + segment.start_seconds,
                    end_seconds: chunk.start_seconds + segment.end_seconds,
                    text: segment.text.clone(),
                }));
            }
        }
    }
    turns.sort_by(|left, right| {
        left.start_seconds
            .total_cmp(&right.start_seconds)
            .then_with(|| left.id.cmp(&right.id))
    });
    turns
}

pub fn meeting_sources(session: &Session) -> Vec<MeetingSource> {
    let mut sources: Vec<_> = [
        ("manual-title", session.title.clone()),
        ("manual-context", session.context.clone()),
        ("manual-notes", session.notes()),
        ("manual-attendees", session.attendees.join(", ")),
    ]
    .into_iter()
    .filter(|(_, text)| !text.trim().is_empty())
    .map(|(id, text)| MeetingSource {
        id: id.into(),
        text,
    })
    .collect();
    let turns = transcript_turns(session);
    if turns.is_empty() {
        if let Some(text) = session
            .transcript
            .as_ref()
            .filter(|text| !text.trim().is_empty())
        {
            sources.push(MeetingSource {
                id: "legacy-transcript".into(),
                text: text.clone(),
            });
        }
    } else {
        sources.extend(turns.into_iter().map(|turn| MeetingSource {
            id: turn.id,
            text: turn.text,
        }));
        // Preserve any raw words not represented by the provider's timed segments.
        for track in &session.transcription {
            for chunk in &track.chunks {
                if let Some(text) = chunk
                    .transcript
                    .as_ref()
                    .filter(|_| !chunk.segments.is_empty())
                {
                    let segment_words: Vec<_> = chunk
                        .segments
                        .iter()
                        .flat_map(|segment| segment.text.split_whitespace())
                        .collect();
                    if text.split_whitespace().collect::<Vec<_>>() != segment_words {
                        sources.push(MeetingSource {
                            id: format!("raw:{}", chunk_key(track.source, chunk)),
                            text: text.clone(),
                        });
                    }
                }
            }
        }
    }
    sources
}
