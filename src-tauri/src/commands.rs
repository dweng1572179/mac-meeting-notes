use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use chrono::{SecondsFormat, Utc};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    domain::{
        transition_to_failed, transition_to_processing, AppError, AppResult, AudioSource,
        CreateSessionInput, MeetingAnswer, Session, SessionStatus, SourceTranscript,
        TranscriptChunk, UpdateSessionInput,
    },
    openai::{sections_to_markdown, OpenAiClient},
    recorder::{Recorder, RecordingFiles, RecordingInfo},
    secrets::ApiKeyStore,
    store::SessionStore,
};

const SESSION_UPDATED: &str = "session-updated";
const QUESTION_SESSION_LIMIT: usize = 20;

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
    let (system_path, microphone_path) = audio_paths(&app, &id)?;
    start_recording_state(
        &state,
        &id,
        system_path,
        microphone_path,
        |id, system_path, microphone_path| state.recorder.start(id, system_path, microphone_path),
    )
}

fn start_recording_state(
    state: &AppState,
    id: &str,
    system_path: PathBuf,
    microphone_path: PathBuf,
    start: impl FnOnce(&str, &std::path::Path, &std::path::Path) -> AppResult<RecordingInfo>,
) -> AppResult<RecordingInfo> {
    let session = load_session(state, id)?;
    if session.status != SessionStatus::Draft {
        return Err(invalid_status("Only a draft session can start recording"));
    }

    state
        .store
        .validate_audio_path(id, &system_path.to_string_lossy())?;
    state
        .store
        .validate_microphone_audio_path(id, &microphone_path.to_string_lossy())?;
    if let Some(parent) = system_path.parent() {
        fs::create_dir_all(parent).map_err(storage_error)?;
    }
    let recording = start(&session.id, &system_path, &microphone_path)?;
    if let Err(error) = persist_recording(
        state,
        id,
        system_path.to_string_lossy().into_owned(),
        microphone_path.to_string_lossy().into_owned(),
    ) {
        let _ = state.recorder.stop(&session.id);
        for path in [
            state
                .store
                .validate_audio_path(id, &system_path.to_string_lossy()),
            state
                .store
                .validate_microphone_audio_path(id, &microphone_path.to_string_lossy()),
        ]
        .into_iter()
        .flatten()
        {
            let _ = fs::remove_file(path);
        }
        return Err(error);
    }
    Ok(recording)
}

