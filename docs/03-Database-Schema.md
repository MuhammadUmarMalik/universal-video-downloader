# Universal Media Downloader — Database Schema

## 1. Database

**Engine:** SQLite

**Rust data layer:** SQLx

Use UUID/ULID-style IDs generated locally.

## 2. Entity Relationship

```text
platforms
   │
   └── media_sources
          │
          ├── collections
          │      └── media_items
          │             └── media_formats
          │                    └── download_jobs
          │                           └── job_events
          │
          └── schedules

settings
license_state
```

## 3. platforms

```sql
CREATE TABLE platforms (
    id TEXT PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    adapter_version TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

## 4. media_sources

```sql
CREATE TABLE media_sources (
    id TEXT PRIMARY KEY,
    platform_id TEXT NOT NULL,
    source_url TEXT NOT NULL,
    normalized_url TEXT NOT NULL,
    source_type TEXT NOT NULL,
    title TEXT,
    creator_name TEXT,
    creator_id TEXT,
    thumbnail_url TEXT,
    item_count INTEGER,
    discovered_at TEXT NOT NULL,
    last_analyzed_at TEXT,
    metadata_json TEXT,
    FOREIGN KEY (platform_id) REFERENCES platforms(id)
);

CREATE INDEX idx_media_sources_platform
ON media_sources(platform_id);

CREATE INDEX idx_media_sources_normalized_url
ON media_sources(normalized_url);
```

`source_type` examples: `single`, `playlist`, `channel`, `profile`, `collection`, `generic`.

## 5. collections

```sql
CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    external_id TEXT,
    title TEXT,
    creator_name TEXT,
    item_count INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (source_id) REFERENCES media_sources(id)
);
```

## 6. media_items

```sql
CREATE TABLE media_items (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    collection_id TEXT,
    external_id TEXT,
    canonical_url TEXT NOT NULL,
    title TEXT NOT NULL,
    creator_name TEXT,
    creator_id TEXT,
    thumbnail_url TEXT,
    duration_ms INTEGER,
    published_at TEXT,
    position INTEGER,
    metadata_json TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    FOREIGN KEY (source_id) REFERENCES media_sources(id),
    FOREIGN KEY (collection_id) REFERENCES collections(id)
);

CREATE INDEX idx_media_items_source
ON media_items(source_id);

CREATE INDEX idx_media_items_external_id
ON media_items(external_id);
```

## 7. media_formats

```sql
CREATE TABLE media_formats (
    id TEXT PRIMARY KEY,
    media_item_id TEXT NOT NULL,
    external_format_id TEXT,
    container TEXT,
    video_codec TEXT,
    audio_codec TEXT,
    width INTEGER,
    height INTEGER,
    fps REAL,
    bitrate INTEGER,
    sample_rate INTEGER,
    channels INTEGER,
    file_size_bytes INTEGER,
    is_video INTEGER NOT NULL DEFAULT 0,
    is_audio INTEGER NOT NULL DEFAULT 0,
    is_progressive INTEGER NOT NULL DEFAULT 0,
    metadata_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id)
);

CREATE INDEX idx_media_formats_item
ON media_formats(media_item_id);
```

## 8. download_jobs

```sql
CREATE TABLE download_jobs (
    id TEXT PRIMARY KEY,
    media_item_id TEXT NOT NULL,
    format_id TEXT,
    status TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    destination_path TEXT NOT NULL,
    temp_path TEXT,
    filename TEXT NOT NULL,
    total_bytes INTEGER,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    speed_bytes_per_sec INTEGER,
    eta_seconds INTEGER,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    error_code TEXT,
    error_message TEXT,
    started_at TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id),
    FOREIGN KEY (format_id) REFERENCES media_formats(id)
);

CREATE INDEX idx_download_jobs_status
ON download_jobs(status);

CREATE INDEX idx_download_jobs_created
ON download_jobs(created_at);
```

Statuses:

`queued`, `resolving`, `downloading`, `processing`, `completed`, `paused`, `cancelled`, `failed`.

## 9. job_events

```sql
CREATE TABLE job_events (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (job_id) REFERENCES download_jobs(id)
);

CREATE INDEX idx_job_events_job
ON job_events(job_id);
```

## 10. schedules

```sql
CREATE TABLE schedules (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    schedule_type TEXT NOT NULL,
    cron_expression TEXT,
    interval_seconds INTEGER,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT,
    next_run_at TEXT,
    configuration_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (source_id) REFERENCES media_sources(id)
);

CREATE INDEX idx_schedules_next_run
ON schedules(next_run_at);
```

## 11. settings

```sql
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

Example keys:

- `download.default_directory`
- `download.concurrent_jobs`
- `download.max_retries`
- `download.retry_backoff`
- `download.filename_template`
- `download.folder_template`
- `download.default_quality`
- `download.default_container`
- `ui.theme`
- `notifications.enabled`
- `scheduler.enabled`

## 12. license_state

```sql
CREATE TABLE license_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    license_key_hash TEXT,
    plan TEXT NOT NULL DEFAULT 'free',
    status TEXT NOT NULL DEFAULT 'inactive',
    expires_at TEXT,
    device_id TEXT,
    last_validated_at TEXT,
    metadata_json TEXT
);
```

## 13. Migration Rules

- Version every migration.
- Never edit an applied migration.
- Back up SQLite before destructive migrations.
- Maintain recovery documentation.
- Enable foreign keys.
- Add indexes based on actual query patterns.

## 14. Retention

- History stays local until user deletes it.
- Job events may be pruned after configurable retention.
- Temporary `.part` files are cleaned after terminal failure.
- Cloud telemetry has explicit retention limits if enabled.

## 15. Implemented Phase 2 Baseline

The initial migration is `apps/desktop/src-tauri/migrations/0001_initial.sql`. It creates all ten required tables in parent-to-child order, enables integrity through foreign keys, adds scoped unique indexes for platform/source/external identifiers, constrains finite status and type values, validates non-negative numeric fields, and seeds one inert `license_state` row with `free` and `inactive` values.

Identifiers are stored as text and are currently generated by the application boundary; the identifier format remains a documented decision for the domain/repository follow-up. Timestamps are stored as RFC 3339 UTC text generated by Rust callers. The migration adds composite foreign keys to prevent cross-source collection membership and cross-item format selection. Delete actions are conservative: source and history-bearing parents use `RESTRICT`, format snapshots cascade from media items, and job events cascade from jobs.

The database file is `umd.sqlite3` under the Tauri app-data directory. The Rust wrapper configures a bounded four-connection SQLx pool, SQLite foreign-key enforcement, WAL journal mode, full synchronous durability, and a five-second busy timeout. SQLx embedded migrations are run during Tauri startup, and the Cargo build script watches the migrations directory for changes.

## 16. Repository and Settings Implementation

The Rust application layer defines repository ports for all ten persisted entities. SQLite adapters live under `src/persistence/sqlite/repositories.rs` and use bound parameters, explicit row mappers, typed enum conversion, JSON parsing, and stable repository errors. The repository bundle is constructed from the shared `SqlitePool`; no SQLx types cross into persisted domain entities or the frontend boundary.

The typed settings service accepts only the enumerated setting keys from this document. It validates key/value pairing, absolute default directories, bounded concurrency and retry values, retry-backoff ranges, template length/path-traversal constraints, and quality/container string sizes before serializing values into `settings.value_json`. It supports typed reads, defaults, snapshots, updates, and reset operations. Unknown keys are rejected by default.
