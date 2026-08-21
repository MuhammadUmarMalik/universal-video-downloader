use crate::application::ports::{
    CollectionRepository, DownloadJobRepository, HistoryRepository, JobEventRepository,
    LicenseStateRepository, MediaFormatRepository, MediaItemRepository, MediaSourceRepository,
    PlatformRepository, RepositoryError, RepositoryResult, ScheduleRepository, SettingsRepository,
};
use crate::domain::entities::{
    Collection, DownloadJob, DownloadStatus, HistoryEntry, JobEvent, LicensePlan, LicenseState,
    LicenseStatus, MediaFormat, MediaItem, MediaSource, Platform, Schedule, ScheduleType,
    SettingRecord, SourceType,
};
use async_trait::async_trait;
use sqlx::{FromRow, SqlitePool};

pub(crate) fn storage(operation: &'static str, error: sqlx::Error) -> RepositoryError {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error.is_unique_violation() {
            return RepositoryError::Conflict {
                details: "unique constraint violated".to_owned(),
            };
        }
    }
    RepositoryError::Storage {
        operation,
        diagnostic: error.to_string(),
    }
}

fn parse_json(
    field: &'static str,
    value: Option<String>,
) -> RepositoryResult<Option<serde_json::Value>> {
    value
        .map(|raw| {
            serde_json::from_str(&raw).map_err(|_| RepositoryError::InvalidData {
                details: format!("{field} contains invalid JSON"),
            })
        })
        .transpose()
}

fn parse_required_json(field: &'static str, value: String) -> RepositoryResult<serde_json::Value> {
    serde_json::from_str(&value).map_err(|_| RepositoryError::InvalidData {
        details: format!("{field} contains invalid JSON"),
    })
}

pub(crate) fn serialize_json(
    field: &'static str,
    value: &Option<serde_json::Value>,
) -> RepositoryResult<Option<String>> {
    value
        .as_ref()
        .map(|json| {
            serde_json::to_string(json).map_err(|_| RepositoryError::InvalidData {
                details: format!("{field} could not be serialized"),
            })
        })
        .transpose()
}

fn serialize_required_json(
    field: &'static str,
    value: &serde_json::Value,
) -> RepositoryResult<String> {
    serde_json::to_string(value).map_err(|_| RepositoryError::InvalidData {
        details: format!("{field} could not be serialized"),
    })
}

fn parse_bool(field: &'static str, value: i64) -> RepositoryResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(RepositoryError::InvalidData {
            details: format!("{field} is not a valid boolean"),
        }),
    }
}

