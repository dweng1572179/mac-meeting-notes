use meeting_notes_lib::commands::{
    combine_transcripts, interrupted_recording, needs_transcription, recover_interrupted,
    recover_interrupted_sessions, retry_start, stop_recording_in_store_on_exit, transcript_deleted,
};
use meeting_notes_lib::domain::{
    transition_to_failed, transition_to_processing, AppError, CreateSessionInput, Session,
    SessionStatus,
};
use meeting_notes_lib::recorder::RecordingFiles;
use meeting_notes_lib::store::SessionStore;

fn session(status: SessionStatus) -> Session {
    let mut session = Session::new(CreateSessionInput {
        title: "Weekly review".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    session.original_notes = "rent roll".into();
    session.status = status;
    session
}

fn recording_session() -> Session {
    session(SessionStatus::Recording)
}

fn processing_session() -> Session {
    let mut session = session(SessionStatus::Processing);
    session.audio_path = Some("/tmp/example.m4a".into());
    session
}

fn failed_with_transcript() -> Session {
    let mut session = session(SessionStatus::Failed);
    session.transcript = Some("spoken transcript".into());
    session.enriched_notes = Some("enhanced notes".into());
    session.error = Some(AppError::new("openai", "request failed"));
    session
}

#[test]
fn stop_persists_before_processing() {
    let session = recording_session();
    let stopped = transition_to_processing(
        session,
        "/tmp/example.m4a",
        Some("/tmp/example-mic.m4a".into()),
    )
    .unwrap();
    assert_eq!(stopped.status, SessionStatus::Processing);
    assert!(stopped.ended_at.is_some());
    assert_eq!(stopped.audio_path.as_deref(), Some("/tmp/example.m4a"));
    assert_eq!(
        stopped.microphone_audio_path.as_deref(),
        Some("/tmp/example-mic.m4a")
    );
}

#[test]
fn failed_processing_keeps_original_notes_and_audio() {
    let failed = transition_to_failed(
        processing_session(),
        AppError::new("openai", "request failed"),
    );
    assert_eq!(failed.original_notes, "rent roll");
    assert!(failed.audio_path.is_some());
}

#[test]
fn stale_recording_is_recovered_as_interrupted() {
    let recovered = recover_interrupted(recording_session());

    assert_eq!(recovered.status, SessionStatus::Failed);
    assert_eq!(recovered.error.unwrap().code, "interrupted");
}

#[test]
fn retry_with_transcript_skips_transcription() {
    let retry = retry_start(failed_with_transcript()).unwrap();

    assert_eq!(retry.status, SessionStatus::Processing);
    assert_eq!(retry.transcript.as_deref(), Some("spoken transcript"));
    assert!(!needs_transcription(&retry));
}

#[test]
fn blank_transcript_is_retried() {
    let mut retry = failed_with_transcript();
    retry.transcript = Some(" \n ".into());

    assert!(needs_transcription(&retry));
}

#[test]
fn system_and_microphone_transcripts_keep_source_labels() {
    let transcript = combine_transcripts("Remote words", "My words").unwrap();

    assert_eq!(transcript, "Meeting audio:\nRemote words\n\nYou:\nMy words");
}

#[test]
fn silent_recording_is_not_a_successful_transcript() {
    let error = combine_transcripts(" \n", "\t").unwrap_err();

    assert_eq!(error.code, "no_speech");
    assert!(error.message.contains("audio was kept"));
}

#[test]
fn transcript_deletion_returns_to_draft_without_changing_original_notes() {
    let reset = transcript_deleted(failed_with_transcript()).unwrap();

    assert_eq!(reset.status, SessionStatus::Draft);
    assert_eq!(reset.original_notes, "rent roll");
    assert!(reset.transcript.is_none());
    assert!(reset.enriched_notes.is_none());
}

#[test]
fn normal_exit_marks_recording_failed_with_returned_audio() {
    let interrupted = interrupted_recording(
        recording_session(),
        RecordingFiles {
            segments: Vec::new(),
            segmented: false,
            health: None,
            system: "/tmp/flushed.m4a".into(),
            microphone: "/tmp/flushed-mic.m4a".into(),
        },
    )
    .unwrap();

    assert_eq!(interrupted.status, SessionStatus::Failed);
    assert_eq!(interrupted.audio_path.as_deref(), Some("/tmp/flushed.m4a"));
    assert_eq!(
        interrupted.microphone_audio_path.as_deref(),
        Some("/tmp/flushed-mic.m4a")
    );
    assert_eq!(interrupted.error.unwrap().code, "interrupted");
    assert!(interrupted.ended_at.is_some());
}

#[test]
fn stale_processing_before_transcription_keeps_retryable_audio() {
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let store = SessionStore::new(root.clone());
    let mut processing = processing_session();
    let audio_dir = root.join("audio");
    std::fs::create_dir_all(&audio_dir).unwrap();
    let audio_path = audio_dir.join(format!("{}.m4a", processing.id));
    std::fs::write(&audio_path, "audio").unwrap();
    processing.audio_path = Some(audio_path.to_string_lossy().into_owned());
    let id = processing.id.clone();
    store.save(&processing).unwrap();

    recover_interrupted_sessions(&store).unwrap();

    let recovered = store.get(&id).unwrap();
    assert_eq!(recovered.status, SessionStatus::Failed);
    assert_eq!(recovered.error.unwrap().code, "interrupted");
    assert_eq!(
        recovered.audio_path.as_deref(),
        processing.audio_path.as_deref()
    );
    assert!(recovered.transcript.is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_processing_before_enrichment_keeps_existing_transcript() {
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let store = SessionStore::new(root.clone());
    let mut processing = processing_session();
    processing.transcript = Some("spoken transcript".into());
    processing.audio_path = None;
    let id = processing.id.clone();
    store.save(&processing).unwrap();

    recover_interrupted_sessions(&store).unwrap();

    let recovered = store.get(&id).unwrap();
    assert_eq!(recovered.status, SessionStatus::Failed);
    assert_eq!(recovered.error.unwrap().code, "interrupted");
    assert_eq!(recovered.transcript.as_deref(), Some("spoken transcript"));
    assert!(recovered.audio_path.is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn exit_cleanup_is_idempotent_after_flushing_the_active_recorder() {
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let store = SessionStore::new(root.clone());
    let mut recording = recording_session();
    let audio_dir = root.join("audio");
    std::fs::create_dir_all(&audio_dir).unwrap();
    let audio_path = audio_dir.join(format!("{}.m4a", recording.id));
    let microphone_audio_path = audio_dir.join(format!("{}-mic.m4a", recording.id));
    std::fs::write(&audio_path, "audio").unwrap();
    std::fs::write(&microphone_audio_path, "microphone audio").unwrap();
    recording.audio_path = Some(audio_path.to_string_lossy().into_owned());
    recording.microphone_audio_path = Some(microphone_audio_path.to_string_lossy().into_owned());
    let id = recording.id.clone();
    store.save(&recording).unwrap();
    let mut stops = 0;

    stop_recording_in_store_on_exit(&store, |_| {
        stops += 1;
        Ok(RecordingFiles {
            segments: Vec::new(),
            segmented: false,
            health: None,
            system: audio_path.clone(),
            microphone: microphone_audio_path.clone(),
        })
    })
    .unwrap();
    stop_recording_in_store_on_exit(&store, |_| panic!("recorder stopped twice")).unwrap();

    assert_eq!(stops, 1);
    let saved = store.get(&id).unwrap();
    assert_eq!(saved.status, SessionStatus::Failed);
    assert_eq!(saved.audio_path.as_deref(), recording.audio_path.as_deref());
    assert_eq!(
        saved.microphone_audio_path.as_deref(),
        recording.microphone_audio_path.as_deref()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovered_recording_keeps_an_honest_coverage_warning_after_retry() {
    let recovered = recover_interrupted(recording_session());
    let retry = retry_start(recovered).unwrap();
    assert!(retry
        .warnings
        .iter()
        .any(|warning| warning.contains("Recording was interrupted")
            && warning.contains("incomplete")));
}

#[test]
fn legacy_meetings_and_chunks_default_new_workspace_fields_without_changing_model() {
    let mut saved = serde_json::to_value(session(SessionStatus::Complete)).unwrap();
    for field in [
        "transcriptionSettings",
        "captureSegments",
        "segmentedCapture",
        "liveTranscriptionError",
        "aiSuggestions",
        "editedEnrichedNotes",
        "dismissedSuggestions",
    ] {
        saved.as_object_mut().unwrap().remove(field);
    }
    let legacy: Session = serde_json::from_value(saved).unwrap();
    assert_eq!(
        legacy.transcription_settings.model,
        "gpt-4o-mini-transcribe"
    );
    let json = serde_json::to_value(legacy).unwrap();
    assert_eq!(json["captureSegments"], serde_json::json!([]));
    assert_eq!(json["segmentedCapture"], false);
    assert_eq!(json["dismissedSuggestions"], serde_json::json!([]));
    let chunk: meeting_notes_lib::domain::TranscriptChunk = serde_json::from_value(
        serde_json::json!({"startSeconds":0.0,"durationSeconds":10.0,"transcript":"Words"}),
    )
    .unwrap();
    let json = serde_json::to_value(chunk).unwrap();
    assert_eq!(json["segments"], serde_json::json!([]));
    assert_eq!(json["segmentIndex"], serde_json::Value::Null);
}

#[test]
fn speaker_turn_keys_separate_uploads_sources_and_adaptive_splits() {
    use meeting_notes_lib::domain::{meeting_sources, transcript_turns, TranscriptionSettings};
    assert_eq!(
        TranscriptionSettings::new_recording_default().model,
        "gpt-4o-transcribe-diarize"
    );
    let mut meeting = session(SessionStatus::Complete);
    meeting.transcription = serde_json::from_value(serde_json::json!([
        {"source":"system","chunks":[
            {"segmentIndex":1,"startSeconds":60.0,"durationSeconds":30.0,"transcript":"First words", "segments":[{"id":"0","speaker":"A","startSeconds":0.1,"endSeconds":2.0,"text":"First words"}]},
            {"segmentIndex":1,"startSeconds":90.0,"durationSeconds":30.0,"transcript":"Later words EXTRA", "segments":[{"id":"0","speaker":"A","startSeconds":0.1,"endSeconds":2.0,"text":"Later words"}]}
        ]},
        {"source":"microphone","chunks":[{"segmentIndex":1,"startSeconds":60.0,"durationSeconds":60.0,"transcript":"Microphone words", "segments":[{"id":"0","speaker":"A","startSeconds":0.1,"endSeconds":2.0,"text":"Microphone words"}]}]}
    ])).unwrap();
    let turns = transcript_turns(&meeting);
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[0].id, "microphone:segment-1:offset-60000:0");
    assert_eq!(turns[1].id, "system:segment-1:offset-60000:0");
    assert_eq!(turns[2].id, "system:segment-1:offset-90000:text");
    assert_ne!(turns[1].speaker_key, turns[2].speaker_key);
    assert_eq!(turns[2].start_seconds, 90.0);
    assert_eq!(turns[2].speaker_key, None);
    assert_eq!(turns[2].text, "Later words EXTRA");
    assert!(!meeting_sources(&meeting)
        .iter()
        .any(|source| source.text == "Later words"));
    assert!(meeting_sources(&meeting)
        .iter()
        .any(|source| source.id == "system:segment-1:offset-90000:text"
            && source.text.ends_with("EXTRA")));
}

#[test]
fn deleting_transcript_keeps_the_visible_notes_document() {
    let mut source = failed_with_transcript();
    source.enriched_notes = Some("Saved summary".into());
    for edit in [None, Some("My revision".into()), Some(String::new())] {
        source.edited_enriched_notes = edit;
        let reset = transcript_deleted(source.clone()).unwrap();
        assert_eq!(reset.notes(), source.notes());
        assert_eq!(reset.original_notes, source.original_notes);
        assert!(reset.transcript.is_none());
    }
}
