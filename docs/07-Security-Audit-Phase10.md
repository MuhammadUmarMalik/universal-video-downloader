# Phase 10 Security-Gate Audit

## Scope and conclusion

This audit covers every category required by `CLAUDE.md` §19 for the current local-first desktop implementation. The review included URL and path boundaries, process execution, persistence and temporary artifacts, scheduler inputs, error/logging behavior, storage exhaustion, and typed FFmpeg processing. The audit found no implementation of credential access, cookie handling, private-content access, DRM circumvention, CAPTCHA bypass, anti-bot behavior, rate-limit evasion, shell interpretation, or arbitrary frontend process execution.

The implementation is **security-gate compliant for the reviewed code paths**, with two explicit environment limitations. Unix permission modes are verified in this Linux environment; Windows ACLs and macOS entitlement/package behavior still require platform-native release testing. Malformed-media handling is bounded and fail-closed, but a dedicated fuzz corpus and packaged-FFmpeg validation remain release-hardening work.

## Audit matrix

| Category | Status | Evidence and result |
| --- | --- | --- |
| URL validation | Pass | Adapter detection and normalization require approved public URL forms. Download plans require HTTPS, reject URL credentials and fragments, and restrict Reddit media hosts to `v.redd.it` or `*.redd.it`. Scheduler re-analysis reuses the analyzer boundary. |
| Path traversal | Pass | Destination roots must be absolute and contain no parent components. Candidate paths are validated inside the selected root. Scheduler output and recovery processing paths use the same Rust-owned boundary. |
| Filename injection | Pass | Filenames reject separators, control characters, reserved device names, invalid filesystem characters, temporary suffixes, trailing dots/spaces, and `.`/`..`. Scheduler templates replace path-danger characters and cap output length. |
| Command injection | Pass | No shell command construction or `sh -c` path exists in production code. FFmpeg is invoked through `tokio::process::Command` with typed arguments. |
| Process arguments | Pass | `MediaProcessingArguments` produces fixed argument arrays for merge and audio extraction. User input cannot provide arbitrary flags, filters, codecs, executable paths, or shell fragments. FFmpeg stdin is closed and stderr is bounded. |
| File permissions | Pass with platform limitation | Unix app-data directories are `0700`; SQLite files and media artifacts are hardened to `0600`. World- and group-writable FFmpeg binaries are rejected on Unix. Windows ACLs, macOS ACLs, installer permissions, and packaged artifact review require native release environments. |
| Token handling | Pass | No access tokens, bearer headers, API keys, signed URL persistence, token extraction, or token logging exists in the reviewed application path. URL parsing explicitly rejects embedded user info and passwords. |
| Credential handling | Pass | No password, cookie, browser-session, credential-store, private-content, authentication-bypass, or CAPTCHA-bypass flow exists. TikTok remains fail-closed rather than attempting authenticated access. |
| Logging | Pass with bounded-diagnostic limitation | Structured JSON tracing is used. User-facing errors map to stable codes and do not expose raw technical exceptions. FFmpeg diagnostics are bounded. A release review should still verify redaction under production log filters and ensure adapter errors never include signed URLs. |
| Temporary files | Pass | Downloads use validated `.part` files and atomic rename. FFmpeg uses unique validated `.processing.part` outputs. Startup recovery inspects and removes only validated regular artifacts inside the destination root. Partial files are retained where safe resume is intended. |
| Symlink attacks | Pass | `symlink_metadata` checks are used for roots, inputs, `.part` files, processing artifacts, final destinations, and outputs. Path components are rejected when symlinked, and recovery never follows or deletes an unsafe persisted path. |
| Disk exhaustion | Pass with runtime-platform limitation | Free space is checked before requests and before each write with fixed headroom. Known and unknown responses remain bounded; disk-full and permission failures map to non-retryable stable codes. Filesystem behavior should be revalidated on Windows and macOS. |
| Malicious media | Pass with corpus limitation | Input and output files must be regular, non-symlink files. Media inputs and processed outputs are bounded at four GiB, outputs must be non-empty, stderr is bounded, FFmpeg has timeout/cancellation controls, and finalization is atomic. Fuzzing and a curated malformed-media corpus remain future hardening work. |

## Test evidence

The Rust suite includes path-safety, URL-host, direct-process, bounded-response, cancellation, resume, atomic-finalization, recovery, permission, storage, scheduler, and malformed-media tests. The new deterministic performance tests exercise 100,000 collection candidates and 10,000 metadata checks against a sparse four-GiB artifact.

The browser suite now includes a mocked-Tauri IPC lifecycle contract covering public-source analysis, queue progress, completion refresh, simulated startup recovery, and a processing-state bridge rejection. This validates the React-to-IPC contract in the existing Vite/Playwright harness. It does not replace a native Tauri packaged-app test that kills and relaunches the desktop process against a real SQLite file; that native restart/crash test remains an environment-dependent release test.

## Remaining release controls

The full application security gate is complete for the current source-level implementation. The remaining controls are release-environment validations rather than unimplemented bypass or access behavior: native Windows/macOS permission and packaging checks, a packaged-FFmpeg validation matrix, a malformed-media fuzz/corpus run, and a real Tauri process-kill/relaunch test with durable database state. These should remain explicit Phase 10 release checklist items and must not be represented as passing based only on the browser preview harness.
