use super::*;
use crate::{domain::CaptureSegment, recorder::CapturedSegment};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

const SECTION_INTERVAL: Duration = Duration::from_secs(60);

pub(super) struct ProcessingLease {
    jobs: Arc<Mutex<HashSet<String>>>,
    id: String,
}

impl ProcessingLease {
    pub(super) fn claim(state: &AppState, id: &str) -> AppResult<Option<Self>> {
        let mut jobs = state
            .processing_jobs
            .lock()
            .map_err(|_| storage_error(std::io::Error::other("Processing is unavailable")))?;
        if !jobs.insert(id.to_owned()) {
            return Ok(None);
        }
        Ok(Some(Self {
            jobs: state.processing_jobs.clone(),
            id: id.to_owned(),
        }))
    }
}

impl Drop for ProcessingLease {
    fn drop(&mut self) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(&self.id);
        }
    }
}

pub(super) fn spawn_capture_supervisor(app: AppHandle, id: String) {
    // The control task owns rotation; a slow HTTP request never delays file finalization.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut next = Instant::now() + SECTION_INTERVAL;
        loop {
            std::thread::sleep(Duration::from_millis(500));
            if !matches!(load_session(&state, &id), Ok(session) if session.status == SessionStatus::Recording)
            {
                break;
            }
            if Instant::now() < next {
                continue;
            }
            next = Instant::now() + SECTION_INTERVAL;
            let result = state
                .recorder
                .rotate(&id)
                .and_then(|segments| register_segments(&state, &id, &segments));
            match result {
                Ok(session) => {
                    let _ = app.emit(SESSION_UPDATED, session);
                }
                Err(error) => {
                    // Stop can win the recorder mutex between the status check and rotation.
                    if matches!(load_session(&state, &id), Ok(session) if session.status == SessionStatus::Recording)
                    {
                        if let Ok(session) = update_processing(&state, &id, |session| {
                            add_warning(session, error.message)
                        }) {
                            let _ = app.emit(SESSION_UPDATED, session);
                        }
                    }
                }
            }
        }
    });
}

pub(super) fn append_segments(session: &mut Session, segments: &[CapturedSegment]) {
    for segment in segments {
        if session
            .capture_segments
            .iter()
            .any(|saved| saved.source == segment.source && saved.index == segment.index)
        {
            continue;
        }
        session.capture_segments.push(CaptureSegment {
            source: segment.source,
            index: segment.index,
            start_seconds: segment.start_seconds,
            duration_seconds: segment.duration_seconds,
        });
        let position = session
            .transcription
            .iter()
            .position(|track| track.source == segment.source)
            .unwrap_or_else(|| {
                session.transcription.push(SourceTranscript {
                    source: segment.source,
                    chunks: Vec::new(),
                });
                session.transcription.len() - 1
            });
        let track = &mut session.transcription[position];
        let mut offset = 0.0;
        while offset < segment.duration_seconds - 0.000_001 {
            let duration = (segment.duration_seconds - offset).min(300.0);
            track.chunks.push(TranscriptChunk {
                error: None,
                start_seconds: segment.start_seconds + offset,
                duration_seconds: duration,
                transcript: None,
                segment_index: Some(segment.index),
                segments: Vec::new(),
            });
            offset += duration;
        }
        track
            .chunks
            .sort_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds));
    }
    session
        .capture_segments
        .sort_by_key(|segment| (segment.source.filename(), segment.index));
}

pub(super) fn register_segments(
    state: &AppState,
    id: &str,
    segments: &[CapturedSegment],
) -> AppResult<Session> {
    for segment in segments {
        if state
            .store
            .segment_path(id, segment.source, segment.index)?
            != segment.path
        {
            return Err(AppError::new(
                "invalid_audio_path",
                "Captured section path is invalid. Audio was kept.",
            ));
        }
    }
    update_processing(state, id, |session| append_segments(session, segments))
}

