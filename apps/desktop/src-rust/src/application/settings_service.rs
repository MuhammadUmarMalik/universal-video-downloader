use super::ports::{RepositoryError, SettingsRepository};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingKey {
    DefaultDirectory,
    ConcurrentJobs,
    BandwidthLimitKbps,
    MaxRetries,
    RetryBackoff,
    FilenameTemplate,
    FolderTemplate,
    DefaultQuality,
    DefaultContainer,
    Theme,
    NotificationsEnabled,
    SchedulerEnabled,
}

impl SettingKey {
    pub const ALL: [Self; 12] = [
        Self::DefaultDirectory,
        Self::ConcurrentJobs,
        Self::BandwidthLimitKbps,
        Self::MaxRetries,
        Self::RetryBackoff,
        Self::FilenameTemplate,
        Self::FolderTemplate,
        Self::DefaultQuality,
        Self::DefaultContainer,
        Self::Theme,
        Self::NotificationsEnabled,
        Self::SchedulerEnabled,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DefaultDirectory => "download.default_directory",
            Self::ConcurrentJobs => "download.concurrent_jobs",
            Self::BandwidthLimitKbps => "download.bandwidth_limit_kbps",
            Self::MaxRetries => "download.max_retries",
            Self::RetryBackoff => "download.retry_backoff",
            Self::FilenameTemplate => "download.filename_template",
            Self::FolderTemplate => "download.folder_template",
            Self::DefaultQuality => "download.default_quality",
            Self::DefaultContainer => "download.default_container",
            Self::Theme => "ui.theme",
            Self::NotificationsEnabled => "notifications.enabled",
            Self::SchedulerEnabled => "scheduler.enabled",
        }
    }

    pub fn default_value(self) -> Option<SettingValue> {
        match self {
            Self::DefaultDirectory => None,
            Self::ConcurrentJobs => Some(SettingValue::ConcurrentJobs(3)),
            Self::BandwidthLimitKbps => Some(SettingValue::BandwidthLimitKbps(0)),
            Self::MaxRetries => Some(SettingValue::MaxRetries(3)),
            Self::RetryBackoff => Some(SettingValue::RetryBackoff(RetryBackoff {
                base_seconds: 2,
                max_seconds: 900,
            })),
            Self::FilenameTemplate => Some(SettingValue::FilenameTemplate(
                "{creator} - {title}".to_owned(),
            )),
            Self::FolderTemplate => Some(SettingValue::FolderTemplate(
                "{platform}/{creator}/{collection}".to_owned(),
            )),
            Self::DefaultQuality => Some(SettingValue::DefaultQuality("best".to_owned())),
            Self::DefaultContainer => Some(SettingValue::DefaultContainer("mp4".to_owned())),
            Self::Theme => Some(SettingValue::Theme(Theme::System)),
            Self::NotificationsEnabled => Some(SettingValue::NotificationsEnabled(true)),
            Self::SchedulerEnabled => Some(SettingValue::SchedulerEnabled(false)),
        }
    }
}

