//! Persisted diagnostics level. Captured content is opt-in, never enabled by RUST_LOG.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[repr(u8)]
pub enum LogLevel {
    Off,
    Verbose,
    #[default]
    #[serde(other)]
    Basic,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LoggingSettings {
    pub level: LogLevel,
}

static LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Basic as u8);

pub fn apply(level: LogLevel) {
    LEVEL.store(level as u8, Ordering::SeqCst);
}

pub fn verbose() -> bool {
    LEVEL.load(Ordering::SeqCst) == LogLevel::Verbose as u8
}

pub fn enabled(level: &tracing::Level) -> bool {
    permits(LEVEL.load(Ordering::SeqCst), level)
}

fn permits(setting: u8, level: &tracing::Level) -> bool {
    match setting {
        value if value == LogLevel::Verbose as u8 => true,
        value if value == LogLevel::Basic as u8 => *level <= tracing::Level::INFO,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_is_default_and_levels_round_trip() {
        let settings: LoggingSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.level, LogLevel::Basic);
        for level in [LogLevel::Off, LogLevel::Basic, LogLevel::Verbose] {
            let json = serde_json::to_string(&level).unwrap();
            assert_eq!(serde_json::from_str::<LogLevel>(&json).unwrap(), level);
        }
    }

    #[test]
    fn filters_respect_off_and_basic() {
        assert!(!permits(LogLevel::Off as u8, &tracing::Level::ERROR));
        assert!(permits(LogLevel::Basic as u8, &tracing::Level::INFO));
        assert!(!permits(LogLevel::Basic as u8, &tracing::Level::DEBUG));
        assert!(permits(LogLevel::Verbose as u8, &tracing::Level::DEBUG));
    }
}
