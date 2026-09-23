use super::*;
use crate::commands::live::{
    reconcile_segments, register_segments, transcribe_available, ProcessingLease,
};
use crate::recorder::CapturedSegment;

#[test]
fn sustained_live_outage_resumes_pending_audio_without_reuploading_saved_sections() {
    use std::cell::{Cell, RefCell};
    let fixture = Fixture::new();
    let input = fixture.source(false, 1, false);
    let mut responses = vec![text_response("[SIMULATION] First checkpoint.")];
    responses
        .extend((0..6).map(|_| response(503, json!({"error":{"message":"temporary outage"}}))));
    responses.extend([
        text_response("[SIMULATION] Recovered while recording."),
        enrichment(),
    ]);
    let (url, server) = server(responses, |_| {});
    let state = fixture.state(&url);
    let paths: Vec<_> = (0..2)
        .map(|index| {
            state
                .store
                .segment_path(&fixture.id, AudioSource::System, index, AudioFormat::Wav)
                .unwrap()
        })
        .collect();
    fs::copy(&input, &paths[0]).unwrap();
    fs::rename(&input, &paths[1]).unwrap();
    let mut session = state.store.get(&fixture.id).unwrap();
    session.status = SessionStatus::Recording;
    session.segmented_capture = true;
    session.audio_path = None;
    state.store.save(&session).unwrap();
    let sections: Vec<_> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| CapturedSegment {
            source: AudioSource::System,
            index: index as u64,
            start_seconds: index as f64,
            duration_seconds: 1.0,
            path: path.clone(),
        })
        .collect();
    register_segments(&state, &fixture.id, &sections).unwrap();
    let lease = ProcessingLease::claim(&state, &fixture.id)
        .unwrap()
        .unwrap();
    let elapsed = Cell::new(Duration::ZERO);
    let attempts = RefCell::new(Vec::new());
    let recovered_live = Cell::new(false);
    let complete = run_processing_worker(
        &state,
        &fixture.id,
        || {
            assert!(ProcessingLease::claim(&state, &fixture.id)
                .unwrap()
                .is_none());
            attempts.borrow_mut().push(elapsed.get());
            tauri::async_runtime::block_on(process_session_with_key(
                &state,
                &fixture.id,
                "test-only",
                |_| {},
            ))
        },
        &|_| {},
        |delay| {
            assert!(
                delay <= Duration::from_millis(500),
                "Stop must remain observable during backoff"
            );
            elapsed.set(elapsed.get() + delay);
            let saved = state.store.get(&fixture.id).unwrap();
            assert_eq!(saved.status, SessionStatus::Recording);
            assert_eq!(saved.original_notes, session.original_notes);
            assert_eq!(
                saved.transcription[0].chunks[0].transcript.as_deref(),
                Some("[SIMULATION] First checkpoint.")
            );
            assert!(!paths[0].exists());
            if saved.transcription[0].chunks[1].transcript.is_some() {
                assert!(saved.live_transcription_error.is_none());
                assert!(!paths[1].exists());
                recovered_live.set(true);
                update_processing(&state, &fixture.id, |session| {
                    session.status = SessionStatus::Processing
                })
                .unwrap();
            } else {
                assert!(paths[1].exists(), "pending audio must remain retryable");
                assert!(
                    elapsed.get() <= Duration::from_secs(165),
                    "live processing stopped retrying"
                );
            }
        },
    )
    .unwrap();
    drop(lease);
    let requests = server.join().unwrap();
    assert!(recovered_live.get());
    assert_eq!(complete.status, SessionStatus::Complete);
    assert_eq!(complete.original_notes, session.original_notes);
    assert_eq!(complete.transcription[0].chunks.len(), 2);
    assert_eq!(
        complete.transcription[0].chunks[1].transcript.as_deref(),
        Some("[SIMULATION] Recovered while recording.")
    );
    assert_eq!(state.store.get(&fixture.id).unwrap(), complete);
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST /v1/audio/transcriptions"))
            .count(),
        8,
        "only the pending section may be uploaded again after each failure"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST /v1/responses"))
            .count(),
        1
    );
    assert_eq!(
        *attempts.borrow(),
        [0, 2000, 6000, 14000, 44000, 104000, 164000, 164500].map(Duration::from_millis)
    );
}