impl TryFrom<&str> for SettingKey {
    type Error = SettingsError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|key| key.as_str() == value)
            .ok_or_else(|| SettingsError::UnknownKey(value.to_owned()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryBackoff {
    pub base_seconds: u64,
    pub max_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SettingValue {
    DefaultDirectory(PathBuf),
    ConcurrentJobs(u8),
    BandwidthLimitKbps(u32),
    MaxRetries(u8),
    RetryBackoff(RetryBackoff),
    FilenameTemplate(String),
    FolderTemplate(String),
    DefaultQuality(String),
    DefaultContainer(String),
    Theme(Theme),
    NotificationsEnabled(bool),
    SchedulerEnabled(bool),
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("unknown setting key: {0}")]
    UnknownKey(String),
    #[error("invalid value for setting {key}: {reason}")]
    InvalidValue { key: String, reason: String },
    #[error("setting value could not be serialized")]
    Serialization(#[source] serde_json::Error),
    #[error("setting value could not be deserialized")]
    Deserialization(#[source] serde_json::Error),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

pub type SettingsResult<T> = Result<T, SettingsError>;

#[derive(Clone)]
pub struct SettingsService<R> {
    repository: R,
}

impl<R> SettingsService<R>
where
    R: SettingsRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn get(&self, key: SettingKey) -> SettingsResult<Option<SettingValue>> {
        let Some(record) = self.repository.get(key.as_str()).await? else {
            return Ok(None);
        };
        Ok(Some(SettingValue::from_json(key, record.value_json)?))
    }

    pub async fn get_or_default(&self, key: SettingKey) -> SettingsResult<Option<SettingValue>> {
        match self.get(key).await? {
            Some(value) => Ok(Some(value)),
            None => Ok(key.default_value()),
        }
    }

    pub async fn set(&self, key: SettingKey, value: SettingValue) -> SettingsResult<()> {
        value.validate_for(key)?;
        let value_json = value.to_json()?;
        let record = crate::domain::entities::SettingRecord {
            key: key.as_str().to_owned(),
            value_json,
            updated_at: now_utc(),
        };
        self.repository.upsert(&record).await?;
        Ok(())
    }

    pub async fn reset(&self, key: SettingKey) -> SettingsResult<bool> {
        Ok(self.repository.delete(key.as_str()).await?)
    }

    pub async fn snapshot(&self) -> SettingsResult<BTreeMap<SettingKey, SettingValue>> {
        let records = self.repository.list().await?;
        let mut snapshot = BTreeMap::new();
        for record in records {
            let key = SettingKey::try_from(record.key.as_str())?;
            let value = SettingValue::from_json(key, record.value_json)?;
            snapshot.insert(key, value);
        }
        Ok(snapshot)
    }
}

impl SettingValue {
    pub fn validate_for(&self, key: SettingKey) -> SettingsResult<()> {
        let valid = matches!(
            (key, self),
            (SettingKey::DefaultDirectory, Self::DefaultDirectory(_))
                | (SettingKey::ConcurrentJobs, Self::ConcurrentJobs(_))
                | (SettingKey::BandwidthLimitKbps, Self::BandwidthLimitKbps(_))
                | (SettingKey::MaxRetries, Self::MaxRetries(_))
                | (SettingKey::RetryBackoff, Self::RetryBackoff(_))
                | (SettingKey::FilenameTemplate, Self::FilenameTemplate(_))
                | (SettingKey::FolderTemplate, Self::FolderTemplate(_))
                | (SettingKey::DefaultQuality, Self::DefaultQuality(_))
                | (SettingKey::DefaultContainer, Self::DefaultContainer(_))
                | (SettingKey::Theme, Self::Theme(_))
                | (
                    SettingKey::NotificationsEnabled,
                    Self::NotificationsEnabled(_)
                )
                | (SettingKey::SchedulerEnabled, Self::SchedulerEnabled(_))
        );
        if !valid {
            return Err(SettingsError::InvalidValue {
                key: key.as_str().to_owned(),
                reason: "value type does not match setting key".to_owned(),
            });
        }

        match self {
            Self::DefaultDirectory(path) => {
                if !path.is_absolute() {
                    return Err(SettingsError::InvalidValue {
                        key: key.as_str().to_owned(),
                        reason: "directory must be an absolute path".to_owned(),
                    });
                }
            }
            Self::ConcurrentJobs(value) if !(1..=8).contains(value) => {
                return Err(SettingsError::InvalidValue {
                    key: key.as_str().to_owned(),
                    reason: "must be between 1 and 8".to_owned(),
                });
            }
            Self::BandwidthLimitKbps(value) if *value > 1_000_000 => {
                return Err(SettingsError::InvalidValue {
                    key: key.as_str().to_owned(),
                    reason: "must be 0 (unlimited) or between 1 and 1,000,000 KB/s".to_owned(),
                });
            }
            Self::MaxRetries(value) if *value > 10 => {
                return Err(SettingsError::InvalidValue {
                    key: key.as_str().to_owned(),
                    reason: "must be between 0 and 10".to_owned(),
                });
            }
            Self::RetryBackoff(value)
                if value.base_seconds == 0
                    || value.base_seconds > value.max_seconds
                    || value.max_seconds > 86_400 =>
            {
                return Err(SettingsError::InvalidValue {
                    key: key.as_str().to_owned(),
                    reason: "must have 1 <= base <= max <= 86400 seconds".to_owned(),
                });
            }
            Self::FilenameTemplate(value) | Self::FolderTemplate(value)
                if value.is_empty() || value.len() > 256 || value.contains("..") =>
            {
                return Err(SettingsError::InvalidValue {
                    key: key.as_str().to_owned(),
                    reason: "must be 1-256 characters and cannot contain '..'".to_owned(),
                });
            }
            Self::DefaultQuality(value) | Self::DefaultContainer(value)
                if value.is_empty() || value.len() > 32 =>
            {
                return Err(SettingsError::InvalidValue {
                    key: key.as_str().to_owned(),
                    reason: "must be 1-32 characters".to_owned(),
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn to_json(&self) -> SettingsResult<serde_json::Value> {
        serde_json::to_value(self).map_err(SettingsError::Serialization)
    }

    fn from_json(key: SettingKey, value: serde_json::Value) -> SettingsResult<Self> {
        let setting: Self =
            serde_json::from_value(value).map_err(SettingsError::Deserialization)?;
        setting.validate_for(key)?;
        Ok(setting)
    }
}

fn now_utc() -> String {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};

    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{RetryBackoff, SettingKey, SettingValue, Theme};
    use std::path::PathBuf;

    #[test]
    fn validates_key_specific_ranges_and_paths() {
        assert!(SettingValue::ConcurrentJobs(8)
            .validate_for(SettingKey::ConcurrentJobs)
            .is_ok());
        assert!(SettingValue::BandwidthLimitKbps(0)
            .validate_for(SettingKey::BandwidthLimitKbps)
            .is_ok());
        assert!(SettingValue::BandwidthLimitKbps(1_000_001)
            .validate_for(SettingKey::BandwidthLimitKbps)
            .is_err());
        assert!(SettingValue::ConcurrentJobs(9)
            .validate_for(SettingKey::ConcurrentJobs)
            .is_err());
        assert!(SettingValue::DefaultDirectory(PathBuf::from("relative"))
            .validate_for(SettingKey::DefaultDirectory)
            .is_err());
        assert!(SettingValue::DefaultDirectory(PathBuf::from("/tmp/umd"))
            .validate_for(SettingKey::DefaultDirectory)
            .is_ok());
    }

    #[test]
    fn defaults_are_typed_and_explicit() {
        assert_eq!(
            SettingKey::ConcurrentJobs.default_value(),
            Some(SettingValue::ConcurrentJobs(3))
        );
        assert_eq!(
            SettingKey::BandwidthLimitKbps.default_value(),
            Some(SettingValue::BandwidthLimitKbps(0))
        );
        assert_eq!(
            SettingKey::Theme.default_value(),
            Some(SettingValue::Theme(Theme::System))
        );
        assert_eq!(
            SettingKey::RetryBackoff.default_value(),
            Some(SettingValue::RetryBackoff(RetryBackoff {
                base_seconds: 2,
                max_seconds: 900,
            }))
        );
        assert_eq!(SettingKey::DefaultDirectory.default_value(), None);
    }
}
