use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use chrono::{SecondsFormat, Utc};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    domain::{
        transition_to_failed, transition_to_processing, AppError, AppResult, AudioFormat, AudioSource,
        CreateSessionInput, MeetingAnswer, Session, SessionStatus, SourceTranscript,
        TranscriptChunk, TranscriptionSettings, UpdateSessionInput,
    },
    openai::{sections_to_markdown, OpenAiClient},
    recorder::{Recorder, RecordingFiles, RecordingInfo},
    secrets::ApiKeyStore,
    store::SessionStore,
};

const SESSION_UPDATED: &str = "session-updated";
const QUESTION_SESSION_LIMIT: usize = 20;
const SPEAKER_TIMING_NOTICE: &str =
    "Speaker labels are unavailable for some audio. Its transcript text was kept.";

#[path = "insights.rs"]
pub mod insights;
#[path = "live_processing.rs"]
mod live;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub sessions: Vec<Session>,
    pub has_api_key: bool,
    pub settings: TranscriptionSettings,
}

pub struct AppState {
    pub store: SessionStore,
    pub recorder: Recorder,
    pub keys: ApiKeyStore,
    pub openai: OpenAiClient,
    // ponytail: one global transaction lock fits the single-user v1; use per-session locks only if contention becomes measurable.
    transactions: Mutex<()>,
    processing_jobs: Arc<Mutex<HashSet<String>>>,
}

