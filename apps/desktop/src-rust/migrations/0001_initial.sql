CREATE TABLE platforms (
    id TEXT PRIMARY KEY NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    adapter_version TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE media_sources (
    id TEXT PRIMARY KEY NOT NULL,
    platform_id TEXT NOT NULL,
    source_url TEXT NOT NULL,
    normalized_url TEXT NOT NULL,
    source_type TEXT NOT NULL CHECK (source_type IN ('single', 'playlist', 'channel', 'profile', 'collection', 'generic')),
    title TEXT,
    creator_name TEXT,
    creator_id TEXT,
    thumbnail_url TEXT,
    item_count INTEGER CHECK (item_count IS NULL OR item_count >= 0),
    discovered_at TEXT NOT NULL,
    last_analyzed_at TEXT,
    metadata_json TEXT,
    FOREIGN KEY (platform_id) REFERENCES platforms(id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX uq_media_sources_platform_normalized_url
    ON media_sources(platform_id, normalized_url);
CREATE INDEX idx_media_sources_platform
    ON media_sources(platform_id);
CREATE INDEX idx_media_sources_normalized_url
    ON media_sources(normalized_url);

CREATE TABLE collections (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL,
    external_id TEXT,
    title TEXT,
    creator_name TEXT,
    item_count INTEGER CHECK (item_count IS NULL OR item_count >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (id, source_id),
    FOREIGN KEY (source_id) REFERENCES media_sources(id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX uq_collections_source_external_id
    ON collections(source_id, external_id)
    WHERE external_id IS NOT NULL;
CREATE INDEX idx_collections_source
    ON collections(source_id);

CREATE TABLE media_items (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL,
    collection_id TEXT,
    external_id TEXT,
    canonical_url TEXT NOT NULL,
    title TEXT NOT NULL,
    creator_name TEXT,
    creator_id TEXT,
    thumbnail_url TEXT,
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    published_at TEXT,
    position INTEGER CHECK (position IS NULL OR position >= 0),
    metadata_json TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    FOREIGN KEY (source_id) REFERENCES media_sources(id) ON DELETE RESTRICT,
    FOREIGN KEY (collection_id, source_id) REFERENCES collections(id, source_id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX uq_media_items_source_external_id
    ON media_items(source_id, external_id)
    WHERE external_id IS NOT NULL;
CREATE INDEX idx_media_items_source
    ON media_items(source_id);
CREATE INDEX idx_media_items_collection
    ON media_items(collection_id);
CREATE INDEX idx_media_items_external_id
    ON media_items(external_id);

CREATE TABLE media_formats (
    id TEXT PRIMARY KEY NOT NULL,
    media_item_id TEXT NOT NULL,
    external_format_id TEXT,
    container TEXT,
    video_codec TEXT,
    audio_codec TEXT,
    width INTEGER CHECK (width IS NULL OR width >= 0),
    height INTEGER CHECK (height IS NULL OR height >= 0),
    fps REAL CHECK (fps IS NULL OR fps >= 0),
    bitrate INTEGER CHECK (bitrate IS NULL OR bitrate >= 0),
    sample_rate INTEGER CHECK (sample_rate IS NULL OR sample_rate >= 0),
    channels INTEGER CHECK (channels IS NULL OR channels >= 0),
    file_size_bytes INTEGER CHECK (file_size_bytes IS NULL OR file_size_bytes >= 0),
    is_video INTEGER NOT NULL DEFAULT 0 CHECK (is_video IN (0, 1)),
    is_audio INTEGER NOT NULL DEFAULT 0 CHECK (is_audio IN (0, 1)),
    is_progressive INTEGER NOT NULL DEFAULT 0 CHECK (is_progressive IN (0, 1)),
    metadata_json TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (id, media_item_id),
    FOREIGN KEY (media_item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX uq_media_formats_item_external_id
    ON media_formats(media_item_id, external_format_id)
    WHERE external_format_id IS NOT NULL;
CREATE INDEX idx_media_formats_item
    ON media_formats(media_item_id);

CREATE TABLE download_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    media_item_id TEXT NOT NULL,
    format_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('queued', 'resolving', 'downloading', 'processing', 'completed', 'paused', 'cancelled', 'failed')),
    priority INTEGER NOT NULL DEFAULT 0,
    destination_path TEXT NOT NULL,
    temp_path TEXT,
    filename TEXT NOT NULL,
    total_bytes INTEGER CHECK (total_bytes IS NULL OR total_bytes >= 0),
    downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK (downloaded_bytes >= 0),
    speed_bytes_per_sec INTEGER CHECK (speed_bytes_per_sec IS NULL OR speed_bytes_per_sec >= 0),
    eta_seconds INTEGER CHECK (eta_seconds IS NULL OR eta_seconds >= 0),
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    max_retries INTEGER NOT NULL DEFAULT 3 CHECK (max_retries >= 0),
    error_code TEXT,
    error_message TEXT,
    started_at TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id) ON DELETE RESTRICT,
    FOREIGN KEY (format_id, media_item_id) REFERENCES media_formats(id, media_item_id) ON DELETE RESTRICT
);

CREATE INDEX idx_download_jobs_status
    ON download_jobs(status);
CREATE INDEX idx_download_jobs_created
    ON download_jobs(created_at);
CREATE INDEX idx_download_jobs_media_item
    ON download_jobs(media_item_id);
CREATE INDEX idx_download_jobs_format
    ON download_jobs(format_id);

CREATE TABLE job_events (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (job_id) REFERENCES download_jobs(id) ON DELETE CASCADE
);

CREATE INDEX idx_job_events_job
    ON job_events(job_id);
CREATE INDEX idx_job_events_created
    ON job_events(created_at);

CREATE TABLE schedules (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL,
    schedule_type TEXT NOT NULL CHECK (schedule_type IN ('once', 'daily', 'weekly', 'interval')),
    cron_expression TEXT,
    interval_seconds INTEGER CHECK (interval_seconds IS NULL OR interval_seconds > 0),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    last_run_at TEXT,
    next_run_at TEXT,
    configuration_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (source_id) REFERENCES media_sources(id) ON DELETE RESTRICT
);

CREATE INDEX idx_schedules_source
    ON schedules(source_id);
CREATE INDEX idx_schedules_next_run
    ON schedules(next_run_at);

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE license_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    license_key_hash TEXT,
    plan TEXT NOT NULL DEFAULT 'free' CHECK (plan IN ('free', 'pro', 'enterprise')),
    status TEXT NOT NULL DEFAULT 'inactive' CHECK (status IN ('inactive', 'active', 'expired', 'revoked')),
    expires_at TEXT,
    device_id TEXT,
    last_validated_at TEXT,
    metadata_json TEXT
);

INSERT INTO license_state (id, plan, status)
VALUES (1, 'free', 'inactive');