#[tauri::command]
pub fn recording_health(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<crate::recorder::RecordingHealth> {
    state.recorder.health(&id)
}

#[tauri::command]
pub fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Session> {
    let session = stop_recording_state(&state, &id, |id| state.recorder.stop(id))?;
    if session.status == SessionStatus::Processing {
        spawn_processing(app, id);
    }
    Ok(session)
}

fn stop_recording_state(
    state: &AppState,
    id: &str,
    stop: impl FnOnce(&str) -> AppResult<RecordingFiles>,
) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let original = state.store.get(id)?;
    if original.status != SessionStatus::Recording {
        return Err(invalid_status("Only a recording session can be stopped"));
    }
    let mut session = match stop(id) {
        Ok(paths) => stopped_session(original.clone(), paths)?,
        Err(error) => transition_to_failed(original.clone(), error),
    };
    session.ended_at.get_or_insert_with(now);
    if let Err(error) = state.store.save(&session) {
        session = transition_to_failed(session, error);
        session.ended_at.get_or_insert_with(now);
        let _ = state.store.save(&session);
    }
    // Returning the failed snapshot also leaves the UI recoverable when storage is unavailable.
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

#[tauri::command]
pub async fn ask_meetings(
    state: State<'_, AppState>,
    folder: Option<String>,
    question: String,
) -> AppResult<MeetingAnswer> {
    let question = question.trim();
    if question.is_empty() {
        return Err(AppError::new("invalid_question", "Enter a question"));
    }
    let sessions = {
        let _guard = lock_sessions(&state)?;
        eligible_meetings(state.store.list()?, folder.as_deref())
    };
    if sessions.is_empty() {
        return Err(AppError::new(
            "no_meeting_sources",
            "No completed meetings with source material are available here yet",
        ));
    }
    let api_key = ApiKeyStore::load()?
        .ok_or_else(|| AppError::new("missing_api_key", "OpenAI API key is required"))?;
    state
        .openai
        .ask_meetings(&sessions, question, &api_key)
        .await
}

fn eligible_meetings(sessions: Vec<Session>, folder: Option<&str>) -> Vec<Session> {
    sessions
        .into_iter()
        .filter(|session| {
            session.status == SessionStatus::Complete
                && match folder {
                    Some(folder) => session.folder == folder,
                    None => true,
                }
                && has_meeting_material(session)
        })
        // ponytail: recent bounded context avoids a retrieval/database layer; add local retrieval when real libraries outgrow 20 meetings.
        .take(QUESTION_SESSION_LIMIT)
        .collect()
}

fn has_meeting_material(session: &Session) -> bool {
    [
        session.context.as_str(),
        session.original_notes.as_str(),
        session.transcript.as_deref().unwrap_or_default(),
        session.enriched_notes.as_deref().unwrap_or_default(),
    ]
    .into_iter()
    .any(|text| !text.trim().is_empty())
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
    F: FnOnce(&str) -> AppResult<RecordingFiles>,
{
    let Some(session) = store
        .list()?
        .into_iter()
        .find(|session| session.status == SessionStatus::Recording)
    else {
        return Ok(());
    };

    let stopped = match stop(&session.id) {
        Ok(paths) => interrupted_recording(session, paths)?,
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
    let api_key = ApiKeyStore::load()?
        .ok_or_else(|| AppError::new("missing_api_key", "OpenAI API key is required"))?;
    let session = process_session_with_key(&state, id, &api_key, |session| {
        let _ = app.emit(SESSION_UPDATED, session);
    })
    .await?;
    let _ = app.emit(SESSION_UPDATED, session);
    Ok(())
}

async fn process_session_with_key(
    state: &AppState,
    id: &str,
    api_key: &str,
    progress: impl Fn(Session),
) -> AppResult<Session> {
    let session = load_session(state, id)?;
    if needs_transcription(&session) || !session.transcription.is_empty() {
        let mut first_error = None;
        for source in [AudioSource::System, AudioSource::Microphone] {
            if let Err(error) = transcribe_source(state, id, source, api_key, &progress).await {
                if !matches!(
                    error.code.as_str(),
                    "audio_file"
                        | "audio_chunk"
                        | "audio_chunk_size"
                        | "invalid_audio"
                        | "no_speech"
                        | "transcription_too_long"
                ) {
                    return Err(error);
                }
                update_processing(state, id, |session| {
                    add_warning(
                        session,
                        format!(
                            "{} could not be fully transcribed. Remaining audio was kept.",
                            source.label()
                        ),
                    );
                })?;
                first_error.get_or_insert(error);
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
    }
    let session = load_session(state, id)?;
    if session
        .transcript
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err(AppError::new(
            "no_speech",
            "No speech was detected. Your typed notes and source audio were kept.",
        ));
    }
    let session = remove_retained_audio(state, id)?;
    let sections = state.openai.enrich(&session, api_key).await?;
    persist_complete(state, id, sections_to_markdown(sections))
}

async fn transcribe_source(
    state: &AppState,
    id: &str,
    source: AudioSource,
    api_key: &str,
    progress: &impl Fn(Session),
) -> AppResult<()> {
    let session = load_session(state, id)?;
    if !session
        .transcription
        .iter()
        .any(|track| track.source == source)
    {
        let Some(path) = source_path(state, &session, source)? else {
            return Ok(());
        };
        let info = crate::audio::inspect(&path)?;
        let duration = info.frames as f64 / info.sample_rate;
        let mut chunks = Vec::new();
        let mut start = 0.0;
        while start < duration {
            let length = (duration - start).min(300.0);
            chunks.push(TranscriptChunk {
                start_seconds: start,
                duration_seconds: length,
                transcript: None,
            });
            start += length;
        }
        let prepared = update_processing(state, id, |session| {
            if chunks.is_empty() {
                add_warning(
                    session,
                    format!(
                        "{} contained no audio frames. Notes use only the available sources.",
                        source.label()
                    ),
                );
            }
            session
                .transcription
                .push(SourceTranscript { source, chunks });
        })?;
        progress(prepared);
    }
    loop {
        let session = load_session(state, id)?;
        let track = session
            .transcription
            .iter()
            .find(|track| track.source == source)
            .expect("prepared source");
        let pending = track
            .chunks
            .iter()
            .position(|chunk| chunk.transcript.is_none());
        let Some(index) = pending else {
            let has_speech = track.chunks.iter().any(|chunk| {
                !chunk
                    .transcript
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
            });
            progress(update_processing(state, id, |session| {
                let resolved = format!(
                    "{} could not be fully transcribed. Remaining audio was kept.",
                    source.label()
                );
                session.warnings.retain(|warning| warning != &resolved);
            })?);
            if !has_speech && !track.chunks.is_empty() {
                progress(update_processing(state, id, |session| {
                    add_warning(
                        session,
                        format!(
                            "{} had no transcribed speech. Notes use only the available sources.",
                            source.label()
                        ),
                    );
                })?);
            }
            if has_speech {
                cleanup_source(state, id, source)?;
            }
            return Ok(());
        };
        let chunk = track.chunks[index].clone();
        let input = source_path(state, &session, source)?
            .ok_or_else(|| AppError::new("audio_file", "Untranscribed source audio is unavailable. Your saved notes and transcript progress were kept."))?;
        let output = state.store.chunk_path(id, source)?;
        remove_audio_file(output.clone())?;
        // Native decode/encode runs off the UI thread, with bounded PCM buffers and no capture-time rotation.
        let output_for_write = output.clone();
        let start = chunk.start_seconds;
        let length = chunk.duration_seconds;
        tauri::async_runtime::spawn_blocking(move || {
            crate::audio::write_chunk(&input, &output_for_write, start, length)
        })
        .await
        .map_err(|_| {
            AppError::new(
                "audio_chunk",
                "Audio preparation was interrupted. Your source audio was kept.",
            )
        })??;
        match state.openai.transcribe(&output, api_key).await {
            Ok(text) => {
                let saved = update_processing(state, id, |session| {
                    let track = session
                        .transcription
                        .iter_mut()
                        .find(|track| track.source == source)
                        .expect("prepared source");
                    track.chunks[index].transcript = Some(text);
                    session.transcript = assembled_transcript(&session.transcription);
                })?;
                // Only the durable checkpoint permits deletion; the full source stays until every chunk is saved.
                remove_audio_file(state.store.chunk_path(id, source)?)?;
                progress(saved);
            }
            Err(error) if error.code == "transcription_too_long" && length > 30.0 => {
                let saved = update_processing(state, id, |session| {
                    let track = session
                        .transcription
                        .iter_mut()
                        .find(|track| track.source == source)
                        .expect("prepared source");
                    track.chunks.splice(
                        index..=index,
                        [
                            TranscriptChunk {
                                start_seconds: start,
                                duration_seconds: length / 2.0,
                                transcript: None,
                            },
                            TranscriptChunk {
                                start_seconds: start + length / 2.0,
                                duration_seconds: length / 2.0,
                                transcript: None,
                            },
                        ],
                    );
                })?;
                progress(saved);
            }
            Err(error) => return Err(error),
        }
    }
}

fn source_path(
    state: &AppState,
    session: &Session,
    source: AudioSource,
) -> AppResult<Option<PathBuf>> {
    match source {
        AudioSource::System => retained_audio_path(state, session),
        AudioSource::Microphone => retained_microphone_audio_path(state, session),
    }
}

fn cleanup_source(state: &AppState, id: &str, source: AudioSource) -> AppResult<()> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    if let Some(path) = source_path(state, &session, source)? {
        remove_audio_file(path)?;
    }
    remove_audio_file(state.store.chunk_path(id, source)?)?;
    match source {
        AudioSource::System => session.audio_path = None,
        AudioSource::Microphone => session.microphone_audio_path = None,
    }
    state.store.save(&session)
}

fn update_processing(
    state: &AppState,
    id: &str,
    apply: impl FnOnce(&mut Session),
) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    apply(&mut session);
    state.store.save(&session)?;
    Ok(session)
}

fn add_warning(session: &mut Session, warning: String) {
    if !session.warnings.contains(&warning) {
        session.warnings.push(warning);
    }
}

fn assembled_transcript(tracks: &[SourceTranscript]) -> Option<String> {
    let mut chunks = tracks
        .iter()
        .flat_map(|track| {
            track.chunks.iter().filter_map(move |chunk| {
                chunk
                    .transcript
                    .as_deref()
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| (track.source, chunk, text))
            })
        })
        .collect::<Vec<_>>();
    chunks
        .sort_by(|(_, left, _), (_, right, _)| left.start_seconds.total_cmp(&right.start_seconds));
    if chunks.is_empty() {
        return None;
    }
    let mut text = "Source tracks overlap in time. Offsets below refer to each source's recorded audio, not verified wall-clock alignment; capture gaps can shift these offsets.

".to_owned();
    for (source, chunk, transcript) in chunks {
        text.push_str(&format!(
            "[{} – {}] {}:
{}

",
            audio_time(chunk.start_seconds),
            audio_time(chunk.start_seconds + chunk.duration_seconds),
            source.label(),
            transcript.trim()
        ));
    }
    Some(text.trim_end().to_owned())
}

fn audio_time(seconds: f64) -> String {
    let seconds = seconds.round() as u64;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
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

fn persist_recording(
    state: &AppState,
    id: &str,
    audio_path: String,
    microphone_audio_path: String,
) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    if session.status != SessionStatus::Draft {
        return Err(invalid_status("Only a draft session can start recording"));
    }
    session.audio_path = Some(audio_path);
    session.microphone_audio_path = Some(microphone_audio_path);
    session.status = SessionStatus::Recording;
    session.started_at = now();
    session.error = None;
    state.store.save(&session)?;
    Ok(session)
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
    if !session.transcription.is_empty() {
        return session
            .transcription
            .iter()
            .any(|track| track.chunks.iter().any(|chunk| chunk.transcript.is_none()))
            || [
                (AudioSource::System, &session.audio_path),
                (AudioSource::Microphone, &session.microphone_audio_path),
            ]
            .iter()
            .any(|(source, path)| {
                path.is_some()
                    && !session
                        .transcription
                        .iter()
                        .any(|track| track.source == *source)
            });
    }
    match session.transcript.as_deref() {
        Some(transcript) => transcript.trim().is_empty(),
        None => true,
    }
}

pub fn combine_transcripts(system: &str, microphone: &str) -> AppResult<String> {
    let mut sources = Vec::new();
    if !system.trim().is_empty() {
        sources.push(format!("Meeting audio:\n{}", system.trim()));
    }
    if !microphone.trim().is_empty() {
        sources.push(format!("You:\n{}", microphone.trim()));
    }
    if sources.is_empty() {
        Err(AppError::new(
            "no_speech",
            "No speech was detected. Your audio was kept so you can retry.",
        ))
    } else {
        Ok(sources.join("\n\n"))
    }
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
    session.transcription.clear();
    session.enriched_notes = None;
    if session.audio_path.is_some() || session.microphone_audio_path.is_some() {
        session.status = SessionStatus::Failed;
        session.error = Some(AppError::new("transcript_deleted", "Transcript deleted. Your original notes and retained audio were kept. Retry will transcribe the remaining audio again."));
    } else {
        session.capture_health = None;
        session.warnings.clear();
        session.status = SessionStatus::Draft;
        session.error = None;
    }
    Ok(session)
}

#[cfg(test)]
fn persist_transcript(state: &AppState, id: &str, transcript: String) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    session.transcript = Some(transcript);
    state.store.save(&session)?;
    Ok(session)
}

