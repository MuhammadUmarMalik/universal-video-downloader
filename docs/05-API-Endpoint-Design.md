# Universal Media Downloader — API Endpoint Design

## 1. API Strategy

The desktop app does not need a backend for core downloading.

The optional cloud API handles:

- Authentication.
- User profile.
- Device registration.
- Licensing.
- Subscription state.
- Entitlements.
- Remote configuration.
- Optional telemetry.

Example base URL:

`https://api.example.com/v1`

Authentication:

`Authorization: Bearer <access_token>`

## 2. Authentication

### POST /auth/register

```json
{
  "email": "user@example.com",
  "password": "..."
}
```

### POST /auth/login

```json
{
  "email": "user@example.com",
  "password": "..."
}
```

### POST /auth/refresh

```json
{
  "refresh_token": "..."
}
```

### POST /auth/logout

Invalidates the refresh-token session.

## 3. User

### GET /me

Returns current user.

### PATCH /me

Updates allowed profile fields.

## 4. Devices

### GET /devices

Returns registered devices.

### POST /devices

```json
{
  "device_name": "Malik PC",
  "platform": "windows",
  "app_version": "1.0.0"
}
```

### DELETE /devices/{deviceId}

Deactivates a device.

## 5. Licensing

### GET /license

Example:

```json
{
  "plan": "pro",
  "status": "active",
  "expires_at": "2027-01-01T00:00:00Z",
  "features": [
    "unlimited_queue",
    "scheduler",
    "advanced_naming"
  ]
}
```

### POST /license/activate

```json
{
  "license_key": "XXXX-XXXX-XXXX",
  "device_id": "dev_123"
}
```

### POST /license/validate

Validates installed license/device state.

### POST /license/deactivate

Releases device activation.

## 6. Remote Configuration

### GET /config

```json
{
  "minimum_supported_version": "1.0.0",
  "latest_version": "1.2.0",
  "features": {
    "scheduler": true,
    "cloud_sync": false
  }
}
```

Security-sensitive features must fail closed if configuration cannot be validated.

## 7. Telemetry

Telemetry is opt-in.

### POST /telemetry/events

```json
{
  "events": [
    {
      "name": "download_completed",
      "timestamp": "2026-08-20T10:00:00Z",
      "properties": {
        "platform": "youtube",
        "duration_ms": 123456
      }
    }
  ]
}
```

Never send downloaded media, private URLs, cookies, passwords, access tokens, or personal filesystem paths.

## 8. Health

### GET /health

Load balancer/service health.

### GET /version

Returns API version.

## 9. Error Format

```json
{
  "error": {
    "code": "DEVICE_LIMIT_REACHED",
    "message": "Maximum device activations reached.",
    "request_id": "req_123",
    "retryable": false
  }
}
```

## 10. HTTP Status Codes

- `200` Success.
- `201` Created.
- `204` No content.
- `400` Validation error.
- `401` Authentication required.
- `403` Forbidden/entitlement denied.
- `404` Not found.
- `409` Conflict.
- `422` Semantic validation error.
- `429` Rate limited.
- `500` Internal server error.
- `503` Temporarily unavailable.

## 11. Idempotency

Use:

`Idempotency-Key: <unique-key>`

for retryable mutating operations such as device registration, license activation, and telemetry batches.

## 12. Example Rate Limits

Authentication: 10 req/min/IP.

License: 30 req/min/device.

General API: 120 req/min/user.

Telemetry: 300 req/min/device.

These are configurable server-side values.

## 13. Versioning

Use:

`/v1/...`

Breaking changes use `/v2/...`.

## 14. Cloud Database

### users

```sql
CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
```

### devices

```sql
CREATE TABLE devices (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    device_name TEXT NOT NULL,
    platform TEXT NOT NULL,
    app_version TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_devices_user_id ON devices(user_id);
```

### subscriptions

