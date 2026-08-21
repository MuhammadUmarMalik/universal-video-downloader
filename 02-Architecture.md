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
│                   Electron Desktop App                      │
│                                                             │
│  ┌─────────────────────┐       ┌─────────────────────────┐  │
│  │ React + TypeScript  │◄─────►│ Electron Preload IPC   │  │
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
│       └── src-rust/
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

Domain code must not depend on React, Electron, SQLite, or HTTP libraries.

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

Electron:

- Context isolation and disabled renderer Node integration.
- Sandboxed preload with a narrow API surface.
- Explicit IPC command allowlist.
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
Electron for lightweight desktop delivery with a secure preload bridge; Rust runs headlessly as a child process.

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