fn remove_retained_audio(state: &AppState, id: &str) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    let retained = [
        retained_audio_path(state, &session)?,
        retained_microphone_audio_path(state, &session)?,
    ];
    for source in [AudioSource::System, AudioSource::Microphone] {
        remove_audio_file(state.store.chunk_path(id, source)?)?;
    }
    if retained.iter().any(Option::is_some) {
        for path in retained.into_iter().flatten() {
            remove_audio_file(path)?;
        }
        session.audio_path = None;
        session.microphone_audio_path = None;
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
    if session.status == SessionStatus::Recording {
        add_warning(&mut session, "Recording was interrupted. Capture may be incomplete; full meeting coverage could not be verified.".into());
    }
    let message = if session.status == SessionStatus::Processing {
        "Processing was interrupted"
    } else {
        "Recording was interrupted"
    };
    session.ended_at.get_or_insert_with(now);
    transition_to_failed(session, AppError::new("interrupted", message))
}

fn stopped_session(session: Session, paths: RecordingFiles) -> AppResult<Session> {
    let mut session = transition_to_processing(
        session,
        paths.system.to_string_lossy().into_owned(),
        Some(paths.microphone.to_string_lossy().into_owned()),
    )?;
    if let Some(health) = paths.health {
        session.warnings.extend(health.warnings.iter().cloned());
        session.capture_health = Some(health);
    }
    Ok(session)
}

