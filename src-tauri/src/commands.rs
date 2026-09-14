use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use chrono::{SecondsFormat, Utc};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    domain::{
        transition_to_failed, transition_to_processing, AppError, AppResult, CreateSessionInput,
        Session, SessionStatus, UpdateSessionInput,
    },
    openai::{sections_to_markdown, OpenAiClient},
    recorder::{Recorder, RecordingInfo},
    secrets::ApiKeyStore,
    store::SessionStore,
};

const SESSION_UPDATED: &str = "session-updated";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub sessions: Vec<Session>,
    pub has_api_key: bool,
}

pub struct AppState {
    pub store: SessionStore,
    pub recorder: Recorder,
    pub keys: ApiKeyStore,
    pub openai: OpenAiClient,
    // ponytail: one global transaction lock fits the single-user v1; use per-session locks only if contention becomes measurable.
    transactions: Mutex<()>,
}

impl AppState {
    fn new(store: SessionStore) -> Self {
        Self {
            store,
            recorder: Recorder::new(),
            keys: ApiKeyStore,
            openai: OpenAiClient::new(),
            transactions: Mutex::new(()),
        }
    }
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> AppResult<Bootstrap> {
    Ok(Bootstrap {
        sessions: state.store.list()?,
        has_api_key: ApiKeyStore::exists()?,
    })
}

#[tauri::command]
pub fn create_session(state: State<'_, AppState>, input: CreateSessionInput) -> AppResult<Session> {
    let session = Session::new(input);
    let _guard = lock_sessions(&state)?;
    state.store.save(&session)?;
    Ok(session)
}

#[tauri::command]
pub fn save_session(state: State<'_, AppState>, input: UpdateSessionInput) -> AppResult<Session> {
    save_session_state(&state, input)
}

#[tauri::command]
pub fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<RecordingInfo> {
    let session = load_session(&state, &id)?;
    if session.status != SessionStatus::Draft {
        return Err(invalid_status("Only a draft session can start recording"));
    }

    let audio_path = audio_path(&app, &session.id)?;
    if let Some(parent) = audio_path.parent() {
        fs::create_dir_all(parent).map_err(storage_error)?;
    }
    let recording = state.recorder.start(&session.id, &audio_path)?;
    if let Err(error) = persist_recording(&state, &id, audio_path.to_string_lossy().into_owned()) {
        let _ = state.recorder.stop(&session.id);
        let _ = fs::remove_file(audio_path);
        return Err(error);
    }
    Ok(recording)
}

#[tauri::command]
pub fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Session> {
    let session = load_session(&state, &id)?;
    if session.status != SessionStatus::Recording {
        return Err(invalid_status("Only a recording session can be stopped"));
    }
    let path = state.recorder.stop(&id)?;
    let session = persist_stopped(&state, &id, path.to_string_lossy().into_owned())?;
    spawn_processing(app, id);
    Ok(session)
}

#[tauri::command]
pub fn retry_processing(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Session> {
    let session = prepare_retry(&state, &id)?;
    spawn_processing(app, id);
    Ok(session)
}

#[tauri::command]
pub fn delete_session(state: State<'_, AppState>, id: String) -> AppResult<()> {
    delete_session_state(&state, &id)
}

#[tauri::command]
pub fn delete_transcript(state: State<'_, AppState>, id: String) -> AppResult<Session> {
    delete_transcript_state(&state, &id)
}

#[tauri::command]
pub async fn save_api_key(state: State<'_, AppState>, key: String) -> AppResult<()> {
    let key = key.trim();
    if key.is_empty() {
        return Err(AppError::new("invalid_api_key", "API key is required"));
    }
    state.openai.validate_key(key).await?;
    ApiKeyStore::save(key)
}

#[tauri::command]
pub fn has_api_key() -> AppResult<bool> {
    ApiKeyStore::exists()
}

pub fn setup(app: &mut tauri::App<tauri::Wry>) -> Result<(), Box<dyn std::error::Error>> {
    let root = app.path().app_data_dir()?;
    fs::create_dir_all(&root)?;
    let store = SessionStore::new(root);
    recover_interrupted_sessions(&store).map_err(boxed_app_error)?;
    app.manage(AppState::new(store));
    Ok(())
}

pub fn stop_recording_on_exit(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let _guard = lock_sessions(&state)?;
    stop_recording_in_store_on_exit(&state.store, |id| state.recorder.stop(id))
}

pub fn stop_recording_in_store_on_exit<F>(store: &SessionStore, stop: F) -> AppResult<()>
where
    F: FnOnce(&str) -> AppResult<PathBuf>,
{
    let Some(session) = store
        .list()?
        .into_iter()
        .find(|session| session.status == SessionStatus::Recording)
    else {
        return Ok(());
    };

    let stopped = match stop(&session.id) {
        Ok(path) => interrupted_recording(session, path.to_string_lossy().into_owned())?,
        Err(stop_error) => {
            let failed = recover_interrupted(session);
            store.save(&failed)?;
            return Err(stop_error);
        }
    };
    store.save(&stopped)
}

fn spawn_processing(app: AppHandle, id: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = process_session(&app, &id).await {
            save_failure(&app, &id, error);
        }
    });
}

