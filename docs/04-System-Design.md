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

## 19. Implemented Phase 2 Persistence Foundation

The Tauri startup path resolves the application-data directory, creates it when necessary, opens `umd.sqlite3` through a bounded SQLx SQLite pool, applies a consistent WAL/full-synchronous profile with foreign keys enabled, runs embedded migrations, and performs a connectivity plus foreign-key health check before managing the database in application state. A migration failure prevents normal startup from continuing.

Phase 2 does not yet expose repository or settings IPC commands. The database wrapper is intentionally isolated behind the Rust persistence module. The application layer now defines repository ports for all persisted entities, while SQLite adapters provide typed row mapping and bound-parameter queries. The `AppServices` bundle is constructed once during Tauri startup and manages repositories, typed settings, and transaction coordination as application state.

Analysis snapshots are written in one short transaction: platform, source, collections, items, and each item’s format snapshot are upserted together, with existing formats replaced for each item. Snapshot ownership is validated before the transaction begins, and a failure in any row or format rolls back the full snapshot. Download-job creation and status transitions update the job and append the corresponding `job_events` record in the same transaction. No transaction spans network I/O, FFmpeg work, filesystem copying, or user interaction. Persistence errors map to stable database-unavailable, database-migration-failed, and database-corrupt application error categories without exposing raw SQLx diagnostics to the UI.

The typed settings service is application-layer code, not UI state. It accepts a finite key set, validates each value against its key-specific constraints, persists serialized JSON through the settings repository, provides typed defaults, and supports reset operations. No settings IPC commands are exposed yet.


## 20. Phase 3/4 Adapter Registry and Analyzer

Tauri initializes the `AdapterRegistry` and `AnalyzerService` after the database, repositories, and transaction coordinator are ready. The registry currently contains only the approved Reddit adapter. The analyzer accepts a parsed URL plus an optional explicit platform, performs automatic or explicit adapter selection, normalizes the source, executes metadata analysis, and persists the resulting snapshot before returning the response.

`analyze_url` is intentionally a thin IPC command. It does not contain platform logic, database queries, network policy, or download behavior. Adapter errors are mapped to user-safe `AppError` values without raw response bodies, input URLs, SQL diagnostics, credentials, cookies, or signed media URLs. A successful response is not a download authorization; it is a persisted analysis result that later queue/download phases may consume.


## 21. Phase 4 Analysis Results UI

The React analyzer panel submits a typed request through the Tauri bridge and keeps the request lifecycle local to the analyzer feature. While pending, the submit control is disabled and reports progress. On success, the results view renders the normalized source, adapter capabilities, media-item metadata, and available format descriptors. On failure, the UI renders only the stable application error message, user action, and retryability hint.

The browser UI does not directly fetch thumbnails, media files, API responses, or platform pages. The analysis-results table is a presentation of Rust-returned metadata. TikTok is visible as a supported registry choice but intentionally returns an authorization-required error until an approved, credential-compatible integration is designed; no credentials are accepted by this feature.

## 22. Phase 5 Slice 1 Downloader Policies

The first downloader slice is pure Rust domain logic. `DownloadStateMachine` permits only the documented queue transitions: queued to resolving, resolving to downloading or failure, downloading to pause/process/complete/failure/cancel, paused back to downloading or cancel, processing to complete or failure, and failed back to queued for an explicit retry. Completed and cancelled states are terminal. It validates non-negative counters, prevents downloaded bytes from exceeding a known total, and rejects completion before a known total has been reached.

`RetryPolicy` automatically classifies only network and rate-limit failures as retryable. It enforces the configured retry budget and backoff bounds, computes capped exponential delays, and accepts bounded jitter as a pure input so future scheduling can supply randomness without making policy tests nondeterministic. No worker sleeps, filesystem operation, network request, IPC command, or database migration is part of this slice.

ETag and Last-Modified persistence are intentionally deferred to the streaming/resume slice, where their schema and response semantics can be tested together. The planned first `create_download` command will accept an optional user-selected absolute destination directory, subject to Rust-owned path containment and filename safety validation; Slice 1 does not expose the command.

## 23. Phase 5 Slice 2 Job Coordination

`create_download_job` is an application use case, not a raw repository insert. It validates the new job state, requires the media item to exist, verifies optional format ownership, checks the initial event reference, and then persists the job and queued event atomically. The selected destination is treated as data only; path containment and filename safety remain dedicated responsibilities of the future download-plan boundary.

