# Universal Media Downloader — Technology Stack

## 1. Recommended Stack

| Layer | Technology | Purpose |
|---|---|---|
| Desktop | Tauri 2.x | Native desktop shell |
| Frontend | React | UI |
| Language | TypeScript | Frontend typing |
| Styling | Tailwind CSS | UI styling |
| Components | shadcn/ui | Accessible primitives |
| Client state | Zustand | Lightweight local state |
| Server state | TanStack Query | Query/cache patterns |
| Core | Rust | Download/orchestration engine |
| Async runtime | Tokio | Async/concurrency |
| Local DB | SQLite | Local-first persistence |
| DB layer | SQLx | Rust SQL access |
| HTTP | Reqwest | HTTP client |
| Serialization | Serde | Data serialization |
| Media | FFmpeg | Media processing |
| Secrets | OS keychain | Secure credential storage |
| Logging | tracing | Structured logging |
| Frontend tests | Vitest | Unit tests |
| E2E | Playwright | UI testing |
| Rust tests | cargo test | Core testing |

## 2. Optional Cloud Stack

| Layer | Technology |
|---|---|
| API | NestJS or Fastify |
| Language | TypeScript |
| Database | PostgreSQL |
| Cache | Redis |
| ORM | Prisma or Drizzle |
| Validation | Zod |
| API spec | OpenAPI 3.1 |
| Auth | JWT + rotating refresh tokens |
| Password hashing | Argon2id |
| Containers | Docker |
| Reverse proxy | Caddy/Nginx |
| CI/CD | GitHub Actions |
| Monitoring | Sentry + structured logs |

## 3. Why Tauri + Rust

The application is download-heavy and filesystem-heavy. Rust provides:

- Efficient streaming I/O.
- Bounded concurrency.
- Cancellation.
- Low memory overhead.
- Strong filesystem/process safety.
- Native cross-platform integration.

React remains focused on UI.

## 4. Frontend Structure

```text
src/
├── app/
├── components/
├── features/
│   ├── analyzer/
│   ├── downloads/
│   ├── history/
│   ├── scheduler/
│   ├── settings/
│   └── platforms/
├── hooks/
├── stores/
├── lib/
└── types/
```

Use feature-oriented modules instead of one giant component directory.

## 5. Rust Structure

```text
src-tauri/src/
├── commands/
├── domain/
├── application/
├── adapters/
│   ├── registry.rs
│   ├── youtube/
│   ├── tiktok/
│   ├── instagram/
│   ├── facebook/
│   └── generic/
├── downloader/
├── media/
├── scheduler/
├── persistence/
├── security/
├── infrastructure/
└── main.rs
```

## 6. Development Tooling

Recommended:

- Node.js LTS
- pnpm
- Rust stable
- Cargo
- FFmpeg
- Git
- Docker Desktop
- VS Code

Pin/test the FFmpeg major version used for release builds.

## 7. Code Quality

Frontend:

- TypeScript strict mode.
- ESLint.
- Prettier.
- Avoid `any`.
- Feature-based architecture.

Rust:

- `cargo fmt`.
- `cargo clippy -- -D warnings`.
- Stable Rust.
- Explicit error types.
- No panics in normal application paths.

## 8. CI/CD

```text
Pull Request
 ↓
Install
 ↓
Lint
 ↓
Type Check
 ↓
Frontend Tests
 ↓
Rust Tests
 ↓
Integration Tests
 ↓
Build
 ↓
Package
 ↓
Security Checks
 ↓
Release
```

Release artifacts:

- Windows installer.
- macOS package.
- Linux package.

## 9. Environment

Cloud secrets:

```text
DATABASE_URL
REDIS_URL
JWT_SECRET
REFRESH_TOKEN_SECRET
SENTRY_DSN
PAYMENT_PROVIDER_SECRET
```

Never commit secrets.

## 10. Dependency Principles

Prefer:

1. Small maintained dependencies.
2. Rust-native libraries for core operations.
3. Mature React libraries.
4. Pinned versions.
5. Automated dependency auditing.

Avoid unnecessary packages for functionality already provided by Tauri/Rust/browser APIs.

## 11. Recommended MVP Stack

```text
Desktop
└── Tauri
    ├── React
    ├── TypeScript
    ├── Tailwind CSS
    ├── shadcn/ui
    ├── Zustand
    └── TanStack Query

Core
└── Rust
    ├── Tokio
    ├── Reqwest
    ├── Serde
    ├── SQLx
    ├── SQLite
    └── FFmpeg

Optional Cloud
├── NestJS/Fastify
├── PostgreSQL
├── Redis
├── Docker
└── OpenAPI
```

## 12. Product Architecture Decision

Build the MVP **local-first**.

Do not introduce PostgreSQL, Redis, Kubernetes, or a large backend merely to download media.

The desktop app should handle:

- URL analysis.
- Platform adapters.
- Downloads.
- FFmpeg processing.
- Queue management.
- Scheduling.
- History.
- File organization.

Add cloud infrastructure when you need accounts, paid licenses, subscriptions, device activation, cloud settings, or opt-in telemetry.

