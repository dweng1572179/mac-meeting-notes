pub mod audio;
pub mod commands;
pub mod domain;
pub mod openai;
pub mod recorder;
pub mod secrets;
pub mod store;

pub const APP_NAME: &str = "Meeting Notes";
pub const BUNDLE_ID: &str = "com.dweng.meetingnotes";
#[cfg(target_os = "macos")]
const SAFE_QUIT_MENU_ID: &str = "quit-after-save";

pub fn build_app() -> tauri::Builder<tauri::Wry> {
    let builder = tauri::Builder::default()
        .setup(commands::setup)
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::create_session,
            commands::save_session,
            commands::insights::save_ai_notes,
            commands::insights::apply_suggestion,
            commands::insights::refresh_insights,
            commands::start_recording,
            commands::stop_recording,
            commands::recording_health,
            commands::retry_processing,
            commands::delete_session,
            commands::delete_transcript,
            commands::save_api_key,
            commands::save_transcription_settings,
            commands::export_markdown,
            commands::has_api_key,
            commands::ask_meetings,
        ]);

    #[cfg(target_os = "macos")]
    let builder = builder.menu(safe_macos_menu).on_menu_event(|app, event| {
        use tauri::Manager;

        if event.id() == SAFE_QUIT_MENU_ID {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.close();
            } else {
                app.exit(0);
            }
        }
    });

    builder
}

#[cfg(target_os = "macos")]
fn safe_macos_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem};

    let menu = Menu::default(app)?;
    let app_menu = menu
        .items()?
        .into_iter()
        .next()
        .and_then(|item| item.as_submenu().cloned())
        .ok_or_else(|| std::io::Error::other("macOS application menu is unavailable"))?;
    let quit_position = app_menu
        .items()?
        .iter()
        .position(|item| {
            item.as_predefined_menuitem()
                .and_then(|item| item.text().ok())
                .is_some_and(|text| text.starts_with("Quit "))
        })
        .ok_or_else(|| std::io::Error::other("macOS Quit menu item is unavailable"))?;
    app_menu.remove_at(quit_position)?;
    app_menu.insert(
        &MenuItem::with_id(
            app,
            SAFE_QUIT_MENU_ID,
            format!("Quit {}", app.package_info().name),
            true,
            Some("CmdOrCtrl+Q"),
        )?,
        quit_position,
    )?;
    Ok(menu)
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
