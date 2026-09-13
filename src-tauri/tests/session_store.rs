use meeting_notes_lib::domain::{CreateSessionInput, Session, UpdateSessionInput};
use meeting_notes_lib::store::SessionStore;

#[test]
fn original_notes_survive_reopen() {
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let store = SessionStore::new(root.clone());
    let mut session = Session::new(CreateSessionInput {
        title: "[SIMULATION] Harbor Office".into(),
        context: "All facts are simulation state.".into(),
        attendees: vec!["[SIMULATION] Alex".into()],
    });
    session.original_notes = "rent roll is the issue".into();
    store.save(&session).unwrap();

    let reopened = SessionStore::new(root.clone()).get(&session.id).unwrap();
    assert_eq!(reopened.original_notes, "rent roll is the issue");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn update_with_different_id_leaves_session_unchanged() {
    let mut session = Session::new(CreateSessionInput {
        title: "Original title".into(),
        context: "Original context".into(),
        attendees: vec!["Original attendee".into()],
    });
    let before = session.clone();

    let result = session.apply(UpdateSessionInput {
        id: "different-session".into(),
        title: "Changed title".into(),
        context: "Changed context".into(),
        attendees: vec!["Changed attendee".into()],
        original_notes: "Changed notes".into(),
    });

    assert!(result.is_err());
    assert_eq!(session, before);
}

#[test]
fn list_sorts_timestamps_chronologically_despite_fractional_precision() {
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let store = SessionStore::new(root.clone());
    let mut older = Session::new(CreateSessionInput {
        title: "Older".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    older.id = "older".into();
    older.started_at = "2026-01-01T10:00:00Z".into();
    let mut newer = Session::new(CreateSessionInput {
        title: "Newer".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    newer.id = "newer".into();
    newer.started_at = "2026-01-01T10:00:00.1Z".into();
    store.save(&older).unwrap();
    store.save(&newer).unwrap();

    let sessions = store.list().unwrap();
    assert_eq!(sessions[0].id, "newer");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn delete_does_not_remove_audio_outside_the_store() {
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let outside_audio = root.with_extension("m4a");
    std::fs::write(&outside_audio, "outside audio").unwrap();
    let store = SessionStore::new(root.clone());
    let mut session = Session::new(CreateSessionInput {
        title: "Meeting".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    session.audio_path = Some(outside_audio.to_string_lossy().into_owned());
    store.save(&session).unwrap();

    store.delete(&session.id).unwrap();

    assert!(outside_audio.exists());
    std::fs::remove_file(outside_audio).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
