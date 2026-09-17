use super::*;
use crate::domain::AudioSource;

#[test]
fn segment_names_are_derived_from_session_and_source() {
    let base = Path::new("/tmp/session-123.m4a");
    assert_eq!(
        segment_path(base, AudioSource::System, 0).unwrap(),
        PathBuf::from("/tmp/session-123-system-segment-00000000.m4a")
    );
    assert_eq!(
        segment_path(base, AudioSource::Microphone, 42).unwrap(),
        PathBuf::from("/tmp/session-123-mic-segment-00000042.m4a")
    );
    assert!(segment_path(Path::new("/tmp/../bad name.m4a"), AudioSource::System, 0).is_err());
}
