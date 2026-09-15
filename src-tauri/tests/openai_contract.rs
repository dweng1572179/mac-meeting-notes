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
    std::fs::write(&path, "disposable audio bytes").unwrap();
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
        sender.send(request).unwrap();
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
