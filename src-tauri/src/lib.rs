pub mod commands;
pub mod domain;
pub mod openai;
pub mod recorder;
pub mod secrets;
pub mod store;

pub const APP_NAME: &str = "Meeting Notes";
pub const BUNDLE_ID: &str = "com.dweng.meetingnotes";

pub fn build_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(commands::setup)
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::create_session,
            commands::save_session,
            commands::start_recording,
            commands::stop_recording,
            commands::retry_processing,
            commands::delete_session,
            commands::delete_transcript,
            commands::save_api_key,
            commands::has_api_key,
        ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_identity_is_stable() {
        assert_eq!(APP_NAME, "Meeting Notes");
        assert_eq!(BUNDLE_ID, "com.dweng.meetingnotes");
    }
}
