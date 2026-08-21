export interface AnalyzeRequest {
  url: string;
  platform_id?: string;
}

export interface PlatformCapabilities {
  single_item: boolean;
  collections: boolean;
  audio_only: boolean;
  thumbnails: boolean;
  metadata: boolean;
  resume: boolean;
  scheduling: boolean;
}

export interface NormalizedSource {
  platform_id: string;
  canonical_url: string;
  external_id: string;
}

export interface MediaItem {
  id: string;
  source_id: string;
  collection_id?: string | null;
  external_id?: string | null;
  canonical_url: string;
  title: string;
  creator_name?: string | null;
  creator_id?: string | null;
  thumbnail_url?: string | null;
  duration_ms?: number | null;
  published_at?: string | null;
  position?: number | null;
  metadata_json?: unknown;
  first_seen_at: string;
  last_seen_at: string;
}

export interface MediaFormat {
  id: string;
  media_item_id: string;
  external_format_id?: string | null;
  container?: string | null;
  video_codec?: string | null;
  audio_codec?: string | null;
  width?: number | null;
  height?: number | null;
  fps?: number | null;
  bitrate?: number | null;
  sample_rate?: number | null;
  channels?: number | null;
  file_size_bytes?: number | null;
  is_video: boolean;
  is_audio: boolean;
  is_progressive: boolean;
  metadata_json?: unknown;
  created_at: string;
}

export interface AnalyzeResponse {
  platform_id: string;
  capabilities: PlatformCapabilities;
  source: NormalizedSource;
  items: MediaItem[];
  formats: MediaFormat[];
}
