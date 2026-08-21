# Phase 2 Persistence Design Review

**Status:** Review only. No Phase 2 implementation has been started.

**Scope:** SQLite schema, SQLx integration, embedded migrations, repository boundaries, settings service, transaction boundaries, crash/restart implications, and persistence security.

## 1. Review context

The repository is currently at the end of Phase 1. The Tauri 2.x shell, React/TypeScript frontend, Rust module boundaries, shared types, structured logging, and typed error system are present. The Rust persistence module is still a placeholder, there are no migrations, no database file, no repositories, and no SQLx dependency. Git has been initialized on `main`, but no commit has been created.

Phase 2 must remain local-first. It may persist application state and authorized public-media metadata, but it must not introduce credential, cookie, session, authentication, DRM, CAPTCHA, anti-bot, rate-limit-evasion, or private-content behavior. It should also avoid persisting ephemeral signed download URLs or other sensitive access material.

## 2. Executive recommendation

The planned entity set is a sound starting point, but the current schema should be tightened before implementation. The most important changes are to add explicit foreign-key actions and domain checks, scope uniqueness to the platform or source where appropriate, define a stable identifier and timestamp convention, prevent cross-source collection/item relationships, and make repository transaction boundaries explicit.

The recommended implementation sequence is to add one atomic baseline migration, configure a single application-owned SQLite database under the Tauri app-data directory, apply connection-level SQLite settings, run embedded SQLx migrations during application startup, then implement repositories and the settings service behind Rust traits that do not leak SQLx into application/domain code. SQLx’s `migrate!()` macro embeds migrations into the binary and requires the migration directory to be relative to the crate’s `Cargo.toml`; the build script should also emit `cargo:rerun-if-changed=migrations` so adding or changing migration files triggers recompilation.[1]

> **Recommended gate:** Do not begin implementation until the open decisions in Section 11 are accepted or explicitly overridden.

## 3. Proposed Phase 2 layout

```text
apps/desktop/src-tauri/
├── migrations/
│   └── 0001_initial.sql
└── src/
    ├── application/
    │   ├── ports/
    │   │   └── repositories.rs
    │   └── settings_service.rs
    ├── domain/
    │   ├── entities/
    │   └── value_objects/
    └── persistence/
        ├── database.rs
        ├── error.rs
        ├── mod.rs
        └── sqlite/
            ├── mod.rs
            ├── row_mappers.rs
            ├── repositories/
            │   ├── collections.rs
            │   ├── download_jobs.rs
            │   ├── job_events.rs
            │   ├── media_formats.rs
            │   ├── media_items.rs
            │   ├── media_sources.rs
            │   ├── platforms.rs
            │   ├── schedules.rs
            │   └── settings.rs
            └── tests.rs
```

The checklist says “repository layer per entity.” That can be satisfied with separate repository modules while still using a shared database wrapper and transaction coordinator. Domain and application layers should depend on repository traits and domain types, not on `sqlx::Pool`, `sqlx::FromRow`, SQL strings, or SQLite-specific types.

## 4. Schema review

### 4.1 Tables and relationships

The ten required tables are present in the design: `platforms`, `media_sources`, `collections`, `media_items`, `media_formats`, `download_jobs`, `job_events`, `schedules`, `settings`, and `license_state`. The relationship graph is coherent, but delete behavior is currently implicit. SQLite defaults to `NO ACTION`, which is safe but leaves repository behavior underspecified.

