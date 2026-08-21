import type { MediaFormat } from "@umd/shared-types";

export function formatDuration(durationMs: number | null | undefined): string {
  if (!durationMs || durationMs <= 0) return "Unknown duration";
  const totalSeconds = Math.floor(durationMs / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) return `${hours}h ${minutes}m ${seconds}s`;
  if (minutes > 0) return `${minutes}m ${seconds}s`;
  return `${seconds}s`;
}

export function formatBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0) return "Unknown size";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(value >= 10 || unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
}

export function formatDimensions(format: MediaFormat): string {
  if (!format.width || !format.height) return "Dimensions unavailable";
  return `${format.width} × ${format.height}`;
}

export function formatCodec(format: MediaFormat): string {
  const codecs = [format.video_codec, format.audio_codec].filter(Boolean);
  return codecs.length > 0 ? codecs.join(" / ") : "Codec metadata unavailable";
}