async fn process_session(app: &AppHandle, id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let mut session = load_session(&state, id)?;
    let api_key = ApiKeyStore::load()?
        .ok_or_else(|| AppError::new("missing_api_key", "OpenAI API key is required"))?;

    if needs_transcription(&session) {
        let path = retained_audio_path(app, &session)?
            .ok_or_else(|| AppError::new("audio_file", "Recorded audio is unavailable"))?;
        let transcript = state.openai.transcribe(&path, &api_key).await?;
        persist_transcript(&state, id, transcript)?;
    }

    session = remove_retained_audio(app, &state, id)?;

    let sections = state.openai.enrich(&session, &api_key).await?;
    session = persist_complete(&state, id, sections_to_markdown(sections))?;
    let _ = app.emit(SESSION_UPDATED, session);
    Ok(())
}

fn save_failure(app: &AppHandle, id: &str, error: AppError) {
    let state = app.state::<AppState>();
    if let Ok(failed) = persist_failure(&state, id, error) {
        let _ = app.emit(SESSION_UPDATED, failed);
    }
}

fn load_session(state: &AppState, id: &str) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    state.store.get(id)
}

fn save_session_state(state: &AppState, input: UpdateSessionInput) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(&input.id)?;
    session.apply(input)?;
    state.store.save(&session)?;
    Ok(session)
}

fn persist_recording(state: &AppState, id: &str, audio_path: String) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    if session.status != SessionStatus::Draft {
        return Err(invalid_status("Only a draft session can start recording"));
    }
    session.audio_path = Some(audio_path);
    session.status = SessionStatus::Recording;
    session.error = None;
    state.store.save(&session)?;
    Ok(session)
}

fn persist_stopped(state: &AppState, id: &str, audio_path: String) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let session = state.store.get(id)?;
    let processing = transition_to_processing(session.clone(), audio_path.clone())?;
    if let Err(error) = state.store.save(&processing) {
        let failed = failed_after_stop_save(session, audio_path, error.clone());
        let _ = state.store.save(&failed);
        return Err(error);
    }
    Ok(processing)
}

fn failed_after_stop_save(mut session: Session, audio_path: String, error: AppError) -> Session {
    session.ended_at.get_or_insert_with(now);
    session.audio_path = Some(audio_path);
    transition_to_failed(session, error)
}

fn prepare_retry(state: &AppState, id: &str) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let session = retry_start(state.store.get(id)?)?;
    state.store.save(&session)?;
    Ok(session)
}

pub fn retry_start(mut session: Session) -> AppResult<Session> {
    if session.status != SessionStatus::Failed {
        return Err(invalid_status("Only a failed session can be retried"));
    }
    session.status = SessionStatus::Processing;
    session.error = None;
    session.ended_at.get_or_insert_with(now);
    Ok(session)
}

pub fn needs_transcription(session: &Session) -> bool {
    session.transcript.is_none()
}

fn delete_session_state(state: &AppState, id: &str) -> AppResult<()> {
    let _guard = lock_sessions(state)?;
    let session = state.store.get(id)?;
    if matches!(
        session.status,
        SessionStatus::Recording | SessionStatus::Processing
    ) {
        return Err(invalid_status(
            "Stop recording or processing before deleting the session",
        ));
    }
    state.store.delete(id)
}

fn delete_transcript_state(state: &AppState, id: &str) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let session = transcript_deleted(state.store.get(id)?)?;
    state.store.save(&session)?;
    Ok(session)
}

