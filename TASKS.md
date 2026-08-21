# Tasks

Tracks current and upcoming work. Always select the highest-priority incomplete task (see `CLAUDE.md` §21–22 for priority tiers and implementation order).

## Phase 1 — Foundation (P0)

- [ ] Repository setup (monorepo layout: `apps/`, `packages/`, `docs/`, `tests/`, `scripts/`)
- [ ] Initialize Tauri 2.x project
- [ ] Initialize React + TypeScript + Tailwind + shadcn/ui
- [ ] Set up Rust core crate structure (`commands/`, `domain/`, `application/`, `adapters/`, `downloader/`, `media/`, `scheduler/`, `persistence/`, `security/`, `infrastructure/`)
- [ ] Define `packages/shared-types`
- [ ] Set up structured logging (Rust + frontend)
- [ ] Set up error system (structured error types, no raw exceptions to UI)
- [ ] CI: lint, typecheck, `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`

## Phase 2 — Persistence (P0)

- [ ] SQLite integration via SQLx
- [ ] Initial migrations: `platforms`, `media_sources`, `collections`, `media_items`, `media_formats`, `download_jobs`, `job_events`, `schedules`, `settings`, `license_state`
- [ ] Repository layer per entity
- [ ] Settings service

## Phase 3 — Platform System (P0)

- [ ] Platform domain model
- [ ] `PlatformAdapter` trait
- [ ] Adapter registry
- [ ] URL detector / normalizer
- [ ] Generic adapter (fallback for arbitrary public media URLs)
- [ ] First named platform adapter (end-to-end)
- [ ] Adapter contract tests

## Phase 4 — Analyzer (P0)

- [ ] `analyze_url` IPC command
- [ ] Analysis service
- [ ] Metadata + format models
- [ ] Results UI

## Phase 5 — Downloader (P0)

- [ ] `DownloadJob` entity + state machine
- [ ] Bounded worker pool (default 3, max 8)
- [ ] Download engine (streaming, `.part` files, atomic finalization)
- [ ] Progress/speed/ETA calculation + throttled events
- [ ] Cancellation, retry (exponential backoff + jitter), resume

## Phase 6 — Media Processing (P0)

- [ ] FFmpeg wrapper (argument-array execution, no shell interpolation)
- [ ] Audio/video merge
- [ ] Audio extraction
- [ ] Processing state handling + tests

## Phase 7 — Queue UI (P0)

- [ ] Queue view (thumbnail, title, platform, quality, progress, speed, ETA, status, actions)
- [ ] Bulk actions, filtering, sorting

## Phase 8 — History (P1)

- [ ] History repository + UI, search, delete/clear

## Phase 9 — Scheduler (P1)

- [ ] Schedule model + engine + persistence
- [ ] Collection monitoring + UI

## Phase 10 — Production Hardening (P1/P2)

- [ ] Full security audit (see `CLAUDE.md` §19)
- [ ] Crash recovery / restart resilience
- [ ] Performance testing (large collections, large files)
- [ ] Full E2E suite (Playwright)
- [ ] Packaging + auto-update
- [ ] Diagnostics panel
- [ ] Final documentation pass

---

*Update this file at the end of every completed task per the loop's Step 8 (Verify).*