| Area | Current design | Recommended Phase 2 decision | Rationale |
|---|---|---|---|
| Primary identifiers | `TEXT PRIMARY KEY`, UUID/ULID-style IDs | Choose one canonical representation, preferably lowercase UUIDv7 or a documented ULID format | Prevents inconsistent sorting, serialization, and test fixtures |
| Timestamps | `TEXT` without a stated format | RFC 3339 UTC text, generated only in Rust | Makes ordering and cross-platform serialization deterministic |
| Foreign keys | Present but no delete actions | Add explicit `ON DELETE` behavior and test it | Prevents accidental orphaning or accidental history loss |
| Booleans | `INTEGER` flags | Add `CHECK (column IN (0, 1))` | SQLite has no native boolean type |
| Status/type fields | Free-form `TEXT` | Add `CHECK` constraints for finite state/type sets | Keeps invalid state values out of the database |
| JSON columns | Unconstrained `TEXT` | Validate shape in Rust; optionally add `json_valid` only if the bundled SQLite guarantees it | Keeps the persistence contract typed without over-coupling to SQLite extensions |
| External IDs | Indexed globally or per table | Scope uniqueness to the owning platform/source and allow multiple `NULL` values | External IDs are not globally unique across platforms |
| Paths | Absolute destination path and filename | Persist validated paths only; never accept raw path fragments from SQL callers | Preserves the file-safety boundary |

### 4.2 Recommended constraints and indexes

The current indexes cover the most obvious foreign keys, but they should be expanded based on actual repository queries and referential integrity. Every child foreign-key column should have an index where parent deletion or lookup can otherwise scan the table; SQLite’s own foreign-key guidance recommends indexing child keys for non-trivial databases.[2]

The recommended baseline constraints are as follows:

```sql
-- Illustrative constraints; exact SQL belongs in the reviewed migration.
CHECK (enabled IN (0, 1));
CHECK (item_count IS NULL OR item_count >= 0);
CHECK (duration_ms IS NULL OR duration_ms >= 0);
CHECK (position IS NULL OR position >= 0);
CHECK (file_size_bytes IS NULL OR file_size_bytes >= 0);
CHECK (downloaded_bytes >= 0);
CHECK (total_bytes IS NULL OR total_bytes >= 0);
CHECK (retry_count >= 0);
CHECK (max_retries >= 0);
CHECK (status IN ('queued', 'resolving', 'downloading', 'processing', 'completed', 'paused', 'cancelled', 'failed'));
```

The following uniqueness rules should be added or explicitly rejected before implementation:

| Entity | Recommended uniqueness |
|---|---|
| `platforms` | `UNIQUE(slug)` as already specified |
| `media_sources` | `UNIQUE(platform_id, normalized_url)` rather than global uniqueness |
| `collections` | `UNIQUE(source_id, external_id)` when `external_id` is non-null |
| `media_items` | `UNIQUE(source_id, external_id)` when `external_id` is non-null; canonical URL should be indexed and may be unique within a source |
| `media_formats` | `UNIQUE(media_item_id, external_format_id)` when `external_format_id` is non-null |
| `settings` | Primary key on `key` as already specified |
| `license_state` | Single-row `CHECK (id = 1)` as already specified |

SQLite permits multiple `NULL` values in a unique constraint. If the project needs partial uniqueness semantics to be explicit, use partial unique indexes such as `WHERE external_id IS NOT NULL` and test them against SQLite’s actual behavior.

### 4.3 Cross-source relationship integrity

`media_items` currently stores both `source_id` and an optional `collection_id`, while `collections` stores `source_id`. Two independent foreign keys do not prevent an item from referencing a collection owned by a different source. The safest schema-level option is a composite relationship:

```sql
UNIQUE (id, source_id) ON collections;
FOREIGN KEY (collection_id, source_id)
  REFERENCES collections(id, source_id);
```

If the implementation chooses not to add this composite constraint, the repository must enforce the invariant in a transaction and include a negative integration test. The decision should not be left implicit.

### 4.4 Delete behavior

Recommended delete actions are conservative because completed download history must survive source refreshes. A proposed baseline is:

| Parent-to-child relationship | Recommended action |
|---|---|
| `platforms` → `media_sources` | `RESTRICT` |
| `media_sources` → `collections` | `CASCADE` only if source deletion is an explicit cache purge |
| `media_sources` → `media_items` | `RESTRICT` while jobs/history reference items |
| `collections` → `media_items` | `SET NULL` or `CASCADE`, depending on whether collection membership is historical data |
| `media_items` → `media_formats` | `CASCADE` for refreshed format snapshots |
| `media_items` → `download_jobs` | `RESTRICT` to preserve job/history records |
| `download_jobs` → `job_events` | `CASCADE` |
| `media_sources` → `schedules` | `RESTRICT` or explicit schedule deletion in the service layer |

