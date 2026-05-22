use super::{MessageLogBackend, RuntimeProfile, RuntimeProfileError};

pub fn select_message_log_backend(
    env_value: Option<&str>,
) -> Result<MessageLogBackend, RuntimeProfileError> {
    let backend = match env_value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => MessageLogBackend::parse(value)?,
        None => RuntimeProfile::compiled().message_log_backend(),
    };
    ensure_message_log_backend_compiled(backend)?;
    Ok(backend)
}

pub fn ensure_message_log_backend_compiled(
    backend: MessageLogBackend,
) -> Result<(), RuntimeProfileError> {
    match backend {
        MessageLogBackend::Kafka if !backend.is_compiled() => {
            Err(RuntimeProfileError::BackendNotCompiled {
                requested: "kafka",
                feature: "kafka-backend",
            })
        }
        MessageLogBackend::Jansu if !backend.is_compiled() => {
            Err(RuntimeProfileError::BackendNotCompiled {
                requested: "jansu",
                feature: "jansu-backend",
            })
        }
        _ => Ok(()),
    }
}