## 13. Phase 1 Implemented Baseline

The repository is a pnpm workspace with `apps/desktop` as the Tauri application and `packages/shared-types`, `packages/ui`, and `packages/config` as package boundaries. The desktop frontend uses React 19, TypeScript 5.9, Vite 7, Tailwind CSS 3, a shadcn/ui-compatible primitive boundary, Zustand 5, and TanStack Query 5. The frontend is compiled in strict TypeScript mode and validated with ESLint, Vitest, and Playwright configuration.

The Rust desktop crate uses Tauri 2, Tokio, Serde, `thiserror`, `tracing`, and `tracing-subscriber`. The Phase 1 Rust crate is intentionally free of SQLite, SQLx, Reqwest, FFmpeg, adapter implementations, and download execution. Those dependencies belong to the phases where their behavior is implemented and tested.

The verified development baseline requires Node.js with pnpm, Rust stable, Cargo, and the Linux Tauri build prerequisites. The project does not commit secrets, credentials, cookies, session data, or signed URLs. The initial Tauri capability file grants only the core default permissions.

## 14. Phase 2 Persistence Baseline

Phase 2 adds SQLx `0.9.0` with SQLite, Tokio runtime, migration, and macro features. The migration source is embedded from `apps/desktop/src-tauri/migrations/` with `sqlx::migrate!("./migrations")`, and the Rust build script emits `cargo:rerun-if-changed=migrations` so migration-file changes trigger recompilation.

The database wrapper uses a four-connection bounded pool, creates `umd.sqlite3` under the Tauri application-data directory, enables foreign keys, selects WAL journal mode, uses full synchronous durability, and applies a five-second busy timeout. Runtime query APIs are used for the current bootstrap and tests; compile-time SQLx query macros remain available for later repository work after an offline-check workflow is chosen.

The application layer now owns async repository ports and typed settings service contracts. SQLite adapters use runtime-bound SQLx queries, explicit `FromRow`-backed storage rows, domain enum conversion, JSON serialization boundaries, and stable repository errors. The settings service uses the `time` crate for RFC 3339 UTC timestamps and rejects unknown keys, mismatched value types, unsafe relative directories, path-traversal template fragments, and out-of-range operational settings.

`AppServices` wires the repository bundle, settings service, and `SqliteTransactionCoordinator` from the single shared `Database` pool. The coordinator uses short SQLx transactions for complete analysis snapshots and job-plus-event writes, with rollback tests covering duplicate format/event failures. Tauri manages the application-service bundle after database migration and health checks complete; no new IPC commands are exposed in this milestone.

Phase 3 adds `reqwest` with JSON and rustls TLS support plus the `url` parser for the Reddit adapter. Requests use a bounded timeout, limited redirects, an explicit non-secret user agent, and a four-megabyte response limit. Media candidates are accepted only when HTTPS and hosted by `v.redd.it` or a `*.redd.it` host; no credentials, cookies, browser automation, shell/process execution, or anti-bot dependencies are introduced.


Phase 3/4 adds an `AdapterRegistry` containing the approved Reddit adapter and an `AnalyzerService` that composes adapter selection with the Phase 2 `AppServices` snapshot transaction. The local Tauri `analyze_url` command receives a typed Rust request and returns serialized analyzer results or the stable `AppError` contract. `packages/shared-types/src/analyzer.ts` mirrors the request, capability, source, item, format, and response shapes used by the frontend bridge.


## 15. Phase 4 Analyzer Frontend and Registry Extension

Phase 4 uses feature-oriented React components under `apps/desktop/src/features/analyzer`: `AnalyzerPanel` owns typed input and Tauri IPC mutation state, `AnalysisResults` owns metadata and format presentation, and `analysis.ts` contains pure formatting helpers covered by Vitest. The frontend receives serialized results through `apps/desktop/src/lib/tauri.ts`; it does not perform platform requests or download operations.

The adapter registry now includes a TikTok URL-detection adapter in addition to Reddit. TikTok uses the same Rust `PlatformAdapter` contract and canonical URL parser but intentionally returns the stable authentication-required error for analysis. No credential, cookie, private-content, CAPTCHA, anti-bot, DRM, or undocumented access dependency is added.

## 16. Phase 5 Slice 4 Streaming Stack

Phase 5 Slice 4 enables reqwest’s `stream` feature and Tokio `io-util` support. `StreamingEngine` uses a redirect-disabled reqwest client with a bounded timeout and maximum response size, consumes response chunks without whole-file buffering, and writes them through Tokio asynchronous file I/O to validated `.part` paths. Successful streams are flushed and synchronized; atomic finalization, range requests, resume validators, worker orchestration, and progress persistence remain later responsibilities.

## 17. Phase 5 Slice 5 Finalization and Durable Results

Slice 5 adds a Rust filesystem finalizer that uses standard-library metadata checks, `sync_all`, same-directory `rename`, and parent-directory synchronization to complete a validated `.part` file without overwriting an existing destination. Durable progress and completion remain SQLx/SQLite transaction operations that append job events atomically with job-field updates. Filesystem and database durability are intentionally separate boundaries until a later recovery/reconciliation design is approved.

