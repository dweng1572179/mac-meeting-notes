use meeting_notes_lib::commands::{
    interrupted_recording, needs_transcription, recover_interrupted, recover_interrupted_sessions,
    retry_start, stop_recording_in_store_on_exit, transcript_deleted,
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
