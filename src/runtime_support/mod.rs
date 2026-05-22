use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateBackend {
    Sqlite,
    RedlineDb,
}

impl StateBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::RedlineDb => "redlinedb",
        }
    }

    pub fn parse(value: &str) -> Result<Self, RuntimeProfileError> {
        match normalize(value).as_str() {
            "sqlite" => Ok(Self::Sqlite),
            "redline" | "redlinedb" => Ok(Self::RedlineDb),
            other => Err(RuntimeProfileError::UnsupportedStateBackend(
                other.to_string(),
            )),
        }
    }

    pub fn is_compiled(self) -> bool {
        match self {
            Self::Sqlite => cfg!(feature = "sqlite-backend"),
            Self::RedlineDb => cfg!(feature = "redlinedb-backend"),
        }
    }
}

impl fmt::Display for StateBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StateBackend {
    type Err = RuntimeProfileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLogBackend {
    Kafka,
    Jansu,
}

impl MessageLogBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kafka => "kafka",
            Self::Jansu => "jansu",
        }
    }

    pub fn parse(value: &str) -> Result<Self, RuntimeProfileError> {
        match normalize(value).as_str() {
            "kafka" => Ok(Self::Kafka),
            "jansu" => Ok(Self::Jansu),
            other => Err(RuntimeProfileError::UnsupportedMessageLogBackend(
                other.to_string(),
            )),
        }
    }

    pub fn is_compiled(self) -> bool {
        match self {
            Self::Kafka => cfg!(feature = "kafka-backend"),
            Self::Jansu => cfg!(feature = "jansu-backend"),
        }
    }
}

impl fmt::Display for MessageLogBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MessageLogBackend {
    type Err = RuntimeProfileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeProfile {
    SqliteKafka,
    RedlineDbJansu,
}

impl RuntimeProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SqliteKafka => "sqlite-kafka",
            Self::RedlineDbJansu => "redlinedb-jansu",
        }
    }

    pub fn parse(value: &str) -> Result<Self, RuntimeProfileError> {
        match normalize(value).as_str() {
            "sqlitekafka" => Ok(Self::SqliteKafka),
            "redlinejansu" | "redlinedbjansu" => Ok(Self::RedlineDbJansu),
            other => Err(RuntimeProfileError::UnsupportedRuntimeProfile(
                other.to_string(),
            )),
        }
    }

    pub fn compiled() -> Self {
        if cfg!(all(
            feature = "profile-redlinedb-jansu",
            not(feature = "profile-sqlite-kafka")
        )) {
            Self::RedlineDbJansu
        } else {
            Self::SqliteKafka
        }
    }

    pub fn state_backend(self) -> StateBackend {
        match self {
            Self::SqliteKafka => StateBackend::Sqlite,
            Self::RedlineDbJansu => StateBackend::RedlineDb,
        }
    }

    pub fn message_log_backend(self) -> MessageLogBackend {
        match self {
            Self::SqliteKafka => MessageLogBackend::Kafka,
            Self::RedlineDbJansu => MessageLogBackend::Jansu,
        }
    }
}

impl fmt::Display for RuntimeProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RuntimeProfile {
    type Err = RuntimeProfileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeProfileError {
    UnsupportedStateBackend(String),
    UnsupportedMessageLogBackend(String),
    UnsupportedRuntimeProfile(String),
    BackendNotCompiled {
        requested: &'static str,
        feature: &'static str,
    },
}

impl fmt::Display for RuntimeProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedStateBackend(value) => {
                write!(
                    f,
                    "unsupported state backend {value:?}; expected sqlite or redlinedb"
                )
            }
            Self::UnsupportedMessageLogBackend(value) => {
                write!(
                    f,
                    "unsupported message log backend {value:?}; expected kafka or jansu"
                )
            }
            Self::UnsupportedRuntimeProfile(value) => {
                write!(
                    f,
                    "unsupported runtime profile {value:?}; expected sqlite-kafka or redlinedb-jansu"
                )
            }
            Self::BackendNotCompiled { requested, feature } => {
                write!(
                    f,
                    "backend {requested:?} was requested but this build does not enable Cargo feature {feature:?}"
                )
            }
        }
    }
}

impl std::error::Error for RuntimeProfileError {}

#[path = "runtime_support_select.rs"]
mod select;
pub use select::{ensure_message_log_backend_compiled, select_message_log_backend};

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(['-', '_'], "")
}

#[cfg(test)]
#[path = "runtime_support_tests.rs"]
mod tests;