#[test]
fn older_section_registered_during_upload_does_not_move_newer_checkpoint() {
    let fixture = Fixture::new();
    let input = fixture.source(false, 1, false);
    let info = crate::audio::inspect(&input).unwrap();
    let duration = info.frames as f64 / info.sample_rate;
    let store = SessionStore::new(fixture.root.clone());
    let older_path = store
        .segment_path(&fixture.id, AudioSource::System, 0, AudioFormat::Wav)
        .unwrap();
    let newer_path = store
        .segment_path(&fixture.id, AudioSource::System, 1, AudioFormat::Wav)
        .unwrap();
    fs::copy(&input, &older_path).unwrap();
    fs::rename(input, &newer_path).unwrap();
    let callback_root = fixture.root.clone();
    let callback_id = fixture.id.clone();
    let older = CapturedSegment {
        source: AudioSource::System,
        index: 0,
        start_seconds: 0.0,
        duration_seconds: duration,
        path: older_path.clone(),
    };
    let (url, server) = server(
        vec![
            text_response("[SIMULATION] Newer section checkpoint."),
            response(
                503,
                json!({"error":{"message":"temporary failure","type":"server_error"}}),
            ),
        ],
        move |index| {
            if index == 0 {
                // The response is held until this registration sorts an older chunk ahead of
                // the in-flight upload. Saving by the old vector index corrupts the checkpoint.
                let state = AppState::new(SessionStore::new(callback_root.clone()));
                register_segments(&state, &callback_id, std::slice::from_ref(&older)).unwrap();
            }
        },
    );
    let state = fixture.state(&url);
    let mut session = store.get(&fixture.id).unwrap();
    session.status = SessionStatus::Recording;
    session.segmented_capture = true;
    session.audio_path = None;
    store.save(&session).unwrap();
    register_segments(
        &state,
        &fixture.id,
        &[CapturedSegment {
            source: AudioSource::System,
            index: 1,
            start_seconds: duration,
            duration_seconds: duration,
            path: newer_path.clone(),
        }],
    )
    .unwrap();
    let progress = std::sync::Mutex::new(Vec::new());
    let result = tauri::async_runtime::block_on(transcribe_available(
        &state,
        &fixture.id,
        "test-only",
        &|saved| progress.lock().unwrap().push(saved),
    ));
    assert!(
        result.is_err(),
        "the older section's injected API failure must propagate"
    );
    assert_eq!(server.join().unwrap().len(), 2);
    let saved = store.get(&fixture.id).unwrap();
    assert_eq!(saved.status, SessionStatus::Recording);
    assert_eq!(saved.original_notes, session.original_notes);
    assert!(saved.enriched_notes.is_none());
    let chunks = &saved.transcription[0].chunks;
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].segment_index, Some(0));
    assert_eq!(chunks[0].transcript, None);
    assert_eq!(chunks[1].segment_index, Some(1));
    assert_eq!(
        chunks[1].transcript.as_deref(),
        Some("[SIMULATION] Newer section checkpoint.")
    );
    assert!(
        older_path.exists(),
        "failed section audio must remain retryable"
    );
    assert!(
        !newer_path.exists(),
        "only the correctly checkpointed section can be deleted"
    );
    assert_eq!(progress.lock().unwrap().len(), 1);
}

