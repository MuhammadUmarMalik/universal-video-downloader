export const ERROR_CODES = [
  "INVALID_URL",
  "UNSUPPORTED_PLATFORM",
  "ACCESS_RESTRICTED",
  "AUTH_REQUIRED",
  "MEDIA_UNAVAILABLE",
  "FORMAT_UNAVAILABLE",
  "NETWORK_ERROR",
  "RATE_LIMITED",
  "DISK_FULL",
  "PERMISSION_DENIED",
  "CHECKSUM_FAILED",
  "FFMPEG_FAILED",
  "DATABASE_UNAVAILABLE",
  "DATABASE_MIGRATION_FAILED",
  "DATABASE_CORRUPT",
  "UNKNOWN_ERROR",
] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];

export interface AppError {
  code: ErrorCode;
  message: string;
  retryable: boolean;
  userAction?: string;
  diagnostic?: string;
}

export function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<AppError>;
  return (
    typeof candidate.code === "string" &&
    ERROR_CODES.includes(candidate.code as ErrorCode) &&
    typeof candidate.message === "string" &&
    typeof candidate.retryable === "boolean"
  );
}
