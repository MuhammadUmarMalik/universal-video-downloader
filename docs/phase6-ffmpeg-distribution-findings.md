# Phase 6 FFmpeg Distribution Findings

## Tauri v2 sidecars

Source: https://v2.tauri.app/develop/sidecar/

Tauri v2 supports bundling external binaries through `bundle.externalBin` in `tauri.conf.json`. Each supported architecture requires a target-triple-suffixed executable. Tauri requires explicit capability permission for sidecar execution or spawning, such as `shell:allow-execute` or `shell:allow-spawn`, scoped to the named sidecar. The sidecar is executed through Tauri's shell plugin and can receive arguments subject to capability-defined allow rules.

Implications: bundling is technically supported, but it adds target-specific artifacts, packaging/signing/update complexity, a new Tauri shell capability, and a strict argument policy. A Rust-owned subprocess wrapper would keep the frontend out of the execution path, but the distribution decision still determines how the executable is located and shipped.

## FFmpeg licensing and redistribution

Source: https://ffmpeg.org/legal.html

The FFmpeg project states that FFmpeg is generally LGPL 2.1-or-later, but optional GPL components can cause GPL obligations for the whole FFmpeg build. Its LGPL checklist for library redistribution includes avoiding `--enable-gpl` and `--enable-nonfree`, providing corresponding source, build configuration and changes, and documenting FFmpeg use. The page also warns that codec patents and jurisdiction-specific obligations may apply independently of copyright licensing.

Implications: a bundled binary requires a reproducible build policy, explicit enabled/disabled components, license notices and corresponding source availability, and a patent/commercial-use review. A system-installed strategy avoids redistributing the binary but creates a user-installation dependency and less deterministic runtime behavior.

## Current development environment

The sandbox has `/usr/bin/ffmpeg` and `/usr/bin/ffprobe`, both version 6.1.1-3ubuntu5. This confirms local development availability only; it does not establish a release distribution strategy.