```sql
CREATE TABLE subscriptions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    provider TEXT NOT NULL,
    provider_subscription_id TEXT,
    plan TEXT NOT NULL,
    status TEXT NOT NULL,
    current_period_end TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
```

### license_activations

```sql
CREATE TABLE license_activations (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    device_id UUID NOT NULL REFERENCES devices(id),
    plan TEXT NOT NULL,
    status TEXT NOT NULL,
    activated_at TIMESTAMPTZ NOT NULL,
    deactivated_at TIMESTAMPTZ
);

CREATE INDEX idx_license_activations_user
ON license_activations(user_id);
```

## 15. API Security

- TLS only.
- Secure headers.
- Short-lived access tokens.
- Rotating refresh tokens.
- Argon2id password hashing.
- Session/device revocation.
- Server-side entitlement checks.
- Rate limiting.
- Audit logs for license actions.
- No media proxying through the API.

## 16. API Documentation

Use OpenAPI 3.1 and maintain:

`docs/openapi.yaml`

Generate Swagger UI, TypeScript types, and contract tests from the specification.


## 17. Local Tauri IPC: `analyze_url`

The desktop client invokes the local Rust analyzer through the Tauri command `analyze_url`. This is a local IPC command, not a cloud API endpoint. The frontend sends a URL and may optionally provide an explicit platform identifier; when omitted, the adapter registry performs automatic detection.

### Request

```json
{
  "request": {
    "url": "https://www.reddit.com/r/videos/comments/abc123/title",
    "platform_id": "reddit"
  }
}
```

`platform_id` is optional. If supplied, the selected adapter must detect the URL or the command returns `UNSUPPORTED_PLATFORM`. If omitted and no registered adapter detects the URL, the command returns `UNSUPPORTED_PLATFORM`.

### Response

```json
{
  "platform_id": "reddit",
  "capabilities": {
    "single_item": true,
    "collections": false,
    "audio_only": false,
    "thumbnails": true,
    "metadata": true,
    "resume": false,
    "scheduling": false
  },
  "source": {
    "platform_id": "reddit",
    "canonical_url": "https://www.reddit.com/comments/abc123/",
    "external_id": "abc123"
  },
  "items": [],
  "formats": []
}
```

The actual `items` and `formats` arrays contain the typed persisted-domain projections returned by the selected adapter. A successful analysis also persists the platform, source, item, and format snapshot through the existing Phase 2 transaction coordinator. The analyzer service performs no download, FFmpeg, filesystem-copy, or queue-worker work.

### Error behavior

The command returns the stable local `AppError` shape. Invalid URLs map to `INVALID_URL`; missing or mismatched adapters map to `UNSUPPORTED_PLATFORM`; malformed or inaccessible public media maps to `MEDIA_UNAVAILABLE`; transient Reddit request failures map to retryable `NETWORK_ERROR`; and local snapshot failures map to database-specific stable error codes. Raw HTTP responses, SQLx diagnostics, signed URLs, credentials, cookies, and input URLs are not placed in user-facing diagnostics.

## 18. Local Tauri IPC: Downloader Queue

The downloader commands are local Tauri IPC operations. They accept only structured values and return the typed local domain projection or the stable `AppError` shape. The frontend never performs the transfer itself.

### `create_download`

Request:

```json
{
  "request": {
    "media_item_id": "reddit:item:abc123",
    "format_id": "reddit:format:abc123:fallback",
    "destination_path": "/Users/example/Downloads",
    "filename": "public-video.mp4"
  }
}
```

The Rust application service verifies persisted media and format ownership, resolves the approved public progressive URL, validates the absolute destination directory and filename against traversal, symlink, reserved-name, separator, control-character, and temporary-suffix rules, and atomically writes the queued job plus its initial `queued` event. The response is the complete `DownloadJob` projection, including `status`, byte counters, retry fields, and optional `etag`/`last_modified` validators. A bounded worker-pool run is then scheduled locally.

### `cancel_download`

Request:

```json
{ "jobId": "download-1-20260821000000Z" }
```