pub fn transcript_deleted(mut session: Session) -> AppResult<Session> {
    if matches!(
        session.status,
        SessionStatus::Recording | SessionStatus::Processing
    ) {
        return Err(invalid_status(
            "Stop recording or processing before deleting the transcript",
        ));
    }
    session.transcript = None;
    session.enriched_notes = None;
    session.status = SessionStatus::Draft;
    session.error = None;
    Ok(session)
}

fn persist_transcript(state: &AppState, id: &str, transcript: String) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    session.transcript = Some(transcript);
    state.store.save(&session)?;
    Ok(session)
}

fn remove_retained_audio(app: &AppHandle, state: &AppState, id: &str) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    if let Some(path) = retained_audio_path(app, &session)? {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(storage_error(error)),
        }
        session.audio_path = None;
        state.store.save(&session)?;
    }
    Ok(session)
}

fn persist_complete(state: &AppState, id: &str, notes: String) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    session.enriched_notes = Some(notes);
    session.status = SessionStatus::Complete;
    session.error = None;
    state.store.save(&session)?;
    Ok(session)
}

fn persist_failure(state: &AppState, id: &str, error: AppError) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let failed = transition_to_failed(state.store.get(id)?, error);
    state.store.save(&failed)?;
    Ok(failed)
}

fn lock_sessions(state: &AppState) -> AppResult<MutexGuard<'_, ()>> {
    state
        .transactions
        .lock()
        .map_err(|_| AppError::new("storage_error", "Session storage is unavailable"))
}

pub fn recover_interrupted_sessions(store: &SessionStore) -> AppResult<()> {
    for session in store.list()? {
        if matches!(
            session.status,
            SessionStatus::Recording | SessionStatus::Processing
        ) {
            store.save(&recover_interrupted(session))?;
        }
    }
    Ok(())
}

pub fn recover_interrupted(mut session: Session) -> Session {
    let message = if session.status == SessionStatus::Processing {
        "Processing was interrupted"
    } else {
        "Recording was interrupted"
    };
    session.ended_at.get_or_insert_with(now);
    transition_to_failed(session, AppError::new("interrupted", message))
}

pub fn interrupted_recording(
    session: Session,
    audio_path: impl Into<String>,
) -> AppResult<Session> {
    transition_to_processing(session, audio_path).map(recover_interrupted)
}

fn audio_path(app: &AppHandle, id: &str) -> AppResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map(|root| root.join("audio").join(format!("{id}.m4a")))
        .map_err(|error| AppError::new("storage_error", error.to_string()))
}

fn retained_audio_path(app: &AppHandle, session: &Session) -> AppResult<Option<PathBuf>> {
    session
        .audio_path
        .as_deref()
        .map(|saved| validate_audio_path(saved, &audio_path(app, &session.id)?))
        .transpose()
}

fn validate_audio_path(saved: &str, assigned: &std::path::Path) -> AppResult<PathBuf> {
    let saved = PathBuf::from(saved);
    if saved != assigned {
        return Err(AppError::new(
            "audio_file",
            "Recorded audio path is invalid",
        ));
    }
    Ok(saved)
}

fn invalid_status(message: &str) -> AppError {
    AppError::new("invalid_session_status", message)
}

fn storage_error(error: std::io::Error) -> AppError {
    AppError::new("storage_error", error.to_string())
}

