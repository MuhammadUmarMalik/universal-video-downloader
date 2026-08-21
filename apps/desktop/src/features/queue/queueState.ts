import { create } from "zustand";
import type {
  BandwidthSnapshot,
  DownloadJob,
  DownloadStatus,
  LiveProgressEvent,
} from "@umd/shared-types";

export type QueueFilter = "all" | "active" | "processing" | "completed" | "failed";
export type QueueSort = "priority" | "created_desc" | "status";

interface QueueState {
  jobs: DownloadJob[];
  bandwidth: BandwidthSnapshot;
  selectedIds: string[];
  filter: QueueFilter;
  sort: QueueSort;
  setJobs: (jobs: DownloadJob[]) => void;
  applyProgress: (event: LiveProgressEvent) => void;
  toggleSelected: (jobId: string) => void;
  selectVisible: (jobIds: string[]) => void;
  clearSelection: () => void;
  setFilter: (filter: QueueFilter) => void;
  setSort: (sort: QueueSort) => void;
}

export const useQueueStore = create<QueueState>((set) => ({
  jobs: [],
  bandwidth: {
    limit_bytes_per_sec: null,
    current_bytes_per_sec: 0,
    total_bytes: 0,
  },
  selectedIds: [],
  filter: "all",
  sort: "priority",
  setJobs: (jobs) =>
    set((state) => ({
      jobs,
      selectedIds: state.selectedIds.filter((id) => jobs.some((job) => job.id === id)),
    })),
  applyProgress: (event) =>
    set((state) => ({
      bandwidth: event.bandwidth ?? state.bandwidth,
      jobs: state.jobs.map((job) =>
        job.id === event.job_id
          ? {
              ...job,
              downloaded_bytes: event.downloaded_bytes,
              total_bytes: event.total_bytes ?? job.total_bytes,
              speed_bytes_per_sec: event.speed_bytes_per_sec,
              eta_seconds: event.eta_seconds,
              updated_at: new Date().toISOString(),
            }
          : job,
      ),
    })),
  toggleSelected: (jobId) =>
    set((state) => ({
      selectedIds: state.selectedIds.includes(jobId)
        ? state.selectedIds.filter((id) => id !== jobId)
        : [...state.selectedIds, jobId],
    })),
  selectVisible: (jobIds) => set({ selectedIds: jobIds }),
  clearSelection: () => set({ selectedIds: [] }),
  setFilter: (filter) => set({ filter }),
  setSort: (sort) => set({ sort }),
}));

const filterPredicates: Record<QueueFilter, (job: DownloadJob) => boolean> = {
  all: () => true,
  active: (job) => ["queued", "resolving", "downloading", "processing"].includes(job.status),
  processing: (job) => job.status === "processing",
  completed: (job) => job.status === "completed",
  failed: (job) => job.status === "failed",
};

const statusOrder: Record<DownloadStatus, number> = {
  processing: 0,
  downloading: 1,
  resolving: 2,
  queued: 3,
  failed: 4,
  cancelled: 5,
  paused: 6,
  completed: 7,
};

export function selectVisibleJobs(
  jobs: DownloadJob[],
  filter: QueueFilter,
  sort: QueueSort,
): DownloadJob[] {
  return [...jobs]
    .filter(filterPredicates[filter])
    .sort((left, right) => {
      if (sort === "status") return statusOrder[left.status] - statusOrder[right.status];
      if (sort === "created_desc") return right.created_at.localeCompare(left.created_at);
      return right.priority - left.priority || left.created_at.localeCompare(right.created_at);
    });
}

export function progressPercent(job: DownloadJob): number | null {
  if (job.total_bytes === null || job.total_bytes <= 0) return null;
  return Math.min(100, Math.max(0, (job.downloaded_bytes / job.total_bytes) * 100));
}

export function isCancellable(status: DownloadStatus): boolean {
  return ["queued", "resolving", "downloading", "processing"].includes(status);
}

export function statusLabel(job: DownloadJob): string {
  if (job.status === "processing") return "Processing · FFmpeg";
  if (job.status === "downloading") return "Downloading";
  if (job.status === "resolving") return "Resolving source";
  if (job.status === "queued") return "Queued";
  if (job.status === "completed") return "Completed";
  if (job.status === "failed") return job.error_code === "FFMPEG_FAILED" ? "Processing failed" : "Download failed";
  if (job.status === "cancelled") return "Cancelled";
  return "Paused";
}

export function formatBytes(value: number | null): string {
  if (value === null || value < 0) return "—";
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let amount = value;
  let index = -1;
  while (amount >= 1024 && index < units.length - 1) {
    amount /= 1024;
    index += 1;
  }
  return `${amount.toFixed(1)} ${units[index]}`;
}

export function formatSpeed(value: number | null): string {
  return value === null || value <= 0 ? "—" : `${formatBytes(value)}/s`;
}

export function formatEta(value: number | null): string {
  if (value === null || value < 0) return "—";
  const seconds = Math.floor(value);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${seconds % 60}s`;
}
