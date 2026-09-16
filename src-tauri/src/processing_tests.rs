use super::*;
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

struct Fixture {
    root: PathBuf,
    id: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(
                self.root.join("sessions"),
                fs::Permissions::from_mode(0o700),
            );
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("meeting-notes-processing-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("audio")).unwrap();
        let store = SessionStore::new(root.clone());
        let mut session = Session::new(CreateSessionInput {
            title: "[SIMULATION] chunk retry".into(),
            context: String::new(),
            attendees: vec![],
        });
        session.status = SessionStatus::Processing;
        session.original_notes = "[SIMULATION] Keep my emphasis and typed notes.".into();
        store.save(&session).unwrap();
        Self {
            root,
            id: session.id,
        }
    }
    fn state(&self, url: &str) -> AppState {
        let mut state = AppState::new(SessionStore::new(self.root.clone()));
        state.openai = OpenAiClient::with_base_url(url);
        state
    }
    fn source(&self, microphone: bool, seconds: u32, silent: bool) -> PathBuf {
        let path = self.root.join("audio").join(format!(
            "{}{}.m4a",
            self.id,
            if microphone { "-mic" } else { "" }
        ));
        if seconds == 0 {
            use objc2_core_audio_types::*;
            let pcm = AudioStreamBasicDescription {
                mSampleRate: 16_000.0,
                mFormatID: kAudioFormatLinearPCM,
                mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
                mBytesPerPacket: 4,
                mFramesPerPacket: 1,
                mBytesPerFrame: 4,
                mChannelsPerFrame: 1,
                mBitsPerChannel: 32,
                mReserved: 0,
            };
            let file = crate::recorder::native::create_audio_file(&path, &pcm).unwrap();
            crate::recorder::native::set_client_format(file, &pcm).unwrap();
            assert_eq!(unsafe { objc2_audio_toolbox::ExtAudioFileDispose(file) }, 0);
        } else {
            let wav = path.with_extension("wav");
            let frames = seconds * 16_000;
            let mut file = fs::File::create(&wav).unwrap();
            file.write_all(b"RIFF").unwrap();
            file.write_all(&(36 + frames * 2).to_le_bytes()).unwrap();
            file.write_all(b"WAVEfmt \x10\0\0\0\x01\0\x01\0").unwrap();
            file.write_all(&16_000u32.to_le_bytes()).unwrap();
            file.write_all(&32_000u32.to_le_bytes()).unwrap();
            file.write_all(b"\x02\0\x10\0data").unwrap();
            file.write_all(&(frames * 2).to_le_bytes()).unwrap();
            let block: Vec<u8> = (0..16_000)
                .flat_map(|i| {
                    let sample = if silent {
                        0
                    } else {
                        ((i as f64 * std::f64::consts::TAU * 440.0 / 16_000.0).sin() * 8000.0)
                            as i16
                    };
                    sample.to_le_bytes()
                })
                .collect();
            for _ in 0..seconds {
                file.write_all(&block).unwrap();
            }
            drop(file);
            let result = std::process::Command::new("/usr/bin/afconvert")
                .args(["-f", "m4af", "-d", "aac", "-b", "32000"])
                .arg(&wav)
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "afconvert: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            fs::remove_file(wav).unwrap();
        }
        let store = SessionStore::new(self.root.clone());
        let mut session = store.get(&self.id).unwrap();
        if microphone {
            session.microphone_audio_path = Some(path.to_string_lossy().into());
        } else {
            session.audio_path = Some(path.to_string_lossy().into());
        }
        store.save(&session).unwrap();
        path
    }
}

fn response(status: u16, value: Value) -> String {
    let body = value.to_string();
    format!("HTTP/1.1 {status} Test\r\nx-request-id: req_middle_123\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())
}
fn text_response(text: &str) -> String {
    response(200, json!({"text":text}))
}
fn enrichment() -> String {
    response(
        200,
        json!({"status":"completed","output":[{"content":[{"type":"output_text","text":"{\"summary\":[\"[SIMULATION] Complete.\"],\"key_points\":[],\"decisions\":[],\"action_items\":[]}"}]}]}),
    )
}
fn server(
    responses: Vec<String>,
    before_response: impl Fn(usize) + Send + 'static,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let mut requests = vec![];
        for (index, response) in responses.into_iter().enumerate() {
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(10))
                    }
                    _ => return requests,
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 8192];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let size = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= end + 4 + size {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8_lossy(&request).into_owned());
            before_response(index);
            stream.write_all(response.as_bytes()).unwrap();
        }
        requests
    });
    (url, task)
}

