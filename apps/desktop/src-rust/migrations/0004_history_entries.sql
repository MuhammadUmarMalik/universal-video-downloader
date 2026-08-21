CREATE TABLE history_entries (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL UNIQUE,
    media_item_id TEXT NOT NULL,
    format_id TEXT,
    platform_id TEXT NOT NULL,
    platform_name TEXT NOT NULL,
    source_url TEXT NOT NULL,
    title TEXT NOT NULL,
    creator_name TEXT,
    destination_path TEXT NOT NULL,
    filename TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('completed', 'failed', 'cancelled')),
    size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    error_code TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    FOREIGN KEY (job_id) REFERENCES download_jobs(id) ON DELETE CASCADE,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id) ON DELETE RESTRICT,
    FOREIGN KEY (platform_id) REFERENCES platforms(id) ON DELETE RESTRICT
);

CREATE INDEX idx_history_entries_finished_at
    ON history_entries(finished_at DESC, id);
CREATE INDEX idx_history_entries_status
    ON history_entries(status);
CREATE INDEX idx_history_entries_media_item
    ON history_entries(media_item_id);
