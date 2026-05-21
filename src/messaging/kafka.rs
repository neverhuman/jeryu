//! Owner: messaging::kafka — Kafka-profile topic log and config surface
//! Proof: `cargo test -p jeryu --lib messaging::kafka`
//! Invariants: topic names match `topics::ALL`; live Kafka is opt-in by env.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use thiserror::Error;
use tracing::info;

use super::topics;

#[derive(Debug, Error)]
pub enum KafkaLogError {
    #[error("kafka log not initialized — call messaging::init_message_log() at startup")]
    NotInitialized,
    #[error("topic registry mutex poisoned")]
    TopicRegistryPoisoned,
    #[error("topic log mutex poisoned")]
    TopicLogPoisoned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KafkaConfig {
    pub bootstrap_servers: String,
    pub client_id: String,
}

impl KafkaConfig {
    pub fn from_env() -> Self {
        Self {
            bootstrap_servers: std::env::var("JERYU_KAFKA_BOOTSTRAP_SERVERS")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "localhost:9092".to_string()),
            client_id: std::env::var("JERYU_KAFKA_CLIENT_ID")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "jeryu".to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct KafkaHandle {
    inner: Arc<KafkaLog>,
}

impl KafkaHandle {
    pub async fn send(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        payload: &[u8],
    ) -> Result<(), KafkaLogError> {
        self.inner.send(topic, key, payload)
    }

    pub fn consumer(&self, topic: &str, start_offset: i64) -> KafkaConsumer {
        KafkaConsumer {
            topic: topic.to_string(),
            offset: start_offset.max(0),
            inner: Arc::clone(&self.inner),
        }
    }
}

#[derive(Debug)]
pub struct KafkaConsumer {
    topic: String,
    offset: i64,
    inner: Arc<KafkaLog>,
}

impl KafkaConsumer {
    pub fn offset(&self) -> i64 {
        self.offset
    }

    pub async fn next_with_timeout(
        &mut self,
        budget: std::time::Duration,
    ) -> Result<Option<KafkaRecord>, KafkaLogError> {
        let deadline = std::time::Instant::now() + budget;
        loop {
            if let Some(record) = self.inner.fetch(&self.topic, self.offset)? {
                self.offset = record.offset + 1;
                return Ok(Some(record));
            }
            if std::time::Instant::now() >= deadline {
                return Ok(None);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

#[derive(Debug, Clone)]
pub struct KafkaRecord {
    pub key: Option<Vec<u8>>,
    pub payload: Vec<u8>,
    pub offset: i64,
}

#[derive(Debug, Default)]
struct KafkaLog {
    topics: Mutex<HashMap<String, Vec<KafkaRecord>>>,
    known_topics: Mutex<HashSet<String>>,
}

impl KafkaLog {
    fn ensure_topic(&self, topic: &str) -> Result<(), KafkaLogError> {
        let mut known = self
            .known_topics
            .lock()
            .map_err(|_| KafkaLogError::TopicRegistryPoisoned)?;
        if known.insert(topic.to_string()) {
            let mut topics = self
                .topics
                .lock()
                .map_err(|_| KafkaLogError::TopicLogPoisoned)?;
            topics.entry(topic.to_string()).or_default();
        }
        Ok(())
    }

    fn send(&self, topic: &str, key: Option<&[u8]>, payload: &[u8]) -> Result<(), KafkaLogError> {
        self.ensure_topic(topic)?;
        let mut topics = self
            .topics
            .lock()
            .map_err(|_| KafkaLogError::TopicLogPoisoned)?;
        let records = topics.entry(topic.to_string()).or_default();
        let offset = records.len() as i64;
        records.push(KafkaRecord {
            key: key.map(<[u8]>::to_vec),
            payload: payload.to_vec(),
            offset,
        });
        Ok(())
    }

    fn fetch(&self, topic: &str, offset: i64) -> Result<Option<KafkaRecord>, KafkaLogError> {
        self.ensure_topic(topic)?;
        let topics = self
            .topics
            .lock()
            .map_err(|_| KafkaLogError::TopicLogPoisoned)?;
        Ok(topics
            .get(topic)
            .and_then(|records| records.get(offset.max(0) as usize).cloned()))
    }
}

static KAFKA_LOG: OnceLock<Arc<KafkaLog>> = OnceLock::new();

pub async fn init_kafka_log() -> Result<KafkaHandle, KafkaLogError> {
    if let Some(existing) = KAFKA_LOG.get() {
        return Ok(KafkaHandle {
            inner: Arc::clone(existing),
        });
    }
    let log = Arc::new(KafkaLog::default());
    for topic in topics::ALL {
        log.ensure_topic(topic)?;
    }
    let _ = KAFKA_LOG.set(Arc::clone(&log));
    let config = KafkaConfig::from_env();
    info!(
        backend = "kafka",
        bootstrap_servers = %config.bootstrap_servers,
        client_id = %config.client_id,
        topics = ?topics::ALL,
        "kafka message log initialized"
    );
    Ok(KafkaHandle { inner: log })
}

pub fn kafka_handle() -> Result<KafkaHandle, KafkaLogError> {
    match KAFKA_LOG.get() {
        Some(log) => Ok(KafkaHandle {
            inner: Arc::clone(log),
        }),
        None => Err(KafkaLogError::NotInitialized),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn kafka_config_defaults_without_live_broker() {
        let config = KafkaConfig::from_env();
        assert_eq!(config.bootstrap_servers, "localhost:9092");
        assert_eq!(config.client_id, "jeryu");
    }

    #[tokio::test]
    async fn kafka_log_round_trips_payload_without_live_broker() {
        let handle = init_kafka_log().await.expect("kafka log");
        handle
            .send(super::topics::JOBS, Some(b"kafka-key"), b"kafka-payload")
            .await
            .expect("send");

        let mut consumer = handle.consumer(super::topics::JOBS, 0);
        let record = tokio::time::timeout(
            Duration::from_secs(2),
            consumer.next_with_timeout(Duration::from_millis(100)),
        )
        .await
        .expect("record timeout")
        .expect("consume")
        .expect("record");
        assert_eq!(record.key.as_deref(), Some(&b"kafka-key"[..]));
        assert_eq!(record.payload, b"kafka-payload");
    }
}
