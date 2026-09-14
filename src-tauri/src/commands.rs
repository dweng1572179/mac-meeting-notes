use std::{fs, path::PathBuf};

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
    state.store.save(&session)?;
    Ok(session)
}

#[tauri::command]
pub fn save_session(state: State<'_, AppState>, input: UpdateSessionInput) -> AppResult<Session> {
    let mut session = state.store.get(&input.id)?;
    session.apply(input)?;
    state.store.save(&session)?;
    Ok(session)
}

#[tauri::command]
pub fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<RecordingInfo> {
    let mut session = state.store.get(&id)?;
    if session.status != SessionStatus::Draft {
        return Err(invalid_status("Only a draft session can start recording"));
    }

    let audio_path = audio_path(&app, &session.id)?;
    if let Some(parent) = audio_path.parent() {
        fs::create_dir_all(parent).map_err(storage_error)?;
    }
    let recording = state.recorder.start(&session.id, &audio_path)?;
    session.audio_path = Some(audio_path.to_string_lossy().into_owned());
    session.status = SessionStatus::Recording;
    session.error = None;
    if let Err(error) = state.store.save(&session) {
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
    let session = state.store.get(&id)?;
    if session.status != SessionStatus::Recording {
        return Err(invalid_status("Only a recording session can be stopped"));
    }
    let path = state.recorder.stop(&id)?;
    let session = transition_to_processing(session, path.to_string_lossy())?;
    state.store.save(&session)?;
    spawn_processing(app, id);
    Ok(session)
}

#[tauri::command]
pub fn retry_processing(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Session> {
    let mut session = state.store.get(&id)?;
    if session.status != SessionStatus::Failed {
        return Err(invalid_status("Only a failed session can be retried"));
    }
    session.status = SessionStatus::Processing;
    session.error = None;
    session.ended_at.get_or_insert_with(now);
    state.store.save(&session)?;
    spawn_processing(app, id);
    Ok(session)
}

#[tauri::command]
pub fn delete_session(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let session = state.store.get(&id)?;
    if matches!(
        session.status,
        SessionStatus::Recording | SessionStatus::Processing
    ) {
        return Err(invalid_status(
            "Stop recording or processing before deleting the session",
        ));
    }
    state.store.delete(&id)
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
    recover_interrupted(&store).map_err(boxed_app_error)?;
    app.manage(AppState {
        store,
        recorder: Recorder::new(),
        keys: ApiKeyStore,
        openai: OpenAiClient::new(),
    });
    Ok(())
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
    let mut session = state.store.get(id)?;
    let api_key = ApiKeyStore::load()?
        .ok_or_else(|| AppError::new("missing_api_key", "OpenAI API key is required"))?;

    if session.transcript.is_none() {
        let path = retained_audio_path(app, &session)?
            .ok_or_else(|| AppError::new("audio_file", "Recorded audio is unavailable"))?;
        let transcript = state.openai.transcribe(&path, &api_key).await?;
        session = state.store.get(id)?;
        session.transcript = Some(transcript);
        state.store.save(&session)?;
    }

    if let Some(path) = retained_audio_path(app, &session)? {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(storage_error(error)),
        }
        session.audio_path = None;
        state.store.save(&session)?;
    }

    let sections = state.openai.enrich(&session, &api_key).await?;
    session = state.store.get(id)?;
    session.enriched_notes = Some(sections_to_markdown(sections));
    session.status = SessionStatus::Complete;
    session.error = None;
    save_and_emit(app, &session)
}

fn save_failure(app: &AppHandle, id: &str, error: AppError) {
    let state = app.state::<AppState>();
    if let Ok(session) = state.store.get(id) {
        let failed = transition_to_failed(session, error);
        if state.store.save(&failed).is_ok() {
            let _ = app.emit(SESSION_UPDATED, failed);
        }
    }
}

fn save_and_emit(app: &AppHandle, session: &Session) -> AppResult<()> {
    app.state::<AppState>().store.save(session)?;
    let _ = app.emit(SESSION_UPDATED, session.clone());
    Ok(())
}

fn recover_interrupted(store: &SessionStore) -> AppResult<()> {
    for session in store.list()? {
        if session.status == SessionStatus::Recording {
            store.save(&transition_to_failed(
                session,
                AppError::new("interrupted", "Recording was interrupted"),
            ))?;
        }
    }
    Ok(())
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
    use std::path::Path;

    #[test]
    fn rejects_audio_outside_the_assigned_path() {
        let error = super::validate_audio_path(
            "/tmp/not-this-session.m4a",
            Path::new("/app-data/audio/session-id.m4a"),
        )
        .unwrap_err();

        assert_eq!(error.code, "audio_file");
    }
}