pub fn interrupted_recording(session: Session, paths: RecordingFiles) -> AppResult<Session> {
    stopped_session(session, paths).map(recover_interrupted)
}

fn audio_paths(app: &AppHandle, id: &str) -> AppResult<(PathBuf, PathBuf)> {
    app.path()
        .app_data_dir()
        .map(|root| {
            let audio = root.join("audio");
            (
                audio.join(format!("{id}.m4a")),
                audio.join(format!("{id}-mic.m4a")),
            )
        })
        .map_err(|error| AppError::new("storage_error", error.to_string()))
}

fn retained_audio_path(state: &AppState, session: &Session) -> AppResult<Option<PathBuf>> {
    session
        .audio_path
        .as_deref()
        .map(|saved| state.store.validate_audio_path(&session.id, saved))
        .transpose()
}

fn retained_microphone_audio_path(
    state: &AppState,
    session: &Session,
) -> AppResult<Option<PathBuf>> {
    session
        .microphone_audio_path
        .as_deref()
        .map(|saved| {
            state
                .store
                .validate_microphone_audio_path(&session.id, saved)
        })
        .transpose()
}

fn remove_audio_file(path: PathBuf) -> AppResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(storage_error(error)),
    }
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
        path::PathBuf,
        sync::{mpsc, Arc},
        thread,
        time::Duration,
    };

    use crate::{
        domain::{AppError, CreateSessionInput, Session, SessionStatus, UpdateSessionInput},
        recorder::RecordingFiles,
        store::SessionStore,
    };

    #[cfg(unix)]
    #[test]
    fn capture_rejects_symlinked_audio_before_starting_native_recording() {
        use std::os::unix::fs::symlink;
        for destination in ["directory", "file", "dangling"] {
            let (state, root, id) = state(SessionStatus::Draft);
            fs::create_dir(root.join("outside")).unwrap();
            let victim = root.join("outside/victim");
            if destination != "dangling" {
                fs::write(&victim, "untouched").unwrap();
            }
            let path = root.join("audio").join(format!("{id}.m4a"));
            let microphone_path = root.join("audio").join(format!("{id}-mic.m4a"));
            if destination == "directory" {
                symlink(root.join("outside"), root.join("audio")).unwrap();
            } else {
                fs::create_dir(root.join("audio")).unwrap();
                symlink(&victim, &path).unwrap();
            }
            let mut started = false;
            let result =
                super::start_recording_state(&state, &id, path, microphone_path, |_, _, _| {
                    started = true;
                    Err(AppError::new("native_reached", "must validate first"))
                });
            fs::remove_dir_all(root).unwrap();
            assert!(
                !started,
                "native recorder reached through {destination} symlink"
            );
            assert_eq!(result.unwrap_err().code, "invalid_audio_path");
        }
    }

    #[cfg(unix)]
    fn stop_failure_case(fail_save: bool) {
        use std::os::unix::fs::PermissionsExt;
        let (state, root, id) = state(SessionStatus::Recording);
        fs::create_dir(root.join("audio")).unwrap();
        let path = root.join("audio").join(format!("{id}.m4a"));
        let microphone_path = root.join("audio").join(format!("{id}-mic.m4a"));
        fs::write(&path, "retryable audio").unwrap();
        fs::write(&microphone_path, "retryable microphone audio").unwrap();
        let mut recording = state.store.get(&id).unwrap();
        recording.original_notes = "rent roll".into();
        recording.audio_path = Some(path.to_string_lossy().into_owned());
        recording.microphone_audio_path = Some(microphone_path.to_string_lossy().into_owned());
        state.store.save(&recording).unwrap();
        if fail_save {
            fs::set_permissions(root.join("sessions"), fs::Permissions::from_mode(0o500)).unwrap();
        }
        let result = super::stop_recording_state(&state, &id, |_| {
            if fail_save {
                Ok(RecordingFiles {
                    health: Some(serde_json::from_value(serde_json::json!({
                        "wallSeconds": 600.0, "identityChanged": false,
                        "warnings": ["System audio capture was incomplete."],
                        "system": {"admittedFrames":48000,"writtenFrames":48000,"capturedSeconds":1.0,"sampleRate":48000.0,"lastCallbackAgeSeconds":599.0,"writeError":null,"status":"short_capture"},
                        "microphone": {"admittedFrames":48000,"writtenFrames":48000,"capturedSeconds":1.0,"sampleRate":48000.0,"lastCallbackAgeSeconds":599.0,"writeError":null,"status":"short_capture"}
                    })).unwrap()),
                    system: path.clone(),
                    microphone: microphone_path.clone(),
                })
            } else {
                Err(AppError::new("audio_capture", "writer finalization failed"))
            }
        });
        fs::set_permissions(root.join("sessions"), fs::Permissions::from_mode(0o700)).unwrap();
        let saved = state.store.get(&id).unwrap();
        let audio_kept = path.exists();
        fs::remove_dir_all(root).unwrap();

        let published =
            result.expect("stop must return a Failed session after releasing the recorder");
        assert_eq!(published.status, SessionStatus::Failed);
        assert_eq!(published.original_notes, "rent roll");
        assert_eq!(published.audio_path, recording.audio_path);
        assert_eq!(
            published.microphone_audio_path,
            recording.microphone_audio_path
        );
        assert!(published.ended_at.is_some());
        if fail_save {
            assert!(
                published.capture_health.is_some(),
                "stop-save failure must preserve capture evidence"
            );
            assert_eq!(published.warnings, ["System audio capture was incomplete."]);
        }
        assert_eq!(
            published.error.unwrap().code,
            if fail_save {
                "storage_error"
            } else {
                "audio_capture"
            }
        );
        assert!(audio_kept);
        if !fail_save {
            assert_eq!(saved.status, SessionStatus::Failed);
        }
    }

    #[cfg(unix)]
    #[test]
    fn stop_publishes_failed_session_after_recorder_failure() {
        stop_failure_case(false);
    }

    #[cfg(unix)]
    #[test]
    fn stop_publishes_failed_session_when_both_state_saves_fail() {
        stop_failure_case(true);
    }

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
            folder: String::new(),
            original_notes: "rent roll".into(),
        }
    }

    #[test]
    fn stalled_openai_request_fails_with_retry_data_preserved() {
        use std::{io::Read, net::TcpListener, time::Instant};
        let (state, root, id) = state(SessionStatus::Processing);
        fs::create_dir(root.join("audio")).unwrap();
        let path = root.join("audio").join(format!("{id}.m4a"));
        fs::write(&path, "retryable audio").unwrap();
        let mut session = state.store.get(&id).unwrap();
        session.original_notes = "rent roll".into();
        session.audio_path = Some(path.to_string_lossy().into_owned());
        state.store.save(&session).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = crate::openai::OpenAiClient::with_base_url(format!(
            "http://{}/v1",
            listener.local_addr().unwrap()
        ));
        let (release_tx, release_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(15)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let _ = release_rx.recv_timeout(Duration::from_secs(13));
        });
        let started = Instant::now();
        let error = tauri::async_runtime::block_on(client.validate_key("disposable-test-value"))
            .unwrap_err();
        let elapsed = started.elapsed();
        let _ = release_tx.send(());
        server.join().unwrap();
        let failed = super::persist_failure(&state, &id, error).unwrap();
        let retry = super::prepare_retry(&state, &id).unwrap();
        let audio = fs::read_to_string(&path).unwrap();
        fs::remove_dir_all(root).unwrap();

        assert!(
            elapsed < Duration::from_secs(12),
            "request stalled for {elapsed:?}"
        );
        assert_eq!(failed.status, SessionStatus::Failed);
        assert_eq!(failed.error.unwrap().code, "openai_timeout");
        assert_eq!(retry.original_notes, "rent roll");
        assert_eq!(retry.audio_path, session.audio_path);
        assert_eq!(audio, "retryable audio");
    }

    #[test]
    fn successful_transcription_cleanup_removes_both_audio_files() {
        let (state, root, id) = state(SessionStatus::Processing);
        let audio_dir = root.join("audio");
        fs::create_dir(&audio_dir).unwrap();
        let system_path = audio_dir.join(format!("{id}.m4a"));
        let microphone_path = audio_dir.join(format!("{id}-mic.m4a"));
        fs::write(&system_path, "system audio").unwrap();
        fs::write(&microphone_path, "microphone audio").unwrap();
        let mut session = state.store.get(&id).unwrap();
        session.audio_path = Some(system_path.to_string_lossy().into_owned());
        session.microphone_audio_path = Some(microphone_path.to_string_lossy().into_owned());
        session.transcript = Some("Meeting audio:\nremote\n\nYou:\nlocal".into());
        state.store.save(&session).unwrap();

        let cleaned = super::remove_retained_audio(&state, &id).unwrap();

        assert_eq!(cleaned.audio_path, None);
        assert_eq!(cleaned.microphone_audio_path, None);
        assert!(!system_path.exists());
        assert!(!microphone_path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_question_sources_are_completed_material_from_that_folder() {
        let mut included = Session::new(CreateSessionInput {
            title: "Included".into(),
            context: String::new(),
            attendees: Vec::new(),
        });
        included.folder = "Acquisitions".into();
        included.status = SessionStatus::Complete;
        included.transcript = Some("source".into());
        let mut wrong_folder = included.clone();
        wrong_folder.id = "wrong-folder".into();
        wrong_folder.folder = "Leasing".into();
        let mut draft = included.clone();
        draft.id = "draft".into();
        draft.status = SessionStatus::Draft;
        let mut empty = included.clone();
        empty.id = "empty".into();
        empty.transcript = None;

        let eligible = super::eligible_meetings(
            vec![included.clone(), wrong_folder, draft, empty],
            Some("Acquisitions"),
        );

        assert_eq!(eligible, vec![included]);
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
}

#[cfg(all(test, target_os = "macos"))]
#[path = "processing_tests.rs"]
mod processing_tests;
