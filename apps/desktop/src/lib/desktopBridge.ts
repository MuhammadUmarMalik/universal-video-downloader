import type {
  AnalyzeRequest,
  AnalyzeResponse,
  BandwidthStatus,
  CreateDownloadRequest,
  DownloadJob,
  FoundationStatus,
  HistoryEntry,
  CreateScheduleRequest,
  LiveProgressEvent,
  Schedule,
  SchedulerRunReport,
  UpdateScheduleRequest,
} from "@umd/shared-types";

type ElectronApi = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  onDownloadProgress(callback: (event: LiveProgressEvent) => void): () => void;
  onBridgeError(callback: (payload: { message: string }) => void): () => void;
};

declare global {
  interface Window {
    electronAPI?: ElectronApi;
  }
}

function api(): ElectronApi {
  if (!window.electronAPI) {
    throw new Error("The Electron desktop bridge is unavailable.");
  }
  return window.electronAPI;
}

function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return api().invoke<T>(command, args);
}

export async function analyzeUrl(request: AnalyzeRequest): Promise<AnalyzeResponse> {
  return invoke<AnalyzeResponse>("analyze_url", { request });
}

export async function createDownload(request: CreateDownloadRequest): Promise<DownloadJob> {
  return invoke<DownloadJob>("create_download", { request });
}

export async function getBandwidthStatus(): Promise<BandwidthStatus> {
  return invoke<BandwidthStatus>("get_bandwidth_status");
}

export async function setBandwidthLimit(limitKbps: number): Promise<BandwidthStatus> {
  return invoke<BandwidthStatus>("set_bandwidth_limit", { limitKbps });
}

export async function cancelDownload(jobId: string): Promise<boolean> {
  return invoke<boolean>("cancel_download", { jobId });
}

export async function getDownloadJobs(): Promise<DownloadJob[]> {
  return invoke<DownloadJob[]>("get_download_jobs");
}

export async function subscribeDownloadProgress(
  onProgress: (event: LiveProgressEvent) => void,
): Promise<() => void> {
  const unlisten = api().onDownloadProgress(onProgress);
  try {
    await invoke<boolean>("subscribe_download_progress");
    return unlisten;
  } catch (error) {
    unlisten();
    throw error;
  }
}

export async function getHistory(query?: string): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>("get_history", { query: query ?? null });
}

export async function deleteHistoryEntry(id: string): Promise<boolean> {
  return invoke<boolean>("delete_history_entry", { id });
}

export async function clearHistory(): Promise<number> {
  return invoke<number>("clear_history");
}

export async function getSchedules(): Promise<Schedule[]> {
  return invoke<Schedule[]>("get_schedules");
}

export async function createSchedule(request: CreateScheduleRequest): Promise<Schedule> {
  return invoke<Schedule>("create_schedule", { request });
}

export async function updateSchedule(request: UpdateScheduleRequest): Promise<Schedule> {
  return invoke<Schedule>("update_schedule", { request });
}

export async function deleteSchedule(id: string): Promise<boolean> {
  return invoke<boolean>("delete_schedule", { id });
}

export async function getSchedulerEnabled(): Promise<boolean> {
  return invoke<boolean>("get_scheduler_enabled");
}

export async function setSchedulerEnabled(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_scheduler_enabled", { enabled });
}

export async function runSchedulerNow(): Promise<SchedulerRunReport> {
  return invoke<SchedulerRunReport>("run_scheduler_now");
}

export async function getFoundationStatus(): Promise<FoundationStatus> {
  try {
    return await invoke<FoundationStatus>("get_foundation_status");
  } catch {
    return {
      appName: "Universal Media Downloader",
      phase: "foundation",
      electron: false,
      message: "The web preview is running outside the Electron shell.",
    };
  }
}