#[test]
fn middle_chunk_failure_reopens_and_resumes_without_rebilling_saved_chunks() {
    let fixture = Fixture::new();
    let audio = fixture.source(false, 601, false);
    let (url, requests) = server(
        vec![
            text_response("FIRST_SAVED"),
            response(
                400,
                json!({"error":{"code":"invalid_value","param":"file","message":"Audio file might be corrupted or unsupported"}}),
            ),
        ],
        |_| {},
    );
    let state = fixture.state(&url);
    let error = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-key",
        |_| {},
    ))
    .unwrap_err();
    persist_failure(&state, &fixture.id, error.clone()).unwrap();
    assert_eq!(requests.join().unwrap().len(), 2);
    assert_eq!(
        error.message,
        "OpenAI could not read this recording. Your audio was kept. Request ID: req_middle_123"
    );
    assert!(
        audio.exists(),
        "untranscribed audio must survive a middle-chunk failure"
    );
    let saved = state.store.get(&fixture.id).unwrap();
    assert!(saved.transcript.as_ref().unwrap().contains("FIRST_SAVED"));
    assert!(saved.original_notes.contains("Keep my emphasis"));
    drop(state);
    let (url, requests) = server(
        vec![
            text_response("SECOND_RESUMED"),
            text_response("THIRD_RESUMED"),
            enrichment(),
        ],
        |_| {},
    );
    let state = fixture.state(&url);
    recover_interrupted_sessions(&state.store).unwrap();
    prepare_retry(&state, &fixture.id).unwrap();
    let complete = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-key",
        |_| {},
    ))
    .unwrap();
    let requests = requests.join().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST /v1/audio/transcriptions"))
            .count(),
        2
    );
    assert_eq!(complete.status, SessionStatus::Complete);
    assert_eq!(
        complete
            .transcript
            .as_ref()
            .unwrap()
            .matches("FIRST_SAVED")
            .count(),
        1
    );
    assert!(complete.transcript.as_ref().unwrap().contains("00:05:00"));
    assert!(complete
        .transcript
        .as_ref()
        .unwrap()
        .contains("THIRD_RESUMED"));
    assert!(complete.original_notes.contains("Keep my emphasis"));
    assert!(!audio.exists());
    assert!(!complete
        .warnings
        .iter()
        .any(|warning| warning.contains("Remaining audio was kept")));
    assert_eq!(fs::read_dir(fixture.root.join("audio")).unwrap().count(), 0);
    assert_eq!(
        SessionStore::new(fixture.root.clone())
            .get(&fixture.id)
            .unwrap(),
        complete
    );
}

#[test]
fn empty_system_and_valid_microphone_complete_with_source_warning() {
    let fixture = Fixture::new();
    fixture.source(false, 0, true);
    fixture.source(true, 1, false);
    let (url, requests) = server(
        vec![text_response("MICROPHONE_WORDS"), enrichment()],
        |_| {},
    );
    let state = fixture.state(&url);
    let complete = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-key",
        |_| {},
    ))
    .unwrap();
    assert_eq!(requests.join().unwrap().len(), 2);
    assert_eq!(complete.status, SessionStatus::Complete);
    let json = serde_json::to_value(&complete).unwrap();
    assert!(json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("System audio")));
    assert!(complete.transcript.unwrap().contains("Microphone"));
}

#[test]
fn valid_system_and_silent_microphone_complete_without_inventing_speech() {
    let fixture = Fixture::new();
    fixture.source(false, 1, false);
    fixture.source(true, 1, true);
    let (url, requests) = server(
        vec![
            text_response("SYSTEM_WORDS"),
            text_response(""),
            enrichment(),
        ],
        |_| {},
    );
    let state = fixture.state(&url);
    let complete = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-key",
        |_| {},
    ))
    .unwrap();
    assert_eq!(requests.join().unwrap().len(), 3);
    assert_eq!(complete.status, SessionStatus::Complete);
    let json = serde_json::to_value(&complete).unwrap();
    assert!(json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("Microphone")));
    assert!(complete.transcript.unwrap().contains("SYSTEM_WORDS"));
}

#[test]
fn transcript_checkpoint_storage_failure_keeps_notes_and_source_audio() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let audio = fixture.source(false, 301, false);
    let directory = fixture.root.join("sessions");
    let (url, requests) = server(vec![text_response("NOT_DURABLY_SAVED")], move |_| {
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
    });
    let state = fixture.state(&url);
    let error = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-key",
        |_| {},
    ))
    .unwrap_err();
    assert_eq!(requests.join().unwrap().len(), 1);
    assert_eq!(error.code, "storage_error");
    assert!(audio.exists());
    assert!(state
        .store
        .get(&fixture.id)
        .unwrap()
        .original_notes
        .contains("Keep my emphasis"));
}