impl AppState {
    fn new(store: SessionStore) -> Self {
        Self {
            store,
            recorder: Recorder::new(),
            keys: ApiKeyStore,
            openai: OpenAiClient::new(),
            transactions: Mutex::new(()),
            processing_jobs: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> AppResult<Bootstrap> {
    Ok(Bootstrap {
        sessions: state.store.list()?,
        has_api_key: ApiKeyStore::exists()?,
        settings: state.store.settings()?,
    })
}

#[tauri::command]
pub fn save_transcription_settings(
    state: State<'_, AppState>,
    settings: TranscriptionSettings,
) -> AppResult<TranscriptionSettings> {
    let _guard = lock_sessions(&state)?;
    state.store.save_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn export_markdown(app: AppHandle, title: String, markdown: String) -> AppResult<String> {
    let directory = app
        .path()
        .download_dir()
        .map_err(|error| AppError::new("storage_error", error.to_string()))?;
    export_markdown_to(&directory, &title, &markdown)
        .map(|path| path.to_string_lossy().into_owned())
}

fn export_markdown_to(
    directory: &std::path::Path,
    title: &str,
    markdown: &str,
) -> AppResult<PathBuf> {
    use std::io::Write;
    if markdown.trim().is_empty() || markdown.len() > 16 * 1024 * 1024 {
        return Err(AppError::new(
            "invalid_export",
            "Export must contain text and be no larger than 16 MiB. Your notes were kept.",
        ));
    }
    let title: String = title
        .chars()
        .take(50)
        .map(|character| {
            if character.is_alphanumeric() || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let path = directory.join(format!(
        "{}-{}.md",
        if title.is_empty() { "meeting" } else { &title },
        uuid::Uuid::new_v4()
    ));
    let mut file = fs::File::options()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(storage_error)?;
    if let Err(error) = file
        .write_all(markdown.as_bytes())
        .and_then(|_| file.sync_all())
    {
        let _ = fs::remove_file(&path);
        return Err(storage_error(error));
    }
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(storage_error)?;
    Ok(path)
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
    let audio_format = AudioFormat::for_recording();
    let (system_path, microphone_path) = audio_paths(&app, &id, audio_format)?;
    // Save the capture format before opening files so interrupted startup is recoverable.
    {
        let _guard = lock_sessions(&state)?;
        let mut session = state.store.get(&id)?;
        if session.status != SessionStatus::Draft {
            return Err(invalid_status("Only a draft session can start recording"));
        }
        session.segmented_capture = true;
        session.audio_format = audio_format;
        state.store.save(&session)?;
    }
    let recording = start_recording_state(
        &state,
        &id,
        system_path,
        microphone_path,
        |id, system_path, microphone_path| {
            state
                .recorder
                .start_segmented(id, system_path, microphone_path)
        },
    );
    let recording = match recording {
        Ok(recording) => recording,
        Err(error) => {
            if let Ok(session) = load_session(&state, &id) {
                let _ = app.emit(SESSION_UPDATED, session);
            }
            return Err(error);
        }
    };
    live::spawn_capture_supervisor(app.clone(), id.clone());
    spawn_processing(app, id);
    Ok(recording)
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
        .validate_audio_path(id, &system_path.to_string_lossy(), session.audio_format)?;
    state
        .store
        .validate_microphone_audio_path(id, &microphone_path.to_string_lossy(), session.audio_format)?;
    if let Some(parent) = system_path.parent() {
        fs::create_dir_all(parent).map_err(storage_error)?;
    }
    let recording = match start(&session.id, &system_path, &microphone_path) {
        Ok(recording) => recording,
        Err(error) => {
            if session.segmented_capture && !state.store.segment_files(id, session.audio_format)?.is_empty() {
                update_processing(state, id, |session| {
                    session.status = SessionStatus::Failed;
                    session.error = Some(error.clone());
                    session.audio_path = Some(system_path.to_string_lossy().into_owned());
                    session.ended_at.get_or_insert_with(now);
                })?;
            }
            return Err(error);
        }
    };
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
                .validate_audio_path(id, &system_path.to_string_lossy(), session.audio_format),
            state
                .store
                .validate_microphone_audio_path(id, &microphone_path.to_string_lossy(), session.audio_format),
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
    session_id: Option<String>,
    history: Option<Vec<crate::domain::QuestionTurn>>,
) -> AppResult<MeetingAnswer> {
    let question = validate_question(&question)?;
    let sessions = {
        let _guard = lock_sessions(&state)?;
        question_sources(&state.store, folder.as_deref(), session_id.as_deref())?
    };
    let api_key = ApiKeyStore::load()?
        .ok_or_else(|| AppError::new("missing_api_key", "OpenAI API key is required"))?;
    state
        .openai
        .ask_meetings_with_history(&sessions, question, &api_key, history.as_deref().unwrap_or_default())
        .await
}

fn validate_question(question: &str) -> AppResult<&str> {
    let question = question.trim();
    if question.is_empty() || question.chars().count() > 2000 {
        return Err(AppError::new(
            "invalid_question",
            "Enter a question of 1 to 2000 characters.",
        ));
    }
    Ok(question)
}

fn question_sources(
    store: &SessionStore,
    folder: Option<&str>,
    session_id: Option<&str>,
) -> AppResult<Vec<Session>> {
    let sessions = match session_id {
        Some(id) => eligible_meetings(vec![store.get(id)?], None),
        None => eligible_meetings(store.list()?, folder),
    };
    if sessions.is_empty() {
        return Err(AppError::new(
            "no_meeting_sources",
            "No completed meetings with source material are available here yet",
        ));
    }
    Ok(sessions)
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
        session.notes().as_str(),
        session.transcript.as_deref().unwrap_or_default(),
    ]
    .into_iter()
    .any(|text| !text.trim().is_empty())
}

pub fn setup(app: &mut tauri::App<tauri::Wry>) -> Result<(), Box<dyn std::error::Error>> {
    let root = app.path().app_data_dir()?;
    fs::create_dir_all(&root)?;
    let store = SessionStore::new(root);
    recover_interrupted_sessions(&store).map_err(boxed_app_error)?;
    let state = AppState::new(store);
    for session in state.store.list().map_err(boxed_app_error)? {
        if session.segmented_capture && session.status == SessionStatus::Failed {
            // No capture is active at launch; recover finalized files without sending audio.
            if let Err(error) = live::reconcile_segments(&state, &session.id) {
                let _ = persist_failure(&state, &session.id, error);
            }
        }
    }
    app.manage(state);
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
    let lease = match live::ProcessingLease::claim(&app.state::<AppState>(), &id) {
        Ok(Some(lease)) => lease,
        Ok(None) => return,
        Err(error) => {
            save_failure(&app, &id, error);
            return;
        }
    };
    // One uploader per meeting. Capture rotation runs independently of HTTP latency.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let key = ApiKeyStore::load().and_then(|key| {
            key.ok_or_else(|| AppError::new("missing_api_key", "OpenAI API key is required"))
        });
        let progress = |session| {
            let _ = app.emit(SESSION_UPDATED, session);
        };
        let result = run_processing_worker(
            &state,
            &id,
            || match &key {
                Ok(key) => tauri::async_runtime::block_on(process_session_with_key(
                    &state, &id, key, &progress,
                )),
                Err(error) => Err(error.clone()),
            },
            &progress,
            std::thread::sleep,
        );
        let saved = match result {
            Ok(session) => Ok(session),
            Err(error) => persist_failure(&state, &id, error),
        };
        drop(lease);
        if let Ok(session) = saved {
            let _ = app.emit(SESSION_UPDATED, session);
        }
    });
}

fn run_processing_worker(
    state: &AppState,
    id: &str,
    mut process: impl FnMut() -> AppResult<Session>,
    progress: &impl Fn(Session),
    mut wait: impl FnMut(Duration),
) -> AppResult<Session> {
    let poll = Duration::from_millis(500);
    let mut paused: Option<AppError> = None;
    let mut retry_after: Option<Duration> = None;
    let mut retries = 0;
    let mut was_recording = false;
    loop {
        let session = load_session(state, id)?;
        if !matches!(
            session.status,
            SessionStatus::Recording | SessionStatus::Processing
        ) {
            return Ok(session);
        }
        // Stop should not inherit a long live backoff. Make a fresh, bounded final attempt.
        if was_recording && session.status == SessionStatus::Processing && retry_after.is_some() {
            retries = 0;
            retry_after = Some(Duration::ZERO);
        }
        was_recording = session.status == SessionStatus::Recording;
        if let Some(error) = paused.as_ref() {
            match retry_after {
                Some(remaining) if !remaining.is_zero() => {
                    let delay = remaining.min(poll);
                    wait(delay);
                    retry_after = Some(remaining - delay);
                    continue;
                }
                None => {
                    if session.status != SessionStatus::Recording {
                        return Err(error.clone());
                    }
                    wait(poll);
                    continue;
                }
                Some(_) => progress(update_processing(state, id, |session| {
                    session.live_transcription_error = None;
                })?),
            }
        }
        match process() {
            Ok(session) => {
                retries = 0;
                paused = None;
                retry_after = None;
                let finished = session.status != SessionStatus::Recording;
                let session = if session.live_transcription_error.is_some() {
                    let cleared = update_processing(state, id, |session| {
                        session.live_transcription_error = None;
                    })?;
                    progress(cleared.clone());
                    cleared
                } else {
                    session
                };
                if finished {
                    return Ok(session);
                }
                wait(poll);
            }
            Err(error) => {
                retry_after = if matches!(
                    error.code.as_str(),
                    "rate_limited" | "openai_server" | "openai_timeout" | "openai_network"
                ) {
                    [2, 4, 8]
                        .get(retries)
                        .copied()
                        .or_else(|| {
                            (session.status == SessionStatus::Recording)
                                .then_some(if retries == 3 { 30 } else { 60 })
                        })
                        .map(Duration::from_secs)
                } else {
                    None
                };
                if retry_after.is_some() {
                    retries = (retries + 1).min(4);
                }
                // Do not transition to Failed if Stop wins the race with a retryable error.
                progress(update_processing(state, id, |session| {
                    session.live_transcription_error = Some(error.clone());
                })?);
                paused = Some(error);
            }
        }
    }
}

async fn process_session_with_key(
    state: &AppState,
    id: &str,
    api_key: &str,
    progress: impl Fn(Session),
) -> AppResult<Session> {
    let session = load_session(state, id)?;
    if session.segmented_capture {
        let recovery_error = if session.status != SessionStatus::Recording {
            live::reconcile_segments(state, id).err()
        } else {
            None
        };
        live::cleanup_checkpointed_segments(state, &session)?;
        live::transcribe_available(state, id, api_key, &progress).await?;
        let current = load_session(state, id)?;
        if current.status != SessionStatus::Processing {
            return Ok(current);
        }
        // Stop may have finalized another section during the last upload.
        let stop_recovery_error = live::reconcile_segments(state, id).err();
        live::transcribe_available(state, id, api_key, &progress).await?;
        if let Some(error) = recovery_error.or(stop_recovery_error) {
            return Err(error);
        }
    } else if needs_transcription(&session) || !session.transcription.is_empty() {
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
    if session.segmented_capture {
        update_processing(state, id, |session| {
            for source in [AudioSource::System, AudioSource::Microphone] {
                let Some(track) = session
                    .transcription
                    .iter()
                    .find(|track| track.source == source)
                else {
                    continue;
                };
                if track.chunks.is_empty() {
                    continue;
                }
                let has_speech = track.chunks.iter().any(|chunk| {
                    chunk
                        .transcript
                        .as_deref()
                        .is_some_and(|text| !text.trim().is_empty())
                });
                let warning = format!(
                    "{} had no transcribed speech. Notes use only the available sources.",
                    source.label()
                );
                if has_speech {
                    session.warnings.retain(|saved| saved != &warning);
                } else {
                    add_warning(session, warning);
                }
            }
        })?;
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
    let requested_notes = session.notes();
    let sections = state.openai.enrich(&session, api_key).await?;
    let suggestions = sections.suggestions.clone();
    let omitted_suggestions = sections.omitted_suggestions;
    let notes = sections_to_markdown(sections);
    update_processing(state, id, |session| {
        session.update_enriched_notes(notes, &requested_notes);
        session.ai_suggestions = Some(suggestions);
        let notice = "Some AI suggestions lacked matching source evidence and were omitted. Your notes and verified suggestions were kept.";
        session.warnings.retain(|warning| warning != notice);
        if omitted_suggestions {
            add_warning(session, notice.into());
        }
        session.status = SessionStatus::Complete;
        session.error = None;
        session.live_transcription_error = None;
    })
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
                error: None,
                segment_index: None,
                segments: Vec::new(),
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
                let empty = format!(
                    "{} had no transcribed speech. Notes use only the available sources.",
                    source.label()
                );
                session
                    .warnings
                    .retain(|warning| warning != &resolved && !(has_speech && warning == &empty));
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
        let output = state.store.chunk_path(id, source, session.audio_format)?;
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
        match state
            .openai
            .transcribe_session(&output, api_key, &session)
            .await
        {
            Ok(mut result) => {
                result.retain_valid_speakers(length)?;
                let saved = update_processing(state, id, |session| {
                    if result.omitted_speakers {
                        add_warning(session, SPEAKER_TIMING_NOTICE.into());
                    }
                    let track = session
                        .transcription
                        .iter_mut()
                        .find(|track| track.source == source)
                        .expect("prepared source");
                    track.chunks[index].transcript = Some(result.text);
                    track.chunks[index].segments = result.segments;
                    session.transcript = assembled_transcript(&session.transcription);
                })?;
                // Only the durable checkpoint permits deletion; the full source stays until every chunk is saved.
                remove_audio_file(state.store.chunk_path(id, source, session.audio_format)?)?;
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
                                error: None,
                                segment_index: None,
                                segments: Vec::new(),
                                start_seconds: start,
                                duration_seconds: length / 2.0,
                                transcript: None,
                            },
                            TranscriptChunk {
                                error: None,
                                segment_index: None,
                                segments: Vec::new(),
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
    remove_audio_file(state.store.chunk_path(id, source, session.audio_format)?)?;
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
    session.transcription_settings = state.store.settings()?;
    session.started_at = now();
    session.error = None;
    state.store.save(&session)?;
    Ok(session)
}

fn ensure_processing_idle(state: &AppState, id: &str) -> AppResult<()> {
    if state
        .processing_jobs
        .lock()
        .map_err(|_| invalid_status("Processing is unavailable"))?
        .contains(id)
    {
        return Err(invalid_status(
            "The last request is still finishing. Try again in a moment.",
        ));
    }
    Ok(())
}

fn prepare_retry(state: &AppState, id: &str) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    ensure_processing_idle(state, id)?;
    let mut session = state.store.get(id)?;
    if session.status == SessionStatus::Failed
        && session
            .error
            .as_ref()
            .is_some_and(|error| error.code == "no_speech")
    {
        session.transcription_settings = state.store.settings()?;
        for chunk in session
            .transcription
            .iter_mut()
            .flat_map(|track| &mut track.chunks)
        {
            if chunk
                .transcript
                .as_ref()
                .is_some_and(|text| text.trim().is_empty())
            {
                chunk.transcript = None;
                chunk.segments.clear();
            }
        }
    }
    let session = retry_start(session)?;
    state.store.save(&session)?;
    Ok(session)
}

pub fn retry_start(mut session: Session) -> AppResult<Session> {
    if session.status != SessionStatus::Failed {
        return Err(invalid_status("Only a failed session can be retried"));
    }
    for chunk in session
        .transcription
        .iter_mut()
        .flat_map(|track| &mut track.chunks)
    {
        chunk.error = None;
    }
    for source in [AudioSource::System, AudioSource::Microphone] {
        session
            .warnings
            .retain(|warning| warning != &live::chunk_failure_notice(source));
    }
    session.status = SessionStatus::Processing;
    session.error = None;
    session.live_transcription_error = None;
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
    ensure_processing_idle(state, id)?;
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
    ensure_processing_idle(state, id)?;
    let mut session = state.store.get(id)?;
    if session.segmented_capture && !state.store.segment_files(id, session.audio_format)?.is_empty() {
        // The base path is the retained-source marker; numbered paths remain derived by the store.
        session.audio_path = Some(state.store.audio_path(id, session.audio_format)?.to_string_lossy().into_owned());
    }
    let session = transcript_deleted(session)?;
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
    session.notes = Some(session.notes());
    session.edited_enriched_notes = None;
    session.enriched_notes = None;
    session.ai_suggestions = None;
    session.dismissed_suggestions.clear();
    session.capture_segments.clear();
    session.live_transcription_error = None;
    if session.audio_path.is_some() || session.microphone_audio_path.is_some() {
        session.status = SessionStatus::Failed;
        session.error = Some(AppError::new("transcript_deleted", "Transcript deleted. Your notes and retained audio were kept. Retry will transcribe the remaining audio again."));
    } else {
        session.capture_health = None;
        session.segmented_capture = false;
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
        remove_audio_file(state.store.chunk_path(id, source, session.audio_format)?)?;
    }
    if session.segmented_capture {
        for (_, _, path) in state.store.segment_files(id, session.audio_format)? {
            remove_audio_file(path)?;
        }
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

fn persist_failure(state: &AppState, id: &str, error: AppError) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut failed = state.store.get(id)?;
    if failed.status == SessionStatus::Recording {
        failed.live_transcription_error = Some(error);
    } else {
        failed = transition_to_failed(failed, error);
    }
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
        ) || (session.status == SessionStatus::Draft
            && session.segmented_capture
            && !store.segment_files(&session.id, session.audio_format)?.is_empty())
        {
            let mut session = session;
            if session.segmented_capture {
                session.audio_path = Some(
                    store
                        .audio_path(&session.id, session.audio_format)?
                        .to_string_lossy()
                        .into_owned(),
                );
            }
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
    session.segmented_capture = paths.segmented;
    live::append_segments(&mut session, &paths.segments);
    if let Some(health) = paths.health {
        session.warnings.extend(health.warnings.iter().cloned());
        session.capture_health = Some(health);
    }
    Ok(session)
}

pub fn interrupted_recording(session: Session, paths: RecordingFiles) -> AppResult<Session> {
    stopped_session(session, paths).map(recover_interrupted)
}

fn audio_paths(app: &AppHandle, id: &str, format: AudioFormat) -> AppResult<(PathBuf, PathBuf)> {
    app.path()
        .app_data_dir()
        .map(|root| {
            let audio = root.join("audio");
            (
                audio.join(format!("{id}.{}", format.extension())),
                audio.join(format!("{id}-mic.{}", format.extension())),
            )
        })
        .map_err(|error| AppError::new("storage_error", error.to_string()))
}

fn retained_audio_path(state: &AppState, session: &Session) -> AppResult<Option<PathBuf>> {
    session
        .audio_path
        .as_deref()
        .map(|saved| state.store.validate_audio_path(&session.id, saved, session.audio_format))
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
                .validate_microphone_audio_path(&session.id, saved, session.audio_format)
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
                    segments: vec![],
                    segmented: false,
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
            notes: None,
        }
    }

    #[test]
    fn transcription_failure_does_not_claim_capture_has_stopped() {
        let (state, root, id) = state(SessionStatus::Recording);
        let session =
            super::persist_failure(&state, &id, AppError::new("network", "Offline")).unwrap();
        let saved = state.store.get(&id).unwrap();
        fs::remove_dir_all(root).unwrap();
        assert_eq!(session.status, SessionStatus::Recording);
        assert_eq!(saved.status, SessionStatus::Recording);
        assert!(saved.error.is_none());
        assert_eq!(saved.live_transcription_error.unwrap().code, "network");
    }

    #[test]
    fn worker_recovers_transient_errors_without_losing_saved_checkpoints() {
        use std::cell::{Cell, RefCell};
        for code in [
            "rate_limited",
            "openai_server",
            "openai_timeout",
            "openai_network",
        ] {
            let (state, root, id) = state(SessionStatus::Recording);
            let checkpoint = super::update_processing(&state, &id, |session| {
                session.original_notes = "Keep these notes".into();
                session.transcript = Some("Already saved words".into());
                session.transcription = vec![super::SourceTranscript {
                    source: super::AudioSource::System,
                    chunks: vec![super::TranscriptChunk {
                        error: None,
                        start_seconds: 0.0,
                        duration_seconds: 60.0,
                        transcript: Some("Already saved words".into()),
                        segment_index: None,
                        segments: vec![],
                    }],
                }];
            })
            .unwrap();
            let lease = super::live::ProcessingLease::claim(&state, &id)
                .unwrap()
                .unwrap();
            let attempts = Cell::new(0);
            let waits = RefCell::new(Vec::new());
            let complete = super::run_processing_worker(
                &state,
                &id,
                || {
                    assert!(super::live::ProcessingLease::claim(&state, &id)
                        .unwrap()
                        .is_none());
                    assert_eq!(
                        state.store.get(&id).unwrap().transcription,
                        checkpoint.transcription
                    );
                    attempts.set(attempts.get() + 1);
                    match attempts.get() {
                        1 => Err(AppError::new(code, "Temporary failure")),
                        2 => {
                            assert_eq!(
                                state.store.get(&id).unwrap().status,
                                SessionStatus::Recording
                            );
                            assert!(state
                                .store
                                .get(&id)
                                .unwrap()
                                .live_transcription_error
                                .is_none());
                            super::load_session(&state, &id)
                        }
                        3 => super::update_processing(&state, &id, |session| {
                            session.status = SessionStatus::Complete
                        }),
                        _ => panic!("unexpected extra processing attempt"),
                    }
                },
                &|_| {},
                |delay| {
                    waits.borrow_mut().push(delay);
                    if attempts.get() == 2 {
                        assert!(state
                            .store
                            .get(&id)
                            .unwrap()
                            .live_transcription_error
                            .is_none());
                        super::update_processing(&state, &id, |session| {
                            session.status = SessionStatus::Processing
                        })
                        .unwrap();
                    }
                },
            )
            .unwrap();
            assert_eq!(complete.status, SessionStatus::Complete);
            assert_eq!(complete.transcription, checkpoint.transcription);
            assert_eq!(complete.original_notes, checkpoint.original_notes);
            assert!(complete.live_transcription_error.is_none());
            assert_eq!(
                waits.borrow().iter().copied().sum::<Duration>(),
                Duration::from_millis(2500)
            );
            drop(lease);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn worker_stop_during_backoff_finishes_processing_instead_of_replaying_failure() {
        use std::cell::Cell;
        let (state, root, id) = state(SessionStatus::Recording);
        let attempts = Cell::new(0);
        let waited = Cell::new(Duration::ZERO);
        let complete = super::run_processing_worker(
            &state,
            &id,
            || {
                attempts.set(attempts.get() + 1);
                if attempts.get() == 1 {
                    Err(AppError::new("openai_network", "Offline briefly"))
                } else {
                    assert_eq!(
                        state.store.get(&id).unwrap().status,
                        SessionStatus::Processing
                    );
                    super::update_processing(&state, &id, |session| {
                        session.status = SessionStatus::Complete
                    })
                }
            },
            &|_| {},
            |delay| {
                waited.set(waited.get() + delay);
                super::update_processing(&state, &id, |session| {
                    session.status = SessionStatus::Processing
                })
                .unwrap();
            },
        )
        .unwrap();
        assert_eq!(attempts.get(), 2);
        assert_eq!(waited.get(), Duration::from_millis(500));
        assert_eq!(complete.status, SessionStatus::Complete);
        assert!(complete.live_transcription_error.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn worker_caps_final_retries_and_never_retries_permanent_errors() {
        use std::cell::Cell;
        for (code, expected_attempts, expected_delay) in [
            ("openai_timeout", 4, 14),
            ("invalid_api_key", 1, 0),
            ("quota_exceeded", 1, 0),
            ("model_access", 1, 0),
            ("invalid_transcription", 1, 0),
        ] {
            for initial_status in [SessionStatus::Recording, SessionStatus::Processing] {
                let (state, root, id) = state(initial_status.clone());
                let stop_after = expected_attempts;
                let final_retry =
                    initial_status == SessionStatus::Recording && code == "openai_timeout";
                let expected_attempts = if final_retry {
                    expected_attempts * 2
                } else {
                    expected_attempts
                };
                let expected_delay = if final_retry {
                    expected_delay * 2
                } else {
                    expected_delay
                };
                let attempts = Cell::new(0);
                let waited = Cell::new(Duration::ZERO);
                let error = super::run_processing_worker(
                    &state,
                    &id,
                    || {
                        attempts.set(attempts.get() + 1);
                        assert!(attempts.get() <= expected_attempts, "retry limit exceeded");
                        Err(AppError::new(code, "Request failed"))
                    },
                    &|_| {},
                    |delay| {
                        waited.set(waited.get() + delay);
                        if attempts.get() == stop_after
                            && state.store.get(&id).unwrap().status == SessionStatus::Recording
                        {
                            let saved = state.store.get(&id).unwrap();
                            assert_eq!(saved.status, SessionStatus::Recording);
                            assert_eq!(saved.live_transcription_error.unwrap().code, code);
                            super::update_processing(&state, &id, |session| {
                                session.status = SessionStatus::Processing
                            })
                            .unwrap();
                        }
                    },
                )
                .unwrap_err();
                assert_eq!(error.code, code);
                assert_eq!(attempts.get(), expected_attempts);
                assert_eq!(
                    waited.get(),
                    Duration::from_secs(expected_delay)
                        + if initial_status == SessionStatus::Recording {
                            Duration::from_millis(500)
                        } else {
                            Duration::ZERO
                        }
                );
                fs::remove_dir_all(root).unwrap();
            }
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
    fn product_recording_snapshots_current_settings() {
        let (state, root, id) = state(SessionStatus::Draft);
        let settings = crate::domain::TranscriptionSettings {
            language: "fr".into(),
            vocabulary: "Élodie".into(),
            model: "gpt-4o-transcribe".into(),
        };
        state.store.save_settings(&settings).unwrap();
        let session = super::persist_recording(
            &state,
            &id,
            root.join("audio")
                .join(format!("{id}.m4a"))
                .to_string_lossy()
                .into_owned(),
            root.join("audio")
                .join(format!("{id}-mic.m4a"))
                .to_string_lossy()
                .into_owned(),
        )
        .unwrap();
        state
            .store
            .save_settings(&crate::domain::TranscriptionSettings::default())
            .unwrap();
        assert_eq!(session.transcription_settings, settings);
        assert_eq!(
            state.store.get(&id).unwrap().transcription_settings,
            settings
        );
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

#[cfg(test)]
#[path = "processing_tests.rs"]
mod processing_tests;
