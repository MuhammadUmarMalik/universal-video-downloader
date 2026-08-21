export type MediaProcessingConfiguration =
  | {
      operation: "merge_audio_video";
      video_input: string;
      audio_input: string;
      output_filename: string;
    }
  | {
      operation: "extract_audio";
      input: string;
      output_filename: string;
    };

export interface CreateDownloadRequest {
  media_item_id: string;
  format_id: string;
  destination_path: string;
  filename: string;
  processing?: MediaProcessingConfiguration;
}

export type DownloadStatus =
  | "queued"
  | "resolving"
  | "downloading"
  | "processing"
  | "completed"
  | "paused"
  | "cancelled"
  | "failed";

export interface DownloadJob {
  id: string;
  media_item_id: string;
  format_id: string | null;
  status: DownloadStatus;
  priority: number;
  destination_path: string;
  temp_path: string | null;
  filename: string;
  total_bytes: number | null;
  downloaded_bytes: number;
  speed_bytes_per_sec: number | null;
  eta_seconds: number | null;
  retry_count: number;
  max_retries: number;
  processing_json: Record<string, unknown> | null;
  etag: string | null;
  last_modified: string | null;
  error_code: string | null;
  error_message: string | null;
  started_at: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
}

export type HistoryStatus = "completed" | "failed" | "cancelled";

export interface HistoryEntry {
  id: string;
  job_id: string;
  media_item_id: string;
  format_id: string | null;
  platform_id: string;
  platform_name: string;
  source_url: string;
  title: string;
  creator_name: string | null;
  destination_path: string;
  filename: string;
  status: HistoryStatus;
  size_bytes: number | null;
  error_code: string | null;
  error_message: string | null;
  created_at: string;
  finished_at: string;
}

export interface BandwidthSnapshot {
  limit_bytes_per_sec: number | null;
  current_bytes_per_sec: number;
  total_bytes: number;
}

export interface BandwidthStatus {
  limit_kbps: number | null;
  current_kbps: number;
  total_bytes: number;
}

export interface LiveProgressEvent {
  job_id: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  speed_bytes_per_sec: number | null;
  eta_seconds: number | null;
  bandwidth?: BandwidthSnapshot;
}

export type ScheduleType = "once" | "daily" | "weekly" | "interval";

export interface ScheduleConfiguration {
  format_id: string | null;
  destination_path: string;
  filename_template: string;
  auto_download_new_items: boolean;
}

export interface Schedule {
  id: string;
  source_id: string;
  schedule_type: ScheduleType;
  cron_expression: string | null;
  interval_seconds: number | null;
  enabled: boolean;
  last_run_at: string | null;
  next_run_at: string | null;
  configuration_json: ScheduleConfiguration | null;
  created_at: string;
  updated_at: string;
}

export interface CreateScheduleRequest {
  source_id: string;
  schedule_type: ScheduleType;
  interval_seconds: number | null;
  next_run_at: string;
  enabled: boolean;
  format_id: string | null;
  destination_path: string;
  filename_template: string;
  auto_download_new_items: boolean;
}

export interface UpdateScheduleRequest extends Omit<CreateScheduleRequest, "next_run_at"> {
  id: string;
  next_run_at: string | null;
}

export interface SchedulerRunReport {
  schedules_checked: number;
  schedules_processed: number;
  jobs_enqueued: number;
  schedules_failed: number;
}