`transition_download_job` loads the current job, prevents media-item or format ownership changes, validates the requested state transition and job invariants, and delegates to a transaction that re-reads the persisted status before writing. Illegal transitions are rejected without changing the job or appending an event.

Queue claiming uses a single atomic SQLite update ordered by priority, creation time, and stable ID. Exactly one queued job becomes `resolving`, and the corresponding `resolving` event is inserted in the same transaction. Concurrent claimers therefore observe one winner and one empty result rather than both acquiring the same job. The transaction is short-lived and does not include network, filesystem, worker, or retry-delay work.

## 24. Phase 5 Slice 3 Download-Plan Resolution

The application resolves a download plan only from persisted identifiers: media item ID, format ID, an absolute destination root, and a safe filename. It loads the media item, source, platform, and format through repositories, verifies format ownership, and then invokes the pure plan policy. No caller-supplied remote URL is accepted by this boundary.

The current policy accepts only adapter-produced Reddit progressive formats whose metadata contains an HTTPS public URL hosted by `v.redd.it` or a `*.redd.it` subdomain. Non-progressive formats such as DASH and HLS are rejected before their metadata URL is considered. Unsupported platform IDs, missing metadata, credentials, fragments, non-HTTPS URLs, and unapproved hosts fail closed.

The path policy produces a final destination and matching `.part` path without creating files or directories. It rejects relative roots, parent traversal, symlinked root ancestors, existing destination symlinks, filename separators, control characters, reserved device names, trailing dot/space names, and final names ending in `.part`. Filesystem transfer, atomic rename, and directory creation remain future streaming-engine responsibilities.

## 25. Phase 5 Slice 4 Streaming Engine

The streaming engine consumes a validated plan and performs one bounded progressive transfer. It disables redirects, applies a timeout and maximum response-byte limit, revalidates the HTTPS Reddit host policy, requires HTTP 200, and checks both the plan’s known size and the response’s declared `Content-Length` when available.

The response body is consumed as chunks and written directly to the plan’s `.part` path using asynchronous file I/O. The engine never buffers the complete media file. It checks cancellation before file creation, between chunks, and after the body ends; a cancellation or transfer error leaves the `.part` file available for later recovery. It flushes and synchronizes successful output but does not rename it, update a job row, emit progress events, perform range requests, or schedule retries.

## 26. Phase 5 Slice 5 Finalization and Durable Results

After a successful stream, `finalize_part` validates the result byte count, confirms that the temporary path is a regular non-symlink file, refuses an existing final destination, synchronizes the temporary file, performs an in-directory rename, synchronizes the parent directory, and verifies final size. It does not overwrite files and does not claim that filesystem rename and database commit form one cross-system transaction.

Durable progress is written only while a job is `downloading`. The transaction validates non-negative counters, total-size bounds, non-negative speed and ETA, updates the progress fields, and inserts a progress event in one SQLite transaction. Completion then requires the `downloading` to `completed` state transition, a final path matching the persisted destination root and filename, and a final byte count equal to the known total when present. It clears `temp_path`, records completion time, and appends the completion event atomically.

The service exposes the finalized result to the persistence boundary but leaves filesystem rename and database commit as separate ordered operations. Recovery/reconciliation for a finalized file with an incomplete job row remains a later requirement; no worker queue or resume state is added here.

## 27. Phase 5 Slice 6 Worker Pool and Queue Execution

A worker-pool run starts from the typed concurrency setting, bounded to one through eight workers with a default of three. Each worker repeatedly performs an atomic queued-job claim ordered by priority, creation time, and stable ID. The claim transaction ends before the worker resolves metadata or performs network and filesystem work.

For a claimed job, the executor resolves the persisted plan, moves the job from `resolving` to `downloading`, streams the approved progressive public-media response to `.part`, records a durable progress checkpoint, finalizes the file atomically, and commits the completed job/event transaction. Resolution, streaming, finalization, and persistence errors are recorded as failed-job transitions where the current state permits; an executor error does not stop other workers.

The pool exposes a cooperative shutdown flag. Workers check it before claiming new work, so shutdown prevents additional claims while allowing already-running operations to finish at their current boundary. Resume/range requests, retry-delay scheduling, scheduler triggers, cancellation IPC, and frontend queue controls remain outside this slice.

## 28. Phase 5 Slice 7 Retry, Cancellation, and Live Progress

