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