#[test]
fn stopped_capture_recovers_unregistered_final_tail_once_and_keeps_audio() {
    let fixture = Fixture::new();
    let input = fixture.source(false, 1, false);
    let info = crate::audio::inspect(&input).unwrap();
    let duration = info.frames as f64 / info.sample_rate;
    let state = fixture.state("http://127.0.0.1:1/v1");
    let path = state
        .store
        .segment_path(&fixture.id, AudioSource::System, 1, AudioFormat::Wav)
        .unwrap();
    fs::rename(input, &path).unwrap();
    let mut session = state.store.get(&fixture.id).unwrap();
    session.segmented_capture = true;
    session.audio_path = None;
    session.capture_segments = vec![crate::domain::CaptureSegment {
        source: AudioSource::System,
        index: 0,
        start_seconds: 0.0,
        duration_seconds: 60.0,
    }];
    session.transcription = vec![SourceTranscript {
        source: AudioSource::System,
        chunks: vec![TranscriptChunk {
            error: None,
            start_seconds: 0.0,
            duration_seconds: 60.0,
            transcript: Some("[SIMULATION] Existing checkpoint.".into()),
            segment_index: Some(0),
            segments: vec![],
        }],
    }];
    state.store.save(&session).unwrap();
    let recovered = reconcile_segments(&state, &fixture.id).unwrap();
    assert_eq!(recovered.audio_format, AudioFormat::Wav);
    assert_eq!(recovered.capture_segments.len(), 2);
    assert_eq!(recovered.capture_segments[1].start_seconds, 60.0);
    assert!((recovered.capture_segments[1].duration_seconds - duration).abs() < 0.001);
    assert_eq!(
        recovered.transcription[0].chunks[0].transcript,
        session.transcription[0].chunks[0].transcript
    );
    assert_eq!(recovered.transcription[0].chunks[1].transcript, None);
    assert_eq!(state.store.get(&fixture.id).unwrap(), recovered);
    assert_eq!(reconcile_segments(&state, &fixture.id).unwrap(), recovered);
    assert!(
        path.exists(),
        "untranscribed final tail must remain on disk"
    );
}

#[test]
fn processing_lease_is_exclusive_per_meeting_and_released_on_drop() {
    let fixture = Fixture::new();
    let state = fixture.state("http://127.0.0.1:1/v1");
    let lease = ProcessingLease::claim(&state, &fixture.id)
        .unwrap()
        .unwrap();
    assert!(ProcessingLease::claim(&state, &fixture.id)
        .unwrap()
        .is_none());
    let other = ProcessingLease::claim(&state, "different-meeting")
        .unwrap()
        .unwrap();
    drop(other);
    assert!(ProcessingLease::claim(&state, &fixture.id)
        .unwrap()
        .is_none());
    drop(lease);
    let next = ProcessingLease::claim(&state, &fixture.id)
        .unwrap()
        .unwrap();
    drop(next);
    assert!(state.processing_jobs.lock().unwrap().is_empty());
}

#[test]
fn failed_start_with_numbered_audio_is_durable_and_retryable() {
    let fixture = Fixture::new();
    let input = fixture.source(false, 1, false);
    let state = fixture.state("http://127.0.0.1:1/v1");
    let tail = state
        .store
        .segment_path(&fixture.id, AudioSource::System, 0, AudioFormat::Wav)
        .unwrap();
    fs::rename(&input, &tail).unwrap();
    let microphone = fixture
        .root
        .join("audio")
        .join(format!("{}-mic.wav", fixture.id));
    let mut session = state.store.get(&fixture.id).unwrap();
    session.status = SessionStatus::Draft;
    session.segmented_capture = true;
    session.audio_path = None;
    state.store.save(&session).unwrap();
    let error = start_recording_state(&state, &fixture.id, input, microphone, |_, _, _| {
        Err(AppError::new("capture_start", "Injected startup failure"))
    })
    .unwrap_err();
    assert_eq!(error.code, "capture_start");
    let failed = state.store.get(&fixture.id).unwrap();
    assert_eq!(failed.status, SessionStatus::Failed);
    assert_eq!(failed.error.unwrap().code, "capture_start");
    assert!(failed.ended_at.is_some());
    assert!(tail.exists());
    let recovered = reconcile_segments(&state, &fixture.id).unwrap();
    assert_eq!(recovered.transcription[0].chunks[0].segment_index, Some(0));
    assert_eq!(recovered.transcription[0].chunks[0].transcript, None);
}

