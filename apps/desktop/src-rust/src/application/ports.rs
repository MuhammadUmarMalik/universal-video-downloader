use crate::domain::entities::{
    Collection, DownloadJob, DownloadStatus, HistoryEntry, JobEvent, LicenseState, MediaFormat,
    MediaItem, MediaSource, Platform, Schedule, SettingRecord,
};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    #[error("{entity} record was not found: {id}")]
    NotFound { entity: &'static str, id: String },
    #[error("repository operation conflicted with existing data")]
    Conflict { details: String },
    #[error("repository data is invalid")]
    InvalidData { details: String },
    #[error("repository storage operation failed")]
    Storage {
        operation: &'static str,
        diagnostic: String,
    },
}

pub type RepositoryResult<T> = Result<T, RepositoryError>;

#[async_trait]
pub trait PlatformRepository: Send + Sync {
    async fn get(&self, id: &str) -> RepositoryResult<Option<Platform>>;
    async fn list_enabled(&self) -> RepositoryResult<Vec<Platform>>;
    async fn upsert(&self, platform: &Platform) -> RepositoryResult<()>;
}

#[async_trait]
pub trait MediaSourceRepository: Send + Sync {
    async fn get(&self, id: &str) -> RepositoryResult<Option<MediaSource>>;
    async fn find_by_normalized_url(
        &self,
        platform_id: &str,
        normalized_url: &str,
    ) -> RepositoryResult<Option<MediaSource>>;
    async fn upsert(&self, source: &MediaSource) -> RepositoryResult<()>;
}

#[async_trait]
pub trait CollectionRepository: Send + Sync {
    async fn get(&self, id: &str) -> RepositoryResult<Option<Collection>>;
    async fn list_by_source(&self, source_id: &str) -> RepositoryResult<Vec<Collection>>;
    async fn upsert(&self, collection: &Collection) -> RepositoryResult<()>;
}

#[async_trait]
pub trait MediaItemRepository: Send + Sync {
    async fn get(&self, id: &str) -> RepositoryResult<Option<MediaItem>>;
    async fn list_by_source(&self, source_id: &str) -> RepositoryResult<Vec<MediaItem>>;
    async fn upsert(&self, item: &MediaItem) -> RepositoryResult<()>;
}

#[async_trait]
pub trait MediaFormatRepository: Send + Sync {
    async fn list_by_item(&self, media_item_id: &str) -> RepositoryResult<Vec<MediaFormat>>;
    async fn upsert(&self, format: &MediaFormat) -> RepositoryResult<()>;
    async fn replace_for_item(
        &self,
        media_item_id: &str,
        formats: &[MediaFormat],
    ) -> RepositoryResult<()>;
}

#[async_trait]
pub trait DownloadJobRepository: Send + Sync {
    async fn get(&self, id: &str) -> RepositoryResult<Option<DownloadJob>>;
    async fn list_by_status(&self, status: DownloadStatus) -> RepositoryResult<Vec<DownloadJob>>;
    async fn list_all(&self) -> RepositoryResult<Vec<DownloadJob>>;
    async fn insert(&self, job: &DownloadJob) -> RepositoryResult<()>;
    async fn update(&self, job: &DownloadJob) -> RepositoryResult<()>;
}

#[async_trait]
pub trait HistoryRepository: Send + Sync {
    async fn list(&self, query: Option<&str>) -> RepositoryResult<Vec<HistoryEntry>>;
    async fn delete(&self, id: &str) -> RepositoryResult<bool>;
    async fn clear(&self) -> RepositoryResult<u64>;
    async fn upsert(&self, entry: &HistoryEntry) -> RepositoryResult<()>;
}

#[async_trait]
pub trait JobEventRepository: Send + Sync {
    async fn list_by_job(&self, job_id: &str) -> RepositoryResult<Vec<JobEvent>>;
    async fn append(&self, event: &JobEvent) -> RepositoryResult<()>;
}

#[async_trait]
pub trait ScheduleRepository: Send + Sync {
    async fn get(&self, id: &str) -> RepositoryResult<Option<Schedule>>;
    async fn list_all(&self) -> RepositoryResult<Vec<Schedule>>;
    async fn list_due(&self, now: &str) -> RepositoryResult<Vec<Schedule>>;
    async fn upsert(&self, schedule: &Schedule) -> RepositoryResult<()>;
    async fn delete(&self, id: &str) -> RepositoryResult<bool>;
}

#[async_trait]
pub trait SettingsRepository: Send + Sync {
    async fn get(&self, key: &str) -> RepositoryResult<Option<SettingRecord>>;
    async fn list(&self) -> RepositoryResult<Vec<SettingRecord>>;
    async fn upsert(&self, setting: &SettingRecord) -> RepositoryResult<()>;
    async fn delete(&self, key: &str) -> RepositoryResult<bool>;
}

#[async_trait]
pub trait LicenseStateRepository: Send + Sync {
    async fn get(&self) -> RepositoryResult<Option<LicenseState>>;
    async fn save(&self, state: &LicenseState) -> RepositoryResult<()>;
}
