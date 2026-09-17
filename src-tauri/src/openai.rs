use std::{path::Path, time::Duration};

use reqwest::{multipart, Client, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::domain::{
    AppError, AppResult, MeetingAnswer, MeetingCitation, Session, TranscriptionSettings,
};

const API_BASE: &str = "https://api.openai.com/v1";
const ENRICHMENT_MODEL: &str = "gpt-6-astra";

pub struct OpenAiClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Deserialize)]
pub struct EnrichedSections {
    pub summary: Vec<String>,
    pub key_points: Vec<String>,
    pub decisions: Vec<String>,
    pub action_items: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawMeetingAnswer {
    answer: String,
    citations: Vec<RawMeetingCitation>,
}

#[derive(Debug, Deserialize)]
struct RawMeetingCitation {
    session_id: String,
    excerpt: String,
}

impl OpenAiClient {
    pub fn new() -> Self {
        Self::with_base_url(API_BASE)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(120))
                .timeout(Duration::from_secs(900))
                .build()
                .expect("OpenAI HTTP client configuration is valid"),
            base_url: base_url.into(),
        }
    }

    pub async fn validate_key(&self, api_key: &str) -> AppResult<()> {
        let response = self
            .client
            .get(format!("{}/models/{ENRICHMENT_MODEL}", self.base_url))
            .timeout(Duration::from_secs(10))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(openai_request_error)?;
        ensure_success(response).await.map(|_| ())
    }

    pub async fn transcribe(&self, audio_path: &Path, api_key: &str) -> AppResult<String> {
        self.transcribe_with_settings(
            audio_path,
            api_key,
            &TranscriptionSettings::default(),
            String::new(),
        )
        .await
    }

    pub async fn transcribe_session(
        &self,
        audio_path: &Path,
        api_key: &str,
        session: &Session,
    ) -> AppResult<String> {
        session.transcription_settings.validate()?;
        let attendees: String = session
            .attendees
            .iter()
            .flat_map(|name| name.chars().chain(std::iter::once(' ')))
            .take(500)
            .collect();
        let context: String = session.context.chars().take(1000).collect();
        let prompt = if session.transcription_settings.vocabulary.is_empty()
            && attendees.trim().is_empty()
            && context.trim().is_empty()
        {
            String::new()
        } else {
            format!("Recognition hints only; transcribe only words actually spoken. Do not add these terms unless heard.\nVocabulary: {}\nAttendees: {}\nContext: {}", session.transcription_settings.vocabulary, attendees, context)
        };
        self.transcribe_with_settings(audio_path, api_key, &session.transcription_settings, prompt)
            .await
    }

    async fn transcribe_with_settings(
        &self,
        audio_path: &Path,
        api_key: &str,
        settings: &TranscriptionSettings,
        prompt: String,
    ) -> AppResult<String> {
        settings.validate()?;
        if std::fs::metadata(audio_path)
            .map_err(|_| AppError::new("audio_file", "Unable to read recorded audio"))?
            .len()
            >= 25_000_000
        {
            return Err(AppError::new(
                "audio_too_large",
                "Recorded audio exceeds the upload limit. Your audio was kept.",
            ));
        }
        let audio = std::fs::read(audio_path)
            .map_err(|_| AppError::new("audio_file", "Unable to read recorded audio"))?;
        if !has_m4a_media(&audio)? {
            return Ok(String::new());
        }
        let filename = audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("recording.m4a")
            .to_owned();
        let mut form = multipart::Form::new()
            .text("model", settings.model.clone())
            .text("response_format", "json")
            .part(
                "file",
                multipart::Part::bytes(audio)
                    .file_name(filename)
                    .mime_str("audio/mp4")
                    .map_err(openai_request_error)?,
            );
        if !settings.language.is_empty() {
            form = form.text("language", settings.language.clone());
        }
        if !prompt.is_empty() {
            form = form.text("prompt", prompt);
        }
        let response = self
            .client
            .post(format!("{}/audio/transcriptions", self.base_url))
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
            .map_err(openai_request_error)?;
        let response = ensure_success(response).await?;
        let transcription = response.json::<Transcription>().await.map_err(|_| {
            AppError::new(
                "openai",
                "OpenAI returned an invalid transcription. Your audio was kept.",
            )
        })?;
        if transcription
            .usage
            .as_ref()
            .and_then(|usage| usage["output_tokens"].as_u64())
            .is_some_and(|tokens| tokens >= 1_900)
        {
            return Err(AppError::new(
                "transcription_too_long",
                "OpenAI reached the transcription output limit. Your audio was kept.",
            ));
        }
        Ok(transcription.text)
    }

    pub async fn enrich(&self, session: &Session, api_key: &str) -> AppResult<EnrichedSections> {
        let response = self
            .client
            .post(format!("{}/responses", self.base_url))
            .timeout(Duration::from_secs(300))
            .bearer_auth(api_key)
            .json(&build_enrichment_request(session))
            .send()
            .await
            .map_err(openai_request_error)?;
        let response = ensure_success(response).await?;
        let response = response
            .json::<Value>()
            .await
            .map_err(|_| AppError::new("openai", "OpenAI returned an invalid response"))?;
        let text = completed_output_text(&response, "enrichment")?;
        serde_json::from_str(text)
            .map_err(|_| AppError::new("openai", "OpenAI returned invalid enrichment content"))
    }

    pub async fn ask_meetings(
        &self,
        sessions: &[Session],
        question: &str,
        api_key: &str,
    ) -> AppResult<MeetingAnswer> {
        let response = self
            .client
            .post(format!("{}/responses", self.base_url))
            .timeout(Duration::from_secs(300))
            .bearer_auth(api_key)
            .json(&build_meeting_question_request(sessions, question)?)
            .send()
            .await
            .map_err(openai_request_error)?;
        let response = ensure_success(response).await?;
        let response = response
            .json::<Value>()
            .await
            .map_err(|_| AppError::new("openai", "OpenAI returned an invalid response"))?;
        let raw: RawMeetingAnswer =
            serde_json::from_str(completed_output_text(&response, "answer")?).map_err(|_| {
                AppError::new("openai", "OpenAI returned an invalid meeting answer")
            })?;
        validate_meeting_answer(raw, sessions)
    }
}

