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