#[test]
fn corrupt_final_tail_does_not_prevent_checkpointing_healthy_section() {
    let fixture = Fixture::new();
    let input = fixture.source(false, 1, false);
    let (url, server) = server(
        vec![text_response("[SIMULATION] Healthy section retained.")],
        |_| {},
    );
    let state = fixture.state(&url);
    let good = state
        .store
        .segment_path(&fixture.id, AudioSource::System, 0, AudioFormat::Wav)
        .unwrap();
    let corrupt = state
        .store
        .segment_path(&fixture.id, AudioSource::System, 1, AudioFormat::Wav)
        .unwrap();
    fs::rename(input, &good).unwrap();
    fs::write(&corrupt, b"unfinished final container").unwrap();
    let mut session = state.store.get(&fixture.id).unwrap();
    session.segmented_capture = true;
    session.audio_path = None;
    state.store.save(&session).unwrap();
    let result = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-only",
        |_| {},
    ));
    assert_eq!(result.unwrap_err().code, "audio_chunk");
    assert_eq!(server.join().unwrap().len(), 1);
    let saved = state.store.get(&fixture.id).unwrap();
    assert_eq!(
        saved.transcription[0].chunks[0].transcript.as_deref(),
        Some("[SIMULATION] Healthy section retained.")
    );
    assert!(saved.enriched_notes.is_none());
    assert!(corrupt.exists());
    assert!(!good.exists());
    assert_eq!(saved.original_notes, session.original_notes);
}

#[test]
fn corrupt_registered_section_does_not_block_healthy_other_source() {
    let fixture = Fixture::new();
    let input = fixture.source(true, 1, false);
    let info = crate::audio::inspect(&input).unwrap();
    let duration = info.frames as f64 / info.sample_rate;
    let (url, server) = server(
        vec![text_response("[SIMULATION] Healthy microphone checkpoint.")],
        |_| {},
    );
    let state = fixture.state(&url);
    let corrupt = state
        .store
        .segment_path(&fixture.id, AudioSource::System, 0, AudioFormat::Wav)
        .unwrap();
    let good = state
        .store
        .segment_path(&fixture.id, AudioSource::Microphone, 0, AudioFormat::Wav)
        .unwrap();
    fs::write(&corrupt, b"registered but unreadable audio").unwrap();
    fs::rename(input, &good).unwrap();
    let mut session = state.store.get(&fixture.id).unwrap();
    session.segmented_capture = true;
    session.microphone_audio_path = None;
    state.store.save(&session).unwrap();
    register_segments(
        &state,
        &fixture.id,
        &[
            CapturedSegment {
                source: AudioSource::System,
                index: 0,
                start_seconds: 0.0,
                duration_seconds: duration,
                path: corrupt.clone(),
            },
            CapturedSegment {
                source: AudioSource::Microphone,
                index: 0,
                start_seconds: 0.1,
                duration_seconds: duration,
                path: good.clone(),
            },
        ],
    )
    .unwrap();
    let result = tauri::async_runtime::block_on(transcribe_available(
        &state,
        &fixture.id,
        "test-only",
        &|_| {},
    ));
    assert!(result.is_err());
    let saved = state.store.get(&fixture.id).unwrap();
    let healthy = saved
        .transcription
        .iter()
        .find(|track| track.source == AudioSource::Microphone)
        .unwrap();
    assert_eq!(
        healthy.chunks[0].transcript.as_deref(),
        Some("[SIMULATION] Healthy microphone checkpoint.")
    );
    assert_eq!(server.join().unwrap().len(), 1);
    assert!(corrupt.exists());
    assert!(!good.exists());
    assert_eq!(saved.original_notes, session.original_notes);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "Uses paid OpenAI diarization/enrichment and authorized login Keychain access"]
