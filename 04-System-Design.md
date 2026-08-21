# Universal Media Downloader — System Design

## 1. System Objectives

The system must reliably:

1. Detect a platform.
2. Analyze media metadata.
3. Resolve available formats.
4. Build download plans.
5. Execute multiple downloads.
6. Persist progress.
7. Recover after interruption.
8. Process media.
9. Organize output.
10. Maintain history.

## 2. Core Components

```text
UI
 │
 ▼
Tauri Command Layer
 │
 ▼
Application Services
 ├── AnalysisService
 ├── DownloadService
 ├── QueueService
 ├── ScheduleService
 ├── HistoryService
 └── SettingsService
 │
 ├───────────────┬───────────────┐
 ▼               ▼               ▼
AdapterRegistry  DownloadEngine  Repository
 │               │               │
 ▼               ▼               ▼
Adapters         HTTP            SQLite
 │               │
 ▼               ▼
Metadata         FFmpeg
```

## 3. Analyze URL Sequence

```text
User
 │
 │ analyze(url)
 ▼
UI
 │
 ▼
Tauri Command
 │
 ▼
AnalysisService
 │
 ▼
URL Parser
 │
 ▼
PlatformDetector
 │
 ▼
AdapterRegistry
 │
 ▼
PlatformAdapter
 │
 ▼
Source
 │
 ▼
MediaItem[]
 │
 ▼
SQLite cache
 │
 ▼
UI
```

## 4. Download Sequence

```text
User
 │
 │ Add selected item
 ▼
QueueService
 │
 ▼
Create DownloadJob
 │
 ▼
SQLite
 │
 ▼
Worker Pool
 │
 ▼
Resolve DownloadPlan
 │
 ▼
DownloadEngine
 │
 ├── create temp file
 ├── transfer
 ├── emit progress
 └── verify
 │
 ▼
MediaProcessor
 │
 ▼
Atomic rename
 │
 ▼
Mark completed
```

## 5. Queue State Machine

```text
QUEUED
  │
  ▼
RESOLVING
  │
  ▼
DOWNLOADING
 ├──► PAUSED
 │      │
 │      └──► DOWNLOADING
 │
 ├──► CANCELLED
 │
 ├──► FAILED
 │      │
 │      └── retry ──► QUEUED
 │
 ▼
PROCESSING
  │
  ├──► FAILED
  │
  ▼
COMPLETED
```

Terminal states: `COMPLETED`, `CANCELLED`, `FAILED`.

## 6. Worker Pool

Default:

```text
Queue
 ├── Worker 1
 ├── Worker 2
 └── Worker 3
```

Worker lifecycle:

1. Claim job.
2. Change status.
3. Resolve source.
4. Download.
5. Process.
6. Finalize.
7. Release worker.

Use a semaphore/bounded worker pool.

## 7. Retry Strategy

Retry only retryable failures.

Example:

```text
Attempt 1
  ↓ 2 sec
Attempt 2
  ↓ 5 sec
Attempt 3
  ↓ 15 sec
Attempt 4
  ↓
FAILED
```

Do not automatically retry invalid URL, restricted content, unsupported format, or disk-full errors until the underlying condition is fixed.

## 8. Download Resume

If the source supports byte ranges:

```text
existing_size = local .part size
Range: bytes=existing_size-
```

If resume is unavailable, restart and explain why.

## 9. Duplicate Detection

Priority:

1. Platform + external media ID.
2. Canonical URL.
3. Local source fingerprint.

Never use title alone for deduplication.

## 10. File Finalization

```text
video.mp4.part
      │
      ▼
 verify
      │
      ▼
video.mp4
```

Only finalized files become `completed`.

## 11. Scheduler

```text
Scheduler tick
      ↓
Find next_run_at <= now
      ↓
Lock schedule
      ↓
Analyze source
      ↓
Deduplicate
      ↓
Create jobs
      ↓
Calculate next run
      ↓
Persist
```

The scheduler is persistent so schedules survive application restarts.

## 12. Generic Adapter

The generic adapter is intentionally conservative. It may inspect public direct media resources and standard public media elements/manifests where technically accessible.

It must not defeat DRM, authentication, access-control mechanisms, or anti-bot/security protections.

## 13. Resource Management

### Memory

Stream media; never load entire files into RAM.

### Disk

Check free space before starting where possible.

### Network

Configure connection timeout, read timeout, bounded connections, and retry policy.

## 14. IPC

Commands:

```text
analyze_url
create_download
pause_download
resume_download
cancel_download
retry_download
remove_download
get_queue
get_history
create_schedule
update_schedule
delete_schedule
get_settings
update_settings
```

Events:

```text
analysis-progress
download-progress
download-status
queue-updated
notification
diagnostic
```

## 15. Optional Cloud Control Plane

```text
Desktop
   │ HTTPS
   ▼
API Gateway
   │
   ├── Auth Service
   ├── License Service
   ├── Device Service
   ├── Config Service
   └── Telemetry Service
   │
   ├── PostgreSQL
   └── Redis
```

The cloud never receives downloaded media.

## 16. Threat Model

Threats:

- Malicious URL.
- Malicious filename.
- Path traversal.
- Command injection.
- Malicious media file.
- Token leakage.
- Unauthorized license use.

Mitigations:

- Strict URL validation.
- Filename/path sanitization.
- Absolute destination validation.
- Structured subprocess APIs.
- Restricted permissions.
- Secure credential storage.
- Short-lived access tokens.
- Device binding.
- Signed application updates.

## 17. Testing Strategy

### Unit

- URL detection.
- URL normalization.
- Filename sanitization.
- Queue transitions.
- Retry decisions.
- Scheduler calculations.

### Integration

- SQLite repositories.
- Download engine.
- FFmpeg wrapper.
- Adapter contracts.

### E2E

- Analyze.
- Select.
- Queue.
- Download.
- Pause.
- Resume.
- Restart.
- Recover.
- Complete.

### Adapter contract tests

Every adapter must pass:

- Detection.
- Normalization.
- Analysis.
- Capabilities.
- Format resolution.
- Error mapping.

## 18. Performance Targets

Initial engineering targets:

- UI interaction under 100ms during normal local workloads.
- Queue state update under 250ms.
- Incremental rendering for large collections.
- No full-file RAM buffering.
- Startup recovery under 5 seconds for normal databases.

Remote platform latency is outside these guarantees.
