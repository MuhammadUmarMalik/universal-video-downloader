import { describe, expect, it } from "vitest";
import { isAppError } from "@umd/shared-types";

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

  it("rejects raw exception-shaped values", () => {
    expect(isAppError(new Error("raw exception"))).toBe(false);
    expect(isAppError({ message: "missing stable code" })).toBe(false);
  });
});
