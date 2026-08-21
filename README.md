# Universal Media Downloader

Universal Media Downloader is a local-first desktop application for managing authorized downloads of publicly accessible media. The desktop stack is Tauri 2.x with React, TypeScript, Tailwind CSS, and a Rust core.

## Phase 1 status

The repository currently contains the **Foundation** phase only. It includes the prescribed monorepo structure, a Tauri 2.x shell, a strict React/TypeScript frontend, Tailwind CSS, a shadcn/ui-compatible button primitive, Zustand and TanStack Query boundaries, shared TypeScript error types, Rust structured logging, and a serializable Rust error model.

SQLite, SQLx, URL analysis, platform adapters, download execution, FFmpeg processing, queue workers, scheduling, history, credentials, cookies, authentication, private-content access, DRM circumvention, CAPTCHA or anti-bot bypass, and rate-limit evasion are intentionally outside Phase 1.

## Development

Install dependencies with `pnpm install`. Run the web preview with `pnpm dev`, build the frontend with `pnpm build`, and run the complete validation suite with `pnpm check`. The Rust-specific gates are available as `pnpm rust:test`, `pnpm rust:clippy`, and `pnpm rust:fmt`. The Playwright smoke test is available as `pnpm test:e2e`.

The Linux development environment requires Rust stable, Cargo, Node.js with pnpm, and the Tauri WebKit/GTK build prerequisites. Tauri capabilities are intentionally limited to `core:default` in the foundation.

## Project guidance

`CLAUDE.md` is the engineering and architecture operating contract. The authoritative product and design documents live in `docs/`, and current work is tracked in `TASKS.md`. Do not begin Phase 2 or choose the first named Phase 3 platform adapter without explicit user confirmation.
