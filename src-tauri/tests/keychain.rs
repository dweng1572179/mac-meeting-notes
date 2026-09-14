#![cfg(target_os = "macos")]

#[link(name = "Security", kind = "framework")]
extern "C" {
    fn SecKeychainSetUserInteractionAllowed(allowed: bool) -> i32;
}

#[test]
fn disposable_credential_survives_fresh_entries() {
    struct Credential {
        service: String,
        account: String,
    }
    impl Drop for Credential {
        fn drop(&mut self) {
            let _ = keyring::Entry::new(&self.service, &self.account)
                .and_then(|entry| entry.delete_credential());
            // SAFETY: restore the test process's normal Keychain interaction policy.
            unsafe { SecKeychainSetUserInteractionAllowed(true) };
        }
    }

    // SAFETY: prevents this disposable integration test from opening a permission prompt.
    assert_eq!(unsafe { SecKeychainSetUserInteractionAllowed(false) }, 0);
    let credential = Credential {
        service: format!("meeting-notes-disposable-test-{}", uuid::Uuid::new_v4()),
        account: uuid::Uuid::new_v4().to_string(),
    };
    let value = uuid::Uuid::new_v4().to_string();
    let fresh = || keyring::Entry::new(&credential.service, &credential.account).unwrap();
    fresh().set_password(&value).unwrap();
    let loaded = fresh().get_password();
    let deleted = fresh().delete_credential();
    let absent = fresh().get_password();
    assert_eq!(loaded.unwrap(), value);
    deleted.unwrap();
    assert!(matches!(absent, Err(keyring::Error::NoEntry)));
}
