//! Owner: messaging::backend — profile-selected message-log facade
//! Proof: `cargo test -p jeryu --lib messaging::backend`

use std::time::Duration;

use thiserror::Error;

use crate::runtime_support::{self, MessageLogBackend};

#[derive(Debug, Error)]
pub enum MessageLogError {
    #[error("{0}")]
    Config(#[from] runtime_support::RuntimeProfileError),
    #[cfg(feature = "kafka-backend")]
    #[error("{0}")]
    Kafka(#[from] super::kafka::KafkaLogError),
    #[cfg(feature = "jansu-broker")]
    #[error("{0}")]
    Jansu(#[from] super::broker::BrokerError),
    #[error("message log backend {0} is not compiled into this binary")]
    NotCompiled(&'static str),
}

#[derive(Clone, Debug)]
pub struct MessageLogHandle {
    inner: MessageLogInner,
}

#[derive(Clone, Debug)]
enum MessageLogInner {
    #[cfg(feature = "kafka-backend")]
    Kafka(super::kafka::KafkaHandle),
    #[cfg(feature = "jansu-broker")]
    Jansu(super::broker::BrokerHandle),
}

impl MessageLogHandle {
    pub async fn send(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        payload: &[u8],
    ) -> Result<(), MessageLogError> {
        match &self.inner {
            #[cfg(feature = "kafka-backend")]
            MessageLogInner::Kafka(handle) => handle.send(topic, key, payload).await?,
            #[cfg(feature = "jansu-broker")]
            MessageLogInner::Jansu(handle) => handle.send(topic, key, payload).await?,
        }
        Ok(())
    }

    pub fn consumer(&self, topic: &str, start_offset: i64) -> MessageLogConsumer {
        match &self.inner {
            #[cfg(feature = "kafka-backend")]
            MessageLogInner::Kafka(handle) => MessageLogConsumer {
                inner: MessageLogConsumerInner::Kafka(handle.consumer(topic, start_offset)),
            },
            #[cfg(feature = "jansu-broker")]
            MessageLogInner::Jansu(handle) => MessageLogConsumer {
                inner: MessageLogConsumerInner::Jansu(handle.consumer(topic, start_offset)),
            },
        }
    }

    pub fn backend(&self) -> MessageLogBackend {
        match &self.inner {
            #[cfg(feature = "kafka-backend")]
            MessageLogInner::Kafka(_) => MessageLogBackend::Kafka,
            #[cfg(feature = "jansu-broker")]
            MessageLogInner::Jansu(_) => MessageLogBackend::Jansu,
        }
    }
}

pub struct MessageLogConsumer {
    inner: MessageLogConsumerInner,
}

enum MessageLogConsumerInner {
    #[cfg(feature = "kafka-backend")]
    Kafka(super::kafka::KafkaConsumer),
    #[cfg(feature = "jansu-broker")]
    Jansu(super::broker::ConsumerHandle),
}

impl MessageLogConsumer {
    pub fn offset(&self) -> i64 {
        match &self.inner {
            #[cfg(feature = "kafka-backend")]
            MessageLogConsumerInner::Kafka(consumer) => consumer.offset(),
            #[cfg(feature = "jansu-broker")]
            MessageLogConsumerInner::Jansu(consumer) => consumer.offset(),
        }
    }

    pub async fn next_with_timeout(
        &mut self,
        budget: Duration,
    ) -> Result<Option<MessageRecord>, MessageLogError> {
        match &mut self.inner {
            #[cfg(feature = "kafka-backend")]
            MessageLogConsumerInner::Kafka(consumer) => {
                Ok(consumer.next_with_timeout(budget).await?.map(Into::into))
            }
            #[cfg(feature = "jansu-broker")]
            MessageLogConsumerInner::Jansu(consumer) => {
                Ok(consumer.next_with_timeout(budget).await?.map(Into::into))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub key: Option<Vec<u8>>,
    pub payload: Vec<u8>,
    pub offset: i64,
}

#[cfg(feature = "kafka-backend")]
impl From<super::kafka::KafkaRecord> for MessageRecord {
    fn from(record: super::kafka::KafkaRecord) -> Self {
        Self {
            key: record.key,
            payload: record.payload,
            offset: record.offset,
        }
    }
}

#[cfg(feature = "jansu-broker")]
impl From<jansu_embedded::EmbeddedRecord> for MessageRecord {
    fn from(record: jansu_embedded::EmbeddedRecord) -> Self {
        Self {
            key: record.key,
            payload: record.payload,
            offset: record.offset,
        }
    }
}

pub async fn init_message_log() -> Result<MessageLogHandle, MessageLogError> {
    let env_value = std::env::var("JERYU_MESSAGE_LOG_BACKEND").ok();
    match runtime_support::select_message_log_backend(env_value.as_deref())? {
        MessageLogBackend::Kafka => init_kafka().await,
        MessageLogBackend::Jansu => init_jansu().await,
    }
}

pub fn message_log_handle() -> Result<MessageLogHandle, MessageLogError> {
    let env_value = std::env::var("JERYU_MESSAGE_LOG_BACKEND").ok();
    match runtime_support::select_message_log_backend(env_value.as_deref())? {
        MessageLogBackend::Kafka => kafka_handle(),
        MessageLogBackend::Jansu => jansu_handle(),
    }
}

#[cfg(feature = "kafka-backend")]
async fn init_kafka() -> Result<MessageLogHandle, MessageLogError> {
    Ok(MessageLogHandle {
        inner: MessageLogInner::Kafka(super::kafka::init_kafka_log().await?),
    })
}

#[cfg(not(feature = "kafka-backend"))]
async fn init_kafka() -> Result<MessageLogHandle, MessageLogError> {
    Err(MessageLogError::NotCompiled("kafka"))
}

#[cfg(feature = "kafka-backend")]
fn kafka_handle() -> Result<MessageLogHandle, MessageLogError> {
    Ok(MessageLogHandle {
        inner: MessageLogInner::Kafka(super::kafka::kafka_handle()?),
    })
}

#[cfg(not(feature = "kafka-backend"))]
fn kafka_handle() -> Result<MessageLogHandle, MessageLogError> {
    Err(MessageLogError::NotCompiled("kafka"))
}

#[cfg(feature = "jansu-broker")]
async fn init_jansu() -> Result<MessageLogHandle, MessageLogError> {
    Ok(MessageLogHandle {
        inner: MessageLogInner::Jansu(super::broker::init_broker().await?),
    })
}

#[cfg(not(feature = "jansu-broker"))]
async fn init_jansu() -> Result<MessageLogHandle, MessageLogError> {
    Err(MessageLogError::NotCompiled("jansu"))
}

#[cfg(feature = "jansu-broker")]
fn jansu_handle() -> Result<MessageLogHandle, MessageLogError> {
    Ok(MessageLogHandle {
        inner: MessageLogInner::Jansu(super::broker::broker_handle()?),
    })
}

#[cfg(not(feature = "jansu-broker"))]
fn jansu_handle() -> Result<MessageLogHandle, MessageLogError> {
    Err(MessageLogError::NotCompiled("jansu"))
}

#[cfg(test)]
mod tests {
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
}
