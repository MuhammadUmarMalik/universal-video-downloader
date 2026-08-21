# Universal Media Downloader — Product Requirements Document (PRD)

## 1. Product Overview

**Working name:** Universal Media Downloader (UMD)

UMD is a desktop-first media management application that lets users select a supported platform, paste a media URL, analyze publicly accessible media, select one or many items, choose output quality/format, and manage downloads through a reliable local queue.

The product uses a platform-adapter architecture so each supported platform can evolve independently. A conservative generic extractor handles simple public media resources where technically possible.

**Positioning:** A fast, organized, privacy-friendly desktop media downloader and bulk media manager for content users are authorized to save.

### Initial platform targets

- YouTube
- TikTok
- Instagram
- Facebook
- Vimeo
- Reddit
- X/Twitter
- Generic public media URLs

Platform support must always respect the platform's current technical capabilities, terms, APIs, and access controls. The product must not bypass DRM, private-content restrictions, authentication controls, or other security mechanisms.

## 2. Goals

### Business goals

1. Launch a polished desktop downloader with a strong bulk-download workflow.
2. Make platform integrations modular and independently replaceable.
3. Keep the MVP local-first to minimize infrastructure and bandwidth costs.
4. Create a clean path to Pro licensing and optional cloud services.

### Product goals

1. Analyze a URL and return available media items.
2. Support single-item and supported collection/playlist workflows.
3. Provide quality, format, audio-only, filename, and folder controls.
4. Provide concurrent download queues with pause/resume/retry.
5. Persist download history locally.
6. Recover gracefully after crashes and network interruptions.
7. Provide clear, actionable errors.
8. Make platform adapters independently testable.

### Non-goals

- DRM circumvention.
- Unauthorized private-content access.
- Credential/session theft or authentication bypass.
- Evading platform security/rate-limit mechanisms.
- Hosting downloaded media on UMD servers.
- Building a centralized scraping/download farm in the MVP.

## 3. Target Users

### Persona A — Content researcher
Needs to save public educational/reference media for offline use where permitted.

### Persona B — Creator/editor
Needs to collect media they own or have permission to use and organize it into folders.

### Persona C — Social media manager
Needs repeatable bulk workflows for authorized/public assets.

### Persona D — Power user
Needs queues, naming templates, scheduling, metadata, and reliable retry behavior.

## 4. Core User Journeys

### Single download

Launch → Select platform/Auto Detect → Paste URL → Analyze → Review metadata/formats → Select quality → Select folder → Add to queue → Download → Open file.

### Playlist/collection

Paste supported collection URL → Analyze → Discover items → Select all/individual items → Apply global format settings → Queue → Download concurrently → Persist results.

### Retry

Failure → classify error → automatic retry if retryable → exponential backoff → final Failed state → manual Retry action.

### Scheduled monitoring

Create schedule → periodically analyze supported source → compare discovered IDs against local history → queue new items → update next-run time.

## 5. Functional Requirements

### FR-001 Platform selection

Provide:

- Platform cards.
- Auto Detect.
- Platform search/filter.
- Enabled/disabled platform state.

### FR-002 URL validation

The application shall:

- Normalize URLs.
- Validate URL syntax.
- Detect platform.
- Reject unsupported schemes.
- Prevent unsafe local-file/network requests.

### FR-003 Media analysis

Return, when available:

- Source URL.
- Platform.
- Collection metadata.
- Media items.
- Title.
- Creator/channel.
- Thumbnail.
- Duration.
- Publish date.
- Available formats.
- Estimated size.
- Capability flags.

### FR-004 Item selection

Users can:

- Select all/deselect all.
- Search.
- Filter.
- Sort.
- Exclude individual items.

### FR-005 Format selection

Logical choices:

- Best available.
- Video + audio.
- Video only.
- Audio only.

The UI must expose only formats actually returned by the source adapter.

### FR-006 Quality

Show only available qualities, e.g.:

- 2160p
- 1440p
- 1080p
- 720p
- 480p
- 360p

### FR-007 Download queue

Support:

- Multiple jobs.
- Configurable concurrency.
- Pause/resume/cancel.
- Retry.
- Remove.
- Reorder.
- Progress.
- Speed.
- ETA.
- Downloaded/total bytes.
- Status.

### FR-008 Persistence

Persist every job across application restart:

- State.
- Source metadata.
- Destination.
- Progress.
- Retry count.
- Error code.
- Timestamps.

### FR-009 Safe file handling

Use `.part` files and atomically rename only after successful completion and post-processing.

### FR-010 File naming

Templates:

- `{title}`
- `{creator} - {title}`
- `{index} - {title}`
- `{date} - {title}`