The exact policy must be aligned with the future History feature. Until then, repository delete methods should prefer explicit operations over broad cascades that could remove audit or recovery data.

### 4.5 `download_jobs` as durable intent

The job table should persist durable download intent, not a transient signed URL. A format row represents the analyzed format snapshot, while the future adapter/download-plan layer will resolve an authorized public resource when the job runs. If a later design needs to persist a remote URL, it must first define an expiration, redaction, encryption, and logging policy; Phase 2 should avoid that requirement.

The current `format_id` is nullable, which is useful for future format resolution, but the repository should validate that a non-null format belongs to the same `media_item_id` as the job. A composite foreign key or a transaction-time ownership check should prevent cross-item format selection.

## 5. SQLx and SQLite bootstrap plan

### 5.1 Dependency features

Add SQLx as a Rust runtime dependency with only the features needed by the desktop core. The exact version should be pinned after the repository’s compatibility check, but the required feature set is conceptually:

```toml
sqlx = {
    version = "<reviewed-version>",
    default-features = false,
    features = ["sqlite", "runtime-tokio", "migrate", "macros"]
}
```

The `macros` feature is needed if compile-time query macros are used. The `migrate` feature is required for the embedded migration API. `sqlx-cli` is a developer tool, not a runtime dependency, and should be installed or invoked separately in development tooling.

### 5.2 Migration location and embedding

Migrations should live at `apps/desktop/src-tauri/migrations/` beside the Rust crate’s `Cargo.toml`. The first migration should be a single atomic baseline, for example `0001_initial.sql`, creating the required tables in parent-to-child order and then creating indexes and seeds. Subsequent changes must use new immutable migration files such as `0002_add_media_item_fingerprint.sql`; an applied migration must never be edited.

The Rust crate should define an embedded migrator:

```rust
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
```

Startup should run `MIGRATOR.run(&pool).await` before repositories are exposed. SQLx validates previously applied migration checksums and detects accidental edits; the application should convert migration failures into a structured startup error and should not continue as if the database were healthy.[1]

The existing `build.rs` should be extended with:

```rust
println!("cargo:rerun-if-changed=migrations");
```

This is required because migration files are external inputs to the compile-time macro and adding a new migration must trigger recompilation.[1]

### 5.3 Connection path and options

The database file should be created under the Tauri app-data directory, not the process working directory and not a user-controlled arbitrary path. The startup sequence should create the app-data directory, build `SqliteConnectOptions` with the validated file path, enable file creation, enable foreign keys, set a bounded busy timeout, and choose WAL deliberately. Current SQLx SQLite options expose `filename`, `create_if_missing`, `foreign_keys`, `journal_mode`, `busy_timeout`, and `synchronous` configuration methods.[3]

A recommended starting profile is:

```text
filename: <Tauri app-data directory>/umd.sqlite3
create_if_missing: true
foreign_keys: true
journal_mode: WAL
busy_timeout: 5 seconds
synchronous: FULL initially; benchmark NORMAL only after durability testing
pool max connections: small bounded value, e.g. 4
```

SQLx documents that foreign-key enforcement is enabled per connection and that WAL is persistent for the database after it is established; it also warns that opening the same database with a different journal mode can require an exclusive lock.[3] Therefore every application connection must use the same connection profile, and the implementation should not toggle journal mode dynamically.

WAL is a reasonable default for a desktop application with short writes and concurrent reads, but it is not a substitute for transaction discipline. No transaction should be held across network I/O, FFmpeg work, filesystem copying, or user interaction. Write transactions should be short and retry only well-defined SQLite busy/locked failures.

### 5.4 Startup ownership

The Tauri application state should own a single initialized database handle, for example `Arc<Database>`, injected into command and application services. A database bootstrap failure should prevent normal commands from being registered as ready. Migration and connection diagnostics may include the database filename and migration version, but must not expose arbitrary local paths or sensitive payload contents to end users.

