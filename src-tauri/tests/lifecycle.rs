use meeting_notes_lib::commands::{
    interrupted_recording, needs_transcription, recover_interrupted, retry_start,
    transcript_deleted,
};
use meeting_notes_lib::domain::{
    transition_to_failed, transition_to_processing, AppError, CreateSessionInput, Session,
    SessionStatus,
};

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
    let stopped = transition_to_processing(session, "/tmp/example.m4a").unwrap();
    assert_eq!(stopped.status, SessionStatus::Processing);
    assert!(stopped.ended_at.is_some());
    assert_eq!(stopped.audio_path.as_deref(), Some("/tmp/example.m4a"));
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
    let interrupted = interrupted_recording(recording_session(), "/tmp/flushed.m4a").unwrap();

    assert_eq!(interrupted.status, SessionStatus::Failed);
    assert_eq!(interrupted.audio_path.as_deref(), Some("/tmp/flushed.m4a"));
    assert_eq!(interrupted.error.unwrap().code, "interrupted");
    assert!(interrupted.ended_at.is_some());
}
