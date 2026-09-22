use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver},
    thread,
};

use meeting_notes_lib::{
    domain::{CreateSessionInput, Session},
    openai::{build_enrichment_request, sections_to_markdown, EnrichedSections, OpenAiClient},
};
use serde_json::{json, Value};

#[test]
fn transcription_rejects_output_limit_instead_of_saving_a_truncated_transcript() {
    let path =
        std::env::temp_dir().join(format!("meeting-notes-limit-{}.m4a", uuid::Uuid::new_v4()));
    std::fs::write(&path, [0, 0, 0, 9, b'm', b'd', b'a', b't', 1]).unwrap();
    let (base_url, _) = local_server(json_response(
        200,
        r#"{"text":"cut off","usage":{"type":"tokens","output_tokens":2000}}"#,
    ));
    let result = tauri::async_runtime::block_on(
        OpenAiClient::with_base_url(base_url).transcribe(&path, "test-key"),
    );
    assert!(path.exists());
    std::fs::remove_file(path).unwrap();
    assert_eq!(result.unwrap_err().code, "transcription_too_long");
}

#[test]
fn broken_success_body_is_a_network_failure_and_missing_words_are_not_silence() {
    let path = std::env::temp_dir().join(format!("body-error-{}.m4a", uuid::Uuid::new_v4()));
    std::fs::write(&path, [0, 0, 0, 9, b'm', b'd', b'a', b't', 1]).unwrap();
    for (response, expected) in [
        ("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n{\"text\":", "openai_network"),
        (json_response(200, r#"{"segments":[]}"#).as_str(), "openai"),
        (json_response(200, r#"{"text":null}"#).as_str(), "openai"),
    ] {
        let (url, _) = local_server(response.to_owned());
        let error = tauri::async_runtime::block_on(
            OpenAiClient::with_base_url(url).transcribe(&path, "test-key"),
        ).unwrap_err();
        assert_eq!(error.code, expected);
        assert!(path.exists());
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn oversized_upload_is_rejected_locally_and_kept() {
    let path =
        std::env::temp_dir().join(format!("meeting-notes-large-{}.m4a", uuid::Uuid::new_v4()));
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(25_000_001).unwrap();
    let result = tauri::async_runtime::block_on(
        OpenAiClient::with_base_url("http://127.0.0.1:9/v1").transcribe(&path, "test-key"),
    );
    assert!(path.exists());
    std::fs::remove_file(path).unwrap();
    assert_eq!(result.unwrap_err().code, "audio_too_large");
}

#[test]
fn openai_invalid_audio_preserves_reason_and_request_id_without_echoing_private_content() {
    let body = r#"{"error":{"message":"Audio file might be corrupted or unsupported","type":"invalid_request_error","code":"invalid_value","param":"file"}}"#;
    let response = json_response(400, body).replacen(
        "Content-Type:",
        "x-request-id: req_audio_123\r\nContent-Type:",
        1,
    );
    let (base_url, _) = local_server(response);
    let error = tauri::async_runtime::block_on(
        OpenAiClient::with_base_url(base_url).validate_key("never-echo-this-key"),
    )
    .unwrap_err();
    assert_eq!(error.code, "invalid_audio");
    assert_eq!(
        error.message,
        "OpenAI could not read this recording. Your audio was kept. Request ID: req_audio_123"
    );
}

#[test]
fn openai_errors_distinguish_actionable_causes_and_never_echo_response_secrets() {
    for (status, envelope, expected) in [
        (
            401,
            r#"{"message":"Incorrect API key: sk-secret","code":"invalid_api_key"}"#,
            "invalid_api_key",
        ),
        (
            403,
            r#"{"message":"Denied private-project","code":"permission_denied"}"#,
            "model_access",
        ),
        (
            404,
            r#"{"code":"model_not_found","param":"model"}"#,
            "model_access",
        ),
        (429, r#"{"code":"insufficient_quota"}"#, "quota_exceeded"),
        (429, r#"{"code":"rate_limit_exceeded"}"#, "rate_limited"),
        (
            400,
            r#"{"code":"context_length_exceeded"}"#,
            "input_too_large",
        ),
        (413, r#"{}"#, "audio_too_large"),
        (
            500,
            r#"{"message":"sk-secret private-project","code":"server_error"}"#,
            "openai_server",
        ),
    ] {
        let body = format!("{{\"error\":{envelope}}}");
        let response = json_response(status, &body).replacen(
            "Content-Type:",
            "x-request-id: req_test_456\r\nContent-Type:",
            1,
        );
        let (base_url, _) = local_server(response);
        let error = tauri::async_runtime::block_on(
            OpenAiClient::with_base_url(base_url).validate_key("sk-secret"),
        )
        .unwrap_err();
        assert_eq!(error.code, expected, "status {status}");
        assert!(error.message.contains("Request ID: req_test_456"));
        assert!(!error.message.contains("sk-secret"));
        assert!(!error.message.contains("private-project"));
    }
}

#[test]
fn non_json_openai_failure_keeps_status_and_network_failure_is_distinct() {
    let (base_url, _) = local_server(json_response(502, "<html>bad gateway</html>"));
    let error = tauri::async_runtime::block_on(
        OpenAiClient::with_base_url(base_url).validate_key("test-key"),
    )
    .unwrap_err();
    assert_eq!(error.code, "openai_server");
    assert!(error.message.contains("502"));
    let error = tauri::async_runtime::block_on(
        OpenAiClient::with_base_url("http://127.0.0.1:9/v1").validate_key("test-key"),
    )
    .unwrap_err();
    assert_eq!(error.code, "openai_network");
}

#[test]
fn enrichment_request_preserves_dynamic_session_content_at_input_path() {
    let session = session();

    let request = build_enrichment_request(&session);
    let input = request["input"][0]["content"][0]["text"].as_str().unwrap();
    assert!(input.contains("[SIMULATION] Harbor Office 73"));
    assert!(input.contains("CONTEXT_TOKEN_814"));
    assert!(input.contains("NOTES_TOKEN_299"));
    assert!(input.contains("Later explicit decision: pause for TRANSCRIPT_TOKEN_552."));
    assert_eq!(request["model"], "gpt-4.1-mini-2025-04-14");
    assert_eq!(request["store"], false);
}

#[test]
fn markdown_omits_empty_sections_and_includes_decisions() {
    let markdown = sections_to_markdown(EnrichedSections {
        suggestions: Default::default(),
        omitted_suggestions: false,
        summary: Vec::new(),
        key_points: Vec::new(),
        decisions: vec!["Pause until the rent roll is verified.".into()],
        action_items: Vec::new(),
    });

    assert_eq!(
        markdown,
        "## Decisions\n\n- Pause until the rent roll is verified.\n"
    );
}

#[test]
fn enrich_posts_request_and_parses_completed_output_text() {
    let response = json_response(
        200,
        &json!({
            "status": "completed",
            "output": [{
                "content": [{
                    "type": "output_text",
                    "text": "{\"summary\":[\"Summary\"],\"key_points\":[],\"decisions\":[\"Pause\"],\"action_items\":[]}"
                }]
            }]
        })
        .to_string(),
    );
    let (base_url, requests) = local_server(response);
    let client = OpenAiClient::with_base_url(base_url);

    let sections = tauri::async_runtime::block_on(client.enrich(&session(), "test-key")).unwrap();
    let request = requests.recv().unwrap();
    let payload: Value = serde_json::from_str(&request.body).unwrap();

    assert_eq!(request.request_line, "POST /v1/responses HTTP/1.1");
    assert!(request.headers.contains("authorization: bearer test-key"));
    assert_eq!(payload["model"], "gpt-4.1-mini-2025-04-14");
    assert_eq!(payload["store"], false);
    assert!(payload["input"][0]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("TRANSCRIPT_TOKEN_552"));
    assert_eq!(sections.decisions, vec!["Pause"]);
}

#[test]
fn enrich_rejects_incomplete_or_non_output_text_responses() {
    for response in [
        json!({
            "status": "in_progress",
            "output": [{ "content": [{ "type": "output_text", "text": "{}" }] }]
        }),
        json!({
            "status": "completed",
            "output": [{ "content": [{ "type": "refusal", "text": "{}" }] }]
        }),
    ] {
        let (base_url, _) = local_server(json_response(200, &response.to_string()));
        let result = tauri::async_runtime::block_on(
            OpenAiClient::with_base_url(base_url).enrich(&session(), "test-key"),
        );

        assert_eq!(result.unwrap_err().code, "openai");
    }
}

#[test]
fn openai_statuses_map_to_stable_error_codes() {
    for (status, code) in [
        (401, "invalid_api_key"),
        (413, "audio_too_large"),
        (429, "rate_limited"),
    ] {
        let (base_url, _) = local_server(json_response(status, "{}"));
        let result = tauri::async_runtime::block_on(
            OpenAiClient::with_base_url(base_url).validate_key("test-key"),
        );

        assert_eq!(result.unwrap_err().code, code);
    }
}

#[test]
fn transcription_upload_identifies_m4a_as_audio_mp4() {
    let path = std::env::temp_dir().join(format!("meeting-notes-{}.m4a", uuid::Uuid::new_v4()));
    std::fs::write(
        &path,
        [
            0, 0, 0, 8, b'f', b't', b'y', b'p', 0, 0, 0, 9, b'm', b'd', b'a', b't', 1,
        ],
    )
    .unwrap();
    let (base_url, requests) = local_server(json_response(200, r#"{"text":"spoken notes"}"#));
    let result = tauri::async_runtime::block_on(
        OpenAiClient::with_base_url(base_url).transcribe(&path, "test-key"),
    );
    let request = requests.recv().unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(result.unwrap(), "spoken notes");
    assert!(request
        .body
        .to_lowercase()
        .contains("content-type: audio/mp4\r\n"));
}

#[test]
fn folder_question_resolves_passage_ids_to_saved_text() {
    let mut first = session();
    first.id = "first-meeting".into();
    first.title = "Harbor review".into();
    first.transcript = Some("Exact source sentence about the rent roll.".into());
    let mut second = session();
    second.id = "second-meeting".into();
    second.title = "Leasing review".into();
    second.transcript = Some("SECOND_SOURCE_TOKEN_916".into());
    let response = json_response(
        200,
        &json!({
            "status": "completed",
            "output": [{
                "content": [{
                    "type": "output_text",
                    "text": serde_json::json!({
                        "answer": "The rent roll still needs review.",
                        "supported": true,
                        "source_ids": ["m0:transcript:0", "m0:transcript:0"]
                    }).to_string()
                }]
            }]
        })
        .to_string(),
    );
    let (base_url, requests) = local_server(response);
    let client = OpenAiClient::with_base_url(base_url);

    let answer = tauri::async_runtime::block_on(client.ask_meetings(
        &[first, second],
        "What is outstanding?",
        "test-key",
    ))
    .unwrap();
    let request = requests.recv().unwrap();
    let payload: Value = serde_json::from_str(&request.body).unwrap();
    let input = payload["input"][0]["content"][0]["text"].as_str().unwrap();

    assert_eq!(payload["model"], "gpt-4.1-mini-2025-04-14");
    assert_eq!(payload["store"], false);
    assert!(input.contains("first-meeting"));
    assert!(input.contains("SECOND_SOURCE_TOKEN_916"));
    assert_eq!(answer.answer, "The rent roll still needs review.");
    assert_eq!(answer.citations.len(), 1);
    assert_eq!(answer.citations[0].session_id, "first-meeting");
    assert_eq!(answer.citations[0].title, "Harbor review");
}

#[test]
fn meeting_questions_use_saved_ai_edits_instead_of_the_old_baseline() {
    let mut source = session();
    source.enriched_notes = Some("OLD_BASELINE_SHOULD_NOT_APPEAR".into());
    source.edited_enriched_notes = Some("My correction: the decision is Wednesday.".into());
    let response = json_response(
        200,
        &json!({"status":"completed","output":[{"content":[{
            "type":"output_text", "text":json!({"answer":"Wednesday.","supported":true,"source_ids":["m0:notes:0"]}).to_string()
        }]}]})
        .to_string(),
    );
    let (url, requests) = local_server(response);
    let answer = tauri::async_runtime::block_on(OpenAiClient::with_base_url(url).ask_meetings(
        &[source],
        "When?",
        "test-key",
    ));
    let request = requests.recv().unwrap();
    assert!(request
        .body
        .contains("My correction: the decision is Wednesday."));
    assert!(!request.body.contains("OLD_BASELINE_SHOULD_NOT_APPEAR"));
    assert_eq!(answer.unwrap().citations.len(), 1);
}

#[test]
fn folder_question_rejects_an_answer_with_only_invented_citations() {
    let mut source = session();
    source.id = "real-meeting".into();
    let response = json_response(
        200,
        &json!({
            "status": "completed",
            "output": [{
                "content": [{
                    "type": "output_text",
                    "text": serde_json::json!({
                        "answer": "Unsupported claim.",
                        "supported": true,
                        "source_ids": ["m0:transcript:0", "invented"]
                    }).to_string()
                }]
            }]
        })
        .to_string(),
    );
    let (base_url, _requests) = local_server(response);

    let error = tauri::async_runtime::block_on(OpenAiClient::with_base_url(base_url).ask_meetings(
        &[source],
        "What happened?",
        "test-key",
    ))
    .unwrap_err();

    assert_eq!(error.code, "unverified_answer");
}

#[test]
fn folder_question_rejects_an_uncited_answer() {
    let response = json_response(
        200,
        &json!({
            "status": "completed",
            "output": [{
                "content": [{
                    "type": "output_text",
                    "text": serde_json::json!({
                        "answer": "Unsupported claim.",
                        "supported": true, "source_ids": []
                    }).to_string()
                }]
            }]
        })
        .to_string(),
    );
    let (base_url, _requests) = local_server(response);

    let error = tauri::async_runtime::block_on(OpenAiClient::with_base_url(base_url).ask_meetings(
        &[session()],
        "What happened?",
        "test-key",
    ))
    .unwrap_err();

    assert_eq!(error.code, "unverified_answer");
}

fn session() -> Session {
    let mut session = Session::new(CreateSessionInput {
        title: "[SIMULATION] Harbor Office 73".into(),
        context: "Only supplied property facts are valid. CONTEXT_TOKEN_814".into(),
        attendees: vec!["[SIMULATION] Alex".into()],
    });
    session.original_notes = "rent roll is the issue NOTES_TOKEN_299".into();
    session.transcript = Some(
        "Alex: We should proceed.\nAlex: Later explicit decision: pause for TRANSCRIPT_TOKEN_552."
            .into(),
    );
    session
}

struct CapturedRequest {
    request_line: String,
    headers: String,
    body: String,
}

fn local_server(response: String) -> (String, Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        stream.write_all(response.as_bytes()).unwrap();
        let _ = sender.send(request);
    });
    (format!("http://{address}/v1"), receiver)
}

fn read_request(stream: &mut TcpStream) -> CapturedRequest {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let read = stream.read(&mut chunk).unwrap();
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(headers_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..headers_end]);
            let content_length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .and_then(|length| length.parse::<usize>().ok())
                .unwrap_or_default();
            if bytes.len() >= headers_end + 4 + content_length {
                let mut lines = headers.lines();
                return CapturedRequest {
                    request_line: lines.next().unwrap().into(),
                    headers: headers.to_lowercase(),
                    body: String::from_utf8_lossy(&bytes[headers_end + 4..]).into(),
                };
            }
        }
    }
}

fn json_response(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn product_transcription_multipart_uses_validated_settings_and_bounded_hints() {
    use meeting_notes_lib::domain::TranscriptionSettings;
    let path = std::env::temp_dir().join(format!("hints-{}.m4a", uuid::Uuid::new_v4()));
    std::fs::write(&path, [0, 0, 0, 9, b'm', b'd', b'a', b't', 1]).unwrap();
    for language in ["", "fr"] {
        let mut source = session();
        source.transcription_settings = TranscriptionSettings {
            language: language.into(),
            vocabulary: "ZyntriQix".into(),
            model: "gpt-4o-transcribe".into(),
        };
        source.context = format!("Bail commercial {}", "界".repeat(5000));
        source.attendees = vec!["Élodie".into()];
        let (url, requests) = local_server(json_response(200, r#"{"text":"spoken notes"}"#));
        tauri::async_runtime::block_on(
            OpenAiClient::with_base_url(url).transcribe_session(&path, "test-key", &source),
        )
        .unwrap();
        let body = requests.recv().unwrap().body;
        assert!(body.contains("gpt-4o-transcribe"));
        assert!(body.contains("ZyntriQix"));
        assert!(body.contains("Élodie"));
        assert!(body.contains("Bail commercial"));
        assert_eq!(body.contains("name=\"language\""), !language.is_empty());
        assert!(body.len() < 20_000);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn meeting_question_includes_late_decisions_and_capture_warnings() {
    let mut source = session();
    let decision = "Final decision: postpone until Monday.";
    source.transcript = Some(format!(
        "{}\n{decision}",
        "Earlier discussion. ".repeat(600)
    ));
    let full_transcript = source.transcript.clone().unwrap();
    source.warnings = vec!["Capture interrupted; coverage is incomplete.".into()];
    let response = json_response(200, &json!({
        "status": "completed", "output": [{ "content": [{ "type": "output_text", "text": json!({
            "answer": "Postpone until Monday.", "supported": true, "source_ids": ["m0:transcript:10"]
        }).to_string() }] }]
    }).to_string());
    let (url, requests) = local_server(response);
    let answer = tauri::async_runtime::block_on(OpenAiClient::with_base_url(url).ask_meetings(
        &[source],
        "Final decision?",
        "test-key",
    ));
    let payload: Value = serde_json::from_str(&requests.recv().unwrap().body).unwrap();
    let text = payload["input"][0]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains(decision),
        "late source material must reach the request"
    );
    assert!(text.contains("Capture interrupted; coverage is incomplete."));
    let context: Value = serde_json::from_str(text).unwrap();
    let joined: String = context["passages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|source| source["id"].as_str().unwrap().starts_with("m0:transcript:"))
        .map(|source| source["text"].as_str().unwrap())
        .collect();
    assert_eq!(joined, full_transcript);
    assert!(answer.unwrap().citations[0].excerpt.contains(decision));
}

#[test]
fn meeting_question_rejects_oversized_sources_before_network() {
    for count in [1, 2] {
        let mut source = session();
        source.transcript = Some("界".repeat(34_000 * (3 - count)));
        let error = tauri::async_runtime::block_on(
            OpenAiClient::with_base_url("http://127.0.0.1:9/v1").ask_meetings(
                &vec![source; count],
                "What happened?",
                "test-key",
            ),
        )
        .unwrap_err();
        assert_eq!(error.code, "question_sources_too_large");
        assert!(error.message.contains(if count == 1 {
            "search or export"
        } else {
            "smaller"
        }));
    }
    let mut source = session();
    source.title = "x".repeat(200_001);
    let error = tauri::async_runtime::block_on(
        OpenAiClient::with_base_url("http://127.0.0.1:9/v1").ask_meetings(
            &[source],
            "What happened?",
            "test-key",
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "question_sources_too_large");
}

#[test]
fn diarization_posts_supported_fields_and_preserves_timed_speakers() {
    let path = std::env::temp_dir().join(format!("diarize-{}.m4a", uuid::Uuid::new_v4()));
    std::fs::write(&path, [0, 0, 0, 9, b'm', b'd', b'a', b't', 1]).unwrap();
    let mut source = session();
    source.transcription_settings.model = "gpt-4o-transcribe-diarize".into();
    source.transcription_settings.language = "en".into();
    source.transcription_settings.vocabulary = "must not send".into();
    let (url, requests) = local_server(json_response(200, &json!({"text":"I'm Maya.","duration":3.0,"segments":[{"id":"seg0","speaker":"A","start":0.2,"end":2.9,"text":"I'm Maya."}]}).to_string()));
    let result = tauri::async_runtime::block_on(
        OpenAiClient::with_base_url(url).transcribe_session(&path, "test-key", &source),
    );
    std::fs::remove_file(path).unwrap();
    assert!(result.is_ok(), "diarization must be supported: {result:?}");
    let result = result.unwrap();
    assert_eq!(result.text, "I'm Maya.");
    assert_eq!(result.segments[0].speaker, "A");
    assert_eq!(result.segments[0].start_seconds, 0.2);
    let body = requests.recv().unwrap().body;
    assert!(body.contains("diarized_json"));
    assert!(body.contains("name=\"chunking_strategy\"\r\n\r\nauto"));
    assert!(!body.contains("name=\"prompt\""));
    assert!(body.contains("name=\"language\"\r\n\r\nen"));
}

#[test]
fn enrichment_rejects_invented_suggestion_evidence() {
    for evidence in [
        json!({"sourceId":"invented","excerpt":"rent roll"}),
        json!({"sourceId":"manual-notes","excerpt":"Invented quote"}),
    ] {
        let response = json_response(200, &json!({"status":"completed","output":[{"content":[{"type":"output_text","text":json!({
            "summary":[],"key_points":[],"decisions":[],"action_items":[],
            "suggestions":{"title":{"value":"Rent roll review","evidence":[evidence]},"context":null,"category":null,"participants":[],"topics":[]}
        }).to_string()}]}]}).to_string());
        let (url, _) = local_server(response);
        let result = tauri::async_runtime::block_on(
            OpenAiClient::with_base_url(url).enrich(&session(), "test-key"),
        );
        let sections = result.expect("Unsupported metadata must not fail usable notes");
        assert!(
            sections.suggestions.title.is_none(),
            "invented evidence must not be accepted"
        );
        assert!(sections.omitted_suggestions);
    }
}

#[test]
fn diarization_keeps_words_when_speaker_metadata_is_invalid() {
    let path = std::env::temp_dir().join(format!("diarize-invalid-{}.m4a", uuid::Uuid::new_v4()));
    std::fs::write(&path, [0, 0, 0, 9, b'm', b'd', b'a', b't', 1]).unwrap();
    let mut source = session();
    source.transcription_settings.model = "gpt-4o-transcribe-diarize".into();
    for segments in [
        json!([]),
        json!(null),
        json!({"invalid":"metadata"}),
        json!([{"id":"s0","speaker":null,"start":0.0,"end":2.0,"text":"Words"}]),
        json!([{"id":"s0","speaker":"A","start":"0","end":2.0,"text":"Words"}]),
        json!([{"id":"s0","speaker":"A","start":0.0,"end":2.0}]),
        json!([{"id":"s0","speaker":"A","start":2.0,"end":2.0,"text":"Words"}]),
        json!([{"id":"s0","speaker":"A","start":-1.0,"end":2.0,"text":"Words"}]),
        json!([{"id":"s0","speaker":"","start":0.0,"end":2.0,"text":"Words"}]),
        json!([{"id":"s0","speaker":"A","start":0.0,"end":5.1,"text":"Words"}]),
        json!([{"id":"s0","speaker":"A","start":0.0,"end":2.0,"text":"Words"},{"id":"s0","speaker":"B","start":2.0,"end":3.0,"text":"Words"}]),
    ] {
        let (url, _) = local_server(json_response(
            200,
            &json!({"text":"Words","duration":4.0,"segments":segments}).to_string(),
        ));
        let result = tauri::async_runtime::block_on(
            OpenAiClient::with_base_url(url).transcribe_session(&path, "test-key", &source),
        );
        let result = result.expect("usable words must survive invalid speaker metadata");
        assert_eq!(result.text, "Words");
        assert!(result.segments.is_empty());
        assert!(path.exists());
    }
    for duration in [json!(null), json!(-1.0), json!("4.0"), json!({})] {
        let (url, _) = local_server(json_response(
            200,
            &json!({
                "text":"Words", "duration":duration,
                "segments":[{"id":"s0","speaker":"A","start":0.0,"end":2.0,"text":"Words"}]
            })
            .to_string(),
        ));
        let result = tauri::async_runtime::block_on(
            OpenAiClient::with_base_url(url).transcribe_session(&path, "test-key", &source),
        )
        .expect("duration metadata must not discard usable text");
        assert_eq!(result.text, "Words");
        assert!(result.segments.is_empty());
    }
    let (url, _) = local_server(json_response(
        200,
        &json!({
            "text":"", "duration":4.0,
            "segments":[{"id":"s0","speaker":"A","start":0.0,"end":2.0,"text":"Words"}]
        })
        .to_string(),
    ));
    let error = tauri::async_runtime::block_on(
        OpenAiClient::with_base_url(url).transcribe_session(&path, "test-key", &source),
    )
    .expect_err("an inconsistent empty transcript must still retain audio for retry");
    assert_eq!(error.code, "invalid_transcription");
    assert!(path.exists());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn enrichment_validates_grounded_speaker_suggestions_and_topic_anchors() {
    let mut source = session();
    source.transcription = serde_json::from_value(json!([{"source":"system","chunks":[{"startSeconds":0.0,"durationSeconds":4.0,"transcript":"I'm Maya. We agreed Monday.","segments":[{"id":"s0","speaker":"A","startSeconds":0.0,"endSeconds":3.0,"text":"I'm Maya. We agreed Monday."}]}]}])).unwrap();
    let suggestions = json!({
        "title":{"value":"Monday release","evidence":[{"sourceId":"system:offset-0:s0","excerpt":"We agreed Monday."}]},
        "context":null,"category":null,
        "participants":[{"name":"Maya","speakerKey":"system:offset-0:A","evidence":[{"sourceId":"system:offset-0:s0","excerpt":"I'm Maya."}]}],
        "topics":[{"title":"Release date","startsAtTurnId":"system:offset-0:s0","evidence":[{"sourceId":"system:offset-0:s0","excerpt":"We agreed Monday."}]}]
    });
    for mutation in [
        "valid",
        "speaker",
        "anchor",
        "duplicate_anchor",
        "missing_evidence",
        "invented_participant",
    ] {
        let mut candidate = suggestions.clone();
        match mutation {
            "speaker" => {
                candidate["participants"][0]["speakerKey"] = json!("system:offset-60000:A")
            }
            "anchor" => candidate["topics"][0]["startsAtTurnId"] = json!("missing"),
            "duplicate_anchor" => {
                let topic = candidate["topics"][0].clone();
                candidate["topics"].as_array_mut().unwrap().push(topic);
            }
            "missing_evidence" => candidate["title"]["evidence"] = json!([]),
            "invented_participant" => {
                candidate["participants"][0]["name"] = json!("Invented Person")
            }
            _ => {}
        }
        let response = json_response(200,&json!({"status":"completed","output":[{"content":[{"type":"output_text","text":json!({"summary":["Release Monday."],"key_points":[],"decisions":[],"action_items":[],"suggestions":candidate}).to_string()}]}]}).to_string());
        let (url, requests) = local_server(response);
        let result = tauri::async_runtime::block_on(
            OpenAiClient::with_base_url(url).enrich(&source, "test-key"),
        );
        if mutation == "valid" {
            assert_eq!(result.unwrap().suggestions.participants[0].name, "Maya");
            let body: Value = serde_json::from_str(&requests.recv().unwrap().body).unwrap();
            assert!(body["text"]["format"]["schema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("suggestions")));
            assert!(body["input"][0]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("system:offset-0:A"));
        } else {
            let sections = result.expect("Keep notes and omit unsupported suggestions");
            assert_eq!(sections.summary, vec!["Release Monday."]);
            assert!(sections.omitted_suggestions, "{mutation}");
            meeting_notes_lib::openai::validate_suggestions(&sections.suggestions, &source)
                .unwrap();
            if mutation == "invented_participant" || mutation == "speaker" {
                assert!(sections.suggestions.participants.is_empty());
                assert!(sections.suggestions.title.is_some());
            }
        }
    }
    assert_eq!(
        source.original_notes,
        "rent roll is the issue NOTES_TOKEN_299"
    );
}

#[test]
fn unavailable_answer_is_a_normal_response_without_invented_facts() {
    let response = json_response(200, &json!({"status":"completed","output":[{"content":[{
        "type":"output_text", "text":json!({"answer":"Untrusted unsupported claim.","supported":false,"source_ids":[]}).to_string()
    }]}]}).to_string());
    let (url, _) = local_server(response);
    let answer = tauri::async_runtime::block_on(OpenAiClient::with_base_url(url).ask_meetings(
        &[session()],
        "What was the unmentioned phone number?",
        "test-key",
    ))
    .unwrap();
    assert!(answer.citations.is_empty());
    assert_eq!(
        answer.answer,
        "The saved notes and transcript don’t contain enough information to answer that."
    );
}

#[test]
fn question_citations_keep_original_unicode_spacing_and_punctuation() {
    let mut source = session();
    source.transcript = Some("Renée:  “Ship 東京.”\n\nFriday — confirmed.".into());
    let expected = source.transcript.clone().unwrap();
    let response = json_response(200, &json!({"status":"completed","output":[{"content":[{
        "type":"output_text", "text":json!({"answer":"A Friday launch was confirmed.","supported":true,"source_ids":["m0:transcript:0"]}).to_string()
    }]}]}).to_string());
    let (url, _) = local_server(response);
    let answer = tauri::async_runtime::block_on(OpenAiClient::with_base_url(url).ask_meetings(
        &[source],
        "what was it about",
        "test-key",
    ))
    .unwrap();
    assert_eq!(answer.citations[0].excerpt, expected);
}

#[test]
#[ignore = "uses paid OpenAI requests on synthetic text and the authorized login Keychain key"]
fn live_synthetic_meeting_questions_with_openai() {
    let credential = std::process::Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "com.dweng.meetingnotes",
            "-a",
            "openai-api-key",
            "-w",
        ])
        .output()
        .unwrap();
    assert!(
        credential.status.success(),
        "Login Keychain requires manual access approval"
    );
    let key = String::from_utf8(credential.stdout).expect("Keychain key is UTF-8");
    let mut source = session();
    source.original_notes.clear();
    source.transcript = Some("[SIMULATION] Maya and Alex reviewed a warehouse acquisition. The roof report is missing. They decided to postpone underwriting until Monday. Alex will request the roof report Friday.".into());
    source.notes = Some("## Summary\nWarehouse acquisition review.\n\n## Decision\nUnderwriting postponed until Monday.\n\n## Next step\nAlex requests the roof report Friday.".into());
    let client = OpenAiClient::new();
    let started = std::time::Instant::now();
    let answer = tauri::async_runtime::block_on(client.ask_meetings(
        &[source.clone()],
        "what was it about",
        key.trim(),
    ))
    .unwrap();
    assert!(!answer.citations.is_empty());
    assert!(answer.answer.to_lowercase().contains("warehouse"));
    for citation in &answer.citations {
        assert!(
            source.notes().contains(&citation.excerpt)
                || source
                    .transcript
                    .as_ref()
                    .unwrap()
                    .contains(&citation.excerpt)
        );
    }
    let unknown = tauri::async_runtime::block_on(client.ask_meetings(
        &[source],
        "What is Maya's mobile phone number?",
        key.trim(),
    ))
    .unwrap();
    assert!(unknown.citations.is_empty());
    assert!(unknown.answer.contains("don’t contain enough information"));
    println!("Synthetic text-only acceptance: broad summary with {} saved passages; unavailable fact handled normally; two requests in {:.1}s.", answer.citations.len(), started.elapsed().as_secs_f64());
}

#[test]
fn generated_notes_are_not_recycled_as_independent_source_evidence() {
    let mut source = session();
    source.enriched_notes = Some("Unsupported generated conclusion".into());
    source.notes = source.enriched_notes.clone();
    let request = build_enrichment_request(&source);
    let input = request["input"][0]["content"][0]["text"].as_str().unwrap();
    assert!(!input.contains("Unsupported generated conclusion"));
    assert!(
        input.contains("NOTES_TOKEN_299"),
        "original typed notes remain evidence"
    );
    source.notes = Some("Human correction and emphasis".into());
    let request = build_enrichment_request(&source);
    assert!(request["input"][0]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Human correction and emphasis"));
}

#[test]
fn enrichment_and_evidence_validation_use_the_same_original_notes() {
    let mut source = session();
    source.original_notes = "Typed emphasis".into();
    source.enriched_notes = Some("AI baseline".into());
    source.notes = source.enriched_notes.clone();
    for (excerpt, accepted) in [("Typed emphasis", true), ("AI baseline", false)] {
        let sections = json!({"summary":["Saved notes"],"key_points":[],"decisions":[],"action_items":[],"suggestions":{
            "title":{"value":"Topic","evidence":[{"sourceId":"manual-notes","excerpt":excerpt}]},
            "context":null,"category":null,"participants":[],"topics":[]
        }});
        let (url, _) = local_server(json_response(200, &json!({"status":"completed","output":[{"content":[{"type":"output_text","text":sections.to_string()}]}]}).to_string()));
        let result = tauri::async_runtime::block_on(
            OpenAiClient::with_base_url(url).enrich(&source, "test-only"),
        )
        .unwrap();
        assert_eq!(result.suggestions.title.is_some(), accepted);
        assert_eq!(result.omitted_suggestions, !accepted);
    }
}
