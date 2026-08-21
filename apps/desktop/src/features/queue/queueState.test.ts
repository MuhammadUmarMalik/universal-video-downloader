import { describe, expect, it } from "vitest";
import type { DownloadJob } from "@umd/shared-types";
import {
  formatEta,
  formatSpeed,
  isCancellable,
  progressPercent,
  selectVisibleJobs,
  statusLabel,
  useQueueStore,
} from "./queueState";

function job(overrides: Partial<DownloadJob> = {}): DownloadJob {
  return {
    id: "job-1",
    media_item_id: "item-1",
    format_id: "format-1",
    status: "downloading",
    priority: 0,
    destination_path: "/downloads",
    temp_path: "/downloads/video.mp4.part",
    filename: "video.mp4",
    total_bytes: 1_000,
    downloaded_bytes: 250,
    speed_bytes_per_sec: 500,
    eta_seconds: 2,
    retry_count: 0,
    max_retries: 3,
    processing_json: null,
    etag: null,
    last_modified: null,
    error_code: null,
    error_message: null,
    started_at: null,
    completed_at: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("queue state projection", () => {
  it("represents processing as a distinct FFmpeg stage", () => {
    const processing = job({ status: "processing", total_bytes: null });
    expect(statusLabel(processing)).toBe("Processing · FFmpeg");
    expect(progressPercent(processing)).toBeNull();
    expect(isCancellable("processing")).toBe(true);
  });

  it("updates a known job from a live progress event without inventing jobs", () => {
    useQueueStore.getState().setJobs([job()]);
    useQueueStore.getState().applyProgress({
      job_id: "job-1",
      downloaded_bytes: 800,
      total_bytes: 2_000,
      speed_bytes_per_sec: 1_000,
      eta_seconds: 1,
      bandwidth: {
        limit_bytes_per_sec: 1_024,
        current_bytes_per_sec: 900,
        total_bytes: 8_000,
      },
    });
    expect(useQueueStore.getState().jobs[0]).toMatchObject({
      downloaded_bytes: 800,
      total_bytes: 2_000,
      speed_bytes_per_sec: 1_000,
      eta_seconds: 1,
    });
    expect(useQueueStore.getState().bandwidth).toEqual({
      limit_bytes_per_sec: 1_024,
      current_bytes_per_sec: 900,
      total_bytes: 8_000,
    });

    useQueueStore.getState().applyProgress({
      job_id: "unknown",
      downloaded_bytes: 1,
      total_bytes: 1,
      speed_bytes_per_sec: null,
      eta_seconds: null,
    });
    expect(useQueueStore.getState().jobs).toHaveLength(1);
  });

  it("filters processing and sorts active jobs by priority", () => {
    const jobs = [
      job({ id: "low", priority: 1, status: "processing" }),
      job({ id: "high", priority: 5, status: "queued" }),
      job({ id: "done", priority: 9, status: "completed" }),
    ];
    expect(selectVisibleJobs(jobs, "processing", "priority").map((item) => item.id)).toEqual(["low"]);
    expect(selectVisibleJobs(jobs, "active", "priority").map((item) => item.id)).toEqual(["high", "low"]);
  });

  it("formats queue telemetry with safe fallbacks", () => {
    expect(formatSpeed(1_024)).toBe("1.0 KB/s");
    expect(formatSpeed(null)).toBe("—");
    expect(formatEta(65)).toBe("1m 5s");
    expect(formatEta(null)).toBe("—");
  });
});
