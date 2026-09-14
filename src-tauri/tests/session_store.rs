use meeting_notes_lib::domain::{CreateSessionInput, Session, UpdateSessionInput};
use meeting_notes_lib::store::SessionStore;

#[cfg(unix)]
fn symlinked_session_destination(destination: &str) {
    use std::{fs, os::unix::fs::symlink};

    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("sessions")).unwrap();
    fs::create_dir_all(root.join("outside")).unwrap();
    let mut session = Session::new(CreateSessionInput {
        title: "Safe notes".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    session.id = "session".into();
    let victim = root.join("outside/victim");
    fs::write(&victim, "untouched").unwrap();
    if destination == "sessions" {
        fs::remove_dir(root.join("sessions")).unwrap();
        symlink(root.join("outside"), root.join("sessions")).unwrap();
    } else {
        symlink(&victim, root.join("sessions").join(destination)).unwrap();
    }
    let result = SessionStore::new(root.clone()).save(&session);
    let untouched = fs::read_to_string(&victim).unwrap();
    let escaped = root.join("outside/session.json").exists();
    fs::remove_dir_all(root).unwrap();
    assert!(result.is_err(), "accepted {destination}");
    assert_eq!(untouched, "untouched", "overwrote {destination}");
    assert!(!escaped, "wrote through sessions symlink");
}

#[cfg(unix)]
#[test]
fn save_rejects_symlinked_sessions_directory() {
    symlinked_session_destination("sessions");
}

#[cfg(unix)]
#[test]
fn save_rejects_symlinked_canonical_session() {
    symlinked_session_destination("session.json");
}

#[cfg(unix)]
#[test]
fn save_rejects_symlinked_temporary_session() {
    symlinked_session_destination("session.json.tmp");
}

#[cfg(unix)]
#[test]
fn list_reports_inaccessible_sessions_instead_of_an_empty_library() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("sessions")).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();
    let result = SessionStore::new(root.clone()).list();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir_all(root).unwrap();
    assert_eq!(result.unwrap_err().code, "storage_error");
}

#[test]
fn cleanup_keeps_tombstone_and_audio_when_the_canonical_session_exists() {
    use std::fs;
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("audio")).unwrap();
    let store = SessionStore::new(root.clone());
    let mut session = Session::new(CreateSessionInput {
        title: "Kept meeting".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    let audio = root.join("audio").join(format!("{}.m4a", session.id));
    fs::write(&audio, "retained audio").unwrap();
    session.audio_path = Some(audio.to_string_lossy().into_owned());
    store.save(&session).unwrap();
    let canonical = root.join("sessions").join(format!("{}.json", session.id));
    let tombstone = canonical.with_extension("json.deleting");
    fs::copy(&canonical, &tombstone).unwrap();
    let listed = store.list().unwrap();
    let kept_audio = audio.exists();
    let kept_tombstone = tombstone.exists();
    fs::remove_dir_all(root).unwrap();
    assert_eq!(listed, vec![session]);
    assert!(kept_audio, "deleted canonical session audio");
    assert!(kept_tombstone, "removed uncommitted tombstone");
}

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
    let error = store.save(&session).unwrap_err();

    assert_eq!(error.code, "invalid_audio_path");
    assert!(outside_audio.exists());
    assert!(!root
        .join("sessions")
        .join(format!("{}.json", session.id))
        .exists());
    std::fs::remove_file(outside_audio).unwrap();
}

