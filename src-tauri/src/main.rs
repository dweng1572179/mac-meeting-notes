#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let app = meeting_notes_lib::build_app()
        .build(tauri::generate_context!())
        .expect("error while building Meeting Notes");
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            if let Err(error) = meeting_notes_lib::commands::stop_recording_on_exit(app) {
                eprintln!("Could not preserve the active recording: {}", error.message);
            }
        }
    });
}
