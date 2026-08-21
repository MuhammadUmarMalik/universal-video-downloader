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

/** Normalize frontend-shaped, Rust-shaped, or nested Tauri invoke errors. */
export function normalizeAppError(value: unknown): AppError | null {
  const candidates: unknown[] = [value];
  if (typeof value === "object" && value !== null) {
    const record = value as Record<string, unknown>;
    candidates.push(record.error, record.payload, record.data);
  }
  if (typeof value === "string") {
    try {
      candidates.push(JSON.parse(value));
    } catch {
      // Tauri may reject with a non-JSON diagnostic string; use the stable fallback.
    }
  }

  for (const candidateValue of candidates) {
    if (typeof candidateValue !== "object" || candidateValue === null) {
      if (isAppError(candidateValue)) return candidateValue;
      continue;
    }
    const candidate = candidateValue as Record<string, unknown>;
    if (
      typeof candidate.code !== "string" ||
      !ERROR_CODES.includes(candidate.code as ErrorCode) ||
      typeof candidate.message !== "string" ||
      typeof candidate.retryable !== "boolean"
    ) {
      continue;
    }
    return {
      code: candidate.code as ErrorCode,
      message: candidate.message,
      retryable: candidate.retryable,
      ...(typeof candidate.user_action === "string"
        ? { userAction: candidate.user_action }
        : typeof candidate.userAction === "string"
          ? { userAction: candidate.userAction }
          : {}),
      ...(typeof candidate.diagnostic === "string" ? { diagnostic: candidate.diagnostic } : {}),
    };
  }
  return null;
}