fn boxed_app_error(error: AppError) -> Box<dyn std::error::Error> {
    std::io::Error::other(format!("{}: {}", error.code, error.message)).into()
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{mpsc, Arc},
        thread,
        time::Duration,
    };

    use crate::{
        domain::{AppError, CreateSessionInput, Session, SessionStatus, UpdateSessionInput},
        store::SessionStore,
    };

    fn state(status: SessionStatus) -> (Arc<super::AppState>, PathBuf, String) {
        let root = std::env::temp_dir().join(format!("meeting-notes-{}", uuid::Uuid::new_v4()));
        let store = SessionStore::new(root.clone());
        let mut session = Session::new(CreateSessionInput {
            title: "Weekly review".into(),
            context: String::new(),
            attendees: Vec::new(),
        });
        session.status = status;
        let id = session.id.clone();
        store.save(&session).unwrap();
        (Arc::new(super::AppState::new(store)), root, id)
    }

    fn notes_input(id: &str) -> UpdateSessionInput {
        UpdateSessionInput {
            id: id.into(),
            title: "Weekly review".into(),
            context: String::new(),
            attendees: Vec::new(),
            original_notes: "rent roll".into(),
        }
    }

    #[test]
    fn rejects_audio_outside_the_assigned_path() {
        let error = super::validate_audio_path(
            "/tmp/not-this-session.m4a",
            Path::new("/app-data/audio/session-id.m4a"),
        )
        .unwrap_err();

        assert_eq!(error.code, "audio_file");
    }

    #[test]
    fn concurrent_note_and_transcript_mutations_preserve_both() {
        let (state, root, id) = state(SessionStatus::Processing);
        let guard = super::lock_sessions(&state).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        let notes_state = Arc::clone(&state);
        let notes_id = id.clone();
        let notes_started = started_tx.clone();
        let notes_done = done_tx.clone();
        let notes = thread::spawn(move || {
            notes_started.send(()).unwrap();
            notes_done
                .send(super::save_session_state(
                    &notes_state,
                    notes_input(&notes_id),
                ))
                .unwrap();
        });

        let transcript_state = Arc::clone(&state);
        let transcript_id = id.clone();
        let transcript = thread::spawn(move || {
            started_tx.send(()).unwrap();
            done_tx
                .send(super::persist_transcript(
                    &transcript_state,
                    &transcript_id,
                    "spoken transcript".into(),
                ))
                .unwrap();
        });

        started_rx.recv().unwrap();
        started_rx.recv().unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
        drop(guard);
        assert!(done_rx.recv().unwrap().is_ok());
        assert!(done_rx.recv().unwrap().is_ok());
        notes.join().unwrap();
        transcript.join().unwrap();

        let saved = state.store.get(&id).unwrap();
        assert_eq!(saved.original_notes, "rent roll");
        assert_eq!(saved.transcript.as_deref(), Some("spoken transcript"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_retries_prepare_one_processor() {
        let (state, root, id) = state(SessionStatus::Failed);
        let guard = super::lock_sessions(&state).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let mut threads = Vec::new();

        for _ in 0..2 {
            let state = Arc::clone(&state);
            let id = id.clone();
            let done_tx = done_tx.clone();
            let started_tx = started_tx.clone();
            threads.push(thread::spawn(move || {
                started_tx.send(()).unwrap();
                done_tx.send(super::prepare_retry(&state, &id)).unwrap();
            }));
        }

        started_rx.recv().unwrap();
        started_rx.recv().unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
        drop(guard);
        let results = [done_rx.recv().unwrap(), done_rx.recv().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        for thread in threads {
            thread.join().unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retry_and_delete_are_one_atomic_choice() {
        let (state, root, id) = state(SessionStatus::Failed);
        let guard = super::lock_sessions(&state).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        let retry_state = Arc::clone(&state);
        let retry_id = id.clone();
        let retry_done = done_tx.clone();
        let retry_started = started_tx.clone();
        let retry = thread::spawn(move || {
            retry_started.send(()).unwrap();
            retry_done
                .send((
                    "retry",
                    super::prepare_retry(&retry_state, &retry_id).is_ok(),
                ))
                .unwrap();
        });
        let delete_state = Arc::clone(&state);
        let delete_id = id.clone();
        let delete = thread::spawn(move || {
            started_tx.send(()).unwrap();
            done_tx
                .send((
                    "delete",
                    super::delete_session_state(&delete_state, &delete_id).is_ok(),
                ))
                .unwrap();
        });

        started_rx.recv().unwrap();
        started_rx.recv().unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
        drop(guard);
        let results = [done_rx.recv().unwrap(), done_rx.recv().unwrap()];
        assert_eq!(results.iter().filter(|(_, ok)| *ok).count(), 1);
        retry.join().unwrap();
        delete.join().unwrap();

        let retry_won = results.contains(&("retry", true));
        assert_eq!(state.store.get(&id).is_ok(), retry_won);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_processing_save_keeps_stopped_audio_retryable() {
        let (_, root, id) = state(SessionStatus::Recording);
        let store = SessionStore::new(root.clone());
        let session = store.get(&id).unwrap();
        let failed = super::failed_after_stop_save(
            session,
            "/app-data/audio/session-id.m4a".into(),
            AppError::new("storage_error", "disk full"),
        );

        assert_eq!(failed.status, SessionStatus::Failed);
        assert_eq!(
            failed.audio_path.as_deref(),
            Some("/app-data/audio/session-id.m4a")
        );
        assert_eq!(failed.error.unwrap().code, "storage_error");
        fs::remove_dir_all(root).unwrap();
    }
}
