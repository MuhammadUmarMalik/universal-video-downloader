import { normalizeAppError, type AnalyzeResponse, type AppError } from "@umd/shared-types";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { analyzeUrl } from "@/lib/desktopBridge";
import { AnalysisResults } from "./AnalysisResults";

function toAppError(error: unknown): AppError {
  return (
    normalizeAppError(error) ?? {
      code: "UNKNOWN_ERROR",
      message: "The analysis could not be completed.",
      retryable: true,
      userAction: "Check the URL and try again.",
    }
  );
}

export function AnalyzerPanel() {
  const [url, setUrl] = useState("");
  const [platformId, setPlatformId] = useState("");
  const [response, setResponse] = useState<AnalyzeResponse | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setResponse(null);
    setIsLoading(true);
    try {
      const result = await analyzeUrl({
        url: url.trim(),
        ...(platformId ? { platform_id: platformId } : {}),
      });
      setResponse(result);
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setIsLoading(false);
    }
  }

  return (
    <div className="space-y-8">
      <section aria-labelledby="analyzer-heading" className="rounded-xl border border-border bg-card p-6 shadow-sm">
        <div className="max-w-2xl">
          <p className="text-sm font-medium uppercase tracking-[0.16em] text-primary">Analyzer</p>
          <h2 id="analyzer-heading" className="mt-2 text-2xl font-semibold tracking-tight">
            Inspect an authorized public media URL
          </h2>
          <p className="mt-3 text-sm leading-6 text-muted-foreground">
            Use a public URL from a registered platform. Reddit and direct HTTPS media-file URLs currently support downloads. TikTok, YouTube, Facebook, and Instagram social-page URLs are detection-only until an official public media-byte path is available. No credentials, cookies, or access-control workarounds are used.
          </p>
        </div>

        <form className="mt-6 grid gap-4 sm:grid-cols-[1fr_180px_auto] sm:items-end" onSubmit={handleSubmit}>
          <label className="grid gap-2 text-sm font-medium" htmlFor="media-url">
            Public media URL
            <input
              id="media-url"
              name="url"
              type="url"
              required
              value={url}
              onChange={(event) => setUrl(event.target.value)}
              placeholder="https://cdn.example.com/video.mp4 or a supported social URL"
              className="h-10 rounded-md border border-input bg-background px-3 font-normal outline-none transition focus-visible:ring-2 focus-visible:ring-ring"
            />
          </label>

          <label className="grid gap-2 text-sm font-medium" htmlFor="platform-id">
            Platform
            <select
              id="platform-id"
              name="platform_id"
              value={platformId}
              onChange={(event) => setPlatformId(event.target.value)}
              className="h-10 rounded-md border border-input bg-background px-3 font-normal outline-none transition focus-visible:ring-2 focus-visible:ring-ring"
            >
              <option value="">Auto-detect</option>
              <option value="reddit">Reddit</option>
              <option value="tiktok">TikTok (detection only)</option>
              <option value="youtube">YouTube (detection only)</option>
              <option value="facebook">Facebook (detection only)</option>
              <option value="instagram">Instagram (detection only)</option>
              <option value="direct">Direct public media URL</option>
            </select>
          </label>

          <Button type="submit" disabled={isLoading || url.trim().length === 0}>
            {isLoading ? "Analyzing…" : "Analyze"}
          </Button>
        </form>

        {error ? (
          <div role="alert" className="mt-5 rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-900">
            <p className="font-semibold">{error.message}</p>
            {error.userAction ? <p className="mt-1">{error.userAction}</p> : null}
            {error.retryable ? <p className="mt-1 text-red-700">This problem may be temporary.</p> : null}
          </div>
        ) : null}
      </section>

      {response ? <AnalysisResults response={response} /> : null}
    </div>
  );
}
