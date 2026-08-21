import { describe, expect, it } from "vitest";
import { isAppError, normalizeAppError } from "@umd/shared-types";

describe("foundation error boundary", () => {
  it("accepts a structured user-facing error", () => {
    expect(
      isAppError({
        code: "UNKNOWN_ERROR",
        message: "Something went wrong.",
        retryable: false,
        userAction: "Try again.",
      }),
    ).toBe(true);
  });

  it("normalizes Rust snake_case error fields", () => {
    expect(
      normalizeAppError({
        code: "MEDIA_UNAVAILABLE",
        message: "This platform was detected, but no official public media download is available.",
        retryable: false,
        user_action: "Use a direct public media URL.",
      }),
    ).toEqual({
      code: "MEDIA_UNAVAILABLE",
      message: "This platform was detected, but no official public media download is available.",
      retryable: false,
      userAction: "Use a direct public media URL.",
    });
  });

  it("normalizes nested Tauri errors", () => {
    expect(
      normalizeAppError({
        error: {
          code: "INVALID_URL",
          message: "Enter a supported public media URL.",
          retryable: false,
        },
      }),
    ).toEqual({
      code: "INVALID_URL",
      message: "Enter a supported public media URL.",
      retryable: false,
    });
  });

  it("rejects raw exception-shaped values", () => {
    expect(isAppError(new Error("raw exception"))).toBe(false);
    expect(isAppError({ message: "missing stable code" })).toBe(false);
    expect(normalizeAppError(new Error("raw exception"))).toBeNull();
  });
});
