#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    meeting_notes_lib::build_app()
        .run(tauri::generate_context!())
        .expect("error while running Meeting Notes");
}
