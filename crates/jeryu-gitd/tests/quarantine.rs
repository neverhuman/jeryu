use jeryu_gitd::quarantine::QuarantineEnv;

#[test]
fn quarantine_env_detects_missing_receive_context() {
    let q = QuarantineEnv {
        object_directory: None,
        alternate_object_directories: None,
    };
    assert!(q.require_for_receive().is_err());
}
