import { describe, expect, test } from "bun:test";

import {
  forgejoInstanceUrlError,
  shouldRedirectAfterHealthPoll,
} from "./install-flow";

describe("shouldRedirectAfterHealthPoll", () => {
  test("waits when the health request fails", () => {
    expect(shouldRedirectAfterHealthPoll({ kind: "error" })).toBe(false);
  });

  test("waits when the server returns a non-success status", () => {
    expect(
      shouldRedirectAfterHealthPoll({
        kind: "response",
        ok: false,
      }),
    ).toBe(false);
  });

  test("waits while the server is still in install mode", () => {
    expect(
      shouldRedirectAfterHealthPoll({
        kind: "response",
        ok: true,
        mode: "install",
      }),
    ).toBe(false);
  });

  test("redirects only after the server returns success outside install mode", () => {
    expect(
      shouldRedirectAfterHealthPoll({
        kind: "response",
        ok: true,
        mode: "normal",
      }),
    ).toBe(true);
  });
});

describe("forgejoInstanceUrlError", () => {
  test("accepts an https instance URL", () => {
    expect(forgejoInstanceUrlError("https://git.example.com")).toBeNull();
  });

  test("accepts an https URL with a path and trims whitespace", () => {
    expect(forgejoInstanceUrlError("  https://git.example.com/  ")).toBeNull();
  });

  test("rejects an http URL", () => {
    expect(forgejoInstanceUrlError("http://git.example.com")).toBe(
      "The Forgejo instance URL must use https.",
    );
  });

  test("rejects a missing URL", () => {
    expect(forgejoInstanceUrlError("   ")).toBe(
      "Enter the Forgejo instance URL before continuing.",
    );
  });

  test("rejects a URL that does not parse", () => {
    expect(forgejoInstanceUrlError("git.example.com")).toBe(
      "Enter a valid Forgejo instance URL, e.g. https://git.example.com.",
    );
  });
});