The command validates the non-empty ID and requests cooperative cancellation through the managed worker pool. It returns `true` when a registered active job received the cancellation signal and `false` when no active token was registered. The worker owns the state transition to `cancelled` and durable event write.

### `get_download_jobs`

The command has no request body and returns all persisted jobs ordered by priority, creation time, and ID. The response includes only local queue records; it does not expose credentials, cookies, signed access material, or private-content metadata.

### `subscribe_download_progress`

The command has no request body and returns `true` after attaching a bounded broadcast receiver to the requesting window. Progress is emitted as the `download-progress` Tauri event with this payload:

```json
{
  "job_id": "download-1-20260821000000Z",
  "downloaded_bytes": 524288,
  "total_bytes": 1048576,
  "speed_bytes_per_sec": 262144,
  "eta_seconds": 2
}
```

The backend throttles live events before publication. Durable progress is written transactionally and is authoritative after restart; live events are advisory UI updates.

### Resume and validator behavior

A positive persisted offset is resumed only when the `.part` file length exactly matches that offset. The engine sends `Range: bytes={offset}-` and, when a validator exists, `If-Range`. It requires `206 Partial Content`, validates `Content-Range` strictly, appends only the validated body, and captures response ETag and Last-Modified values. Unsupported or mismatched range responses fail closed. Retryable network failures preserve the `.part` file and requeue the job; non-retryable validation and access failures do not retry automatically.


## 15. Phase 6 Processing IPC Extension

`create_download` accepts an optional typed `processing` object. The object is persisted as validated JSON with the job and is reconstructed by the Rust worker; it is not interpreted as a shell command or raw FFmpeg argument list.

Merge request:

```json
{
  "operation": "merge_audio_video",
  "video_input": "/selected/root/video.mp4",
  "audio_input": "/selected/root/audio.m4a",
  "output_filename": "merged.mp4"
}
```

Audio-extraction request:

```json
{
  "operation": "extract_audio",
  "input": "/selected/root/video.mp4",
  "output_filename": "audio.m4a"
}
```

The output filename must match the download job filename. At execution time, the Rust media boundary revalidates all input paths against the selected destination root, rejects symlinks and non-regular files, constructs only the fixed operation policy, invokes system-installed FFmpeg directly, validates the non-empty temporary output, and atomically finalizes it. Invalid configuration, unavailable FFmpeg, process timeout, cancellation, non-zero exit, or output validation failure maps to the stable `FFMPEG_FAILED` application error; cancellation additionally transitions the job to `cancelled`.

When processing is selected, the worker emits a durable `processing` job event after the download has been checkpointed. Completion is permitted only from `processing` after successful output finalization. The IPC surface never exposes raw FFmpeg diagnostics, shell syntax, executable paths, credentials, cookies, or private-content material.


## 19. Phase 8 Local Tauri IPC: History

History commands are local Tauri IPC operations backed by SQLite. They operate on terminal download records only and return typed local values or the stable `AppError` shape. The bridge does not expose SQL, filesystem enumeration, remote fetching, credentials, cookies, signed URLs, or private-content access.

### `get_history`

Request:

```json
{
  "query": "optional title or filename search"
}
```

The request may omit `query` or provide a bounded non-empty search string. The Rust command trims and validates the value before passing it to the application service. The repository performs a case-insensitive bound-parameter search over title, filename, platform name, source URL, and creator name, then returns newest terminal entries first.

Response:

```json
[
  {
    "id": "history-1",
    "job_id": "download-1",
    "media_item_id": "reddit:item:abc123",
    "format_id": "reddit:format:abc123:fallback",
    "platform_id": "reddit",
    "platform_name": "Reddit",
    "source_url": "https://www.reddit.com/comments/abc123/",
    "title": "Public video",
    "creator_name": "creator",
    "destination_path": "/Users/example/Downloads",
    "filename": "public-video.mp4",
    "status": "completed",
    "size_bytes": 524288,
    "error_code": null,
    "error_message": null,
    "created_at": "2026-08-21T00:00:00Z",
    "finished_at": "2026-08-21T00:01:00Z"
  }
]
```