fn live_synthetic_segmented_diarization_with_openai() {
    let fixture = Fixture::new();
    let script = fixture.root.join("speaker-script.txt");
    let speech = format!("This is a simulated Cedar project meeting. My name is Maya. We initially proposed a Friday release. {} Final decision: release on Monday. Maya will complete verification before the Monday release. This final decision replaces the initial Friday proposal.", "We are reviewing the meeting notes application. Reliable audio capture and saved transcripts are the priorities. This synthetic test contains no customer information. The team will verify that the final decision remains visible after the meeting ends. ".repeat(8));
    fs::write(&script, speech).unwrap();
    let aiff = fixture.root.join("speaker.aiff");
    assert!(std::process::Command::new("/usr/bin/say")
        .args(["-v", "Samantha", "-r", "190", "-f"])
        .arg(&script)
        .arg("-o")
        .arg(&aiff)
        .output()
        .unwrap()
        .status
        .success());
    let audio = fixture.root.join("synthetic-full.m4a");
    assert!(std::process::Command::new("/usr/bin/afconvert")
        .args(["-f", "m4af", "-d", "aac", "-b", "32000"])
        .arg(&aiff)
        .arg(&audio)
        .output()
        .unwrap()
        .status
        .success());
    let info = crate::audio::inspect(&audio).unwrap();
    let duration = info.frames as f64 / info.sample_rate;
    assert!(
        duration > 60.0,
        "Synthetic speech must cross a section boundary"
    );
    let mut state = fixture.state("https://api.openai.com/v1");
    state.openai = OpenAiClient::new();
    let mut session = state.store.get(&fixture.id).unwrap();
    session.audio_format = AudioFormat::M4a;
    session.status = SessionStatus::Recording;
    session.segmented_capture = true;
    session.transcription_settings = crate::domain::TranscriptionSettings::new_recording_default();
    session.transcription_settings.language = "en".into();
    session.original_notes =
        "[SIMULATION] Prioritize the final release decision and verification owner.".into();
    state.store.save(&session).unwrap();
    let mut sections = Vec::new();
    let mut start = 0.0;
    while start < duration - 0.000_001 {
        let index = sections.len() as u64;
        let length = (duration - start).min(60.0);
        let path = state
            .store
            .segment_path(&fixture.id, AudioSource::System, index, AudioFormat::M4a)
            .unwrap();
        crate::audio::write_chunk(&audio, &path, start, length).unwrap();
        sections.push(CapturedSegment {
            source: AudioSource::System,
            index,
            start_seconds: start,
            duration_seconds: length,
            path,
        });
        start += length;
    }
    // Use the same authorized system client as the existing paid acceptance test;
    // the disposable test binary should not request a new persistent Keychain grant.
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
    let key = key.trim();
    register_segments(&state, &fixture.id, &sections[..1]).unwrap();
    let live =
        tauri::async_runtime::block_on(process_session_with_key(&state, &fixture.id, key, |_| {}))
            .unwrap();
    assert_eq!(live.status, SessionStatus::Recording);
    assert!(live.enriched_notes.is_none());
    assert!(!live.transcription[0].chunks[0].segments.is_empty());
    assert!(!sections[0].path.exists());
    register_segments(&state, &fixture.id, &sections[1..]).unwrap();
    update_processing(&state, &fixture.id, |session| {
        session.status = SessionStatus::Processing;
    })
    .unwrap();
    let complete =
        tauri::async_runtime::block_on(process_session_with_key(&state, &fixture.id, key, |_| {}))
            .unwrap();
    assert_eq!(complete.status, SessionStatus::Complete);
    assert!(complete
        .transcript
        .as_ref()
        .unwrap()
        .to_lowercase()
        .contains("monday"));
    assert!(complete
        .enriched_notes
        .as_ref()
        .unwrap()
        .to_lowercase()
        .contains("monday"));
    assert!(complete.ai_suggestions.as_ref().unwrap().title.is_some());
    assert!(complete.transcription[0]
        .chunks
        .iter()
        .all(|chunk| chunk.transcript.is_some() && !chunk.segments.is_empty()));
    assert_eq!(complete.original_notes, session.original_notes);
    assert_eq!(state.store.get(&fixture.id).unwrap(), complete);
    assert!(sections.iter().all(|section| !section.path.exists()));
    println!("Live segmented acceptance passed: {:.1} seconds, {} sections, {} speaker turns; live checkpoint, final decision, suggestions, durable notes, and section cleanup verified.", duration, sections.len(), crate::domain::transcript_turns(&complete).len());
}

