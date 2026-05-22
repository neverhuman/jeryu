use super::*;
#[cfg(feature = "jansu-broker")]
use std::time::Duration;

#[tokio::test]
async fn default_message_log_backend_is_kafka() {
    let handle = init_message_log().await.expect("message log");
    let expected = if cfg!(all(
        feature = "profile-redlinedb-jansu",
        not(feature = "profile-sqlite-kafka")
    )) {
        MessageLogBackend::Jansu
    } else {
        MessageLogBackend::Kafka
    };
    assert_eq!(handle.backend(), expected);
}

#[cfg(feature = "jansu-broker")]
#[tokio::test]
async fn jansu_message_log_round_trips_payload() {
    let handle = init_jansu().await.expect("jansu log");
    handle
        .send(
            super::super::topics::JOBS,
            Some(b"jansu-key"),
            b"jansu-payload",
        )
        .await
        .expect("send");

    let mut consumer = handle.consumer(super::super::topics::JOBS, 0);
    let record = tokio::time::timeout(
        Duration::from_secs(2),
        consumer.next_with_timeout(Duration::from_millis(100)),
    )
    .await
    .expect("record timeout")
    .expect("consume")
    .expect("record");
    assert_eq!(record.key.as_deref(), Some(&b"jansu-key"[..]));
    assert_eq!(record.payload, b"jansu-payload");
}