Sanitize invalid filesystem characters and path traversal sequences.

### FR-011 Folder organization

Default option:

`Downloads/{platform}/{creator}/{collection}/`

Users can configure folder templates.

### FR-012 History

Store:

- Title.
- Source.
- Platform.
- Local path.
- Status.
- Size.
- Date.
- Error state.

### FR-013 Scheduler

Support:

- One-time.
- Daily.
- Weekly.
- Interval.
- Enable/disable.
- Last execution.
- Next execution.

### FR-014 Collection monitoring

For supported collection sources:

- Analyze on schedule.
- Deduplicate by platform/external ID.
- Queue new items.
- Record monitoring results.

### FR-015 Notifications

Notify when:

- Queue completes.
- Download fails.
- Scheduled monitoring discovers new items.
- Scheduled task finishes.

### FR-016 Settings

Include:

- Default download directory.
- Quality.
- Container.
- Concurrency.
- Retry count/delay.
- Filename template.
- Folder template.
- Notifications.
- Theme.
- Language.
- Update channel.

## 6. Platform Adapter Requirements

Every adapter implements:

- `detect(url)`
- `normalize(url)`
- `analyze(url)`
- `getCapabilities()`
- `resolveFormats(item)`
- `createDownloadPlan(item, format, options)`

Adapters must not directly modify UI state, write arbitrary files, store credentials, access unrelated adapters, or bypass authentication/DRM/security controls.

## 7. Non-Functional Requirements

### Performance

- UI remains responsive during downloads.
- Analysis is asynchronous.
- Queue operations are non-blocking.
- Large lists use virtualization.
- Downloaded media is streamed rather than loaded fully into RAM.

### Reliability

- Crash-safe persistence.
- Resume when source supports range requests.
- Exponential retry.
- Atomic file completion.
- Startup recovery.

### Security

- No plaintext credentials.
- Minimal filesystem/network permissions.
- URL validation.
- Restricted process execution.
- OS secure storage for secrets.
- Structured FFmpeg arguments; never build shell commands from user input.

### Privacy

MVP is local-first. Downloaded media is never uploaded to UMD servers. Optional telemetry must be opt-in.

### Accessibility

Keyboard navigation, visible focus, screen-reader labels, sufficient contrast, and status indicators that do not rely only on color.

## 8. UX Screens

1. Dashboard
2. New Download
3. Platform Selector
4. URL Analyzer
5. Analysis Results
6. Format/Quality Configuration
7. Download Queue
8. Download Detail
9. History
10. Scheduled Jobs
11. Settings
12. Platform Manager
13. License/Account
14. Diagnostics/About

## 9. Error Model

Stable codes:

- `INVALID_URL`
- `UNSUPPORTED_PLATFORM`
- `ACCESS_RESTRICTED`
- `AUTH_REQUIRED`
- `MEDIA_UNAVAILABLE`
- `FORMAT_UNAVAILABLE`
- `NETWORK_ERROR`
- `RATE_LIMITED`
- `DISK_FULL`
- `PERMISSION_DENIED`
- `CHECKSUM_FAILED`
- `FFMPEG_FAILED`
- `UNKNOWN_ERROR`

Each error includes a human-readable message, stable code, retryable flag, and suggested action.

## 10. MVP Scope

### P0

- Tauri desktop shell.
- React/TypeScript UI.
- Platform selector.
- URL analyzer.
- Adapter registry.
- Initial supported adapters.
- Download queue.
- Progress.
- Pause/resume/cancel.
- Retry.
- SQLite.
- FFmpeg.
- History.
- Settings.
- Diagnostics.

### P1

- Scheduler.
- Collection monitoring.
- Smart folders.
- Filename templates.
- Notifications.
- Advanced metadata.

### P2

- Accounts.
- Licensing.
- Pro subscriptions.
- Device activation.
- Cloud settings sync.
- Optional telemetry.

## 11. Success Metrics

Product:

- Analysis success rate.
- Download completion rate.
- Median analysis time.
- Crash-free sessions.
- Failed-download recovery rate.

Business:

- Free-to-Pro conversion.
- Trial-to-paid conversion.
- MAU.
- Retention.
- License activation success.

## 12. Acceptance Criteria

MVP is ready when:

- A supported public URL can be analyzed.
- Items can be selected.
- Selected media can be queued and downloaded.
- Multiple jobs work concurrently.
- Queue survives restart.
- Pause/resume/cancel work.
- Failed jobs can be retried.
- Partial files are never reported as complete.
- History is searchable.
- Unsupported/restricted sources fail safely.
- No authentication, DRM, or security control is bypassed.