#[test]
fn held_processing_lease_blocks_retry_and_destructive_actions_on_failed_session() {
    let fixture = Fixture::new();
    let state = fixture.state("http://127.0.0.1:1/v1");
    let mut session = state.store.get(&fixture.id).unwrap();
    session.status = SessionStatus::Failed;
    session.transcript = Some("[SIMULATION] Retained transcript".into());
    state.store.save(&session).unwrap();
    let _lease = ProcessingLease::claim(&state, &fixture.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        prepare_retry(&state, &fixture.id).unwrap_err().code,
        "invalid_session_status"
    );
    assert_eq!(
        delete_transcript_state(&state, &fixture.id)
            .unwrap_err()
            .code,
        "invalid_session_status"
    );
    assert_eq!(
        delete_session_state(&state, &fixture.id).unwrap_err().code,
        "invalid_session_status"
    );
    assert_eq!(state.store.get(&fixture.id).unwrap(), session);
}

#[test]
fn segmented_silent_microphone_adds_warning_without_losing_system_notes() {
    let fixture = Fixture::new();
    let system = fixture.source(false, 1, false);
    let microphone = fixture.source(true, 1, true);
    let (url, server) = server(
        vec![
            text_response("[SIMULATION] System speech retained."),
            text_response(""),
            enrichment(),
        ],
        |_| {},
    );
    let state = fixture.state(&url);
    let mut sections = Vec::new();
    for (source, input) in [
        (AudioSource::System, system),
        (AudioSource::Microphone, microphone),
    ] {
        let info = crate::audio::inspect(&input).unwrap();
        let path = state
            .store
            .segment_path(&fixture.id, source, 0, AudioFormat::Wav)
            .unwrap();
        fs::rename(input, &path).unwrap();
        sections.push(CapturedSegment {
            source,
            index: 0,
            start_seconds: 0.0,
            duration_seconds: info.frames as f64 / info.sample_rate,
            path,
        });
    }
    let mut session = state.store.get(&fixture.id).unwrap();
    session.segmented_capture = true;
    session.audio_path = None;
    session.microphone_audio_path = None;
    state.store.save(&session).unwrap();
    register_segments(&state, &fixture.id, &sections).unwrap();
    let complete = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        "test-only",
        |_| {},
    ))
    .unwrap();
    assert_eq!(server.join().unwrap().len(), 3);
    assert_eq!(complete.status, SessionStatus::Complete);
    assert!(complete
        .transcript
        .as_deref()
        .unwrap()
        .contains("System speech retained"));
    assert!(complete.enriched_notes.is_some());
    assert_eq!(complete.original_notes, session.original_notes);
    assert!(
        complete
            .warnings
            .iter()
            .any(|warning| warning.to_lowercase().contains("microphone")
                && warning.contains("no transcribed speech")),
        "Missing microphone coverage warning: {:?}",
        complete.warnings
    );
    assert_eq!(state.store.get(&fixture.id).unwrap(), complete);
}