// Call only after capture has stopped: unregistered files may still be open during recording.
pub(super) fn reconcile_segments(state: &AppState, id: &str) -> AppResult<Session> {
    let mut session = load_session(state, id)?;
    let mut first_error = None;
    for (source, index, path) in state.store.segment_files(id)? {
        if session
            .capture_segments
            .iter()
            .any(|segment| segment.source == source && segment.index == index)
        {
            continue;
        }
        match crate::audio::inspect(&path) {
            Ok(info) if info.frames > 0 => {
                let duration = info.frames as f64 / info.sample_rate;
                let start = session
                    .capture_segments
                    .iter()
                    .filter(|segment| segment.source == source && segment.index < index)
                    .map(|segment| segment.start_seconds + segment.duration_seconds)
                    .fold(0.0, f64::max);
                let recovered = CapturedSegment {
                    source,
                    index,
                    start_seconds: start,
                    duration_seconds: duration,
                    path,
                };
                register_segments(state, id, &[recovered])?;
                session = update_processing(state, id, |session| {
                    add_warning(session,
                    "Recovered a finalized audio section after interruption. Source timing may include a capture gap.".into())
                })?;
            }
            Ok(_) => {
                session = update_processing(state, id, |session| {
                    add_warning(
                        session,
                        format!(
                            "{} contained an empty audio section. Notes use available speech.",
                            source.label()
                        ),
                    )
                })?;
            }
            Err(_) => {
                let error = AppError::new("audio_chunk", format!("{} has an unfinished audio section that could not be read. That file and your saved transcript were kept.", source.label()));
                session = update_processing(state, id, |session| {
                    add_warning(session, error.message.clone())
                })?;
                first_error.get_or_insert(error);
            }
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(session),
    }
}

pub(super) async fn transcribe_available(
    state: &AppState,
    id: &str,
    api_key: &str,
    progress: &impl Fn(Session),
) -> AppResult<()> {
    let mut first_error = load_session(state, id)?
        .transcription
        .iter()
        .flat_map(|track| &track.chunks)
        .find_map(|chunk| chunk.error.clone());
    loop {
        let session = load_session(state, id)?;
        let pending = session
            .transcription
            .iter()
            .flat_map(|track| {
                track
                    .chunks
                    .iter()
                    .enumerate()
                    .filter(|(_, chunk)| chunk.transcript.is_none() && chunk.error.is_none())
                    .map(move |(_, chunk)| (track.source, chunk.clone()))
            })
            .min_by(|(_, a), (_, b)| a.start_seconds.total_cmp(&b.start_seconds));
        let Some((source, chunk)) = pending else {
            return if session.status == SessionStatus::Recording {
                Ok(())
            } else {
                first_error.map_or(Ok(()), Err)
            };
        };
        let segment_index = chunk.segment_index.ok_or_else(|| {
            AppError::new(
                "invalid_transcription",
                "Captured section reference is missing",
            )
        })?;
        let section = session
            .capture_segments
            .iter()
            .find(|segment| segment.source == source && segment.index == segment_index)
            .ok_or_else(|| {
                AppError::new(
                    "invalid_transcription",
                    "Captured section metadata is missing",
                )
            })?;
        let input = state.store.segment_path(id, source, segment_index)?;
        let output = state.store.chunk_path(id, source)?;
        let start = chunk.start_seconds - section.start_seconds;
        let length = chunk.duration_seconds;
        remove_audio_file(output.clone())?;
        let scratch = output.clone();
        let prepared = tauri::async_runtime::spawn_blocking(move || {
            crate::audio::write_chunk(&input, &scratch, start, length)
        })
        .await
        .map_err(|_| {
            AppError::new(
                "audio_chunk",
                "Audio preparation was interrupted; captured sections were kept",
            )
        })?;
        if let Err(error) = prepared {
            progress(checkpoint_chunk_error(state, id, source, &chunk, &error)?);
            first_error.get_or_insert(error);
            continue;
        }
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
                        .expect("saved source");
                    let saved = track
                        .chunks
                        .iter_mut()
                        .find(|saved| same_chunk(saved, &chunk))
                        .expect("pending section retained");
                    saved.transcript = Some(result.text);
                    saved.segments = result.segments;
                    session.transcript = assembled_transcript(&session.transcription);
                })?;
                remove_audio_file(output)?;
                cleanup_checkpointed_segments(state, &saved)?;
                progress(saved);
            }
            Err(error) if error.code == "transcription_too_long" && length > 30.0 => {
                progress(update_processing(state, id, |session| {
                    let track = session
                        .transcription
                        .iter_mut()
                        .find(|track| track.source == source)
                        .expect("saved source");
                    let index = track
                        .chunks
                        .iter()
                        .position(|saved| same_chunk(saved, &chunk))
                        .expect("pending section retained");
                    track.chunks.splice(
                        index..=index,
                        [
                            TranscriptChunk {
                                error: None,
                                duration_seconds: length / 2.0,
                                ..chunk.clone()
                            },
                            TranscriptChunk {
                                error: None,
                                start_seconds: chunk.start_seconds + length / 2.0,
                                duration_seconds: length / 2.0,
                                ..chunk
                            },
                        ],
                    );
                })?);
            }
            Err(error)
                if matches!(
                    error.code.as_str(),
                    "invalid_audio" | "transcription_too_long" | "invalid_transcription"
                ) =>
            {
                progress(checkpoint_chunk_error(state, id, source, &chunk, &error)?);
                remove_audio_file(output)?;
                first_error.get_or_insert(error);
            }
            Err(error) => return Err(error),
        }
    }
}

pub(super) fn chunk_failure_notice(source: AudioSource) -> String {
    format!("{} has a section that could not be transcribed. That audio was kept; other sections can still be transcribed.", source.label())
}

fn checkpoint_chunk_error(
    state: &AppState,
    id: &str,
    source: AudioSource,
    chunk: &TranscriptChunk,
    error: &AppError,
) -> AppResult<Session> {
    update_processing(state, id, |session| {
        let failed = session
            .transcription
            .iter_mut()
            .find(|track| track.source == source)
            .and_then(|track| {
                track
                    .chunks
                    .iter_mut()
                    .find(|saved| same_chunk(saved, chunk))
            })
            .expect("pending section retained");
        failed.error = Some(error.clone());
        add_warning(session, chunk_failure_notice(source));
    })
}

fn same_chunk(left: &TranscriptChunk, right: &TranscriptChunk) -> bool {
    left.segment_index == right.segment_index
        && left.start_seconds == right.start_seconds
        && left.duration_seconds == right.duration_seconds
}

pub(super) fn cleanup_checkpointed_segments(state: &AppState, session: &Session) -> AppResult<()> {
    for segment in &session.capture_segments {
        let chunks: Vec<_> = session
            .transcription
            .iter()
            .filter(|track| track.source == segment.source)
            .flat_map(|track| &track.chunks)
            .filter(|chunk| chunk.segment_index == Some(segment.index))
            .collect();
        if !chunks.is_empty()
            && chunks.iter().all(|chunk| chunk.transcript.is_some())
            && chunks.iter().any(|chunk| {
                chunk
                    .transcript
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty())
            })
        {
            remove_audio_file(state.store.segment_path(
                &session.id,
                segment.source,
                segment.index,
            )?)?;
        }
    }
    Ok(())
}