impl Default for OpenAiClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize)]
struct Transcription {
    text: String,
    #[serde(default)]
    usage: Option<Value>,
}

pub fn build_enrichment_request(session: &Session) -> Value {
    json!({
        "model": ENRICHMENT_MODEL,
        "store": false,
        "instructions": "Create meeting notes from only the supplied content. Preserve substantive user notes and prioritize their emphasis. Treat a later explicit decision as overriding an earlier tentative suggestion. Retain [SIMULATION] labels. Use only supplied property facts, mark uncertainty, and leave unsupported sections empty. Source tracks may overlap and are not separate sequential meetings. Respect capture warnings; never imply missing parts were captured.",
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": format!(
                    "Title: {}\nContext: {}\nAttendees: {}\nOriginal notes: {}\nTranscript: {}\nCapture warnings: {}",
                    session.title,
                    session.context,
                    session.attendees.join(", "),
                    session.original_notes,
                    session.transcript.as_deref().unwrap_or_default(),
                    session.warnings.join(" "),
                ),
            }]
        }],
        "text": {
            "format": {
                "type": "json_schema",
                "name": "meeting_notes",
                "strict": true,
                "schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["summary", "key_points", "decisions", "action_items"],
                    "properties": {
                        "summary": { "type": "array", "items": { "type": "string" } },
                        "key_points": { "type": "array", "items": { "type": "string" } },
                        "decisions": { "type": "array", "items": { "type": "string" } },
                        "action_items": { "type": "array", "items": { "type": "string" } }
                    }
                }
            }
        }
    })
}