#[derive(Debug, FromRow)]
struct PlatformRow {
    id: String,
    slug: String,
    name: String,
    enabled: i64,
    adapter_version: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<PlatformRow> for Platform {
    type Error = RepositoryError;

    fn try_from(row: PlatformRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            slug: row.slug,
            name: row.name,
            enabled: parse_bool("platforms.enabled", row.enabled)?,
            adapter_version: row.adapter_version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct MediaSourceRow {
    id: String,
    platform_id: String,
    source_url: String,
    normalized_url: String,
    source_type: String,
    title: Option<String>,
    creator_name: Option<String>,
    creator_id: Option<String>,
    thumbnail_url: Option<String>,
    item_count: Option<i64>,
    discovered_at: String,
    last_analyzed_at: Option<String>,
    metadata_json: Option<String>,
}

impl TryFrom<MediaSourceRow> for MediaSource {
    type Error = RepositoryError;

    fn try_from(row: MediaSourceRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            platform_id: row.platform_id,
            source_url: row.source_url,
            normalized_url: row.normalized_url,
            source_type: SourceType::try_from(row.source_type.as_str()).map_err(|error| {
                RepositoryError::InvalidData {
                    details: error.to_string(),
                }
            })?,
            title: row.title,
            creator_name: row.creator_name,
            creator_id: row.creator_id,
            thumbnail_url: row.thumbnail_url,
            item_count: row.item_count,
            discovered_at: row.discovered_at,
            last_analyzed_at: row.last_analyzed_at,
            metadata_json: parse_json("media_sources.metadata_json", row.metadata_json)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct CollectionRow {
    id: String,
    source_id: String,
    external_id: Option<String>,
    title: Option<String>,
    creator_name: Option<String>,
    item_count: Option<i64>,
    created_at: String,
    updated_at: String,
}

impl From<CollectionRow> for Collection {
    fn from(row: CollectionRow) -> Self {
        Self {
            id: row.id,
            source_id: row.source_id,
            external_id: row.external_id,
            title: row.title,
            creator_name: row.creator_name,
            item_count: row.item_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, FromRow)]
struct MediaItemRow {
    id: String,
    source_id: String,
    collection_id: Option<String>,
    external_id: Option<String>,
    canonical_url: String,
    title: String,
    creator_name: Option<String>,
    creator_id: Option<String>,
    thumbnail_url: Option<String>,
    duration_ms: Option<i64>,
    published_at: Option<String>,
    position: Option<i64>,
    metadata_json: Option<String>,
    first_seen_at: String,
    last_seen_at: String,
}

impl TryFrom<MediaItemRow> for MediaItem {
    type Error = RepositoryError;

    fn try_from(row: MediaItemRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            source_id: row.source_id,
            collection_id: row.collection_id,
            external_id: row.external_id,
            canonical_url: row.canonical_url,
            title: row.title,
            creator_name: row.creator_name,
            creator_id: row.creator_id,
            thumbnail_url: row.thumbnail_url,
            duration_ms: row.duration_ms,
            published_at: row.published_at,
            position: row.position,
            metadata_json: parse_json("media_items.metadata_json", row.metadata_json)?,
            first_seen_at: row.first_seen_at,
            last_seen_at: row.last_seen_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct MediaFormatRow {
    id: String,
    media_item_id: String,
    external_format_id: Option<String>,
    container: Option<String>,
    video_codec: Option<String>,
    audio_codec: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    fps: Option<f64>,
    bitrate: Option<i64>,
    sample_rate: Option<i64>,
    channels: Option<i64>,
    file_size_bytes: Option<i64>,
    is_video: i64,
    is_audio: i64,
    is_progressive: i64,
    metadata_json: Option<String>,
    created_at: String,
}

impl TryFrom<MediaFormatRow> for MediaFormat {
    type Error = RepositoryError;

    fn try_from(row: MediaFormatRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            media_item_id: row.media_item_id,
            external_format_id: row.external_format_id,
            container: row.container,
            video_codec: row.video_codec,
            audio_codec: row.audio_codec,
            width: row.width,
            height: row.height,
            fps: row.fps,
            bitrate: row.bitrate,
            sample_rate: row.sample_rate,
            channels: row.channels,
            file_size_bytes: row.file_size_bytes,
            is_video: parse_bool("media_formats.is_video", row.is_video)?,
            is_audio: parse_bool("media_formats.is_audio", row.is_audio)?,
            is_progressive: parse_bool("media_formats.is_progressive", row.is_progressive)?,
            metadata_json: parse_json("media_formats.metadata_json", row.metadata_json)?,
            created_at: row.created_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct DownloadJobRow {
    id: String,
    media_item_id: String,
    format_id: Option<String>,
    status: String,
    priority: i64,
    destination_path: String,
    temp_path: Option<String>,
    filename: String,
    total_bytes: Option<i64>,
    downloaded_bytes: i64,
    speed_bytes_per_sec: Option<i64>,
    eta_seconds: Option<i64>,
    retry_count: i64,
    max_retries: i64,
    processing_json: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<DownloadJobRow> for DownloadJob {
    type Error = RepositoryError;

    fn try_from(row: DownloadJobRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            media_item_id: row.media_item_id,
            format_id: row.format_id,
            status: DownloadStatus::try_from(row.status.as_str()).map_err(|error| {
                RepositoryError::InvalidData {
                    details: error.to_string(),
                }
            })?,
            priority: row.priority,
            destination_path: row.destination_path,
            temp_path: row.temp_path,
            filename: row.filename,
            total_bytes: row.total_bytes,
            downloaded_bytes: row.downloaded_bytes,
            speed_bytes_per_sec: row.speed_bytes_per_sec,
            eta_seconds: row.eta_seconds,
            retry_count: row.retry_count,
            max_retries: row.max_retries,
            processing_json: parse_json("download_jobs.processing_json", row.processing_json)?,
            etag: row.etag,
            last_modified: row.last_modified,
            error_code: row.error_code,
            error_message: row.error_message,
            started_at: row.started_at,
            completed_at: row.completed_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct HistoryEntryRow {
    id: String,
    job_id: String,
    media_item_id: String,
    format_id: Option<String>,
    platform_id: String,
    platform_name: String,
    source_url: String,
    title: String,
    creator_name: Option<String>,
    destination_path: String,
    filename: String,
    status: String,
    size_bytes: Option<i64>,
    error_code: Option<String>,
    error_message: Option<String>,
    created_at: String,
    finished_at: String,
}

impl TryFrom<HistoryEntryRow> for HistoryEntry {
    type Error = RepositoryError;

    fn try_from(row: HistoryEntryRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            job_id: row.job_id,
            media_item_id: row.media_item_id,
            format_id: row.format_id,
            platform_id: row.platform_id,
            platform_name: row.platform_name,
            source_url: row.source_url,
            title: row.title,
            creator_name: row.creator_name,
            destination_path: row.destination_path,
            filename: row.filename,
            status: DownloadStatus::try_from(row.status.as_str()).map_err(|error| {
                RepositoryError::InvalidData {
                    details: error.to_string(),
                }
            })?,
            size_bytes: row.size_bytes,
            error_code: row.error_code,
            error_message: row.error_message,
            created_at: row.created_at,
            finished_at: row.finished_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct JobEventRow {
    id: String,
    job_id: String,
    event_type: String,
    payload_json: Option<String>,
    created_at: String,
}

impl TryFrom<JobEventRow> for JobEvent {
    type Error = RepositoryError;

    fn try_from(row: JobEventRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            job_id: row.job_id,
            event_type: row.event_type,
            payload_json: parse_json("job_events.payload_json", row.payload_json)?,
            created_at: row.created_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct ScheduleRow {
    id: String,
    source_id: String,
    schedule_type: String,
    cron_expression: Option<String>,
    interval_seconds: Option<i64>,
    enabled: i64,
    last_run_at: Option<String>,
    next_run_at: Option<String>,
    configuration_json: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ScheduleRow> for Schedule {
    type Error = RepositoryError;

    fn try_from(row: ScheduleRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            source_id: row.source_id,
            schedule_type: ScheduleType::try_from(row.schedule_type.as_str()).map_err(|error| {
                RepositoryError::InvalidData {
                    details: error.to_string(),
                }
            })?,
            cron_expression: row.cron_expression,
            interval_seconds: row.interval_seconds,
            enabled: parse_bool("schedules.enabled", row.enabled)?,
            last_run_at: row.last_run_at,
            next_run_at: row.next_run_at,
            configuration_json: parse_json("schedules.configuration_json", row.configuration_json)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct SettingRow {
    key: String,
    value_json: String,
    updated_at: String,
}

impl TryFrom<SettingRow> for SettingRecord {
    type Error = RepositoryError;

    fn try_from(row: SettingRow) -> Result<Self, Self::Error> {
        Ok(Self {
            key: row.key,
            value_json: parse_required_json("settings.value_json", row.value_json)?,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct LicenseStateRow {
    id: i64,
    license_key_hash: Option<String>,
    plan: String,
    status: String,
    expires_at: Option<String>,
    device_id: Option<String>,
    last_validated_at: Option<String>,
    metadata_json: Option<String>,
}

impl TryFrom<LicenseStateRow> for LicenseState {
    type Error = RepositoryError;

    fn try_from(row: LicenseStateRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            license_key_hash: row.license_key_hash,
            plan: LicensePlan::try_from(row.plan.as_str()).map_err(|error| {
                RepositoryError::InvalidData {
                    details: error.to_string(),
                }
            })?,
            status: LicenseStatus::try_from(row.status.as_str()).map_err(|error| {
                RepositoryError::InvalidData {
                    details: error.to_string(),
                }
            })?,
            expires_at: row.expires_at,
            device_id: row.device_id,
            last_validated_at: row.last_validated_at,
            metadata_json: parse_json("license_state.metadata_json", row.metadata_json)?,
        })
    }
}

macro_rules! repository {
    ($name:ident) => {
        #[derive(Clone)]
        pub struct $name {
            pool: SqlitePool,
        }

        impl $name {
            fn new(pool: &SqlitePool) -> Self {
                Self { pool: pool.clone() }
            }
        }
    };
}

repository!(SqlitePlatformRepository);
repository!(SqliteMediaSourceRepository);
repository!(SqliteCollectionRepository);
repository!(SqliteMediaItemRepository);
repository!(SqliteMediaFormatRepository);
repository!(SqliteDownloadJobRepository);
repository!(SqliteJobEventRepository);
repository!(SqliteHistoryRepository);
repository!(SqliteScheduleRepository);
repository!(SqliteSettingsRepository);
repository!(SqliteLicenseStateRepository);

#[derive(Clone)]
pub struct SqliteRepositories {
    pub platforms: SqlitePlatformRepository,
    pub media_sources: SqliteMediaSourceRepository,
    pub collections: SqliteCollectionRepository,
    pub media_items: SqliteMediaItemRepository,
    pub media_formats: SqliteMediaFormatRepository,
    pub download_jobs: SqliteDownloadJobRepository,
    pub job_events: SqliteJobEventRepository,
    pub history: SqliteHistoryRepository,
    pub schedules: SqliteScheduleRepository,
    pub settings: SqliteSettingsRepository,
    pub license_state: SqliteLicenseStateRepository,
}

impl SqliteRepositories {
    pub fn new(pool: &SqlitePool) -> Self {
        Self {
            platforms: SqlitePlatformRepository::new(pool),
            media_sources: SqliteMediaSourceRepository::new(pool),
            collections: SqliteCollectionRepository::new(pool),
            media_items: SqliteMediaItemRepository::new(pool),
            media_formats: SqliteMediaFormatRepository::new(pool),
            download_jobs: SqliteDownloadJobRepository::new(pool),
            job_events: SqliteJobEventRepository::new(pool),
            history: SqliteHistoryRepository::new(pool),
            schedules: SqliteScheduleRepository::new(pool),
            settings: SqliteSettingsRepository::new(pool),
            license_state: SqliteLicenseStateRepository::new(pool),
        }
    }
}

#[async_trait]
impl PlatformRepository for SqlitePlatformRepository {
    async fn get(&self, id: &str) -> RepositoryResult<Option<Platform>> {
        sqlx::query_as::<_, PlatformRow>(
            "SELECT id, slug, name, enabled, adapter_version, created_at, updated_at FROM platforms WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| storage("platforms.get", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_enabled(&self) -> RepositoryResult<Vec<Platform>> {
        let rows = sqlx::query_as::<_, PlatformRow>(
            "SELECT id, slug, name, enabled, adapter_version, created_at, updated_at FROM platforms WHERE enabled = 1 ORDER BY name, id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| storage("platforms.list_enabled", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn upsert(&self, platform: &Platform) -> RepositoryResult<()> {
        sqlx::query(
            "INSERT INTO platforms (id, slug, name, enabled, adapter_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET slug = excluded.slug, name = excluded.name, enabled = excluded.enabled, adapter_version = excluded.adapter_version, updated_at = excluded.updated_at",
        )
        .bind(&platform.id)
        .bind(&platform.slug)
        .bind(&platform.name)
        .bind(i64::from(platform.enabled))
        .bind(&platform.adapter_version)
        .bind(&platform.created_at)
        .bind(&platform.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|error| storage("platforms.upsert", error))?;
        Ok(())
    }
}

#[async_trait]
impl MediaSourceRepository for SqliteMediaSourceRepository {
    async fn get(&self, id: &str) -> RepositoryResult<Option<MediaSource>> {
        sqlx::query_as::<_, MediaSourceRow>("SELECT id, platform_id, source_url, normalized_url, source_type, title, creator_name, creator_id, thumbnail_url, item_count, discovered_at, last_analyzed_at, metadata_json FROM media_sources WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| storage("media_sources.get", error))?
            .map(TryInto::try_into)
            .transpose()
    }

    async fn find_by_normalized_url(
        &self,
        platform_id: &str,
        normalized_url: &str,
    ) -> RepositoryResult<Option<MediaSource>> {
        sqlx::query_as::<_, MediaSourceRow>("SELECT id, platform_id, source_url, normalized_url, source_type, title, creator_name, creator_id, thumbnail_url, item_count, discovered_at, last_analyzed_at, metadata_json FROM media_sources WHERE platform_id = ? AND normalized_url = ?")
            .bind(platform_id)
            .bind(normalized_url)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| storage("media_sources.find_by_normalized_url", error))?
            .map(TryInto::try_into)
            .transpose()
    }

    async fn upsert(&self, source: &MediaSource) -> RepositoryResult<()> {
        let metadata_json = serialize_json("media_sources.metadata_json", &source.metadata_json)?;
        sqlx::query("INSERT INTO media_sources (id, platform_id, source_url, normalized_url, source_type, title, creator_name, creator_id, thumbnail_url, item_count, discovered_at, last_analyzed_at, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET platform_id = excluded.platform_id, source_url = excluded.source_url, normalized_url = excluded.normalized_url, source_type = excluded.source_type, title = excluded.title, creator_name = excluded.creator_name, creator_id = excluded.creator_id, thumbnail_url = excluded.thumbnail_url, item_count = excluded.item_count, discovered_at = excluded.discovered_at, last_analyzed_at = excluded.last_analyzed_at, metadata_json = excluded.metadata_json")
            .bind(&source.id)
            .bind(&source.platform_id)
            .bind(&source.source_url)
            .bind(&source.normalized_url)
            .bind(source.source_type.as_str())
            .bind(&source.title)
            .bind(&source.creator_name)
            .bind(&source.creator_id)
            .bind(&source.thumbnail_url)
            .bind(source.item_count)
            .bind(&source.discovered_at)
            .bind(&source.last_analyzed_at)
            .bind(metadata_json)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("media_sources.upsert", error))?;
        Ok(())
    }
}

#[async_trait]
impl CollectionRepository for SqliteCollectionRepository {
    async fn get(&self, id: &str) -> RepositoryResult<Option<Collection>> {
        sqlx::query_as::<_, CollectionRow>("SELECT id, source_id, external_id, title, creator_name, item_count, created_at, updated_at FROM collections WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| storage("collections.get", error))
            .map(|result| result.map(Into::into))
    }

    async fn list_by_source(&self, source_id: &str) -> RepositoryResult<Vec<Collection>> {
        sqlx::query_as::<_, CollectionRow>("SELECT id, source_id, external_id, title, creator_name, item_count, created_at, updated_at FROM collections WHERE source_id = ? ORDER BY created_at, id")
            .bind(source_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| storage("collections.list_by_source", error))
            .map(|rows| rows.into_iter().map(Into::into).collect())
    }

    async fn upsert(&self, collection: &Collection) -> RepositoryResult<()> {
        sqlx::query("INSERT INTO collections (id, source_id, external_id, title, creator_name, item_count, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET source_id = excluded.source_id, external_id = excluded.external_id, title = excluded.title, creator_name = excluded.creator_name, item_count = excluded.item_count, updated_at = excluded.updated_at")
            .bind(&collection.id)
            .bind(&collection.source_id)
            .bind(&collection.external_id)
            .bind(&collection.title)
            .bind(&collection.creator_name)
            .bind(collection.item_count)
            .bind(&collection.created_at)
            .bind(&collection.updated_at)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("collections.upsert", error))?;
        Ok(())
    }
}

#[async_trait]
impl MediaItemRepository for SqliteMediaItemRepository {
    async fn get(&self, id: &str) -> RepositoryResult<Option<MediaItem>> {
        sqlx::query_as::<_, MediaItemRow>("SELECT id, source_id, collection_id, external_id, canonical_url, title, creator_name, creator_id, thumbnail_url, duration_ms, published_at, position, metadata_json, first_seen_at, last_seen_at FROM media_items WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| storage("media_items.get", error))?
            .map(TryInto::try_into)
            .transpose()
    }

    async fn list_by_source(&self, source_id: &str) -> RepositoryResult<Vec<MediaItem>> {
        let rows = sqlx::query_as::<_, MediaItemRow>("SELECT id, source_id, collection_id, external_id, canonical_url, title, creator_name, creator_id, thumbnail_url, duration_ms, published_at, position, metadata_json, first_seen_at, last_seen_at FROM media_items WHERE source_id = ? ORDER BY COALESCE(position, 9223372036854775807), first_seen_at, id")
            .bind(source_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| storage("media_items.list_by_source", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn upsert(&self, item: &MediaItem) -> RepositoryResult<()> {
        let metadata_json = serialize_json("media_items.metadata_json", &item.metadata_json)?;
        sqlx::query("INSERT INTO media_items (id, source_id, collection_id, external_id, canonical_url, title, creator_name, creator_id, thumbnail_url, duration_ms, published_at, position, metadata_json, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET source_id = excluded.source_id, collection_id = excluded.collection_id, external_id = excluded.external_id, canonical_url = excluded.canonical_url, title = excluded.title, creator_name = excluded.creator_name, creator_id = excluded.creator_id, thumbnail_url = excluded.thumbnail_url, duration_ms = excluded.duration_ms, published_at = excluded.published_at, position = excluded.position, metadata_json = excluded.metadata_json, last_seen_at = excluded.last_seen_at")
            .bind(&item.id)
            .bind(&item.source_id)
            .bind(&item.collection_id)
            .bind(&item.external_id)
            .bind(&item.canonical_url)
            .bind(&item.title)
            .bind(&item.creator_name)
            .bind(&item.creator_id)
            .bind(&item.thumbnail_url)
            .bind(item.duration_ms)
            .bind(&item.published_at)
            .bind(item.position)
            .bind(metadata_json)
            .bind(&item.first_seen_at)
            .bind(&item.last_seen_at)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("media_items.upsert", error))?;
        Ok(())
    }
}

#[async_trait]
impl MediaFormatRepository for SqliteMediaFormatRepository {
    async fn list_by_item(&self, media_item_id: &str) -> RepositoryResult<Vec<MediaFormat>> {
        let rows = sqlx::query_as::<_, MediaFormatRow>("SELECT id, media_item_id, external_format_id, container, video_codec, audio_codec, width, height, fps, bitrate, sample_rate, channels, file_size_bytes, is_video, is_audio, is_progressive, metadata_json, created_at FROM media_formats WHERE media_item_id = ? ORDER BY COALESCE(height, 0) DESC, id")
            .bind(media_item_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| storage("media_formats.list_by_item", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn upsert(&self, format: &MediaFormat) -> RepositoryResult<()> {
        let metadata_json = serialize_json("media_formats.metadata_json", &format.metadata_json)?;
        sqlx::query("INSERT INTO media_formats (id, media_item_id, external_format_id, container, video_codec, audio_codec, width, height, fps, bitrate, sample_rate, channels, file_size_bytes, is_video, is_audio, is_progressive, metadata_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET media_item_id = excluded.media_item_id, external_format_id = excluded.external_format_id, container = excluded.container, video_codec = excluded.video_codec, audio_codec = excluded.audio_codec, width = excluded.width, height = excluded.height, fps = excluded.fps, bitrate = excluded.bitrate, sample_rate = excluded.sample_rate, channels = excluded.channels, file_size_bytes = excluded.file_size_bytes, is_video = excluded.is_video, is_audio = excluded.is_audio, is_progressive = excluded.is_progressive, metadata_json = excluded.metadata_json, created_at = excluded.created_at")
            .bind(&format.id)
            .bind(&format.media_item_id)
            .bind(&format.external_format_id)
            .bind(&format.container)
            .bind(&format.video_codec)
            .bind(&format.audio_codec)
            .bind(format.width)
            .bind(format.height)
            .bind(format.fps)
            .bind(format.bitrate)
            .bind(format.sample_rate)
            .bind(format.channels)
            .bind(format.file_size_bytes)
            .bind(i64::from(format.is_video))
            .bind(i64::from(format.is_audio))
            .bind(i64::from(format.is_progressive))
            .bind(metadata_json)
            .bind(&format.created_at)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("media_formats.upsert", error))?;
        Ok(())
    }

    async fn replace_for_item(
        &self,
        media_item_id: &str,
        formats: &[MediaFormat],
    ) -> RepositoryResult<()> {
        if formats
            .iter()
            .any(|format| format.media_item_id != media_item_id)
        {
            return Err(RepositoryError::InvalidData {
                details: "all replacement formats must belong to the requested media item"
                    .to_owned(),
            });
        }

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| storage("media_formats.replace.begin", error))?;
        sqlx::query("DELETE FROM media_formats WHERE media_item_id = ?")
            .bind(media_item_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| storage("media_formats.replace.delete", error))?;
        for format in formats {
            let metadata_json =
                serialize_json("media_formats.metadata_json", &format.metadata_json)?;
            sqlx::query("INSERT INTO media_formats (id, media_item_id, external_format_id, container, video_codec, audio_codec, width, height, fps, bitrate, sample_rate, channels, file_size_bytes, is_video, is_audio, is_progressive, metadata_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(&format.id)
                .bind(&format.media_item_id)
                .bind(&format.external_format_id)
                .bind(&format.container)
                .bind(&format.video_codec)
                .bind(&format.audio_codec)
                .bind(format.width)
                .bind(format.height)
                .bind(format.fps)
                .bind(format.bitrate)
                .bind(format.sample_rate)
                .bind(format.channels)
                .bind(format.file_size_bytes)
                .bind(i64::from(format.is_video))
                .bind(i64::from(format.is_audio))
                .bind(i64::from(format.is_progressive))
                .bind(metadata_json)
                .bind(&format.created_at)
                .execute(&mut *transaction)
                .await
                .map_err(|error| storage("media_formats.replace.insert", error))?;
        }
        transaction
            .commit()
            .await
            .map_err(|error| storage("media_formats.replace.commit", error))?;
        Ok(())
    }
}

#[async_trait]
impl DownloadJobRepository for SqliteDownloadJobRepository {
    async fn get(&self, id: &str) -> RepositoryResult<Option<DownloadJob>> {
        sqlx::query_as::<_, DownloadJobRow>("SELECT id, media_item_id, format_id, status, priority, destination_path, temp_path, filename, total_bytes, downloaded_bytes, speed_bytes_per_sec, eta_seconds, retry_count, max_retries, processing_json, etag, last_modified, error_code, error_message, started_at, completed_at, created_at, updated_at FROM download_jobs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| storage("download_jobs.get", error))?
            .map(TryInto::try_into)
            .transpose()
    }

    async fn list_by_status(&self, status: DownloadStatus) -> RepositoryResult<Vec<DownloadJob>> {
        let rows = sqlx::query_as::<_, DownloadJobRow>("SELECT id, media_item_id, format_id, status, priority, destination_path, temp_path, filename, total_bytes, downloaded_bytes, speed_bytes_per_sec, eta_seconds, retry_count, max_retries, processing_json, etag, last_modified, error_code, error_message, started_at, completed_at, created_at, updated_at FROM download_jobs WHERE status = ? ORDER BY priority DESC, created_at, id")
            .bind(status.as_str())
            .fetch_all(&self.pool)
            .await
            .map_err(|error| storage("download_jobs.list_by_status", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_all(&self) -> RepositoryResult<Vec<DownloadJob>> {
        let rows = sqlx::query_as::<_, DownloadJobRow>("SELECT id, media_item_id, format_id, status, priority, destination_path, temp_path, filename, total_bytes, downloaded_bytes, speed_bytes_per_sec, eta_seconds, retry_count, max_retries, processing_json, etag, last_modified, error_code, error_message, started_at, completed_at, created_at, updated_at FROM download_jobs ORDER BY priority DESC, created_at, id")
            .fetch_all(&self.pool)
            .await
            .map_err(|error| storage("download_jobs.list_all", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn insert(&self, job: &DownloadJob) -> RepositoryResult<()> {
        sqlx::query("INSERT INTO download_jobs (id, media_item_id, format_id, status, priority, destination_path, temp_path, filename, total_bytes, downloaded_bytes, speed_bytes_per_sec, eta_seconds, retry_count, max_retries, processing_json, etag, last_modified, error_code, error_message, started_at, completed_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&job.id)
            .bind(&job.media_item_id)
            .bind(&job.format_id)
            .bind(job.status.as_str())
            .bind(job.priority)
            .bind(&job.destination_path)
            .bind(&job.temp_path)
            .bind(&job.filename)
            .bind(job.total_bytes)
            .bind(job.downloaded_bytes)
            .bind(job.speed_bytes_per_sec)
            .bind(job.eta_seconds)
            .bind(job.retry_count)
            .bind(job.max_retries)
            .bind(serialize_json("download_jobs.processing_json", &job.processing_json)?)
            .bind(&job.etag)
            .bind(&job.last_modified)
            .bind(&job.error_code)
            .bind(&job.error_message)
            .bind(&job.started_at)
            .bind(&job.completed_at)
            .bind(&job.created_at)
            .bind(&job.updated_at)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("download_jobs.insert", error))?;
        Ok(())
    }

    async fn update(&self, job: &DownloadJob) -> RepositoryResult<()> {
        let result = sqlx::query("UPDATE download_jobs SET media_item_id = ?, format_id = ?, status = ?, priority = ?, destination_path = ?, temp_path = ?, filename = ?, total_bytes = ?, downloaded_bytes = ?, speed_bytes_per_sec = ?, eta_seconds = ?, retry_count = ?, max_retries = ?, processing_json = ?, etag = ?, last_modified = ?, error_code = ?, error_message = ?, started_at = ?, completed_at = ?, updated_at = ? WHERE id = ?")
            .bind(&job.media_item_id)
            .bind(&job.format_id)
            .bind(job.status.as_str())
            .bind(job.priority)
            .bind(&job.destination_path)
            .bind(&job.temp_path)
            .bind(&job.filename)
            .bind(job.total_bytes)
            .bind(job.downloaded_bytes)
            .bind(job.speed_bytes_per_sec)
            .bind(job.eta_seconds)
            .bind(job.retry_count)
            .bind(job.max_retries)
            .bind(serialize_json("download_jobs.processing_json", &job.processing_json)?)
            .bind(&job.etag)
            .bind(&job.last_modified)
            .bind(&job.error_code)
            .bind(&job.error_message)
            .bind(&job.started_at)
            .bind(&job.completed_at)
            .bind(&job.updated_at)
            .bind(&job.id)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("download_jobs.update", error))?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                entity: "download_job",
                id: job.id.clone(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl HistoryRepository for SqliteHistoryRepository {
    async fn list(&self, query: Option<&str>) -> RepositoryResult<Vec<HistoryEntry>> {
        let query = query.map(str::trim).filter(|value| !value.is_empty());
        let rows = if let Some(query) = query {
            let pattern = format!("%{query}%");
            sqlx::query_as::<_, HistoryEntryRow>("SELECT id, job_id, media_item_id, format_id, platform_id, platform_name, source_url, title, creator_name, destination_path, filename, status, size_bytes, error_code, error_message, created_at, finished_at FROM history_entries WHERE title LIKE ? OR filename LIKE ? OR source_url LIKE ? OR creator_name LIKE ? OR platform_name LIKE ? ORDER BY finished_at DESC, id")
                .bind(&pattern)
                .bind(&pattern)
                .bind(&pattern)
                .bind(&pattern)
                .bind(&pattern)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| storage("history.list.search", error))?
        } else {
            sqlx::query_as::<_, HistoryEntryRow>("SELECT id, job_id, media_item_id, format_id, platform_id, platform_name, source_url, title, creator_name, destination_path, filename, status, size_bytes, error_code, error_message, created_at, finished_at FROM history_entries ORDER BY finished_at DESC, id")
                .fetch_all(&self.pool)
                .await
                .map_err(|error| storage("history.list", error))?
        };
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn delete(&self, id: &str) -> RepositoryResult<bool> {
        let result = sqlx::query("DELETE FROM history_entries WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("history.delete", error))?;
        Ok(result.rows_affected() > 0)
    }

    async fn clear(&self) -> RepositoryResult<u64> {
        let result = sqlx::query("DELETE FROM history_entries")
            .execute(&self.pool)
            .await
            .map_err(|error| storage("history.clear", error))?;
        Ok(result.rows_affected())
    }

    async fn upsert(&self, entry: &HistoryEntry) -> RepositoryResult<()> {
        sqlx::query("INSERT INTO history_entries (id, job_id, media_item_id, format_id, platform_id, platform_name, source_url, title, creator_name, destination_path, filename, status, size_bytes, error_code, error_message, created_at, finished_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(job_id) DO UPDATE SET media_item_id = excluded.media_item_id, format_id = excluded.format_id, platform_id = excluded.platform_id, platform_name = excluded.platform_name, source_url = excluded.source_url, title = excluded.title, creator_name = excluded.creator_name, destination_path = excluded.destination_path, filename = excluded.filename, status = excluded.status, size_bytes = excluded.size_bytes, error_code = excluded.error_code, error_message = excluded.error_message, finished_at = excluded.finished_at")
            .bind(&entry.id)
            .bind(&entry.job_id)
            .bind(&entry.media_item_id)
            .bind(&entry.format_id)
            .bind(&entry.platform_id)
            .bind(&entry.platform_name)
            .bind(&entry.source_url)
            .bind(&entry.title)
            .bind(&entry.creator_name)
            .bind(&entry.destination_path)
            .bind(&entry.filename)
            .bind(entry.status.as_str())
            .bind(entry.size_bytes)
            .bind(&entry.error_code)
            .bind(&entry.error_message)
            .bind(&entry.created_at)
            .bind(&entry.finished_at)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("history.upsert", error))?;
        Ok(())
    }
}

#[async_trait]
impl JobEventRepository for SqliteJobEventRepository {
    async fn list_by_job(&self, job_id: &str) -> RepositoryResult<Vec<JobEvent>> {
        let rows = sqlx::query_as::<_, JobEventRow>("SELECT id, job_id, event_type, payload_json, created_at FROM job_events WHERE job_id = ? ORDER BY created_at, id")
            .bind(job_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| storage("job_events.list_by_job", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn append(&self, event: &JobEvent) -> RepositoryResult<()> {
        let payload_json = serialize_json("job_events.payload_json", &event.payload_json)?;
        sqlx::query("INSERT INTO job_events (id, job_id, event_type, payload_json, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&event.id)
            .bind(&event.job_id)
            .bind(&event.event_type)
            .bind(payload_json)
            .bind(&event.created_at)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("job_events.append", error))?;
        Ok(())
    }
}

#[async_trait]
impl ScheduleRepository for SqliteScheduleRepository {
    async fn get(&self, id: &str) -> RepositoryResult<Option<Schedule>> {
        sqlx::query_as::<_, ScheduleRow>("SELECT id, source_id, schedule_type, cron_expression, interval_seconds, enabled, last_run_at, next_run_at, configuration_json, created_at, updated_at FROM schedules WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| storage("schedules.get", error))?
            .map(TryInto::try_into)
            .transpose()
    }

    async fn list_all(&self) -> RepositoryResult<Vec<Schedule>> {
        let rows = sqlx::query_as::<_, ScheduleRow>("SELECT id, source_id, schedule_type, cron_expression, interval_seconds, enabled, last_run_at, next_run_at, configuration_json, created_at, updated_at FROM schedules ORDER BY COALESCE(next_run_at, '9999-12-31T23:59:59Z'), id")
            .fetch_all(&self.pool)
            .await
            .map_err(|error| storage("schedules.list_all", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_due(&self, now: &str) -> RepositoryResult<Vec<Schedule>> {
        let rows = sqlx::query_as::<_, ScheduleRow>("SELECT id, source_id, schedule_type, cron_expression, interval_seconds, enabled, last_run_at, next_run_at, configuration_json, created_at, updated_at FROM schedules WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ? ORDER BY next_run_at, id")
            .bind(now)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| storage("schedules.list_due", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn upsert(&self, schedule: &Schedule) -> RepositoryResult<()> {
        let configuration_json =
            serialize_json("schedules.configuration_json", &schedule.configuration_json)?;
        sqlx::query("INSERT INTO schedules (id, source_id, schedule_type, cron_expression, interval_seconds, enabled, last_run_at, next_run_at, configuration_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET source_id = excluded.source_id, schedule_type = excluded.schedule_type, cron_expression = excluded.cron_expression, interval_seconds = excluded.interval_seconds, enabled = excluded.enabled, last_run_at = excluded.last_run_at, next_run_at = excluded.next_run_at, configuration_json = excluded.configuration_json, updated_at = excluded.updated_at")
            .bind(&schedule.id)
            .bind(&schedule.source_id)
            .bind(schedule.schedule_type.as_str())
            .bind(&schedule.cron_expression)
            .bind(schedule.interval_seconds)
            .bind(i64::from(schedule.enabled))
            .bind(&schedule.last_run_at)
            .bind(&schedule.next_run_at)
            .bind(configuration_json)
            .bind(&schedule.created_at)
            .bind(&schedule.updated_at)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("schedules.upsert", error))?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> RepositoryResult<bool> {
        let result = sqlx::query("DELETE FROM schedules WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("schedules.delete", error))?;
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl SettingsRepository for SqliteSettingsRepository {
    async fn get(&self, key: &str) -> RepositoryResult<Option<SettingRecord>> {
        sqlx::query_as::<_, SettingRow>(
            "SELECT key, value_json, updated_at FROM settings WHERE key = ?",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| storage("settings.get", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list(&self) -> RepositoryResult<Vec<SettingRecord>> {
        let rows = sqlx::query_as::<_, SettingRow>(
            "SELECT key, value_json, updated_at FROM settings ORDER BY key",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| storage("settings.list", error))?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn upsert(&self, setting: &SettingRecord) -> RepositoryResult<()> {
        let value_json = serialize_required_json("settings.value_json", &setting.value_json)?;
        sqlx::query("INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at")
            .bind(&setting.key)
            .bind(value_json)
            .bind(&setting.updated_at)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("settings.upsert", error))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> RepositoryResult<bool> {
        let result = sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("settings.delete", error))?;
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl LicenseStateRepository for SqliteLicenseStateRepository {
    async fn get(&self) -> RepositoryResult<Option<LicenseState>> {
        sqlx::query_as::<_, LicenseStateRow>("SELECT id, license_key_hash, plan, status, expires_at, device_id, last_validated_at, metadata_json FROM license_state WHERE id = 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| storage("license_state.get", error))?
            .map(TryInto::try_into)
            .transpose()
    }

    async fn save(&self, state: &LicenseState) -> RepositoryResult<()> {
        let metadata_json = serialize_json("license_state.metadata_json", &state.metadata_json)?;
        sqlx::query("INSERT INTO license_state (id, license_key_hash, plan, status, expires_at, device_id, last_validated_at, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET license_key_hash = excluded.license_key_hash, plan = excluded.plan, status = excluded.status, expires_at = excluded.expires_at, device_id = excluded.device_id, last_validated_at = excluded.last_validated_at, metadata_json = excluded.metadata_json")
            .bind(state.id)
            .bind(&state.license_key_hash)
            .bind(state.plan.as_str())
            .bind(state.status.as_str())
            .bind(&state.expires_at)
            .bind(&state.device_id)
            .bind(&state.last_validated_at)
            .bind(metadata_json)
            .execute(&self.pool)
            .await
            .map_err(|error| storage("license_state.save", error))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{SqliteRepositories, SqliteSettingsRepository};
    use crate::application::ports::{
        HistoryRepository, MediaSourceRepository, PlatformRepository, ScheduleRepository,
        SettingsRepository,
    };
    use crate::application::settings_service::{SettingKey, SettingValue, SettingsService, Theme};
    use crate::domain::entities::{
        DownloadStatus, HistoryEntry, MediaSource, Platform, Schedule, ScheduleType, SettingRecord,
        SourceType,
    };
    use serde_json::json;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;

    async fn test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .in_memory(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .connect_with(options)
            .await
            .expect("pool should initialize");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations should apply");
        pool
    }

    fn platform() -> Platform {
        Platform {
            id: "platform-1".to_owned(),
            slug: "generic".to_owned(),
            name: "Generic".to_owned(),
            enabled: true,
            adapter_version: Some("0.1.0".to_owned()),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[tokio::test]
    async fn platform_repository_round_trips_and_lists_enabled_rows() {
        let pool = test_pool().await;
        let repositories = SqliteRepositories::new(&pool);
        repositories
            .platforms
            .upsert(&platform())
            .await
            .expect("platform should persist");

        let loaded = repositories
            .platforms
            .get("platform-1")
            .await
            .expect("platform lookup should succeed")
            .expect("platform should exist");
        assert_eq!(loaded, platform());
        assert_eq!(
            repositories.platforms.list_enabled().await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn history_repository_round_trips_search_delete_and_clear() {
        let pool = test_pool().await;
        let repositories = SqliteRepositories::new(&pool);

        sqlx::query("INSERT INTO platforms (id, slug, name, enabled, adapter_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind("platform-1")
            .bind("generic")
            .bind("Generic")
            .bind(true)
            .bind("0.1.0")
            .bind("2026-01-01T00:00:00Z")
            .bind("2026-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .expect("platform fixture should persist");
        sqlx::query("INSERT INTO media_sources (id, platform_id, source_url, normalized_url, source_type, discovered_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind("source-1")
            .bind("platform-1")
            .bind("https://example.com/media/1")
            .bind("https://example.com/media/1")
            .bind("generic")
            .bind("2026-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .expect("source fixture should persist");
        sqlx::query("INSERT INTO media_items (id, source_id, canonical_url, title, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind("item-1")
            .bind("source-1")
            .bind("https://example.com/media/1")
            .bind("Example title")
            .bind("2026-01-01T00:00:00Z")
            .bind("2026-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .expect("item fixture should persist");
        for job_id in ["job-1", "job-2"] {
            sqlx::query("INSERT INTO download_jobs (id, media_item_id, status, destination_path, filename, downloaded_bytes, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(job_id)
                .bind("item-1")
                .bind("completed")
                .bind("/downloads")
                .bind(format!("{job_id}.mp4"))
                .bind(128_i64)
                .bind("2026-01-01T00:00:00Z")
                .bind("2026-01-01T00:01:00Z")
                .execute(&pool)
                .await
                .expect("job fixture should persist");
        }

        let entry = |id: &str, job_id: &str, title: &str| HistoryEntry {
            id: id.to_owned(),
            job_id: job_id.to_owned(),
            media_item_id: "item-1".to_owned(),
            format_id: None,
            platform_id: "platform-1".to_owned(),
            platform_name: "Generic".to_owned(),
            source_url: "https://example.com/media/1".to_owned(),
            title: title.to_owned(),
            creator_name: Some("Creator".to_owned()),
            destination_path: "/downloads".to_owned(),
            filename: format!("{job_id}.mp4"),
            status: DownloadStatus::Completed,
            size_bytes: Some(128),
            error_code: None,
            error_message: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            finished_at: "2026-01-01T00:01:00Z".to_owned(),
        };
        repositories
            .history
            .upsert(&entry("history-1", "job-1", "Example title"))
            .await
            .expect("first history entry should persist");
        repositories
            .history
            .upsert(&entry("history-2", "job-2", "Another title"))
            .await
            .expect("second history entry should persist");

        let all = repositories
            .history
            .list(None)
            .await
            .expect("history should list");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "history-1");
        assert_eq!(
            repositories
                .history
                .list(Some("example title"))
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(repositories
            .history
            .list(Some("not present"))
            .await
            .unwrap()
            .is_empty());
        assert!(repositories.history.delete("history-1").await.unwrap());
        assert_eq!(repositories.history.list(None).await.unwrap().len(), 1);
        assert_eq!(repositories.history.clear().await.unwrap(), 1);
        assert!(repositories.history.list(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn schedule_repository_round_trips_lists_due_and_deletes() {
        let pool = test_pool().await;
        let repositories = SqliteRepositories::new(&pool);
        repositories.platforms.upsert(&platform()).await.unwrap();
        repositories
            .media_sources
            .upsert(&MediaSource {
                id: "source-1".to_owned(),
                platform_id: "platform-1".to_owned(),
                source_url: "https://example.com/source".to_owned(),
                normalized_url: "https://example.com/source".to_owned(),
                source_type: SourceType::Generic,
                title: Some("Source".to_owned()),
                creator_name: None,
                creator_id: None,
                thumbnail_url: None,
                item_count: None,
                discovered_at: "2026-01-01T00:00:00Z".to_owned(),
                last_analyzed_at: None,
                metadata_json: None,
            })
            .await
            .unwrap();
        let schedule = Schedule {
            id: "schedule-1".to_owned(),
            source_id: "source-1".to_owned(),
            schedule_type: ScheduleType::Interval,
            cron_expression: None,
            interval_seconds: Some(3600),
            enabled: true,
            last_run_at: None,
            next_run_at: Some("2026-01-01T00:00:00Z".to_owned()),
            configuration_json: Some(json!({
                "format_id": null,
                "destination_path": "/downloads",
                "filename_template": "{title}.mp4",
                "auto_download_new_items": true
            })),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        repositories.schedules.upsert(&schedule).await.unwrap();
        assert_eq!(
            repositories.schedules.get("schedule-1").await.unwrap(),
            Some(schedule.clone())
        );
        assert_eq!(repositories.schedules.list_all().await.unwrap().len(), 1);
        assert_eq!(
            repositories
                .schedules
                .list_due("2026-01-01T00:01:00Z")
                .await
                .unwrap(),
            vec![schedule]
        );
        assert!(repositories.schedules.delete("schedule-1").await.unwrap());
        assert!(repositories.schedules.list_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn typed_settings_service_round_trips_and_resets_values() {
        let pool = test_pool().await;
        let repositories = SqliteRepositories::new(&pool);
        let service = SettingsService::new(repositories.settings.clone());

        service
            .set(SettingKey::Theme, SettingValue::Theme(Theme::Dark))
            .await
            .expect("typed setting should persist");
        assert_eq!(
            service.get(SettingKey::Theme).await.unwrap(),
            Some(SettingValue::Theme(Theme::Dark))
        );
        assert!(service.reset(SettingKey::Theme).await.unwrap());
        assert_eq!(service.get(SettingKey::Theme).await.unwrap(), None);
        assert_eq!(
            service
                .get_or_default(SettingKey::ConcurrentJobs)
                .await
                .unwrap(),
            Some(SettingValue::ConcurrentJobs(3))
        );
    }

    #[tokio::test]
    async fn settings_repository_round_trips_json_and_deletes() {
        let pool = test_pool().await;
        let repository = SqliteSettingsRepository::new(&pool);
        repository
            .upsert(&SettingRecord {
                key: "ui.theme".to_owned(),
                value_json: json!("dark"),
                updated_at: "unix:1".to_owned(),
            })
            .await
            .expect("setting should persist");
        let record = repository
            .get("ui.theme")
            .await
            .expect("setting lookup should succeed")
            .expect("setting should exist");
        assert_eq!(record.value_json, json!("dark"));
        assert!(repository.delete("ui.theme").await.unwrap());
        assert!(repository.get("ui.theme").await.unwrap().is_none());
    }
}