#[test]
fn delete_removes_session_and_its_contained_audio() {
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let audio_dir = root.join("audio");
    std::fs::create_dir_all(&audio_dir).unwrap();
    let store = SessionStore::new(root.clone());
    let mut session = Session::new(CreateSessionInput {
        title: "Meeting".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    let audio_path = audio_dir.join(format!("{}.m4a", session.id));
    std::fs::write(&audio_path, "meeting audio").unwrap();
    session.audio_path = Some(audio_path.to_string_lossy().into_owned());
    store.save(&session).unwrap();
    let session_path = root.join("sessions").join(format!("{}.json", session.id));

    store.delete(&session.id).unwrap();

    assert!(!session_path.exists());
    assert!(!audio_path.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn delete_rejects_traversal_before_constructing_a_path() {
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let store = SessionStore::new(root.clone());

    let error = store.delete("../outside").unwrap_err();

    assert_eq!(error.code, "invalid_session_id");
    assert!(!root.exists());
}

#[test]
fn save_rejects_audio_that_belongs_to_another_session() {
    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let audio_dir = root.join("audio");
    std::fs::create_dir_all(&audio_dir).unwrap();
    let wrong_audio = audio_dir.join("another-session.m4a");
    std::fs::write(&wrong_audio, "audio").unwrap();
    let store = SessionStore::new(root.clone());
    let mut session = Session::new(CreateSessionInput {
        title: "Meeting".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    session.audio_path = Some(wrong_audio.to_string_lossy().into_owned());

    let error = store.save(&session).unwrap_err();

    assert_eq!(error.code, "invalid_audio_path");
    assert!(!root
        .join("sessions")
        .join(format!("{}.json", session.id))
        .exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn delete_restores_audio_when_session_removal_fails() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let audio_dir = root.join("audio");
    std::fs::create_dir_all(&audio_dir).unwrap();
    let store = SessionStore::new(root.clone());
    let mut session = Session::new(CreateSessionInput {
        title: "Meeting".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    let audio_path = audio_dir.join(format!("{}.m4a", session.id));
    std::fs::write(&audio_path, "retryable audio").unwrap();
    session.audio_path = Some(audio_path.to_string_lossy().into_owned());
    store.save(&session).unwrap();
    let sessions_dir = root.join("sessions");
    std::fs::set_permissions(&sessions_dir, std::fs::Permissions::from_mode(0o500)).unwrap();

    let error = store.delete(&session.id).unwrap_err();

    assert_eq!(error.code, "storage_error");
    assert!(audio_path.exists());
    assert!(store.get(&session.id).is_ok());
    std::fs::set_permissions(&sessions_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn delete_commits_and_defers_cleanup_when_audio_removal_fails() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let audio_dir = root.join("audio");
    std::fs::create_dir_all(&audio_dir).unwrap();
    let store = SessionStore::new(root.clone());
    let mut session = Session::new(CreateSessionInput {
        title: "Meeting".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    let audio_path = audio_dir.join(format!("{}.m4a", session.id));
    std::fs::write(&audio_path, "retryable audio").unwrap();
    session.audio_path = Some(audio_path.to_string_lossy().into_owned());
    store.save(&session).unwrap();
    let session_path = root.join("sessions").join(format!("{}.json", session.id));
    std::fs::set_permissions(&audio_dir, std::fs::Permissions::from_mode(0o500)).unwrap();

    let result = store.delete(&session.id);
    let deletion_committed = !session_path.exists();
    let audio_retained = audio_path.exists();
    std::fs::set_permissions(&audio_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let sessions_after_cleanup = store.list().unwrap();
    let deferred_audio_removed = !audio_path.exists();
    std::fs::remove_dir_all(root).unwrap();

    assert!(result.is_ok());
    assert!(deletion_committed);
    assert!(audio_retained);
    assert!(sessions_after_cleanup.is_empty());
    assert!(deferred_audio_removed);
}

#[cfg(unix)]
#[test]
fn next_store_access_finishes_a_committed_tombstone_cleanup() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let audio_dir = root.join("audio");
    std::fs::create_dir_all(&audio_dir).unwrap();
    let store = SessionStore::new(root.clone());
    let mut session = Session::new(CreateSessionInput {
        title: "Meeting".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    let audio_path = audio_dir.join(format!("{}.m4a", session.id));
    std::fs::write(&audio_path, "retryable audio").unwrap();
    session.audio_path = Some(audio_path.to_string_lossy().into_owned());
    store.save(&session).unwrap();
    let sessions_dir = root.join("sessions");
    let session_path = sessions_dir.join(format!("{}.json", session.id));
    let tombstone_path = sessions_dir.join(format!("{}.json.deleting", session.id));
    std::fs::rename(&session_path, &tombstone_path).unwrap();
    std::fs::set_permissions(&sessions_dir, std::fs::Permissions::from_mode(0o500)).unwrap();

    let sessions_during_deferred_cleanup = store.list().unwrap();
    let audio_removed = !audio_path.exists();
    let tombstone_retained = tombstone_path.exists();
    std::fs::set_permissions(&sessions_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let sessions_after_cleanup = store.list().unwrap();
    let tombstone_removed = !tombstone_path.exists();
    std::fs::remove_dir_all(root).unwrap();

    assert!(sessions_during_deferred_cleanup.is_empty());
    assert!(audio_removed);
    assert!(tombstone_retained);
    assert!(sessions_after_cleanup.is_empty());
    assert!(tombstone_removed);
}

#[cfg(unix)]
#[test]
fn delete_rejects_an_audio_directory_symlinked_outside_app_data() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
    let outside =
        std::env::temp_dir().join(format!("meeting-notes-audio-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("sessions")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, root.join("audio")).unwrap();
    let mut session = Session::new(CreateSessionInput {
        title: "Meeting".into(),
        context: String::new(),
        attendees: Vec::new(),
    });
    let escaped_audio = outside.join(format!("{}.m4a", session.id));
    std::fs::write(&escaped_audio, "outside audio").unwrap();
    session.audio_path = Some(
        root.join("audio")
            .join(format!("{}.m4a", session.id))
            .to_string_lossy()
            .into_owned(),
    );
    let session_path = root.join("sessions").join(format!("{}.json", session.id));
    std::fs::write(&session_path, serde_json::to_vec_pretty(&session).unwrap()).unwrap();
    let store = SessionStore::new(root.clone());

    let error = store.delete(&session.id).unwrap_err();

    assert_eq!(error.code, "invalid_audio_path");
    assert!(session_path.exists());
    assert!(escaped_audio.exists());
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(outside).unwrap();
}