fn build_meeting_question_request(sessions: &[Session], question: &str) -> AppResult<Value> {
    let mut sources = String::new();
    for session in sessions {
        let source = format!(
            "SOURCE ID: {}\nTITLE: {}\nDATE: {}\n{}\n\n---\n\n",
            session.id,
            session.title,
            session.started_at,
            meeting_source_text(session)
        );
        // Bound the complete selected context; silently clipping would omit later decisions.
        if sources.len().saturating_add(source.len()) > 200_000 {
            let guidance = if sessions.len() == 1 {
                "This meeting is too large to answer from in full. Use transcript search or export to review its complete content."
            } else {
                "These meetings are too large to answer from in full. Choose a smaller folder or ask about one meeting."
            };
            return Err(AppError::new("question_sources_too_large", guidance));
        }
        sources.push_str(&source);
    }
    Ok(json!({
        "model": ENRICHMENT_MODEL,
        "store": false,
        "instructions": "Answer only from the supplied meeting sources. Be concise and specific. Cite each factual claim with one or more source records. Each citation excerpt must be an exact contiguous quote from that source. Never invent a meeting ID or excerpt. Respect capture warnings; never imply missing parts were captured. If the sources do not answer the question, say so plainly and return no citations.",
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": format!("Question: {}\n\nMeeting sources:\n{}", question.trim(), sources),
            }]
        }],
        "text": {
            "format": {
                "type": "json_schema",
                "name": "meeting_answer",
                "strict": true,
                "schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["answer", "citations"],
                    "properties": {
                        "answer": { "type": "string" },
                        "citations": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["session_id", "excerpt"],
                                "properties": {
                                    "session_id": { "type": "string" },
                                    "excerpt": { "type": "string" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }))
}

fn meeting_source_text(session: &Session) -> String {
    format!(
        "Attendees: {}\nContext: {}\nOriginal notes: {}\nTranscript: {}\nEnhanced notes: {}\nCapture warnings: {}",
        session.attendees.join(", "),
        session.context,
        session.original_notes,
        session.transcript.as_deref().unwrap_or_default(),
        session.enriched_notes.as_deref().unwrap_or_default(),
        session.warnings.join("\n"),
    )
}

fn validate_meeting_answer(
    raw: RawMeetingAnswer,
    sessions: &[Session],
) -> AppResult<MeetingAnswer> {
    let answer = raw.answer.trim().to_owned();
    if answer.is_empty() {
        return Err(AppError::new(
            "openai",
            "OpenAI returned an empty meeting answer",
        ));
    }
    let citations = raw
        .citations
        .into_iter()
        .filter_map(|citation| {
            let session = sessions
                .iter()
                .find(|session| session.id == citation.session_id)?;
            let excerpt = citation.excerpt.trim();
            (!excerpt.is_empty() && meeting_source_text(session).contains(excerpt)).then(|| {
                MeetingCitation {
                    session_id: session.id.clone(),
                    title: session.title.clone(),
                    excerpt: excerpt.to_owned(),
                }
            })
        })
        .collect::<Vec<_>>();
    if citations.is_empty() {
        return Err(AppError::new(
            "unverified_answer",
            "OpenAI returned an answer without a verifiable meeting source",
        ));
    }
    Ok(MeetingAnswer { answer, citations })
}

fn completed_output_text<'a>(response: &'a Value, kind: &str) -> AppResult<&'a str> {
    if response["status"] != "completed" {
        return Err(AppError::new("openai", "OpenAI response was not completed"));
    }
    response["output"]
        .as_array()
        .and_then(|output| {
            output.iter().find_map(|item| {
                item["content"].as_array().and_then(|content| {
                    content.iter().find_map(|part| {
                        (part["type"] == "output_text")
                            .then(|| part["text"].as_str())
                            .flatten()
                    })
                })
            })
        })
        .ok_or_else(|| AppError::new("openai", format!("OpenAI returned no {kind} content")))
}

pub fn sections_to_markdown(sections: EnrichedSections) -> String {
    let markdown = [
        ("Summary", sections.summary),
        ("Key Points", sections.key_points),
        ("Decisions", sections.decisions),
        ("Action Items", sections.action_items),
    ]
    .into_iter()
    .filter(|(_, items)| !items.is_empty())
    .map(|(heading, items)| {
        let items = items
            .into_iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("## {heading}\n\n{items}")
    })
    .collect::<Vec<_>>()
    .join("\n\n");
    if markdown.is_empty() {
        markdown
    } else {
        format!("{markdown}\n")
    }
}

async fn ensure_success(mut response: reqwest::Response) -> AppResult<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            value.starts_with("req_")
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        })
        .map(str::to_owned);
    // Error messages can echo credentials or meeting content; classify them, never display the body.
    let mut body = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        if body.len() + chunk.len() > 65_536 {
            break;
        }
        body.extend_from_slice(&chunk);
    }
    let envelope = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    let error = &envelope["error"];
    let api_code = error["code"].as_str().unwrap_or_default();
    let param = error["param"].as_str().unwrap_or_default();
    let reason = error["message"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (code, message) = if status == StatusCode::UNAUTHORIZED || api_code == "invalid_api_key" {
        (
            "invalid_api_key",
            "OpenAI rejected the API key. Update it in Settings.".into(),
        )
    } else if status == StatusCode::FORBIDDEN
        || api_code == "model_not_found"
        || api_code == "permission_denied"
    {
        ("model_access", "This API key cannot access the requested OpenAI model. Check project permissions and model access.".into())
    } else if api_code == "insufficient_quota" {
        (
            "quota_exceeded",
            "OpenAI API quota is exhausted. Check billing and project limits, then retry.".into(),
        )
    } else if status == StatusCode::TOO_MANY_REQUESTS {
        (
            "rate_limited",
            "OpenAI rate limit reached. Wait briefly, then retry.".into(),
        )
    } else if status == StatusCode::PAYLOAD_TOO_LARGE {
        (
            "audio_too_large",
            "Recorded audio exceeds the upload limit. Your audio was kept.".into(),
        )
    } else if api_code == "context_length_exceeded" || reason.contains("maximum content size") {
        (
            "input_too_large",
            "OpenAI rejected input that exceeds its limit. Your notes and pending audio were kept."
                .into(),
        )
    } else if status == StatusCode::BAD_REQUEST
        && (param == "file" || reason.contains("audio file") || reason.contains("invalid audio"))
    {
        (
            "invalid_audio",
            "OpenAI could not read this recording. Your audio was kept.".into(),
        )
    } else if status.is_server_error() {
        (
            "openai_server",
            format!(
                "OpenAI is temporarily unavailable (HTTP {}). Retry later; saved progress is kept.",
                status.as_u16()
            ),
        )
    } else if status == StatusCode::REQUEST_TIMEOUT {
        (
            "openai_timeout",
            "OpenAI timed out. Your notes and pending audio were kept. Retry when ready.".into(),
        )
    } else {
        (
            "openai",
            format!(
                "OpenAI rejected the request (HTTP {}). Your notes and pending audio were kept.",
                status.as_u16()
            ),
        )
    };
    let message = match request_id {
        Some(id) => format!("{message} Request ID: {id}"),
        None => message,
    };
    Err(AppError::new(code, message))
}