#[test]
fn rejected_section_does_not_block_later_checkpoints_and_retry_only_uploads_missing_audio() {
    let fixture = Fixture::new();
    let input = fixture.source(false, 1, false);
    let info = crate::audio::inspect(&input).unwrap();
    let duration = info.frames as f64 / info.sample_rate;
    let (url, server) = server(
        vec![
            text_response("[SIMULATION] First section."),
            response(
                400,
                json!({"error":{"code":"invalid_value","param":"file"}}),
            ),
            text_response("[SIMULATION] Last section."),
            text_response("[SIMULATION] Recovered middle section."),
        ],
        |_| {},
    );
    let state = fixture.state(&url);
    let mut session = state.store.get(&fixture.id).unwrap();
    session.segmented_capture = true;
    session.audio_path = None;
    state.store.save(&session).unwrap();
    let segments: Vec<_> = (0..3)
        .map(|index| {
            let path = state
                .store
                .segment_path(&fixture.id, AudioSource::System, index, AudioFormat::Wav)
                .unwrap();
            fs::copy(&input, &path).unwrap();
            CapturedSegment {
                source: AudioSource::System,
                index,
                start_seconds: duration * index as f64,
                duration_seconds: duration,
                path,
            }
        })
        .collect();
    register_segments(&state, &fixture.id, &segments).unwrap();
    let error = tauri::async_runtime::block_on(transcribe_available(
        &state,
        &fixture.id,
        "test-only",
        &|_| {},
    ))
    .unwrap_err();
    assert_eq!(error.code, "invalid_audio");
    let saved = state.store.get(&fixture.id).unwrap();
    let chunks = &saved.transcription[0].chunks;
    assert!(chunks[0].transcript.is_some());
    assert!(chunks[1].transcript.is_none());
    assert!(chunks[2].transcript.is_some());
    assert!(saved.enriched_notes.is_none());
    assert_ne!(saved.status, SessionStatus::Complete);
    assert!(!segments[0].path.exists());
    assert!(segments[1].path.exists());
    assert!(!segments[2].path.exists());
    let mut failed = saved;
    failed.status = SessionStatus::Failed;
    state.store.save(&retry_start(failed).unwrap()).unwrap();
    tauri::async_runtime::block_on(transcribe_available(
        &state,
        &fixture.id,
        "test-only",
        &|_| {},
    ))
    .unwrap();
    assert_eq!(server.join().unwrap().len(), 4);
    assert!(!segments[1].path.exists());
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "Uses paid OpenAI transcription/enrichment on synthetic Spanish/English speech"]
fn live_bilingual_conversation_quality_with_openai() {
    let fixture = Fixture::new();
    let state = fixture.state("https://api.openai.com/v1");
    let mut session = state.store.get(&fixture.id).unwrap();
    session.audio_format = AudioFormat::M4a;
    session.title = "[SIMULATION] Language practice".into();
    session.context = "Spanish and English language practice about food. Preserve concrete examples and unanswered questions; no project is being planned. Write the notes in English.".into();
    session.original_notes.clear();
    session.segmented_capture = true;
    session.transcription_settings = crate::domain::TranscriptionSettings::new_recording_default();
    state.store.save(&session).unwrap();
    let mut start = 0.0;
    let mut sections = Vec::new();
    for (index, voice, speech) in [
        (0, "Paulina", "Hola, me llamo Lucía. Esta es una conversación simulada para practicar idiomas. Los domingos preparo pozole con mi abuela. No uso pollo; uso cerdo. La receta lleva tres tipos de chile. No sé cuánto cuesta prepararla. ¿Tienes una receta familiar? No hemos decidido organizar una cena ni comprar nada."),
        (1, "Samantha", "My name is Alex. This is simulated language practice. My grandfather taught me to make dumplings. We fold thirty dumplings together every Saturday. I do not know whether the restaurant is open on Monday. That question is unanswered. We have reached the end of our practice conversation. There are no assigned tasks or future plans."),
    ] {
        let script = fixture.root.join(format!("voice-{index}.txt"));
        let aiff = fixture.root.join(format!("voice-{index}.aiff"));
        fs::write(&script, speech).unwrap();
        assert!(std::process::Command::new("/usr/bin/say").args(["-v", voice, "-r", "160", "-f"]).arg(&script).arg("-o").arg(&aiff).output().unwrap().status.success());
        let path = state.store.segment_path(&fixture.id, AudioSource::System, index, AudioFormat::M4a).unwrap();
        assert!(std::process::Command::new("/usr/bin/afconvert").args(["-f", "m4af", "-d", "aac", "-b", "32000"]).arg(&aiff).arg(&path).output().unwrap().status.success());
        let info = crate::audio::inspect(&path).unwrap();
        let duration = info.frames as f64 / info.sample_rate;
        sections.push(CapturedSegment { source: AudioSource::System, index, start_seconds: start, duration_seconds: duration, path });
        start += duration;
    }
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
    let key = String::from_utf8(credential.stdout).unwrap();
    register_segments(&state, &fixture.id, &sections).unwrap();
    let result = tauri::async_runtime::block_on(process_session_with_key(
        &state,
        &fixture.id,
        key.trim(),
        |_| {},
    ))
    .unwrap();
    let transcript = result.transcript.as_ref().unwrap().to_lowercase();
    let notes = result.notes().to_lowercase();
    // Only synthetic output is printed; never use an existing local meeting for this test.
    println!("SYNTHETIC BILINGUAL TRANSCRIPT: {transcript}\nSYNTHETIC BILINGUAL NOTES: {notes}");
    for word in ["pozole", "cerdo", "dumplings", "monday"] {
        assert!(transcript.contains(word), "missing spoken detail: {word}");
    }
    for word in ["pozole", "pork", "dumplings", "saturday", "monday"] {
        assert!(notes.contains(word), "missing note detail: {word}");
    }
    assert!(notes.contains("30") || notes.contains("thirty"));
    assert!(notes.contains("three") || notes.contains('3'));
    assert!(
        !notes.contains("## decisions"),
        "social wrap-up is not a decision"
    );
    assert!(
        !notes.contains("## action items"),
        "no commitments were made"
    );
    assert_eq!(result.status, SessionStatus::Complete);
    assert!(sections.iter().all(|section| !section.path.exists()));
    assert_eq!(
        state.store.get(&fixture.id).unwrap().notes(),
        result.notes()
    );
    println!(
        "BILINGUAL_ACCEPTANCE seconds={start:.1} sections={} model={} status=complete",
        sections.len(),
        result.transcription_settings.model
    );
}

