use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid persisted value for {field}: {value}")]
pub struct DomainValueError {
    pub field: &'static str,
    pub value: String,
}

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub enum $name {
            $( $variant ),+
        }

        impl $name {
            pub const fn as_str(&self) -> &'static str {
                match self {
                    $( Self::$variant => $value ),+
                }
            }
        }

        impl TryFrom<&str> for $name {
            type Error = DomainValueError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                match value {
                    $( $value => Ok(Self::$variant), )+
                    _ => Err(DomainValueError { field: stringify!($name), value: value.to_owned() }),
                }
            }
        }
    };
}

string_enum!(SourceType {
    Single => "single",
    Playlist => "playlist",
    Channel => "channel",
    Profile => "profile",
    Collection => "collection",
    Generic => "generic",
});

string_enum!(DownloadStatus {
    Queued => "queued",
    Resolving => "resolving",
    Downloading => "downloading",
    Processing => "processing",
    Completed => "completed",
    Paused => "paused",
    Cancelled => "cancelled",
    Failed => "failed",
});

string_enum!(ScheduleType {
    Once => "once",
    Daily => "daily",
    Weekly => "weekly",
    Interval => "interval",
});

string_enum!(LicensePlan {
    Free => "free",
    Pro => "pro",
    Enterprise => "enterprise",
});

string_enum!(LicenseStatus {
    Inactive => "inactive",
    Active => "active",
    Expired => "expired",
    Revoked => "revoked",
});

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Platform {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub enabled: bool,
    pub adapter_version: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaSource {
    pub id: String,
    pub platform_id: String,
    pub source_url: String,
    pub normalized_url: String,
    pub source_type: SourceType,
    pub title: Option<String>,
    pub creator_name: Option<String>,
    pub creator_id: Option<String>,
    pub thumbnail_url: Option<String>,
    pub item_count: Option<i64>,
    pub discovered_at: String,
    pub last_analyzed_at: Option<String>,
    pub metadata_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Collection {
    pub id: String,
    pub source_id: String,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub creator_name: Option<String>,
    pub item_count: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaItem {
    pub id: String,
    pub source_id: String,
    pub collection_id: Option<String>,
    pub external_id: Option<String>,
    pub canonical_url: String,
    pub title: String,
    pub creator_name: Option<String>,
    pub creator_id: Option<String>,
    pub thumbnail_url: Option<String>,
    pub duration_ms: Option<i64>,
    pub published_at: Option<String>,
    pub position: Option<i64>,
    pub metadata_json: Option<serde_json::Value>,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaFormat {
    pub id: String,
    pub media_item_id: String,
    pub external_format_id: Option<String>,
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub fps: Option<f64>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
    pub file_size_bytes: Option<i64>,
    pub is_video: bool,
    pub is_audio: bool,
    pub is_progressive: bool,
    pub metadata_json: Option<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadJob {
    pub id: String,
    pub media_item_id: String,
    pub format_id: Option<String>,
    pub status: DownloadStatus,
    pub priority: i64,
    pub destination_path: String,
    pub temp_path: Option<String>,
    pub filename: String,
    pub total_bytes: Option<i64>,
    pub downloaded_bytes: i64,
    pub speed_bytes_per_sec: Option<i64>,
    pub eta_seconds: Option<i64>,
    pub retry_count: i64,
    pub max_retries: i64,
    pub processing_json: Option<serde_json::Value>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: String,
    pub job_id: String,
    pub media_item_id: String,
    pub format_id: Option<String>,
    pub platform_id: String,
    pub platform_name: String,
    pub source_url: String,
    pub title: String,
    pub creator_name: Option<String>,
    pub destination_path: String,
    pub filename: String,
    pub status: DownloadStatus,
    pub size_bytes: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobEvent {
    pub id: String,
    pub job_id: String,
    pub event_type: String,
    pub payload_json: Option<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Schedule {
    pub id: String,
    pub source_id: String,
    pub schedule_type: ScheduleType,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub configuration_json: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SettingRecord {
    pub key: String,
    pub value_json: serde_json::Value,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicenseState {
    pub id: i64,
    pub license_key_hash: Option<String>,
    pub plan: LicensePlan,
    pub status: LicenseStatus,
    pub expires_at: Option<String>,
    pub device_id: Option<String>,
    pub last_validated_at: Option<String>,
    pub metadata_json: Option<serde_json::Value>,
}

impl Display for SourceType {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Display for DownloadStatus {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Display for ScheduleType {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Display for LicensePlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Display for LicenseStatus {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