#[test]
fn unreadable_source_does_not_discard_healthy_source_progress() {
    let fixture = Fixture::new();
    let corrupt = fixture.source(false, 1, false);
    fs::write(&corrupt, b"corrupt source").unwrap();
    fixture.source(true, 1, false);
    let (url, requests) = server(vec![text_response("HEALTHY_MICROPHONE")], |_| {});
    let state = fixture.state(&url);
    assert!(tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-key",
        |_| {}
    ))
    .is_err());
    let requests = requests.join().unwrap();
    assert_eq!(requests.len(), 1);
    let saved = state.store.get(&fixture.id).unwrap();
    assert!(saved.transcript.unwrap().contains("HEALTHY_MICROPHONE"));
    assert!(saved.original_notes.contains("Keep my emphasis"));
    assert!(corrupt.exists());
}

#[test]
fn token_limit_splits_only_the_affected_chunk_without_saving_cutoff_text() {
    let fixture = Fixture::new();
    fixture.source(false, 301, false);
    let (url, requests) = server(
        vec![
            response(
                200,
                json!({"text":"TRUNCATED_BAD","usage":{"output_tokens":2000}}),
            ),
            text_response("FIRST_HALF"),
            text_response("SECOND_HALF"),
            text_response("LAST_SECOND"),
            enrichment(),
        ],
        |_| {},
    );
    let state = fixture.state(&url);
    let complete = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-key",
        |_| {},
    ))
    .unwrap();
    assert_eq!(requests.join().unwrap().len(), 5);
    let transcript = complete.transcript.unwrap();
    assert!(!transcript.contains("TRUNCATED_BAD"));
    assert!(transcript.contains("FIRST_HALF"));
    assert!(transcript.contains("SECOND_HALF"));
    assert!(transcript.contains("00:02:30"));
}

#[test]
fn reopen_after_final_checkpoint_cleans_scratch_and_reports_silent_source() {
    use crate::domain::{AudioSource, SourceTranscript, TranscriptChunk};
    let fixture = Fixture::new();
    fixture.source(false, 1, false);
    fixture.source(true, 1, true);
    let (url, requests) = server(vec![enrichment()], |_| {});
    let state = fixture.state(&url);
    let mut saved = state.store.get(&fixture.id).unwrap();
    saved.transcript = Some("SAVED_SYSTEM_WORDS".into());
    saved.transcription = [AudioSource::System, AudioSource::Microphone]
        .into_iter()
        .map(|source| SourceTranscript {
            source,
            chunks: vec![TranscriptChunk {
                start_seconds: 0.0,
                duration_seconds: 1.0,
                transcript: Some(if source == AudioSource::System {
                    "SAVED_SYSTEM_WORDS".into()
                } else {
                    String::new()
                }),
            }],
        })
        .collect();
    state.store.save(&saved).unwrap();
    for source in [AudioSource::System, AudioSource::Microphone] {
        fs::write(
            state.store.chunk_path(&fixture.id, source).unwrap(),
            b"saved chunk scratch",
        )
        .unwrap();
    }
    recover_interrupted_sessions(&state.store).unwrap();
    prepare_retry(&state, &fixture.id).unwrap();
    let complete = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-key",
        |_| {},
    ))
    .unwrap();
    assert_eq!(
        requests.join().unwrap().len(),
        1,
        "saved chunks must not be uploaded again"
    );
    assert_eq!(
        fs::read_dir(fixture.root.join("audio")).unwrap().count(),
        0,
        "durably transcribed scratch must not remain after reopen"
    );
    assert!(complete
        .warnings
        .iter()
        .any(|warning| warning.contains("Microphone had no transcribed speech")));
}

