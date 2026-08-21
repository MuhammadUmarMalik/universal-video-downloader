import type { MediaFormat } from "@umd/shared-types";
import { describe, expect, it } from "vitest";
import { formatBytes, formatCodec, formatDimensions, formatDuration } from "./analysis";

function format(overrides: Partial<MediaFormat> = {}): MediaFormat {
  return {
    id: "format-1",
    media_item_id: "item-1",
    external_format_id: "mp4",
    container: "mp4",
    video_codec: null,
    audio_codec: null,
    width: null,
    height: null,
    fps: null,
    bitrate: null,
    sample_rate: null,
    channels: null,
    file_size_bytes: null,
    is_video: true,
    is_audio: false,
    is_progressive: true,
    metadata_json: null,
    created_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("analysis presentation helpers", () => {
  it("formats durations without exposing invalid values", () => {
    expect(formatDuration(0)).toBe("Unknown duration");
    expect(formatDuration(65_000)).toBe("1m 5s");
    expect(formatDuration(3_661_000)).toBe("1h 1m 1s");
  });

  it("formats byte sizes with bounded units", () => {
    expect(formatBytes(0)).toBe("Unknown size");
    expect(formatBytes(1_024)).toBe("1.0 KB");
    expect(formatBytes(5 * 1_024 * 1_024)).toBe("5.0 MB");
  });

  it("formats available format metadata with safe fallbacks", () => {
    expect(formatDimensions(format({ width: 1280, height: 720 }))).toBe("1280 × 720");
    expect(formatDimensions(format())).toBe("Dimensions unavailable");
    expect(formatCodec(format({ video_codec: "h264", audio_codec: "aac" }))).toBe("h264 / aac");
    expect(formatCodec(format())).toBe("Codec metadata unavailable");
  });
});
