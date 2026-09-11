export type FinishHealthPollResult =
  | { kind: "error" }
  | { kind: "response"; ok: boolean; mode?: string };

export function shouldRedirectAfterHealthPoll(
  result: FinishHealthPollResult,
): boolean {
  return result.kind === "response" && result.ok && result.mode !== "install";
}

/**
 * Returns the validation message for a Forgejo instance base URL, or null when
 * the URL is usable. Forgejo instances are only contacted over HTTPS.
 */
export function forgejoInstanceUrlError(url: string): string | null {
  const trimmed = url.trim();
  if (!trimmed) {
    return "Enter the Forgejo instance URL before continuing.";
  }
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    return "Enter a valid Forgejo instance URL, e.g. https://git.example.com.";
  }
  if (parsed.protocol !== "https:") {
    return "The Forgejo instance URL must use https.";
  }
  return null;
}
