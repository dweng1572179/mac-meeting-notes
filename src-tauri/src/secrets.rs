use keyring::Entry;

use crate::domain::{AppError, AppResult};

const SERVICE: &str = "com.dweng.meetingnotes";
const ACCOUNT: &str = "openai-api-key";

pub struct ApiKeyStore;

impl ApiKeyStore {
    pub fn save(key: &str) -> AppResult<()> {
        let key = validate_key_input(key)?;
        entry()?.set_password(key).map_err(keychain_error)
    }

    pub fn remove() -> AppResult<()> {
        remove_entry(&entry()?)
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

pub fn validate_key_input(key: &str) -> AppResult<&str> {
    let trimmed = key.trim();
    if trimmed.is_empty()
        || key.len() > 4096
        || key.chars().any(char::is_control)
        || trimmed.chars().any(char::is_whitespace)
    {
        return Err(AppError::new(
            "invalid_api_key",
            "Enter an API key of at most 4096 bytes without spaces, line breaks, or control characters.",
        ));
    }
    Ok(trimmed)
}

fn remove_entry(entry: &Entry) -> AppResult<()> {
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(keychain_error(error)),
    }
}

fn entry() -> AppResult<Entry> {
    Entry::new(SERVICE, ACCOUNT).map_err(keychain_error)
}

fn keychain_error(error: keyring::Error) -> AppError {
    AppError::new("keychain", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_key_input_rejects_oversize_and_header_controls() {
        for invalid in [
            "".to_owned(),
            " ".into(),
            "x".repeat(4097),
            "key\r\nheader".into(),
            "key\0".into(),
            "key\t".into(),
            "key value".into(),
        ] {
            assert_eq!(
                validate_key_input(&invalid).unwrap_err().code,
                "invalid_api_key"
            );
        }
        assert_eq!(
            validate_key_input("  valid-test-key  ").unwrap(),
            "valid-test-key"
        );
        assert!(validate_key_input(&"x".repeat(4096)).is_ok());
    }

    #[test]
    fn settings_key_removal_is_idempotent_and_keeps_credential_on_failure() {
        let entry = Entry::new_with_credential(Box::<keyring::mock::MockCredential>::default());
        entry.set_password("test-only").unwrap();
        entry
            .get_credential()
            .downcast_ref::<keyring::mock::MockCredential>()
            .unwrap()
            .set_error(keyring::Error::Invalid("test".into(), "unavailable".into()));
        assert_eq!(remove_entry(&entry).unwrap_err().code, "keychain");
        assert_eq!(entry.get_password().unwrap(), "test-only");
        remove_entry(&entry).unwrap();
        remove_entry(&entry).unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
    }
}
