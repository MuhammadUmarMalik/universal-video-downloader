import type {
  AnalyzeRequest,
  AnalyzeResponse,
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
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

export async function analyzeUrl(request: AnalyzeRequest): Promise<AnalyzeResponse> {
  return invoke<AnalyzeResponse>("analyze_url", { request });
}

export async function createDownload(request: CreateDownloadRequest): Promise<DownloadJob> {
  return invoke<DownloadJob>("create_download", { request });
}

export async function cancelDownload(jobId: string): Promise<boolean> {
  return invoke<boolean>("cancel_download", { jobId });
}

export async function getDownloadJobs(): Promise<DownloadJob[]> {
  return invoke<DownloadJob[]>("get_download_jobs");
}

export async function subscribeDownloadProgress(
  onProgress: (event: LiveProgressEvent) => void,
): Promise<UnlistenFn> {
  const unlisten = await listen<LiveProgressEvent>("download-progress", (event) => {
    onProgress(event.payload);
  });
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
      tauri: false,
      message: "The web preview is running outside the Tauri shell.",
    };
  }
}
