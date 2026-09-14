use std::{path::Path, time::Duration};

use reqwest::{multipart, Client, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::domain::{AppError, AppResult, Session};

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
        ensure_success(response.status())
    }

    pub async fn transcribe(&self, audio_path: &Path, api_key: &str) -> AppResult<String> {
        let audio = std::fs::read(audio_path)
            .map_err(|_| AppError::new("audio_file", "Unable to read recorded audio"))?;
        let filename = audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("recording.m4a")
            .to_owned();
        let form = multipart::Form::new()
            .text("model", "gpt-4o-mini-transcribe")
            .text("response_format", "json")
            .part(
                "file",
                multipart::Part::bytes(audio)
                    .file_name(filename)
                    .mime_str("audio/mp4")
                    .map_err(openai_request_error)?,
            );
        let response = self
            .client
            .post(format!("{}/audio/transcriptions", self.base_url))
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
            .map_err(openai_request_error)?;
        ensure_success(response.status())?;
        response
            .json::<Transcription>()
            .await
            .map(|response| response.text)
            .map_err(|_| AppError::new("openai", "OpenAI returned an invalid transcription"))
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
        ensure_success(response.status())?;
        let response = response
            .json::<Value>()
            .await
            .map_err(|_| AppError::new("openai", "OpenAI returned an invalid response"))?;
        if response["status"] != "completed" {
            return Err(AppError::new("openai", "OpenAI response was not completed"));
        }
        let text = response["output"]
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
            .ok_or_else(|| AppError::new("openai", "OpenAI returned no enrichment content"))?;
        serde_json::from_str(text)
            .map_err(|_| AppError::new("openai", "OpenAI returned invalid enrichment content"))
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
}

pub fn build_enrichment_request(session: &Session) -> Value {
    json!({
        "model": ENRICHMENT_MODEL,
        "store": false,
        "instructions": "Create meeting notes from only the supplied content. Preserve substantive user notes and prioritize their emphasis. Treat a later explicit decision as overriding an earlier tentative suggestion. Retain [SIMULATION] labels. Use only supplied property facts, mark uncertainty, and leave unsupported sections empty.",
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": format!(
                    "Title: {}\nContext: {}\nAttendees: {}\nOriginal notes: {}\nTranscript: {}",
                    session.title,
                    session.context,
                    session.attendees.join(", "),
                    session.original_notes,
                    session.transcript.as_deref().unwrap_or_default(),
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

fn ensure_success(status: StatusCode) -> AppResult<()> {
    match status {
        status if status.is_success() => Ok(()),
        StatusCode::UNAUTHORIZED => Err(AppError::new(
            "invalid_api_key",
            "OpenAI rejected the API key",
        )),
        StatusCode::PAYLOAD_TOO_LARGE => Err(AppError::new(
            "audio_too_large",
            "Recorded audio is too large",
        )),
        StatusCode::TOO_MANY_REQUESTS => {
            Err(AppError::new("rate_limited", "OpenAI rate limit reached"))
        }
        _ => Err(AppError::new("openai", "OpenAI request failed")),
    }
}

fn openai_request_error(_: reqwest::Error) -> AppError {
    AppError::new("openai", "OpenAI request failed")
}
