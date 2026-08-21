# Universal Media Downloader — Architecture Document

## 1. Architecture Style

UMD uses a **local-first modular desktop architecture** based on:

- Clean/Hexagonal Architecture.
- Dependency inversion.
- Adapter pattern.
- Repository pattern.
- Event-driven queue updates.

Layers:

1. Presentation
2. Application
3. Domain
4. Platform Adapters
5. Download Engine
6. Media Processing
7. Persistence
8. Infrastructure

## 2. High-Level Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│                     Tauri Desktop App                       │
│                                                             │
│  ┌─────────────────────┐       ┌─────────────────────────┐  │
│  │ React + TypeScript  │◄─────►│ Tauri IPC / Commands    │  │
│  └─────────────────────┘       └────────────┬────────────┘  │
│                                             │               │
│  ┌──────────────────────────────────────────▼─────────────┐ │
│  │                 Rust Application Core                  │ │
│  │ Analyzer │ Queue │ Scheduler │ Settings │ Events       │ │
│  └──────┬─────────────┬──────────────┬───────────┬────────┘ │
│         │             │              │           │          │
│         ▼             ▼              ▼           ▼          │
│ Adapter Registry  Download Engine  SQLite    Secure Store  │
│         │             │                                     │
│   ┌─────┼─────┐       ▼                                     │
│   ▼     ▼     ▼   HTTP + FFmpeg                             │
│ YouTube TikTok Generic                                       │
│ Adapter Adapter Adapter                                      │
│                                                             │
│                     Local File System                       │
└─────────────────────────────────────────────────────────────┘

                    Optional Cloud Control Plane
                               │
                          HTTPS / JSON
                               │
                ┌──────────────▼──────────────┐
                │ Auth / License / Telemetry  │
                │ PostgreSQL + Redis          │
                └─────────────────────────────┘
```

## 3. Repository Structure

```text
universal-media-downloader/
├── apps/
│   └── desktop/
│       ├── src/
│       │   ├── app/
│       │   ├── components/
│       │   ├── features/
│       │   ├── hooks/
│       │   ├── lib/
│       │   ├── stores/
│       │   └── types/
│       └── src-tauri/
│           ├── src/
│           │   ├── commands/
│           │   ├── domain/
│           │   ├── application/
│           │   ├── adapters/
│           │   ├── downloader/
│           │   ├── media/
│           │   ├── scheduler/
│           │   ├── persistence/
│           │   ├── security/
│           │   └── infrastructure/
│           └── migrations/
├── packages/
│   ├── shared-types/
│   ├── ui/
│   └── config/
├── services/
│   └── control-plane/
│       ├── auth/
│       ├── licensing/
│       ├── telemetry/
│       └── api/
├── docs/
├── scripts/
└── tests/
```

## 4. Module Responsibilities

### Presentation

UI rendering, user interaction, local UI state, progress display.

### Application

Use cases such as:

- Analyze URL.
- Create download job.
- Pause/resume/retry.
- Create schedule.
- Search history.

### Domain

Pure business entities:

- DownloadJob
- MediaItem
- MediaFormat
- Collection
- Platform
- Schedule
- DownloadStatus

Domain code must not depend on React, Tauri, SQLite, or HTTP libraries.

### Adapter Layer

Platform-specific detection, analysis, format resolution, and download-plan creation.

### Download Engine

HTTP transfer, range requests, progress, retries, temporary files, concurrency, cancellation.

### Media Processor

FFmpeg integration for merging, container processing, audio extraction, and metadata operations.

### Persistence

SQLite repositories for jobs, media, history, schedules, settings, and license state.

## 5. Adapter Registry

```text
AdapterRegistry
 ├── YouTubeAdapter
 ├── TikTokAdapter
 ├── InstagramAdapter
 ├── FacebookAdapter
 ├── VimeoAdapter
 ├── RedditAdapter
 ├── XAdapter
 └── GenericMediaAdapter