When streaming returns a retryable network or rate-limit failure, the executor first persists the failed transition, computes a capped exponential delay with bounded deterministic jitter, waits without holding a database transaction, and requeues the job with an incremented retry count. The existing retry budget is authoritative; non-retryable errors and exhausted budgets remain failed.

Active jobs are associated with cancellation tokens in an in-memory registry. A cancellation request sets the token, wakes any token waiters, causes the streaming loop to stop at its next checkpoint, and results in a cancelled job transition rather than a retry. Shutdown prevents new queue claims while active cancellation and transfer boundaries remain cooperative.

The stream observes bytes after successful writes and emits at most one live progress sample per 250 milliseconds per stream, plus a changed final sample. Samples include downloaded bytes, optional total bytes, speed, and ETA. They are delivered through a bounded broadcast channel for future UI/IPC consumers, while the application persists the final checkpoint atomically with its durable progress event.

## 13. Slice 8 Resume, Validators, and IPC

On each download attempt, the worker resolves the persisted plan and reads the durable `.part` offset from the job record. Offset zero uses a normal bounded `200 OK` stream and truncating write. A positive offset requires an exact `.part` length match, then issues a range request with `If-Range` when an ETag or Last-Modified value is available. The engine accepts only a validated `206 Partial Content` response and appends only the validated byte range. The final `downloaded_bytes` value is the complete file length, not merely the length of the latest response body.

The streaming result carries response validators, and the application service persists them atomically with durable progress and completion events. A retryable transfer failure preserves the partial file and requeues the job after policy-controlled backoff; the next attempt can reuse the persisted offset and validator. A missing, malformed, mismatched, or unsupported range response fails closed, preserving the existing partial file for inspection and preventing accidental byte corruption.

The local IPC surface is intentionally narrow. `create_download` validates media/format ownership and the user-selected destination through the Rust download-plan resolver, writes the queued job and initial event atomically, and starts the bounded worker pool. `cancel_download` requests cooperative cancellation by job ID. `get_download_jobs` returns the ordered local queue projection. `subscribe_download_progress` forwards bounded live progress events to the requesting Tauri window. No frontend code performs network transfer or filesystem writes.

## 23. Phase 6 Slice 6.1 Typed Processing Plans

Slice 6.1 resolves processing requests before any subprocess is started. The only accepted operation types are audio/video merge and audio extraction. A plan contains the typed operation and a validated output object with a final path and a unique `.processing.part` path. It does not contain raw FFmpeg arguments, shell fragments, arbitrary filters, user-selected executable paths, or unrestricted codec options.

The processing root is Rust-owned and must satisfy the existing absolute-root, traversal, ancestor-symlink, and containment rules. Every input must be inside that root, must be an existing regular file, and must not be a symlink. Output collisions and temporary-output collisions are rejected before execution. The original input files remain untouched by the planning layer.

Slice 6.1 intentionally does not change the worker lifecycle or invoke FFmpeg. The next execution slice will add an executable resolver and direct argument-array runner, then insert `downloading → processing → completed/failed` around typed plan execution. System-installed FFmpeg is the provisional first runtime strategy; bundled sidecar distribution remains a later replaceable packaging option.

## 24. Phase 6 Slice 6.2 FFmpeg Runner

The process runner sequence is:

```text
Typed processing plan
        ↓
Rust-owned typed argument construction
        ↓
System PATH executable resolution
        ↓
Direct Tokio child process, stdin disabled
        ↓
Bounded stderr collection + cancellation polling
        ↓
Success, non-zero exit, timeout, or cancellation
```

The resolver first verifies a regular executable and probes `ffmpeg -version` without shell interpretation. It accepts only the pinned FFmpeg major version. The runner accepts crate-local argument vectors, never a shell command string, captures at most the configured diagnostic limit, and does not expose raw stderr to the frontend. A cancellation signal terminates the child cooperatively; a timeout terminates the child and returns a non-retryable processing failure. A non-zero exit is mapped to `FFMPEG_FAILED` with a safe user message and an optional internal diagnostic.

Slice 6.2 does not yet construct merge or extraction arguments and does not change the download worker’s `downloading → completed` lifecycle. Later integration will transition through `processing` only after typed operation execution and output validation are available.

## 25. Phase 6 Slice 6.3 Worker Integration Plan

The current `ApplicationJobExecutor` remains download-only. The next integration slice will add a Rust-owned media-processing dependency without exposing it through IPC:

