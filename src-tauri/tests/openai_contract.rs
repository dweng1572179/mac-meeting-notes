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
    assert_eq!(request["model"], "gpt-6-astra");
    assert_eq!(request["store"], false);
}

#[test]
fn markdown_omits_empty_sections_and_includes_decisions() {
    let markdown = sections_to_markdown(EnrichedSections {
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
    assert_eq!(payload["model"], "gpt-6-astra");
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
fn folder_question_posts_sources_and_rejects_invented_citations() {
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
                        "citations": [
                            {
                                "session_id": "first-meeting",
                                "excerpt": "Exact source sentence about the rent roll."
                            },
                            {
                                "session_id": "made-up-meeting",
                                "excerpt": "Invented evidence"
                            }
                        ]
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

    assert_eq!(payload["model"], "gpt-6-astra");
    assert_eq!(payload["store"], false);
    assert!(input.contains("first-meeting"));
    assert!(input.contains("SECOND_SOURCE_TOKEN_916"));
    assert_eq!(answer.answer, "The rent roll still needs review.");
    assert_eq!(answer.citations.len(), 1);
    assert_eq!(answer.citations[0].session_id, "first-meeting");
    assert_eq!(answer.citations[0].title, "Harbor review");
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
                        "citations": [{
                            "session_id": "made-up-meeting",
                            "excerpt": "Invented evidence"
                        }]
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
                        "citations": []
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
