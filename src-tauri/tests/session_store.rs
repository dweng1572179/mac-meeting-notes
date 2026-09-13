use meeting_notes_lib::domain::{CreateSessionInput, Session};
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
