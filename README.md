# Universal Media Downloader

Universal Media Downloader is a local-first desktop application for managing authorized downloads of publicly accessible media. The desktop stack is Electron.js with React, TypeScript, Tailwind CSS, and a headless Rust core connected through a local JSON-lines IPC protocol.

## Current status

The repository contains the complete production-hardened application: a strict React/TypeScript renderer, an Electron main process with a context-isolated preload bridge, and a headless Rust downloader core. Implemented features include URL analysis, Reddit and generic public-media downloads, detection-only handling for restricted platforms, resumable queue execution, history, recurring scheduling, bandwidth limiting, batch direct-URL import, FFmpeg processing, and startup crash recovery.

The application remains intentionally fail-closed and does not implement DRM circumvention, authentication or private-content bypass, CAPTCHA or anti-bot bypass, credential or cookie handling, or rate-limit evasion.

## Development

Install dependencies with `pnpm install`. Run the web preview with `pnpm dev`, build the frontend with `pnpm build`, and run the complete validation suite with `pnpm check`. The Rust-specific gates are available as `pnpm rust:test`, `pnpm rust:clippy`, and `pnpm rust:fmt`. The Playwright smoke test is available as `pnpm test:e2e`.

The Linux development environment requires Rust stable, Cargo, Node.js with pnpm, Electron prerequisites, and FFmpeg for media processing. The Electron shell uses context isolation, disabled renderer Node integration, a sandboxed preload bridge, and a fixed Rust-command allowlist.

Run the desktop app in development with `pnpm --filter @umd/desktop dev`. Build Linux packages with `cargo build --release --manifest-path apps/desktop/src-rust/Cargo.toml` followed by `pnpm --filter @umd/desktop package:electron`.

## Project guidance

`CLAUDE.md` is the engineering and architecture operating contract. The authoritative product and design documents live in `docs/`, and current work is tracked in `TASKS.md`. Do not begin Phase 2 or choose the first named Phase 3 platform adapter without explicit user confirmation.