The foundation command can remain available for smoke tests, while Phase 2 should add an internal database-health test rather than expanding user-facing IPC prematurely. The persistence work should not add analyzer, downloader, adapter, or scheduler commands.

## 6. Repository and service boundaries

### 6.1 Ports and adapters

Repository traits should be declared in an application-facing port module or a domain repository module. SQLite implementations should remain under `persistence/sqlite`. Domain entities should be free of SQLx derives and should use explicit conversions from database rows.

A useful first boundary is:

```text
Application/domain repository ports
        ↓
SQLite repository implementations
        ↓
Database wrapper + SQLx pool
```

`SettingsService` belongs in the application layer. It should expose typed settings operations and use a validated key/value model rather than allowing arbitrary UI JSON to reach SQL directly.

### 6.2 Transaction boundaries

The following operations should be atomic:

| Operation | Required transaction contents |
|---|---|
| Save analysis snapshot | Upsert source, collection, items, formats, and snapshot timestamps |
| Create download job | Insert job and its initial `job_events` record |
| Change job status | Update job state, timestamps, retry/error fields, and append the event |
| Clear history | Delete only the explicitly selected terminal records and dependent events |
| Update settings | Validate the typed value and upsert the setting |

Repositories must not expose long-lived mutable transactions to higher layers. A transaction coordinator or application service should combine calls where multiple tables must change together. The future downloader must update durable job state without writing progress on every byte; progress persistence should be throttled or checkpointed in Phase 5.

### 6.3 Error mapping

SQLx errors must be mapped at the persistence boundary to stable application errors. User-facing errors should distinguish database unavailable, migration failed, constraint violation, busy/locked timeout, corruption/integrity failure, and unknown persistence failure. Raw SQL statements, local filesystem paths, and driver diagnostics belong only in redacted developer diagnostics.

## 7. Settings service design

The settings table is intentionally generic, but the service must not be. Define a finite key enum or typed setting identifiers for the keys listed in the schema document. Each key should have a serializer, validator, default, and redacted logging policy.

The initial settings service should support:

| Capability | Phase 2 behavior |
|---|---|
| Read one setting | Return typed value or documented default |
| Read all settings | Return a typed settings snapshot |
| Update one setting | Validate before transaction, then upsert |
| Reset one setting | Delete or replace with default, with deterministic semantics |
| Seed defaults | Idempotent initialization outside destructive migrations |

The settings service must reject unknown keys unless an explicit forward-compatible extension policy is adopted. It must also enforce bounded concurrency, retry, path, and template values before they are persisted.

## 8. Testing plan

Phase 2 should add integration tests that exercise real SQLite behavior rather than only mocked repositories.

| Test group | Required coverage |
|---|---|
| Migration | Fresh database, all migrations applied, rerun is a no-op, checksum/edit detection, migration failure stops startup |
| Connection | App-data path creation, foreign-key pragma enabled on every connection, WAL profile, busy timeout, pool shutdown |
| Schema | Table existence, required indexes, check constraints, unique constraints, foreign-key list, `PRAGMA foreign_key_check` |
| Repositories | CRUD and upsert behavior for each repository, not-found semantics, typed error mapping |
| Transactions | Rollback on partial snapshot failure, job plus event atomicity, settings update rollback |
| Relationship integrity | Cross-source collection/item rejection, format/item ownership rejection, delete-policy tests |
| Settings | Typed roundtrip, default seeding, invalid value rejection, unknown-key policy, path validation |
| Recovery | Reopen the same file after process restart, preserve terminal and non-terminal job state without implementing workers |
| Concurrency | Short concurrent reads/writes, bounded busy behavior, no leaked connections |

Use `sqlite::memory:` with a single connection for fast schema/repository tests, and a temporary file database for WAL, restart, path, and migration lifecycle tests. The test harness should explicitly execute `PRAGMA foreign_keys` and fail if it returns `0`; SQLite requires foreign keys to be enabled at runtime per connection.[2]

## 9. Security and reliability gate

Before Phase 2 is marked complete, review the following items explicitly:

