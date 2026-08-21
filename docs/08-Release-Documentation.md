# Universal Media Downloader 0.1.0

## Release summary

Universal Media Downloader 0.1.0 is a local-first Tauri desktop application for analyzing authorized public media URLs, selecting adapter-exposed formats, downloading through a bounded resumable queue, applying typed FFmpeg processing, maintaining local history, and running opt-in schedules while the application is open.

The release preserves the project’s hard security boundary: it does not handle credentials or cookies, access private content, bypass authentication, circumvent DRM or CAPTCHAs, evade anti-bot controls, or construct shell commands from user input. Reddit is the first end-to-end adapter. TikTok is present only as a fail-closed registry-validation adapter and does not expose download capability.

## Verified release artifacts

| Platform | Artifact | Status | Notes |
| --- | --- | --- | --- |
| Linux x86_64 | `apps/desktop/src-tauri/target/release/bundle/deb/Universal Media Downloader_0.1.0_amd64.deb` | Built and verified | Native Debian package produced with Tauri 2.11.4 on Ubuntu 24.04-compatible Linux. |
| Windows x86_64 | MSI/NSIS installer | Not built in this environment | Requires a Windows packaging/signing environment and Windows WebView2 validation. |
| macOS Intel/Apple Silicon | DMG/app bundle | Not built in this environment | Requires macOS SDK, native WebKit framework, architecture-specific signing, and notarization credentials. |

The Windows and macOS artifacts are intentionally not represented as passing or available. They require native release runners because cross-compiling desktop WebView applications and producing signed installers cannot be validated correctly from this Linux environment.

## Native recovery verification

The built Linux binary was launched against an isolated XDG data directory, allowed to initialize its real SQLite database, force-terminated, and relaunched against the same database. A valid interrupted `downloading` job with a five-byte `.part` file was seeded before the test. On relaunch, the real packaged binary emitted `startup_recovery_completed` with one inspected and one requeued job. The durable SQLite row remained `queued` with `downloaded_bytes = 5`, and a `recovery_queued` event was persisted.

This test verifies the Rust startup coordinator and real packaged process behavior on Linux. It does not substitute for native Windows/macOS restart testing.

## Quality gates

The source release passed the complete validation suite after the Phase 10 hardening work: 125 Rust tests, 9 frontend unit tests, ESLint, TypeScript typecheck, Clippy with warnings denied, Rust formatting, production frontend build, 7 Playwright tests, and Git diff hygiene. The release also passed deterministic scheduler performance checks for 100,000 collection candidates and 10,000 metadata checks against a sparse four-GiB artifact.

## Installation on Linux

Install the Debian package with the system package manager:

```bash
sudo apt install ./Universal\ Media\ Downloader_0.1.0_amd64.deb
```

The package installs the desktop application and registers the `com.umd.desktop` application identity. FFmpeg is not bundled in this release; install a trusted system FFmpeg package if merge or audio-extraction processing is required. The application validates the resolved FFmpeg executable and rejects group- or world-writable binaries on Unix.

## Operational model

All downloads, file writes, database operations, recovery, scheduling, and FFmpeg execution are Rust-owned. React provides presentation and typed IPC requests only. Download jobs use bounded workers, `.part` files, HTTP range support where available, validator persistence, atomic finalization, cancellation, retry classification, and terminal history recording.

The embedded scheduler is opt-in and runs only while the application is open. It re-analyzes supported public sources, rejects unsupported scheduling capabilities, suppresses duplicate jobs, and submits work through the existing queue. It is not an operating-system service and does not run while the application is closed.

## Release limitations

The current release has no Windows or macOS installer artifact from this build environment. Native platform permission, packaging, signing, updater, and notarization validation remain release-runner responsibilities. The current browser lifecycle tests use a mocked Tauri IPC harness; native Linux crash/restart behavior is verified separately against the packaged binary and real SQLite database. Reddit scheduling and collection monitoring remain disabled until an adapter explicitly advertises those public capabilities.

## Security and audit record

The complete source-level security-gate audit is recorded in [`07-Security-Audit-Phase10.md`](07-Security-Audit-Phase10.md). It covers URL validation, path traversal, filename injection, command injection, process arguments, permissions, token and credential handling, logging, temporary files, symlink attacks, disk exhaustion, and malicious media. Native platform and packaged-release validation limitations are recorded rather than concealed.

## References

[1]: ../CLAUDE.md "Project operating contract and release security gate"
[2]: 07-Security-Audit-Phase10.md "Phase 10 security-gate audit"
[3]: 02-Architecture.md "Project architecture decisions"
[4]: 04-System-Design.md "System behavior and recovery design"
[5]: 06-Tech-Stack.md "Technology decisions"