#[test]
fn deleting_transcript_retains_audio_without_allowing_recording_to_overwrite_it() {
    let fixture = Fixture::new();
    let audio = fixture.source(false, 1, false);
    let (url, requests) = server(
        vec![text_response("Recovered speech"), enrichment()],
        |_| {},
    );
    let state = fixture.state(&url);
    let mut saved = state.store.get(&fixture.id).unwrap();
    saved.status = SessionStatus::Failed;
    saved.transcript = Some("Delete this transcript".into());
    saved
        .warnings
        .push("Recording was interrupted; capture is incomplete.".into());
    saved.capture_health = Some(serde_json::from_value(json!({
        "wallSeconds": 600.0, "identityChanged": false, "warnings": saved.warnings,
        "system": {"admittedFrames":16000,"writtenFrames":16000,"capturedSeconds":1.0,"sampleRate":16000.0,"lastCallbackAgeSeconds":599.0,"writeError":null,"status":"short_capture"},
        "microphone": {"admittedFrames":0,"writtenFrames":0,"capturedSeconds":0.0,"sampleRate":16000.0,"lastCallbackAgeSeconds":null,"writeError":null,"status":"no_frames"}
    })).unwrap());
    state.store.save(&saved).unwrap();
    let deleted = delete_transcript_state(&state, &fixture.id).unwrap();
    assert_eq!(deleted.status, SessionStatus::Failed);
    assert!(deleted.transcription.is_empty());
    assert!(deleted.transcript.is_none());
    assert!(audio.exists());
    assert!(deleted.original_notes.contains("Keep my emphasis"));
    assert_eq!(deleted.warnings, saved.warnings);
    assert_eq!(deleted.capture_health, saved.capture_health);
    prepare_retry(&state, &fixture.id).unwrap();
    let complete = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-key",
        |_| {},
    ))
    .unwrap();
    let requests = requests.join().unwrap();
    assert!(requests
        .last()
        .unwrap()
        .contains("Recording was interrupted; capture is incomplete."));
    assert_eq!(complete.status, SessionStatus::Complete);
    assert_eq!(complete.capture_health, saved.capture_health);
    assert!(complete.warnings.contains(&saved.warnings[0]));
}

#[test]
#[ignore = "Uses paid OpenAI API and requires authorized login Keychain access"]
fn live_synthetic_long_recording_with_openai() {
    let fixture = Fixture::new();
    let script = fixture.root.join("synthetic-script.txt");
    let mut speech = "This is a simulation meeting about project Cedar. The initial proposal is to release on Friday. ".to_owned();
    for checkpoint in 1..=28 {
        speech.push_str(&format!("Checkpoint {checkpoint}. This is a synthetic reliability test. We are reviewing the meeting notes application. The team is checking audio capture, transcript storage, and retry behavior. The original notes emphasize reliability. No real customer information is present in this simulated meeting. "));
    }
    speech.push_str("Final decision: postpone the release until Monday. Maya owns the final verification. This final decision replaces the initial Friday proposal. The simulation meeting is now finished.");
    fs::write(&script, speech).unwrap();
    let aiff = fixture.root.join("synthetic.aiff");
    let generated = std::process::Command::new("/usr/bin/say")
        .args(["-r", "210", "-f"])
        .arg(&script)
        .arg("-o")
        .arg(&aiff)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "Synthetic speech generation failed"
    );
    let audio = fixture
        .root
        .join("audio")
        .join(format!("{}.m4a", fixture.id));
    let encoded = std::process::Command::new("/usr/bin/afconvert")
        .args(["-f", "m4af", "-d", "aac", "-b", "32000"])
        .arg(&aiff)
        .arg(&audio)
        .output()
        .unwrap();
    assert!(encoded.status.success(), "Synthetic AAC encoding failed");
    fs::remove_file(aiff).unwrap();
    let info = crate::audio::inspect(&audio).unwrap();
    let duration = info.frames as f64 / info.sample_rate;
    assert!(
        duration > 300.0,
        "Synthetic audio must span multiple chunks"
    );
    let mut state = fixture.state("https://api.openai.com/v1");
    state.openai = OpenAiClient::new();
    let mut session = state.store.get(&fixture.id).unwrap();
    session.audio_path = Some(audio.to_string_lossy().into());
    session.original_notes = "[SIMULATION] Prioritize the final decision, the release day and the person who owns verification.".into();
    state.store.save(&session).unwrap();
    // Credential bytes stay in process memory; never print or persist command output.
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
    let complete = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        key.trim(),
        |_| {},
    ))
    .unwrap();
    assert_eq!(complete.status, SessionStatus::Complete);
    let transcript = complete.transcript.as_ref().unwrap();
    assert!(
        transcript.to_ascii_lowercase().contains("monday"),
        "Final decision missing from the transcript"
    );
    assert!(
        complete
            .enriched_notes
            .as_ref()
            .unwrap()
            .to_ascii_lowercase()
            .contains("monday"),
        "Final decision missing from enhanced notes"
    );
    assert!(complete.transcription[0].chunks.len() >= 2);
    assert!(complete.transcription[0]
        .chunks
        .iter()
        .all(|chunk| chunk.transcript.is_some()));
    assert_eq!(complete.original_notes, session.original_notes);
    assert!(!audio.exists());
    assert_eq!(fs::read_dir(fixture.root.join("audio")).unwrap().count(), 0);
    assert_eq!(
        SessionStore::new(fixture.root.clone())
            .get(&fixture.id)
            .unwrap(),
        complete
    );
    println!("Live synthetic acceptance passed: {:.1} seconds, {} chunks, {} transcript words; completed notes reopened, original notes preserved, audio cleaned.",duration,complete.transcription[0].chunks.len(),transcript.split_whitespace().count());
}
