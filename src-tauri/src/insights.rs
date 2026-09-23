use super::*;

#[tauri::command]
pub fn apply_suggestion(
    state: State<'_, AppState>,
    id: String,
    key: String,
    action: String,
) -> AppResult<Session> {
    apply_suggestion_state(&state, &id, &key, &action)
}

#[tauri::command]
pub fn refresh_insights(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Session> {
    let session = prepare_refresh(&state, &id)?;
    super::spawn_processing(app, id);
    Ok(session)
}

#[tauri::command]
pub fn restore_notes(
    state: State<'_, AppState>,
    id: String,
    expected_notes: String,
) -> AppResult<Session> {
    restore_notes_state(&state, &id, &expected_notes)
}

fn restore_notes_state(state: &AppState, id: &str, expected_notes: &str) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    if matches!(
        session.status,
        SessionStatus::Recording | SessionStatus::Processing
    ) {
        return Err(invalid_status(
            "Wait until recording and processing finish before restoring notes",
        ));
    }
    let current = session.notes();
    if current != expected_notes {
        return Err(AppError::new(
            "notes_changed",
            "Your notes changed. Review the current document before restoring.",
        ));
    }
    let previous = session.previous_notes.take().ok_or_else(|| {
        AppError::new(
            "no_previous_notes",
            "There is no previous notes document to restore",
        )
    })?;
    session.notes = Some(previous);
    session.previous_notes = Some(current);
    state.store.save(&session)?;
    Ok(session)
}

fn apply_suggestion_state(
    state: &AppState,
    id: &str,
    key: &str,
    action: &str,
) -> AppResult<Session> {
    let invalid = || {
        AppError::new("invalid_suggestion", "Choose an available title, context, category, or participants suggestion and apply or dismiss it")
    };
    if !matches!(key, "title" | "context" | "category" | "participants")
        || !matches!(action, "apply" | "dismiss")
    {
        return Err(invalid());
    }
    let _guard = lock_sessions(state)?;
    let mut session = state.store.get(id)?;
    let suggestions = session.ai_suggestions.as_ref().ok_or_else(invalid)?;
    let valid_text =
        |value: &str, limit: usize| !value.trim().is_empty() && value.chars().count() <= limit;
    if key == "participants" {
        if suggestions.participants.is_empty()
            || suggestions.participants.len() > 50
            || suggestions
                .participants
                .iter()
                .any(|item| !valid_text(&item.name, 120))
        {
            return Err(invalid());
        }
        if action == "apply" {
            for participant in &suggestions.participants {
                let name = participant.name.trim();
                if !session
                    .attendees
                    .iter()
                    .any(|existing| existing.trim().to_lowercase() == name.to_lowercase())
                {
                    session.attendees.push(name.to_owned());
                }
            }
        }
    } else {
        let (suggestion, limit) = match key {
            "title" => (suggestions.title.as_ref(), 160),
            "context" => (suggestions.context.as_ref(), 2000),
            "category" => (suggestions.category.as_ref(), 100),
            _ => unreachable!(),
        };
        let suggestion = suggestion.ok_or_else(invalid)?;
        if !valid_text(&suggestion.value, limit) {
            return Err(invalid());
        }
        if action == "apply" {
            let value = suggestion.value.trim().to_owned();
            match key {
                "title" => session.title = value,
                "context" => session.context = value,
                "category" => session.folder = value,
                _ => unreachable!(),
            }
        }
    }
    if !session
        .dismissed_suggestions
        .iter()
        .any(|dismissed| dismissed == key)
    {
        session.dismissed_suggestions.push(key.to_owned());
    }
    state.store.save(&session)?;
    Ok(session)
}