`status` is restricted to `completed`, `failed`, or `cancelled`. Failed records may include stable `error_code` and safe `error_message` values; raw SQL, network, FFmpeg, or signed-URL diagnostics are not returned.

### `delete_history_entry`

Request:

```json
{ "id": "history-1" }
```

The command rejects an empty or malformed identifier before invoking the repository. It returns `true` when a row was deleted and `false` when no row matched. Deleting history does not delete the completed media file and does not mutate the underlying `download_job`.

### `clear_history`

Request: no body.

Response:

```json
3
```

The result is the number of local history rows deleted. The operation affects only `history_entries`; it does not remove media files, media snapshots, platforms, or download jobs. A repository failure maps to the stable local database error boundary.

### Terminal recording contract

The worker invokes the application history-recording service after a completed, terminal failed, or cancelled job. The service loads the related job, media item, media source, and platform, enriches a `HistoryEntry`, and upserts it by `job_id`. History writes are best-effort after the terminal job transaction: a history write failure is not allowed to change the download outcome or prevent queue workers from continuing.


## 20. Phase 9 Local Tauri IPC: Scheduler

The scheduler IPC surface is local-only. Commands manage persisted schedules and the opt-in loop; they do not execute arbitrary commands, accept credentials, or access private content. All schedule writes validate source ownership, timing fields, typed JSON configuration, absolute destination roots, filename templates, and the selected adapter’s scheduling capability before persistence.

### `get_schedules`

Request: no body.

Response: an ordered array of persisted `Schedule` records. Each record includes `id`, `source_id`, `schedule_type`, `interval_seconds`, `enabled`, `last_run_at`, `next_run_at`, timestamps, and the typed `configuration_json` object.

### `create_schedule`

Request:

```json
{
  "source_id": "reddit:source:collection-id",
  "schedule_type": "interval",
  "interval_seconds": 3600,
  "next_run_at": "2026-08-21T12:00:00Z",
  "enabled": true,
  "format_id": null,
  "destination_path": "/Users/example/Downloads",
  "filename_template": "{creator} - {title}.mp4",
  "auto_download_new_items": true
}
```

The command rejects empty IDs, malformed RFC3339 run times, incompatible timing fields, intervals outside 60 seconds through one year, invalid configuration, missing sources, and adapters that do not advertise scheduling. The created schedule is local SQLite state and does not start a separate service.

### `update_schedule`

Request: the create payload plus an existing `id`; `next_run_at` may be `null` when disabling a schedule. The command reloads the existing record, preserves its creation timestamp, validates the replacement source/configuration, and upserts the complete schedule. It never changes historical download jobs.

### `delete_schedule`

Request:

```json
{ "id": "schedule-1" }
```

Response: a boolean indicating whether a persisted row was deleted. Deletion removes only the schedule definition and does not delete media, queue jobs, or history.

### `get_scheduler_enabled` and `set_scheduler_enabled`

`get_scheduler_enabled` has no request body and returns the persisted boolean. `set_scheduler_enabled` accepts `{ "enabled": true }` and returns the saved boolean. The default is `false`; enabling it allows the embedded loop to process due schedules only while the desktop application remains open.

### `run_scheduler_now`

Request: no body.

Response:

```json
{
  "schedules_checked": 2,
  "schedules_processed": 2,
  "jobs_enqueued": 3,
  "schedules_failed": 0
}
```

The command runs the same due-schedule path as the embedded loop and then invokes the existing bounded worker pool if new jobs were queued. Analysis, scheduling, persistence, and queue errors map to stable `UNSUPPORTED_PLATFORM`, `NETWORK_ERROR`, `DATABASE_UNAVAILABLE`, `DATABASE_CORRUPT`, or `UNKNOWN_ERROR` values without returning raw diagnostics.