## 18. Phase 5 Slice 7 Retry, Cancellation, and Live Progress

Slice 7 uses Tokio-compatible cancellation tokens backed by atomic state and notification, a bounded Tokio broadcast channel for live progress events, and a pure 250-millisecond progress sampler for speed and ETA calculation. Retry scheduling reuses the existing typed settings, stable error catalog, capped exponential backoff, and deterministic jitter policy without adding credential, session, or anti-bot dependencies.

## 13. Phase 6 FFmpeg Distribution Decision

For the first Phase 6 implementation, UMD will use **system-installed FFmpeg** through a Rust-owned executable resolver. The resolver will later validate availability and the supported major version before invoking the direct process runner. The frontend will not invoke FFmpeg and no arbitrary executable path will be accepted from IPC.

System-installed FFmpeg reduces the application bundle size and avoids adding Tauri shell sidecar permissions, target-triple binary artifacts, signing and updater complexity, and immediate redistribution obligations for a project whose supported release packaging is not yet established. Its disadvantages are user setup friction, platform-specific installation differences, and less deterministic codec/build availability. These are acceptable for the initial development and execution slice, but release packaging must eventually provide a guided diagnostic and a supported-version check.

An application-bundled FFmpeg sidecar remains the preferred future cross-platform product direction once release packaging is ready. Tauri supports target-specific external binaries and scoped sidecar execution permissions, but bundling requires per-target artifacts, reproducible builds, signing, update handling, license notices and corresponding source/build information, and a deliberate choice to exclude GPL/nonfree components where LGPL distribution is intended. The sidecar strategy must be evaluated separately before release and must not be smuggled into the system-installed implementation through arbitrary path configuration.

## 14. Phase 6 Slice 6.2 Process Execution

Slice 6.2 enables Tokio’s `process` feature and uses `tokio::process::Command` with direct argument arrays. The FFmpeg executable is resolved from the system PATH and validated by probing `-version`; the current supported major-version constant is 6. The runner disables stdin, discards stdout, bounds stderr diagnostics to 16 KiB by default, polls cooperative cancellation, kills cancelled or timed-out children, and checks the exit status.

The process runner is crate-local and is not exposed as a frontend or arbitrary command API. Only typed Rust code may construct its argument container. Stable `FFMPEG_FAILED` errors hide raw executable paths and stderr from the user-facing contract. The current system-installed strategy is intentionally replaceable by a later bundled sidecar resolver once cross-platform release packaging, signing, updater handling, and FFmpeg redistribution compliance are ready.

## 15. Phase 6 Slice 6.3 Operation Policies

The initial typed operation policies are intentionally narrow. Audio/video merge maps the first video stream and first audio stream, copies video, encodes audio as AAC, stops at the shorter stream, and emits MP4. Audio extraction maps the first audio stream, disables video, encodes AAC at 192k, and emits M4A. These choices are internal policy constants rather than user-controlled FFmpeg arguments.

`MediaProcessingArguments` accepts only a validated `MediaProcessingPlan` and produces a crate-local argument container for `FfmpegProcessRunner`. Future UI configuration may expand the typed policy only through explicit validation and documentation; arbitrary codec names, filter graphs, stream maps, shell fragments, and process paths remain outside the product boundary.


## 16. Phase 6 Worker Integration

Migration `0003_download_processing.sql` adds nullable `download_jobs.processing_json`. The value is a tagged typed configuration for merge or audio extraction and is serialized/deserialized through the Rust domain boundary. The worker does not persist executable paths or arbitrary FFmpeg arguments.

`MediaProcessor` composes the typed argument builder and direct Tokio process runner. It executes against a unique `.processing.part` artifact, validates regular-file/non-empty output, synchronizes the file and destination directory, and atomically renames to the final path. The application worker passes the existing cancellation atomic to the processor and maps process or output errors to stable application errors.

Integration tests use system-installed FFmpeg to generate small local fixtures and exercise both fixed operations. The test suite does not depend on network media, credentials, cookies, or private URLs. Release packaging remains responsible for ensuring that the supported FFmpeg major version and redistribution policy are satisfied on each target platform.

## 17. Phase 10 Security and Recovery Hardening

Phase 10 adds a Rust-owned `StartupRecoveryCoordinator` that runs after SQLite migration and health checks and before scheduler startup. It reconciles finalized files, safely requeues bounded `.part` files, removes only validated processing artifacts, and marks unsafe or unrecoverable jobs failed with durable recovery events.

The downloader uses `fs2` for cross-platform free-space checks with fixed headroom before requests and on each write. The application-data directory and SQLite database use private Unix modes, while partial, finalized, recovered, and FFmpeg output files are hardened to private modes where the platform exposes Unix permissions. System FFmpeg resolution rejects group- and world-writable binaries on Unix.

Typed media processing enforces a four-GiB input/output bound, regular-file and symlink checks, bounded diagnostics, direct arguments, timeout, cancellation, and atomic output finalization. These controls mitigate resource exhaustion and malformed-media risks without attempting to identify or bypass any platform access control.
