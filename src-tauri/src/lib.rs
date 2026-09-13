pub mod domain;
pub mod openai;
pub mod secrets;
pub mod store;

pub const APP_NAME: &str = "Meeting Notes";
pub const BUNDLE_ID: &str = "com.dweng.meetingnotes";

pub fn build_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
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