| Gate | Required control |
|---|---|
| Database location | Resolve only from the Tauri app-data directory; do not accept a raw database path from the UI |
| SQL safety | Use bound parameters; no string interpolation for values or identifiers derived from users |
| File permissions | Use the platform’s application-data directory and least-privilege creation behavior |
| Sensitive data | Do not store credentials, cookies, session data, access tokens, or signed URLs |
| JSON payloads | Enforce size limits and schema validation before persistence |
| Metadata | Treat titles, creators, URLs, and adapter metadata as untrusted data |
| Migrations | Never edit applied files; report checksum mismatch and stop startup |
| Backups | Define a safe backup procedure before any future destructive migration; do not copy a live WAL database naively |
| Corruption | Run integrity checks in diagnostics/recovery workflows, not on every normal query |
| Logging | Redact SQL values, metadata payloads, local paths where sensitive, and all future access material |

## 10. Implementation order after approval

The recommended execution order is intentionally incremental:

1. Decide the open schema and runtime options in Section 11.
2. Add SQLx with reviewed, pinned features and update the Cargo lockfile.
3. Add `migrations/0001_initial.sql` with explicit constraints, foreign keys, indexes, and only safe default seeds.
4. Add the `Database` wrapper, connection options, embedded migrator, startup error mapping, and build-script rerun directive.
5. Add domain entities/value objects for persisted records and explicit row mappers.
6. Add repository ports and one SQLite repository at a time, starting with platforms and settings.
7. Add source/collection/item/format snapshot persistence as one transactionally coordinated use case.
8. Add download-job and job-event repositories without implementing the downloader or queue worker.
9. Add the typed settings service and its validation/default policy.
10. Add integration and restart/migration tests, then run the full validation suite.
11. Update `docs/03-Database-Schema.md`, `docs/04-System-Design.md`, `docs/06-Tech-Stack.md`, and `TASKS.md` to reflect the actual implementation.

## 11. Decisions required before implementation

| Decision | Recommended default | Needs confirmation |
|---|---|---|
| Identifier format | UUIDv7 text or documented ULID; use one format everywhere | Yes |
| Timestamp format | RFC 3339 UTC text generated in Rust | No objection assumed, but document it |
| Migration location | `apps/desktop/src-tauri/migrations/` | No objection assumed |
| Migration baseline | One atomic `0001_initial.sql` for all required tables | Yes |
| Foreign-key delete policy | Conservative `RESTRICT` for history-bearing parents, explicit cascades only for cache-like children | Yes |
| Database journal | WAL, set consistently on every connection | Yes |
| Pool size | Small bounded pool, starting at 4 connections | Yes; benchmark later |
| JSON validation | Typed Rust validation; optional SQLite `json_valid` only if portability is confirmed | Yes |
| Settings unknown keys | Reject by default | Yes |
| `license_state` | Create inert free/inactive row or defer to a later migration while preserving the required table contract | Yes |
| SQLx query style | Runtime `query`/`query_as` first, or compile-time macros with an offline-check workflow | Yes |

The most important choice is not whether SQLite or SQLx is used; that is already authoritative. It is how much integrity is enforced by the schema versus the application layer, and how deletion of source metadata interacts with durable history and future recovery.

## 12. Review outcome

**Recommendation: approved in principle, not yet ready for implementation.** The existing documents provide the correct table inventory and local-first direction. Before coding, the schema should be amended with explicit constraints, uniqueness scope, relationship ownership, delete actions, and timestamp/identifier conventions. The migration plan should use embedded SQLx migrations under the Rust crate, a build-script rerun directive, a single application-owned database handle, and tested connection-level SQLite settings.

No Phase 2 code, migration file, SQLx dependency, repository, settings service, or database file has been added by this review.

## References

[1]: https://docs.rs/sqlx/latest/sqlx/macro.migrate.html "SQLx migrate! macro documentation"

[2]: https://sqlite.org/foreignkeys.html "SQLite Foreign Key Support"

[3]: https://docs.rs/sqlx/latest/sqlx/sqlite/struct.SqliteConnectOptions.html "SQLx SqliteConnectOptions documentation"

[4]: https://sqlite.org/pragma.html "SQLite PRAGMA documentation"
