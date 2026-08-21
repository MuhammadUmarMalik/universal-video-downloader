# Universal Media Downloader User Guide

## What the application does

Universal Media Downloader analyzes an authorized public media URL, shows the formats exposed by the selected platform adapter, and manages a local download queue. The application can resume supported downloads, process compatible media through a trusted system FFmpeg installation, retain terminal outcomes in local history, and run recurring public-source schedules while the application remains open.

> Use the application only for media you are authorized to download and process. The application does not provide access to private content and does not bypass authentication, DRM, CAPTCHA, anti-bot controls, or other platform security controls.

## Requirements

The Linux release is distributed as a Debian package for x86_64 systems. Install a compatible Linux desktop environment and a trusted system FFmpeg package if you need audio extraction or merge operations. The application itself owns its SQLite database and download files; no cloud account or external credential is required.

Windows and macOS installers are not included in this Linux-built release. Native packages for those platforms must be produced and signed in their respective release environments.

## Install and start

Install the Debian package from the directory containing the artifact:

```bash
sudo apt install ./Universal\ Media\ Downloader_0.1.0_amd64.deb
```

Start **Universal Media Downloader** from the desktop application menu. On first launch, the application creates its local database and applies all migrations automatically. The application data directory is protected with least-privilege Unix modes where supported.

## Analyze a public URL

Open the **Analyzer** workspace. Paste an authorized public URL into **Public media URL**, optionally select a platform, and choose **Analyze**. Auto-detection is available when the platform selector is left at its default. The analyzer displays the canonical source, media items, available formats, delivery type, and adapter capabilities.

The analyzer does not promise that every platform URL is downloadable. A platform may be recognized for validation while exposing no download or scheduling capability. Unsupported, restricted, malformed, or credential-bearing URLs fail closed with a user-safe error.

## Queue and download media

The **Queue** workspace displays Rust-managed jobs, status, progress, downloaded bytes, speed, ETA, and processing state. The queue uses bounded concurrency and does not expose filesystem or network operations to the browser layer.

A supported download follows the local lifecycle `queued → resolving → downloading → processing → completed`. Depending on the adapter and format, the job may use HTTP range requests and persisted validators to resume safely. During transfer, the application writes a validated `.part` file. The final filename is created only after successful verification and atomic finalization.

Use **Cancel** for an active job. Cancelled, failed, and completed jobs remain durable records. Retry behavior is limited to transient network conditions; invalid URLs, unsupported platforms, access restrictions, malformed media, insufficient space, and permission failures are not retried automatically.

## FFmpeg processing

Typed processing operations include merging separate audio and video inputs and extracting audio. The application resolves a trusted system FFmpeg executable, passes direct structured arguments, closes standard input, bounds diagnostics, supports timeout and cancellation, validates input and output paths, and rejects empty, oversized, non-regular, or symlinked media artifacts.

If processing fails, the queue displays a safe processing failure state. Detailed diagnostics remain developer-oriented and are bounded rather than displayed as raw process exceptions.

## History

Open **History** to review terminal outcomes. History entries include the source title, platform, filename, destination metadata, status, size when known, timestamps, and safe error information. Use the search field to filter entries, the individual delete action to remove one entry, or **Clear all** to remove all history records.

Deleting history does not delete downloaded files and does not alter the underlying download job state.

## Scheduler

Open **Scheduler** to configure recurring monitoring of a source that explicitly supports public scheduling. Scheduling is disabled by default. Enable it only when you want the application to check schedules while it is open.

A schedule can run once, daily, weekly, or at a bounded interval. Provide the source ID, an absolute destination directory, a safe filename template, and an optional format ID. The Rust boundary validates the configuration, re-analyzes the public source, rejects unsupported capabilities, suppresses duplicate jobs, advances the next-run time, and submits new work to the existing queue.

The scheduler does not run while the application is closed. It does not accept cookies or credentials and does not attempt to defeat access controls. If the application restarts, the startup recovery coordinator runs before the scheduler and reconciles stale non-terminal jobs first.

## Restart and crash recovery

On startup, the application scans non-terminal jobs left by an interrupted process. A safe `.part` file is requeued with its durable byte offset. A valid finalized file can be reconciled as completed. Interrupted processing is returned to a clean queue state. Unsafe paths, symlinked artifacts, malformed outputs, and exhausted recovery cases are marked failed with a durable recovery event.

The recovery coordinator never follows or deletes an unsafe persisted path. It validates destination roots and artifact types before touching the filesystem.

## Storage and permissions

The application checks available space before beginning a transfer and again before writes. It reserves fixed headroom so a destination that cannot safely accept the next write fails before or during transfer with a stable disk-full error. Output, partial, processing, database, and application-data files receive private Unix modes where supported.

Choose a destination directory with sufficient capacity and avoid placing the application’s data directory on a removable or intermittently mounted volume.

## Troubleshooting

| Problem | Recommended action |
| --- | --- |
| The URL is rejected | Confirm that it is a public URL you are authorized to use, contains no embedded credentials, and is supported by a registered adapter. |
| No downloadable formats appear | The adapter may expose metadata only, the source may be restricted, or the platform may not support downloading through this application. Do not attempt to bypass the restriction. |
| The job reports insufficient space | Select a destination with more free space and remove unneeded files using the operating system. The application does not automatically delete user files. |
| The job reports permission denied | Choose a writable destination and verify that the system FFmpeg executable is not group- or world-writable. |
| FFmpeg processing fails | Confirm that FFmpeg is installed, executable, and compatible with the media. Review the safe status message and retry only after correcting the input or installation. |
| The scheduler does not run | Confirm that the scheduler is enabled, the application is open, the schedule is due, and the source adapter explicitly supports scheduling. |
| The queue is empty after a crash | Startup recovery may have marked an unsafe or unrecoverable job as failed. Open History or inspect the queue’s stable error status; never manually edit the SQLite database. |
| The UI says the bridge is unavailable | Restart the desktop application. The browser preview is not a substitute for the native Tauri shell. |

## Privacy and security

The application is local-first. It does not require an account, does not collect credentials, and does not expose private-content access. Keep the application data directory protected, use trusted FFmpeg binaries, and download only media for which you have authorization.

## References

[1]: ../CLAUDE.md "Project operating contract and security constraints"
[2]: 02-Architecture.md "Architecture and boundary decisions"
[3]: 04-System-Design.md "Queue, scheduler, and recovery behavior"
[4]: 07-Security-Audit-Phase10.md "Phase 10 security-gate audit"
[5]: 08-Release-Documentation.md "Release artifact and platform status"