```text
claimed job
   ↓
resolve download plan
   ↓
transition resolving → downloading
   ↓
stream download to validated download .part
   ↓
if processing plan is required:
    transition downloading → processing
    append durable processing event
    build MediaProcessingArguments from MediaProcessingPlan
    run FfmpegProcessRunner with the job cancellation signal
    validate processing temporary output
   ↓
atomic finalization to the user destination
   ↓
complete job and append completion event
```

A process timeout, cancellation, spawn failure, non-zero exit, or output-validation failure must stop before finalization. Cancellation transitions to `cancelled`; process failures transition to `failed` with stable `FFMPEG_FAILED`; deterministic media failures are not automatically retried. A later recovery slice must persist enough typed processing configuration to resume a job that was interrupted after entering `processing`; Slice 6.3 does not add that persistence yet.


## 26. Phase 6 Processing Execution and Recovery

Processing configuration is stored on the download job as tagged JSON so a claimed or restarted job can reconstruct the same typed request. The worker validates the configuration against the job destination before execution; a mismatched final filename, outside-root input, symlink, missing input, or unsupported output extension fails closed.

The completed runtime sequence is:

```text
queued → resolving → downloading
                         ↓ checkpoint
                      processing
                         ↓ typed arguments
                    direct FFmpeg child
                         ↓ output validation
                 atomic processed finalization
                         ↓
                     completed
```

The download intermediate remains present until FFmpeg succeeds. The processing temporary output is independent from the download `.part` path, is synchronized before rename, and is removed on process failure or cancellation when possible. Existing final destinations are rejected rather than overwritten.

The SQLite transaction coordinator now persists `processing_json`, commits the `processing` event atomically with the job transition, and permits completion from `processing` as well as the direct-download `downloading` state. A processing failure does not enter the network retry policy; it becomes `failed` with `FFMPEG_FAILED`. Cancellation terminates the child cooperatively and becomes `cancelled`.

Deterministic fixture tests exercise system-installed FFmpeg when available by generating a small video-only MP4 and AAC M4A, merging them through the fixed policy, extracting audio from the merged result, validating non-empty outputs, and checking atomic finalization. The fixture test returns early when the supported system FFmpeg is unavailable, while resolver tests separately cover that missing-tool condition.


## 27. Phase 7 Queue UI State Flow

The Queue workspace hydrates from `get_download_jobs` through TanStack Query with a five-second refresh interval. It attaches the existing `download-progress` event subscription and applies payloads to matching jobs in the transient Zustand projection. A refresh remains authoritative for status, error code, retry count, destination, and processing configuration; live events are limited to transfer telemetry.

```text
get_download_jobs
       ↓
TanStack Query authoritative job list
       ↓ merge
Zustand transient selection/filter/sort/progress projection
       ↑
download-progress Tauri events
```

The `processing` state is rendered separately from `downloading`: it uses an FFmpeg label, an indeterminate-style progress track when no processed byte total exists, and remains cancellable. A failed processing job displays `Processing failed` when its stable error code is `FFMPEG_FAILED`, without exposing raw FFmpeg stderr. Queue actions invoke only validated Rust commands, and the preview shell handles an unavailable Tauri bridge with a visible retry/error state.


## 28. Phase 8 History Recording Sequence

History is created only after a job reaches a terminal outcome. The worker records completed jobs after successful finalization and durable completion, records non-retryable failures after the failed transition, and records cancellations after the cancelled transition. Retryable failures that return to `queued` are not recorded as history until a later terminal outcome exists.

```text
Terminal job transition
          ↓
Load job, media item, media source, and platform
          ↓
Enrich terminal outcome into HistoryEntry
          ↓
SQLite upsert by job_id
          ↓
History workspace reads ordered entries
```

The application service performs the enrichment join and copies only safe local metadata into the history row: public source URL, title, creator, destination path, filename, terminal status, size, stable error code/message, and timestamps. `HistoryRepository::upsert` is idempotent by job identity, so repeated worker completion paths do not create duplicate records. The worker treats history writes as best-effort and ignores a history persistence error after the terminal job state has already been committed.

Search is implemented as a bound-parameter SQLite query over title, filename, platform name, source URL, and creator name. Delete removes one entry by validated ID, while clear removes all local history rows and returns the affected count. Neither operation touches the underlying media file or download job; history is an independently removable local record.

The frontend calls the following Rust-owned commands and remains unaware of SQLite schema details:

| Command | Purpose | Result |
| --- | --- | --- |
| `get_history` | Load newest-first local terminal entries, optionally filtered by search text. | `HistoryEntry[]` |
| `delete_history_entry` | Delete one history row by ID. | Boolean affected-row result |
| `clear_history` | Delete all history rows. | Number of deleted rows |