```

Each adapter exposes capability flags such as:

- `singleItem`
- `collections`
- `audioOnly`
- `thumbnails`
- `metadata`
- `resume`
- `scheduling`

## 6. Analysis Pipeline

```text
URL
 ↓
URL Parser
 ↓
Platform Detector
 ↓
Adapter Registry
 ↓
Selected Adapter
 ↓
Capability/Access Validation
 ↓
Analyze
 ↓
MediaItem[]
 ↓
Format Resolution
 ↓
UI + SQLite cache
```

Analysis should remain metadata-oriented and asynchronous.

## 7. Download Pipeline

```text
DownloadJob
 ↓
Validate
 ↓
Resolve Download Plan
 ↓
Create .part file
 ↓
Stream transfer
 ↓
Progress events
 ↓
Verify
 ↓
FFmpeg post-processing if required
 ↓
Atomic rename
 ↓
History update
```

## 8. Concurrency

Default: 3 simultaneous jobs.

Configurable: 1–8.

Use a bounded worker pool/semaphore. Each job owns cancellation, retry, progress, and destination-lock state.

## 9. Crash Recovery

At startup:

1. Load non-terminal jobs.
2. Inspect `.part` files.
3. Check whether source supports resume.
4. Resume or restart.
5. Reconcile completed files.
6. Mark unrecoverable jobs failed.

## 10. Security

Tauri:

- Minimal filesystem permissions.
- Minimal shell permissions.
- Explicit IPC commands.
- Strict CSP.
- Validate all command inputs.

FFmpeg:

- Structured argument arrays.
- No shell command construction from untrusted data.
- Filename/path sanitization.
- Pinned/tested version.

Storage:

- SQLite in application-data directory.
- OS secure storage for secrets.

## 11. Optional Cloud Control Plane

Cloud responsibilities:

- Authentication.
- Subscription state.
- Device activation.
- License validation.
- Feature entitlements.
- Optional telemetry.

Cloud must not proxy downloaded media.

## 12. Observability

Local logs:

- application.log
- analyzer.log
- downloader.log
- ffmpeg.log

Levels:

- ERROR
- WARN
- INFO
- DEBUG

Never log passwords, cookies, access tokens, or sensitive signed URLs.

## 13. Architecture Decisions

### ADR-001
Tauri for lightweight native desktop delivery.

### ADR-002
Rust for core orchestration and download workers.

### ADR-003
SQLite for local-first persistence.

### ADR-004
Adapter interfaces for all platform integrations.

### ADR-005
Cloud is optional.

### ADR-006
FFmpeg is the media-processing layer.

## 14. Scalability

Desktop downloads scale naturally because transfer happens on the user's machine.

Cloud scaling applies only to:

- Authentication.
- Licensing.
- Telemetry.
- Configuration.

This avoids a centralized bandwidth bottleneck.


## 15. Phase 1 Foundation Status

Phase 1 establishes the repository layout under `apps/desktop`, `packages`, `docs`, `tests`, and `scripts`. The desktop presentation boundary is a React/TypeScript Vite application with Tailwind CSS and a local shadcn/ui-compatible button primitive. Zustand and TanStack Query are installed at the application boundary for the planned client-state and server-state responsibilities.

The Tauri shell is configured as a Tauri 2.x Rust crate. Its command boundary currently exposes only `get_foundation_status`, which verifies that the frontend-to-Rust bridge is connected. Rust modules for commands, domain, application, adapters, downloader, media, scheduler, persistence, security, and infrastructure are present; only the domain error/foundation status and infrastructure logging foundations contain Phase 1 behavior. Persistence, platform adapters, download execution, FFmpeg, scheduling, and credential handling remain intentionally deferred.

The initial Tauri capability set is restricted to `core:default`, and the desktop shell does not request filesystem, shell, process, network, or secret-store permissions in Phase 1. Structured logs use `tracing` and JSON formatting. The shared error contract contains stable error codes, a user-facing message, retryability, optional user action, and optional developer diagnostics; raw exceptions are not sent to the UI.

## 16. Phase 3 Reddit Adapter Baseline

Reddit is the approved first Phase 3 adapter. Its core implements the shared `PlatformAdapter` contract for HTTPS Reddit post URLs, canonicalizes supported `/comments/{id}` routes, retrieves bounded JSON through a Rust HTTP client, parses one public video post, and returns typed metadata plus explicitly exposed public media representations.

The v1 capability set is intentionally narrow: single-item posts, metadata, thumbnails, and public video formats are supported; collections, scheduling, audio-only extraction, and resume are not claimed. Media URLs are accepted only over HTTPS and only for `v.redd.it`/`*.redd.it` hosts. The adapter fails closed for absent, malformed, private/restricted, or non-Reddit media resources. It does not use credentials, cookies, browser sessions, CAPTCHA handling, anti-bot workarounds, private-content access, or access-control bypasses.


## 17. Adapter Registry and Analyzer Boundary

The Phase 3 `AdapterRegistry` owns the set of registered platform adapters and exposes two selection modes. Automatic detection selects the first adapter whose `detect` method accepts the parsed URL. Explicit selection looks up the requested adapter and verifies that it detects the URL; a mismatch fails with `UNSUPPORTED_PLATFORM` rather than allowing a platform-specific parser to process an unrelated URL.

The Phase 4 `AnalyzerService` parses and validates the input URL, selects and normalizes the adapter source, invokes adapter analysis, groups the returned formats by media item, persists the complete analysis snapshot through `AppServices`, and returns a frontend-safe typed response. The Tauri `analyze_url` command is the only new IPC entry point in this milestone. It does not initiate downloading or media processing.


## 18. Phase 4 Analysis Results UI and Second Adapter

The React analyzer panel owns URL input, optional explicit platform selection, loading state, typed error presentation, and the `analyze_url` IPC invocation. `AnalysisResults` renders the normalized source, platform capabilities, item metadata, and format table without implementing download behavior or fetching remote media directly from the browser.

TikTok is registered as the second adapter to validate registry extensibility. Its current scope is deliberately limited to canonical HTTPS video URL detection and normalization. Because the documented Display API requires an authorized integration and does not provide a clean anonymous media-byte path, TikTok analysis returns a typed authentication-required error rather than accepting credentials or attempting an undocumented access path. This is a registry-validation adapter, not a download-capable TikTok implementation.

## 19. Phase 5 Slice 1 Downloader Domain Boundary

Phase 5 Slice 1 introduces a pure downloader policy boundary under `src-tauri/src/downloader`. `DownloadStateMachine` owns legal `DownloadStatus` transitions and validates persisted job invariants such as non-negative counters, known-total bounds, and completion consistency. It returns a new job value rather than mutating the caller’s job, keeping transition decisions independent of SQLite, network I/O, filesystem work, and Tauri commands.

`RetryPolicy` classifies only `NETWORK_ERROR` and `RATE_LIMITED` as automatically retryable in this slice. It enforces configured backoff bounds, retry budget, capped exponential delay, and a bounded deterministic jitter input. The policy is pure and does not schedule sleeps or perform network requests; those responsibilities remain for the worker/application slice. No ETag/Last-Modified migration is included because persistent validators belong with the later streaming/resume implementation.

The first `create_download` IPC slice is planned to accept a user-selected absolute destination directory, but only through a Rust path-safety boundary that validates root containment, filename sanitization, traversal rejection, and symlink behavior. Slice 1 does not expose that command yet.

## 20. Phase 5 Slice 2 Application and Persistence Boundary

Slice 2 adds application-owned job creation and queue coordination. A new job must be a non-empty, queued record with no progress, retry, error, start, or completion state. The application verifies that the referenced media item exists and that an optional format belongs to that item before delegating the atomic job-plus-event insert to SQLite.

Job transitions are validated twice at the boundary: the application checks ownership and the requested state change against the current job, while the transaction coordinator re-reads the current persisted status inside its transaction before updating. This prevents callers from skipping queue states and prevents stale application reads from authorizing an illegal transition.

Queue claiming uses one SQLite `UPDATE ... RETURNING` statement ordered by descending priority, creation time, and ID. The claim changes exactly one queued job to `resolving`, appends the claim event in the same transaction, and returns no job when the queue is empty or another claimant wins. No transaction spans worker execution, network transfer, filesystem operations, or future retry sleeps.

## 21. Phase 5 Slice 3 Download Plan and Destination Boundary

`AppServices::resolve_download_plan` resolves the selected media item, source, platform, and format from SQLite before constructing a plan. The plan resolver accepts only the persisted platform identity, a format belonging to the selected item, a progressive format, and adapter-provided metadata containing a validated public URL. The current direct-download policy is intentionally restricted to Reddit HTTPS media on `v.redd.it` or `*.redd.it`; DASH/HLS and TikTok remain unavailable to the downloader.

Destination validation is pure policy with filesystem metadata checks. It requires an absolute root without parent traversal, rejects root and destination symlinks including symlinked ancestors, allows only one safe filename component, rejects control characters, separators, reserved device names, trailing dots/spaces, and `.part` final names, and produces both final and `.part` paths only after lexical root containment is established. It does not create directories or transfer bytes.

## 22. Phase 5 Slice 4 Streaming Boundary

`StreamingEngine` consumes an already validated `DownloadPlan`; it does not accept arbitrary URLs. It uses a bounded reqwest client with redirects disabled, a fixed public-media user agent, a timeout, and a configurable maximum response size. The target is revalidated defensively as HTTPS without credentials or fragments and as a Reddit-approved `v.redd.it`/`*.redd.it` host.

The engine accepts only HTTP 200 responses, rejects partial or redirect statuses, checks declared content lengths against both the plan and configured byte limit, streams chunks directly into the validated `.part` path, checks cancellation between chunks and before flushing, flushes and synchronizes the file, and preserves partial data when a transfer is cancelled, truncated, or otherwise fails. Atomic final rename, resume/range handling, progress persistence, and worker scheduling remain outside this slice.

## 23. Phase 5 Slice 5 Finalization and Durable Result Boundary

`finalize_part` owns the filesystem half of completion. It verifies the streamed byte result against the plan, rejects missing, non-regular, symlinked, or already-existing paths, synchronizes the `.part` file, renames it within the same destination directory, synchronizes the directory, and verifies the final regular-file size. The operation refuses overwrite rather than replacing an existing destination.

The application layer then passes the resulting `FinalizationResult` to the SQLite completion transaction. `persist_download_progress` updates only a `downloading` job and appends its progress event atomically. `complete_download_job` requires a legal `downloading` to `completed` transition, checks the final path against the persisted destination root and filename, clears `temp_path`, records final byte count and completion timestamp, and appends the completion event atomically. Filesystem rename and database commit are intentionally separate durability boundaries; a future recovery/reconciliation slice must handle failures between them.

## 24. Phase 5 Slice 6 Bounded Worker Orchestration

`DownloadWorkerPool` owns bounded queue execution over the application services. It reads the typed `download.concurrent_jobs` setting with a default of three and a hard maximum of eight, starts that many Tokio workers, and claims queued jobs through the existing atomic SQLite claim operation. Workers hold no database transaction across plan resolution, network transfer, filesystem finalization, or result persistence.

`ApplicationJobExecutor` performs the ordered job lifecycle: resolve the persisted download plan, transition `resolving` to `downloading`, stream to `.part`, persist a durable progress checkpoint, finalize the file, and commit the completed job/event transaction. Recoverable execution failures are converted into failed-job transitions without terminating other workers. A shared shutdown flag prevents new claims and allows an idle run to return cleanly; resume validators, retry scheduling, scheduler integration, and IPC controls remain deferred.

A semaphore-backed bounded batch runner is also exposed for deterministic concurrency tests and future queue integrations. Its report distinguishes completed jobs, terminal failed jobs, and executor-level errors so one job failure cannot silently terminate the pool.

## 25. Phase 5 Slice 7 Retry, Cancellation, and Live Progress

The worker executor now maps retryable streaming failures to the stable `NETWORK_ERROR` and `RATE_LIMITED` categories, applies the existing bounded `RetryPolicy`, derives deterministic jitter from job identity and retry count, persists a failed event, waits outside database transactions, and requeues the job only while retry budget remains. Non-retryable failures remain terminal.

Each active job receives a `CancellationToken` registered in a shared `CancellationRegistry`. The token is passed to the streaming engine and checked before requests and between chunks. Cancellation transitions an active job to `cancelled` and prevents retry continuation. The registry is intentionally separate from IPC so future commands can bind to it without coupling the downloader core to presentation code.

Streaming reports `StreamProgress` samples through a `ProgressSampler` with a 250-millisecond minimum interval. The worker publishes `LiveProgressEvent` values through a bounded broadcast channel and uses the final sample for the durable progress checkpoint. Durable SQLite progress remains transactionally persisted at the existing job boundary; high-frequency live events are not written per chunk.

## 11. Phase 5 Slice 8 — Conservative Resume and Downloader IPC

Resume is deliberately conservative. A retry may resume only when the persisted `.part` file exists, is a regular file, and its byte length exactly equals the persisted `downloaded_bytes` offset. The HTTP engine sends `Range: bytes={offset}-` and, when available, `If-Range` with the persisted ETag or Last-Modified validator. A non-zero-offset attempt must receive `206 Partial Content`; the `Content-Range` start, end, total, and body length are validated before any append write occurs. Invalid or unsupported range responses fail closed rather than attempting to bypass access controls or server policy.

The `download_jobs` table persists `etag` and `last_modified`. Response validators are captured by the streaming engine and written through transaction-coordinated progress and completion updates. Retryable network failures return the job to `queued`; the next claimed attempt reads the durable byte offset and validators and invokes the resumable engine. If the server does not provide a safe range response, the attempt fails with a non-retryable resume error instead of silently concatenating incompatible bytes.

The bounded worker pool remains the sole queue executor. It claims jobs atomically in SQLite, propagates cooperative cancellation, emits throttled live progress through a bounded broadcast channel, and reports claimed, completed, failed, retried, and execution-error counts. Tauri exposes only validated commands: `create_download`, `cancel_download`, `get_download_jobs`, and `subscribe_download_progress`. Destination directory and filename policy remains Rust-owned; credentials, cookies, private-content access, DRM, CAPTCHA bypass, anti-bot behavior, and rate-limit evasion are not implemented.

## 26. Phase 6 Slice 6.1 Typed Media Processing Boundary

Slice 6.1 introduces a public Rust media-processing contract without starting an external process. `MediaProcessingRequest` exposes only two typed operations: `MergeAudioVideo` with separate video and audio inputs, and `ExtractAudio` with one input. `MediaProcessingPlan` resolves a request into a typed operation plus a `ProcessingOutput` containing final and temporary paths for the future FFmpeg runner.

The media boundary reuses the downloader path-safety policy. The selected processing root must be absolute, free of parent traversal and symlinked ancestors, and all inputs must remain inside that root. Inputs must be existing regular files and must not be symlinks. Final output and processing-temporary paths remain inside the root, reject existing collisions, and are generated from a validated single-component output filename. The contract rejects duplicate merge inputs before any process execution is possible.

The initial distribution decision is **system-installed FFmpeg for development and the first implementation**, behind a Rust-owned executable-resolution abstraction. This keeps Slice 6.1 independent of Tauri shell capabilities and avoids prematurely committing target-specific sidecar artifacts, signing, updater, and FFmpeg-license-source packaging. The abstraction must remain replaceable so a later release can add a bundled FFmpeg sidecar after licensing, reproducibility, and platform packaging are approved. No arbitrary executable path or shell command is part of the processing contract.

## 27. Phase 6 Slice 6.2 FFmpeg Process Boundary

Slice 6.2 adds a Rust-owned FFmpeg process boundary under `src-tauri/src/media/process.rs`. `FfmpegExecutable::resolve_system` searches the operating-system PATH, verifies that the candidate is a regular executable file, invokes `-version` without a shell, parses the major version, and accepts only the pinned supported major version. The current development policy accepts FFmpeg major version 6.

`FfmpegProcessRunner` accepts only crate-local `FfmpegArguments` constructed by Rust code. It launches the resolved executable directly with Tokio’s process API, disables stdin, discards stdout, captures stderr through a bounded reader, and uses child kill-on-drop behavior. It polls the existing atomic cancellation signal, terminates cancelled children, enforces a configured timeout, checks the exit status, and returns only bounded developer diagnostics. No Tauri shell capability, frontend process invocation, raw command string, shell fragment, or user-selected executable path is introduced.

Resolver and runner failures map to the stable `FFMPEG_FAILED` application error. User-facing messages do not expose raw stderr or executable paths; bounded diagnostics remain an internal result for future structured logging. Deterministic media-operation retry policy and worker integration remain deferred to later slices.

## 28. Phase 6 Slice 6.3 Typed FFmpeg Argument Construction

Slice 6.3 converts a validated `MediaProcessingPlan` into a crate-local `MediaProcessingArguments` value. Merge construction uses two validated `-i` paths, explicit video/audio stream maps, video stream copy, AAC audio encoding, `-shortest`, MP4 format selection, and the validated processing-temporary output path. Audio extraction uses one validated input, an explicit first-audio-stream map, `-vn`, AAC encoding at the fixed initial 192k policy, M4A/IPOD format selection, and the processing-temporary output path. Both policies include `-hide_banner`, `-loglevel error`, `-nostdin`, and `-n`; no user-provided codec, filter, map, or raw argument values are accepted.

The planned worker integration keeps `FfmpegProcessRunner` behind the application executor. `ApplicationJobExecutor` will receive a media-processing service or runner configuration, determine whether a typed plan is required, transition `downloading` to `processing` with a durable event, construct arguments from the plan, run the process with the existing cancellation token, validate the temporary output, and only then atomically finalize and complete the job. Typed runner errors will map to safe stable application errors; deterministic media failures will not be automatically retried.

## 29. Phase 6 Remaining Processing Integration

The remaining Phase 6 implementation adds migration `0003_download_processing.sql`, which persists optional typed processing configuration as `download_jobs.processing_json`. The configuration uses a tagged snake_case enum and is reconstructed only by the Rust worker. No raw FFmpeg command, shell fragment, arbitrary filter, or unrestricted codec setting is persisted.

`MediaProcessor` now consumes a validated `MediaProcessingPlan`, builds the fixed Slice 6.3 argument policy, invokes `FfmpegProcessRunner`, validates that the processing temporary output is a non-symlinked non-empty regular file, synchronizes it, atomically renames it to the validated final destination, and synchronizes the containing directory. Existing destinations are never overwritten.

`ApplicationJobExecutor` keeps the existing direct progressive download flow for jobs without processing configuration. For configured jobs it checkpoints the download, transitions `downloading → processing`, persists a durable processing event, reconstructs and revalidates the typed plan, invokes the Rust-owned processor with the existing cancellation signal, and completes the job only after processed-output finalization. Process failures map to `FFMPEG_FAILED`; cancellation maps to the existing cancelled transition. SQLite completion now accepts both `downloading` and `processing` current states while retaining state-machine validation.

The current Reddit adapter still exposes only one progressive fallback URL and does not generate a separate persisted audio format. Consequently, merge execution is available through the typed backend contract and deterministic fixtures, while production merge selection remains dependent on a future adapter/format expansion that supplies safe paired inputs.


## 30. Phase 7 Queue UI and Frontend State

Phase 7 adds a Queue workspace under `src/features/queue`. TanStack Query owns the authoritative periodic `get_download_jobs` refresh, while a small Zustand projection owns transient selection, filter, sort, and live progress updates. Progress events update only jobs already present in the query projection; they never invent jobs or alter Rust-owned status transitions.

The queue presents `processing` as a first-class `Processing · FFmpeg` stage with violet visual treatment, no download ETA, and continued cancellation availability. Downloading rows show byte progress, speed, and ETA. Completed, failed, cancelled, queued, resolving, and paused states receive distinct labels and safe action affordances. Single-job and selected-job cancellation remain calls to the existing validated Tauri command; the browser never performs cancellation, filesystem, network, or FFmpeg work.

Filtering supports all, active, processing, completed, and failed jobs. Sorting supports priority, newest, and status. Selection is a transient frontend concern and is reconciled whenever the authoritative job list refreshes.


## 31. Phase 8 History Architecture

Phase 8 adds `HistoryEntry` as a terminal-outcome domain entity rather than treating history as a frontend projection of active jobs. Each entry preserves the job identity, media item and platform identity, public source URL, title and creator metadata, destination path and filename, terminal status, byte size, safe error fields, and terminal timestamps. The entity accepts only `completed`, `failed`, and `cancelled` statuses, keeping in-progress queue state outside the history boundary.

`HistoryRepository` is a persistence port implemented by `SqliteHistoryRepository`. SQLite migration `0004_history_entries.sql` creates a local table with foreign keys to `download_jobs`, `media_items`, and `platforms`, a unique job relationship, status and size constraints, and indexes for terminal-time, status, and media-item lookup. Repository queries use bound parameters for exact deletion, optional case-insensitive search across title, filename, platform, source URL, and creator, deterministic newest-first ordering, and upsert semantics for idempotent worker recording.

The worker remains responsible for recording terminal outcomes, but history persistence is explicitly best-effort. After completion, non-retryable failure, or cancellation, the application service enriches the outcome by joining the job to its media item, source, and platform before upserting the entry. A history write error is ignored at the worker boundary so a local history problem cannot change download semantics or block terminal job persistence. The join and enrichment remain Rust-owned; the UI never reconstructs history from multiple tables.

The React History workspace is presentation-only. It calls `get_history`, `delete_history_entry`, and `clear_history` through the Tauri bridge, while the Rust command layer validates query and identifier inputs and maps repository failures to stable application errors. The UI does not read SQLite, inspect the filesystem, fetch media, or implement download behavior. TikTok remains fail-closed and history adds no credential, cookie, private-content, DRM, CAPTCHA, anti-bot, or rate-limit bypass capability.


## 32. Phase 9 Embedded Scheduler Architecture

Phase 9 implements the approved **embedded Rust scheduler loop**. The scheduler runs inside the Tauri application while it is open, wakes on a bounded 15-second interval, checks the persisted `scheduler.enabled` setting, loads due schedules from SQLite, and reuses the existing `DownloadWorkerPool` after placing validated jobs into the queue. It does not create a second downloader, run while the application is closed, or require an external service.

Schedules use the existing `Schedule` entity and `schedules` table. The application boundary now exposes list, get, save, and delete operations, while the SQLite repository provides deterministic ordering and due filtering. Supported schedule types are one-time, daily, weekly, and bounded interval execution. Interval values are restricted to 60 seconds through one year; calendar schedules cannot carry conflicting interval or cron fields. One-time schedules are disabled after their due attempt, and recurring schedules advance their next-run timestamp after each attempt.

A schedule configuration is typed JSON containing an optional persisted format ID, an absolute destination directory, a filename template, and an opt-in flag for downloading newly observed items. Each due run re-analyzes the persisted public source through `AnalyzerService`, verifies that the selected adapter explicitly advertises scheduling capability, selects only progressive formats, validates destination and filename policy through the existing download-plan boundary, and inserts jobs through `AppServices::create_download_job`. Existing jobs matching the item, format, destination, and filename are skipped, making repeated scheduler ticks idempotent.

The scheduler capability gate is intentional. An adapter that does not claim scheduling support is rejected before schedule creation or scheduled analysis. Reddit remains a single-item, download-capable adapter with scheduling disabled, and TikTok remains fail-closed with no download capability. Future collection-capable adapters must explicitly implement public monitoring semantics and set the scheduling capability before they can be scheduled.

The scheduler loop is managed as Tauri state and starts during application setup, but the persisted setting defaults to disabled. Its manual `run_scheduler_now` command uses the same due-run and worker-pool path as the background loop. Scheduler errors are logged with bounded event names and mapped at the IPC boundary without exposing raw URLs, SQL diagnostics, credentials, cookies, or private-content material.


## 33. Phase 10 Startup Recovery and Security Hardening

Phase 10 adds a Rust-owned `StartupRecoveryCoordinator` that runs after SQLite initialization and before the embedded scheduler starts. It scans all non-terminal jobs, validates the persisted destination policy, reconciles an already-finalized regular file, inspects safe `.part` files, and atomically records either a queued recovery candidate or an unrecoverable failure with a durable recovery event. Recovery never follows or deletes symlinks, never trusts a persisted temporary path outside the validated destination root, and preserves the existing worker and download state boundaries.

A downloading or resolving job with a validated `.part` file is requeued with the exact on-disk offset and an incremented retry count. A processing job discards only a validated processing artifact and restarts through the normal queue path. A valid final file whose size matches the persisted total is reconciled to `completed`; malformed, unsafe, oversized, or inconsistent artifacts are marked `failed` with the stable `RECOVERY_UNRECOVERABLE` code. Recovery writes state and event rows atomically and records history best-effort.

The focused security hardening pass also applies least-privilege Unix modes: application-data directories are `0700`, the SQLite database is `0600`, and downloaded, partial, finalized, recovered, and FFmpeg processing artifacts are hardened to `0600`. System FFmpeg resolution rejects missing, non-regular, non-executable, group-writable, and world-writable binaries on Unix. These checks are no-ops where the platform does not expose Unix mode bits; platform packaging and ACL review remain separate release-hardening work.

Streaming now performs a free-space preflight and a per-chunk capacity check with a fixed safety headroom, bounds known and unknown responses, and maps permission and disk-full failures to stable non-retryable application codes. Typed media processing bounds input and output artifacts at four GiB, rejects symlink/non-regular media, keeps FFmpeg direct-argument execution, and retains bounded stderr, timeout, cancellation, and malformed-output validation.


## 29. Detection-only YouTube and Facebook adapters

The adapter registry now includes `YouTubeAdapter` and `FacebookAdapter` alongside the existing Reddit and TikTok adapters. These adapters strictly recognize HTTPS public video URL shapes and normalize them to canonical platform URLs for detection and diagnostics.

They intentionally do not perform page scraping, browser-session reuse, credential handling, cookie handling, undocumented media extraction, or access-control workarounds. Their `analyze` and `resolve_formats` methods return `PublicMediaUnavailable`, which maps to a user-facing `MEDIA_UNAVAILABLE` response explaining that the platform was detected but no official public media download path is available. This keeps platform selection and URL detection extensible without falsely claiming download support.

The current capability matrix is therefore: Reddit is public-media download capable within its narrow approved-host boundary; TikTok, YouTube, and Facebook are detection-only; and the generic downloader remains restricted to validated direct public media resources accepted by the existing download-plan policy.
