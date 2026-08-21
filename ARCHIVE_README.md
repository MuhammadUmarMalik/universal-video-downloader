# Universal Media Downloader 0.1.0 — Complete Source Archive

This archive contains the complete application source, not only the initial directory skeleton.

## Where the code is

The main application code is under `apps/desktop/`:

- `apps/desktop/src/` — React and TypeScript frontend.
- `apps/desktop/electron/` — Electron main and isolated preload processes.
- `apps/desktop/src-rust/src/` — Headless Rust backend, downloader, recovery coordinator, scheduler, media processing, persistence, and security boundaries.
- `apps/desktop/tests/` — Playwright smoke and lifecycle tests.
- `packages/shared-types/src/` — Shared TypeScript contracts.
- `scripts/` — Native recovery fixture, assertions, and smoke-test wrapper.
- `docs/` — Architecture, API, security audit, release documentation, and user guide.

## Setup

```bash
pnpm install
pnpm check
pnpm test:e2e
```

For the Linux native recovery smoke test, build the release binary first:

```bash
cargo build --release --manifest-path apps/desktop/src-rust/Cargo.toml
pnpm --filter @umd/desktop package:electron
pnpm test:native:recovery
```

Generated dependencies and build outputs such as `node_modules`, Rust `target`, frontend `dist`, Playwright results, and `.git` metadata are intentionally excluded. They are recreated by the setup and build commands above.
