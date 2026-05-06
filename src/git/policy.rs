//! Owner: Git execution policy
//! Proof: `cargo test -p jeryu -- git_policy`
//! Invariants: Policy defaults are fail-closed for destructive or unknown commands.

use crate::git::classify::GitCommandClass;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitMode {
    Observe,
    AfterSuccess,
    Parallel,
    Strict,
}

impl GitMode {
    pub fn current() -> Self {
        let mode = match std::env::var("JERYU_GIT_MODE") {
            Ok(value) => value,
            Err(_) => crate::settings::get().git.mode.clone(),
        };
        match mode.to_ascii_lowercase().as_str() {
            "observe" => GitMode::Observe,
            "parallel" => GitMode::Parallel,
            "strict" => GitMode::Strict,
            _ => GitMode::AfterSuccess,
        }
    }
}

pub fn should_mirror(class: GitCommandClass, argv: &[String]) -> bool {
    if !mirror_enabled() {
        return false;
    }

    matches!(
        class,
        GitCommandClass::NetworkWrite | GitCommandClass::RefMutation
    ) && matches!(argv.first().map(String::as_str), Some("push"))
}

pub fn strict_mode_enabled() -> bool {
    matches!(GitMode::current(), GitMode::Strict)
}

pub fn mirror_enabled() -> bool {
    match std::env::var("JERYU_MIRROR_ENABLED").ok() {
        Some(value) => parse_bool(&value),
        None => match std::env::var("JERYU_GIT_MIRROR_ENABLED").ok() {
            Some(value) => parse_bool(&value),
            None => crate::settings::get().mirror.enabled,
        },
    }
}

pub fn mirror_remote() -> String {
    let remote = match std::env::var("JERYU_MIRROR_REMOTE").ok() {
        Some(value) if !value.trim().is_empty() => Some(value),
        _ => match std::env::var("JERYU_GIT_MIRROR_REMOTE").ok() {
            Some(value) if !value.trim().is_empty() => Some(value),
            _ => None,
        },
    };
    match remote {
        Some(value) => value,
        None => crate::settings::get().mirror.remote.clone(),
    }
}

fn parse_bool(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off" | "disabled"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn push_is_mirrored() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: tests isolate the environment behind a mutex.
        unsafe {
            std::env::remove_var("JERYU_MIRROR_ENABLED");
        }
        assert!(should_mirror(
            GitCommandClass::NetworkWrite,
            &["push".into(), "origin".into()]
        ));
    }

    #[test]
    fn env_can_disable_mirror() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: tests isolate the environment behind a mutex.
        unsafe {
            std::env::set_var("JERYU_MIRROR_ENABLED", "0");
        }
        assert!(!should_mirror(
            GitCommandClass::NetworkWrite,
            &["push".into(), "origin".into()]
        ));
        // SAFETY: tests isolate the environment behind a mutex.
        unsafe {
            std::env::remove_var("JERYU_MIRROR_ENABLED");
        }
    }

    #[test]
    fn env_can_override_mirror_remote() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: tests isolate the environment behind a mutex.
        unsafe {
            std::env::set_var("JERYU_MIRROR_REMOTE", "backup");
        }
        assert_eq!(mirror_remote(), "backup");
        // SAFETY: tests isolate the environment behind a mutex.
        unsafe {
            std::env::remove_var("JERYU_MIRROR_REMOTE");
        }
    }
}
