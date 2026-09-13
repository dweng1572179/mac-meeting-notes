use meeting_notes_lib::{
    domain::{CreateSessionInput, Session},
    openai::{build_enrichment_request, sections_to_markdown, EnrichedSections},
};

#[test]
fn enrichment_request_preserves_notes_and_later_decisions() {
    let mut session = Session::new(CreateSessionInput {
        title: "[SIMULATION] Harbor Office".into(),
        context: "Only supplied property facts are valid.".into(),
        attendees: vec!["[SIMULATION] Alex".into()],
    });
    session.original_notes = "rent roll is the issue".into();
    session.transcript = Some(
        "Alex: We should proceed.\nAlex: Later explicit decision: pause until the rent roll is verified."
            .into(),
    );

    let request = build_enrichment_request(&session);
    let encoded = request.to_string();
    assert!(encoded.contains("rent roll is the issue"));
    assert!(encoded.contains("later explicit decision"));
    assert!(encoded.contains("[SIMULATION]"));
    assert_eq!(request["model"], "gpt-6-astra");
    assert_eq!(request["store"], false);
}

#[test]
fn markdown_omits_empty_sections_and_includes_decisions() {
    let markdown = sections_to_markdown(EnrichedSections {
        summary: Vec::new(),
        key_points: Vec::new(),
        decisions: vec!["Pause until the rent roll is verified.".into()],
        action_items: Vec::new(),
    });

    assert_eq!(markdown, "## Decisions\n\n- Pause until the rent roll is verified.\n");
}
