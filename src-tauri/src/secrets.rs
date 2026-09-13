use keyring::Entry;

use crate::domain::{AppError, AppResult};

const SERVICE: &str = "com.dweng.meetingnotes";
const ACCOUNT: &str = "openai-api-key";

pub struct ApiKeyStore;

impl ApiKeyStore {
    pub fn save(key: &str) -> AppResult<()> {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::new("invalid_api_key", "API key is required"));
        }
        entry()?.set_password(key).map_err(keychain_error)
    }

    pub fn load() -> AppResult<Option<String>> {
        match entry()?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keychain_error(error)),
        }
    }

    pub fn exists() -> AppResult<bool> {
        Self::load().map(|key| key.is_some())
    }
}

fn entry() -> AppResult<Entry> {
    Entry::new(SERVICE, ACCOUNT).map_err(keychain_error)
}

fn keychain_error(error: keyring::Error) -> AppError {
    AppError::new("keychain", error.to_string())
}