fn prepare_refresh(state: &AppState, id: &str) -> AppResult<Session> {
    let _guard = lock_sessions(state)?;
    let jobs = state.processing_jobs.lock().map_err(|_| {
        AppError::new(
            "processing_unavailable",
            "Meeting processing is unavailable",
        )
    })?;
    if jobs.contains(id) {
        return Err(invalid_status("This meeting is still processing"));
    }
    let mut session = state.store.get(id)?;
    if session.status != SessionStatus::Complete {
        return Err(invalid_status(
            "Only a completed meeting can refresh its insights",
        ));
    }
    if !session
        .transcript
        .as_ref()
        .is_some_and(|text| !text.trim().is_empty())
    {
        return Err(AppError::new(
            "no_meeting_sources",
            "A retained transcript is required to refresh insights",
        ));
    }
    session.status = SessionStatus::Processing;
    session.error = None;
    state.store.save(&session)?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AiSuggestions, ParticipantSuggestion, Suggestion};

    struct Fixture {
        root: PathBuf,
        state: AppState,
        id: String,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("meeting-insights-{}", uuid::Uuid::new_v4()));
            let state = AppState::new(SessionStore::new(root.clone()));
            let mut session = Session::new(CreateSessionInput {
                title: "Manual title".into(),
                context: "Manual context".into(),
                attendees: vec!["MAYA".into()],
            });
            session.status = SessionStatus::Complete;
            session.original_notes = "My untouched notes".into();
            session.enriched_notes = Some("AI baseline".into());
            session.transcript = Some("Saved transcript".into());
            session.ai_suggestions = Some(AiSuggestions {
                title: Some(Suggestion {
                    value: "Suggested title".into(),
                    evidence: vec![],
                }),
                context: Some(Suggestion {
                    value: "Suggested context".into(),
                    evidence: vec![],
                }),
                category: Some(Suggestion {
                    value: "Review".into(),
                    evidence: vec![],
                }),
                participants: vec![
                    ParticipantSuggestion {
                        name: "Maya".into(),
                        speaker_key: None,
                        evidence: vec![],
                    },
                    ParticipantSuggestion {
                        name: "Alex".into(),
                        speaker_key: None,
                        evidence: vec![],
                    },
                    ParticipantSuggestion {
                        name: "alex".into(),
                        speaker_key: None,
                        evidence: vec![],
                    },
                ],
                topics: vec![],
            });
            state.store.save(&session).unwrap();
            Self {
                root,
                state,
                id: session.id,
            }
        }
        fn saved(&self) -> Session {
            self.state.store.get(&self.id).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn selected_suggestion_changes_only_that_manual_field_and_persists_dismissal() {
        let fixture = Fixture::new();
        let saved = apply_suggestion_state(&fixture.state, &fixture.id, "title", "apply").unwrap();
        assert_eq!(saved.title, "Suggested title");
        assert_eq!(saved.context, "Manual context");
        assert_eq!(saved.original_notes, "My untouched notes");
        assert_eq!(saved.enriched_notes.as_deref(), Some("AI baseline"));
        assert_eq!(saved.dismissed_suggestions, vec!["title"]);
        let dismissed =
            apply_suggestion_state(&fixture.state, &fixture.id, "context", "dismiss").unwrap();
        assert_eq!(dismissed.context, "Manual context");
        assert!(dismissed.dismissed_suggestions.contains(&"context".into()));
        let merged =
            apply_suggestion_state(&fixture.state, &fixture.id, "participants", "apply").unwrap();
        assert_eq!(merged.attendees, vec!["MAYA", "Alex"]);
        assert_eq!(fixture.saved(), merged);
    }

    #[test]
    fn invalid_suggestion_requests_do_not_mutate_saved_meeting() {
        let fixture = Fixture::new();
        let original = fixture.saved();
        for (key, action) in [("unknown", "apply"), ("title", "unknown")] {
            assert!(apply_suggestion_state(&fixture.state, &fixture.id, key, action).is_err());
            assert_eq!(fixture.saved(), original);
        }
        let mut empty = original;
        empty.ai_suggestions = None;
        fixture.state.store.save(&empty).unwrap();
        assert!(apply_suggestion_state(&fixture.state, &fixture.id, "title", "apply").is_err());
        assert_eq!(fixture.saved(), empty);
    }

    #[test]
    fn refresh_preserves_notes_edits_and_dismissals_and_rejects_duplicate_start() {
        let fixture = Fixture::new();
        let mut original = fixture.saved();
        original.edited_enriched_notes = Some("My revised AI notes".into());
        original.dismissed_suggestions = vec!["title".into()];
        fixture.state.store.save(&original).unwrap();
        let refreshed = prepare_refresh(&fixture.state, &fixture.id).unwrap();
        assert_eq!(refreshed.status, SessionStatus::Processing);
        assert_eq!(
            refreshed.edited_enriched_notes,
            original.edited_enriched_notes
        );
        assert_eq!(
            refreshed.dismissed_suggestions,
            original.dismissed_suggestions
        );
        assert_eq!(refreshed.original_notes, original.original_notes);
        assert_eq!(refreshed.enriched_notes, original.enriched_notes);
        assert_eq!(refreshed.ai_suggestions, original.ai_suggestions);
        assert!(prepare_refresh(&fixture.state, &fixture.id).is_err());
        assert_eq!(fixture.saved(), refreshed);
    }
    #[test]
    fn active_job_and_missing_transcript_block_refresh_without_changes() {
        let fixture = Fixture::new();
        let original = fixture.saved();
        fixture
            .state
            .processing_jobs
            .lock()
            .unwrap()
            .insert(fixture.id.clone());
        assert!(prepare_refresh(&fixture.state, &fixture.id).is_err());
        assert_eq!(fixture.saved(), original);
        fixture
            .state
            .processing_jobs
            .lock()
            .unwrap()
            .remove(&fixture.id);
        let mut empty = original;
        empty.transcript = Some(" \n".into());
        fixture.state.store.save(&empty).unwrap();
        assert!(prepare_refresh(&fixture.state, &fixture.id).is_err());
        assert_eq!(fixture.saved(), empty);
    }

    #[test]
    fn oversized_suggestions_leave_previous_values_intact() {
        let fixture = Fixture::new();
        let original = fixture.saved();
        let mut invalid = original;
        invalid
            .ai_suggestions
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .value = "x".repeat(161);
        fixture.state.store.save(&invalid).unwrap();
        assert!(apply_suggestion_state(&fixture.state, &fixture.id, "title", "apply").is_err());
        assert_eq!(fixture.saved(), invalid);
    }

    #[test]
    fn restore_swaps_documents_durably_without_changing_meeting_data() {
        let fixture = Fixture::new();
        let mut original = fixture.saved();
        original.notes = Some("My current refinements".into());
        original.previous_notes = Some(String::new());
        fixture.state.store.save(&original).unwrap();
        let restored =
            restore_notes_state(&fixture.state, &fixture.id, "My current refinements").unwrap();
        let mut expected = original.clone();
        expected.notes = Some(String::new());
        expected.previous_notes = Some("My current refinements".into());
        assert_eq!(fixture.saved(), expected);
        assert_eq!(restored, expected);
        assert_eq!(
            restore_notes_state(&fixture.state, &fixture.id, "").unwrap(),
            original
        );
    }

    #[test]
    fn restore_rejects_stale_missing_and_active_documents_without_mutation() {
        let fixture = Fixture::new();
        let original = fixture.saved();
        assert!(restore_notes_state(&fixture.state, &fixture.id, &original.notes()).is_err());
        assert_eq!(fixture.saved(), original);
        let mut saved = original;
        saved.notes = Some("Current".into());
        saved.previous_notes = Some("Previous".into());
        fixture.state.store.save(&saved).unwrap();
        assert_eq!(
            restore_notes_state(&fixture.state, &fixture.id, "Stale")
                .unwrap_err()
                .code,
            "notes_changed"
        );
        assert_eq!(fixture.saved(), saved);
        for status in [SessionStatus::Recording, SessionStatus::Processing] {
            saved.status = status;
            fixture.state.store.save(&saved).unwrap();
            assert!(restore_notes_state(&fixture.state, &fixture.id, "Current").is_err());
            assert_eq!(fixture.saved(), saved);
        }
    }

    #[test]
    fn failed_restore_checkpoint_keeps_both_saved_documents() {
        let fixture = Fixture::new();
        let mut saved = fixture.saved();
        saved.notes = Some("Current".into());
        saved.previous_notes = Some("Previous".into());
        fixture.state.store.save(&saved).unwrap();
        // An invalid temporary destination makes atomic_write fail on both platforms.
        let blocked = fixture
            .root
            .join("sessions")
            .join(format!("{}.json.tmp", fixture.id));
        fs::create_dir(&blocked).unwrap();
        assert!(restore_notes_state(&fixture.state, &fixture.id, "Current").is_err());
        assert_eq!(fixture.saved(), saved);
    }
}