fn openai_request_error(error: reqwest::Error) -> AppError {
    if error.is_timeout() {
        AppError::new(
            "openai_timeout",
            "OpenAI timed out. Your notes and pending audio were kept. Retry when ready.",
        )
    } else {
        AppError::new("openai_network", "Could not connect to OpenAI. Check your internet connection and retry. Your notes and pending audio were kept.")
    }
}

fn has_m4a_media(mut bytes: &[u8]) -> AppResult<bool> {
    if bytes.is_empty() {
        return Err(incomplete_audio());
    }
    while !bytes.is_empty() {
        if bytes.len() < 8 {
            return Err(incomplete_audio());
        }
        let declared = u32::from_be_bytes(bytes[..4].try_into().expect("four-byte atom size"));
        let (size, header) = match declared {
            0 => (bytes.len(), 8),
            1 if bytes.len() >= 16 => (
                usize::try_from(u64::from_be_bytes(
                    bytes[8..16].try_into().expect("eight-byte atom size"),
                ))
                .map_err(|_| incomplete_audio())?,
                16,
            ),
            1 => return Err(incomplete_audio()),
            size => (size as usize, 8),
        };
        if size < header || size > bytes.len() {
            return Err(incomplete_audio());
        }
        if &bytes[4..8] == b"mdat" && size > header {
            return Ok(true);
        }
        bytes = &bytes[size..];
    }
    Ok(false)
}

fn incomplete_audio() -> AppError {
    AppError::new(
        "audio_file",
        "Recorded audio is incomplete. Your audio was kept so you can retry.",
    )
}

#[cfg(test)]
mod tests {
    use super::OpenAiClient;

    #[test]
    fn empty_m4a_is_not_sent_to_openai() {
        let path =
            std::env::temp_dir().join(format!("meeting-notes-empty-{}.m4a", uuid::Uuid::new_v4()));
        std::fs::write(
            &path,
            [
                0, 0, 0, 8, b'f', b't', b'y', b'p', 0, 0, 0, 8, b'm', b'o', b'o', b'v',
            ],
        )
        .unwrap();
        let client = OpenAiClient::with_base_url("http://127.0.0.1:9/v1");

        let transcript = tauri::async_runtime::block_on(client.transcribe(&path, "test-key"));

        std::fs::remove_file(path).unwrap();
        assert_eq!(transcript.unwrap(), "");
    }

    #[test]
    fn truncated_m4a_is_not_treated_as_silence() {
        let path = std::env::temp_dir().join(format!(
            "meeting-notes-truncated-{}.m4a",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, [0, 0, 0, 12, b'm', b'd', b'a', b't', 1]).unwrap();
        let client = OpenAiClient::with_base_url("http://127.0.0.1:9/v1");

        let error =
            tauri::async_runtime::block_on(client.transcribe(&path, "test-key")).unwrap_err();

        std::fs::remove_file(path).unwrap();
        assert_eq!(error.code, "audio_file");
    }

    #[test]
    fn extended_and_later_mdat_boxes_are_supported() {
        let bytes = [
            0, 0, 0, 8, b'm', b'd', b'a', b't', 0, 0, 0, 1, b'm', b'd', b'a', b't', 0, 0, 0, 0, 0,
            0, 0, 17, 1,
        ];

        assert!(super::has_m4a_media(&bytes).unwrap());
    }
}
