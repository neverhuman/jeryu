use super::*;

#[test]
fn config_parser_default_profile_is_kafka() {
    assert_eq!(
        RuntimeProfile::compiled().message_log_backend(),
        if cfg!(all(
            feature = "profile-redlinedb-jansu",
            not(feature = "profile-sqlite-kafka")
        )) {
            MessageLogBackend::Jansu
        } else {
            MessageLogBackend::Kafka
        }
    );
}

#[test]
fn parses_runtime_vocabulary() {
    assert_eq!(StateBackend::parse("sqlite").unwrap(), StateBackend::Sqlite);
    assert_eq!(
        StateBackend::parse("redline").unwrap(),
        StateBackend::RedlineDb
    );
    assert_eq!(
        MessageLogBackend::parse("kafka").unwrap(),
        MessageLogBackend::Kafka
    );
    assert_eq!(
        RuntimeProfile::parse("redlinedb-jansu").unwrap(),
        RuntimeProfile::RedlineDbJansu
    );
}