The history UI handles an unavailable bridge with a retryable error state. It does not fetch public sources, open private content, read local files, or perform any credential, cookie, DRM, CAPTCHA, anti-bot, or access-control operation.


## 29. Phase 9 Embedded Scheduler Execution

The scheduler is local, opt-in, and active only while the desktop application is running. Its loop wakes every 15 seconds, reads the typed `scheduler.enabled` setting, and does nothing when the setting is false. When enabled, it loads schedules whose persisted `next_run_at` is due, processes each schedule independently, advances its run timestamps, and invokes the existing bounded worker pool only when new queue jobs were created.

```text
Tauri app open
      ↓
Embedded 15-second tick
      ↓
Read scheduler.enabled
      ↓ enabled
Load enabled schedules with next_run_at <= now
      ↓
Validate schedule and typed configuration
      ↓
Verify source adapter advertises scheduling
      ↓
Re-analyze the public source
      ↓
Select progressive formats and expand safe filename template
      ↓
Skip matching existing jobs
      ↓
Create validated queued jobs + queued events
      ↓
Advance last_run_at / next_run_at
      ↓
Run existing DownloadWorkerPool until idle
```

The loop reuses the existing analyzer and queue boundaries rather than performing network or filesystem work in the scheduler module. A schedule run does not accept credentials, cookies, private URLs, signed access material, arbitrary commands, or raw filesystem paths outside the existing Rust-owned destination policy. A source whose adapter does not explicitly advertise scheduling is rejected fail-closed.

Supported timing behavior is deliberately bounded. One-time schedules are disabled after their due attempt; daily and weekly schedules advance by one or seven days; interval schedules require a value between 60 seconds and one year. A failed scheduled analysis is logged and the schedule advances to its next cadence instead of retrying every tick. Duplicate jobs are suppressed by matching media item, format, destination root, and filename across all persisted queue states.

Collection monitoring is an adapter capability, not a scheduler bypass. The scheduler can process a multi-item public analysis result when a future adapter exposes it, but current Reddit capability metadata does not claim scheduling or collections. No platform security or access-control restriction is bypassed to make monitoring work.


## 30. Phase 10 Startup Recovery

Startup recovery runs after database migrations and health checks but before the embedded scheduler is started. The coordinator enumerates non-terminal jobs and handles each job independently so one corrupt artifact does not prevent the rest of the application from starting.

```text
SQLite initialized and healthy
      ↓
Enumerate queued/resolving/downloading/processing jobs
      ↓
Validate persisted destination root, filename, and artifact paths
      ↓
Final file exists and matches known size?
   yes ────────────────► harden permissions → atomically reconcile completed state
   no
      ↓
Processing job?
   yes ────────────────► remove only validated processing artifact → requeue from zero
   no
      ↓
Safe `.part` exists and fits bounds?
   yes ────────────────► requeue with exact durable offset
   no ────────────────► requeue from zero
      ↓
Invalid, unsafe, oversized, or exhausted retry state?
      ↓
Atomically mark failed + append recovery event + best-effort history
      ↓
Start scheduler and worker-enabled application state
```

Recovery uses optimistic status predicates in SQLite transactions. Requeue and unrecoverable failure writes include the complete job state and a recovery event in one transaction. Final-file reconciliation validates the expected destination path, regular-file status, symlink absence, known byte count, and configured response limit before completing the job. The coordinator never follows a persisted symlink or removes an artifact outside the selected destination root.

## 31. Phase 10 Storage and Malformed-Media Safeguards

Before an HTTP body is requested, streaming checks available space in the destination filesystem and requires a fixed free-space headroom in addition to the expected remaining bytes. The same check runs before each chunk is written, which protects against concurrent disk consumption and unknown-length responses. File creation, writes, flushes, syncs, and resume opens classify permission denial and common disk-full conditions separately from generic write failures. Partial files remain available for safe retry or startup recovery when the failure is not a security violation.

Media processing rejects symlink and non-regular inputs, rejects input files above the supported four-GiB bound, and rejects empty, symlinked, non-regular, or oversized FFmpeg outputs before atomic rename. FFmpeg continues to receive direct typed arguments, null standard input, bounded stderr, cancellation polling, and a hard timeout. Malformed media therefore fails as a bounded processing error rather than becoming an unbounded resource-consumption path.