#[test]
fn worker_keeps_transcribing_new_sections_after_a_section_specific_rejection() {
    let fixture = Fixture::new();
    let input = fixture.source(false, 1, false);
    let info = crate::audio::inspect(&input).unwrap();
    let duration = info.frames as f64 / info.sample_rate;
    let (url, server) = server(
        vec![
            response(
                400,
                json!({"error":{"code":"invalid_value","param":"file"}}),
            ),
            text_response("[SIMULATION] Arrived after the rejection."),
        ],
        |_| {},
    );
    let state = fixture.state(&url);
    let mut session = state.store.get(&fixture.id).unwrap();
    session.status = SessionStatus::Recording;
    session.segmented_capture = true;
    session.audio_path = None;
    state.store.save(&session).unwrap();
    let segments: Vec<_> = (0..2)
        .map(|index| {
            let path = state
                .store
                .segment_path(&fixture.id, AudioSource::System, index, AudioFormat::Wav)
                .unwrap();
            fs::copy(&input, &path).unwrap();
            CapturedSegment {
                source: AudioSource::System,
                index,
                start_seconds: duration * index as f64,
                duration_seconds: duration,
                path,
            }
        })
        .collect();
    register_segments(&state, &fixture.id, &segments[..1]).unwrap();
    let mut polls = 0;
    let error = run_processing_worker(
        &state,
        &fixture.id,
        || {
            tauri::async_runtime::block_on(process_session_with_key(
                &state,
                &fixture.id,
                "test-only",
                |_| {},
            ))
        },
        &|_| {},
        |_| {
            polls += 1;
            match polls {
                1 => {
                    register_segments(&state, &fixture.id, &segments[1..]).unwrap();
                }
                2 => {
                    update_processing(&state, &fixture.id, |session| {
                        session.status = SessionStatus::Processing
                    })
                    .unwrap();
                }
                _ => panic!("a section rejection must not pause the uploader"),
            }
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "invalid_audio");
    assert_eq!(
        server.join().unwrap().len(),
        2,
        "the rejected section is not automatically rebilled"
    );
    let saved = state.store.get(&fixture.id).unwrap();
    assert_eq!(
        saved.transcription[0].chunks[0]
            .error
            .as_ref()
            .unwrap()
            .code,
        "invalid_audio"
    );
    assert!(saved.transcription[0].chunks[0].transcript.is_none());
    assert!(saved.transcription[0].chunks[1].transcript.is_some());
    assert!(saved.enriched_notes.is_none());
    assert!(segments[0].path.exists());
    assert!(!segments[1].path.exists());
}
