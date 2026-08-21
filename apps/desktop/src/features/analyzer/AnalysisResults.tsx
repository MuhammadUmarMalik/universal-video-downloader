import type { AnalyzeResponse, MediaFormat } from "@umd/shared-types";
import { formatBytes, formatCodec, formatDimensions, formatDuration } from "./analysis";

interface AnalysisResultsProps {
  response: AnalyzeResponse;
}

function Capability({ label, enabled }: { label: string; enabled: boolean }) {
  return (
    <span
      className={enabled
        ? "rounded-full bg-emerald-100 px-3 py-1 text-xs font-medium text-emerald-800"
        : "rounded-full bg-muted px-3 py-1 text-xs font-medium text-muted-foreground"}
    >
      {enabled ? "Available" : "Not available"} · {label}
    </span>
  );
}

function FormatRow({ format }: { format: MediaFormat }) {
  return (
    <tr className="border-b border-border last:border-0">
      <td className="px-4 py-3 font-medium text-foreground">{format.container?.toUpperCase() ?? "Unknown"}</td>
      <td className="px-4 py-3 text-muted-foreground">{formatDimensions(format)}</td>
      <td className="px-4 py-3 text-muted-foreground">{formatCodec(format)}</td>
      <td className="px-4 py-3 text-muted-foreground">{formatBytes(format.file_size_bytes)}</td>
      <td className="px-4 py-3 text-muted-foreground">{format.is_progressive ? "Direct" : "Playlist"}</td>
    </tr>
  );
}

export function AnalysisResults({ response }: AnalysisResultsProps) {
  return (
    <section aria-labelledby="analysis-results-heading" className="space-y-6">
      <div className="flex flex-col justify-between gap-3 sm:flex-row sm:items-start">
        <div>
          <p className="text-sm font-medium uppercase tracking-[0.16em] text-primary">Analysis complete</p>
          <h2 id="analysis-results-heading" className="mt-2 text-2xl font-semibold tracking-tight">
            {response.items.length} media item{response.items.length === 1 ? "" : "s"} found
          </h2>
          <p className="mt-2 break-all text-sm text-muted-foreground">{response.source.canonical_url}</p>
        </div>
        <span className="rounded-full bg-primary/10 px-3 py-1 text-xs font-semibold uppercase tracking-wide text-primary">
          {response.platform_id}
        </span>
      </div>

      <div className="flex flex-wrap gap-2" aria-label="Platform capabilities">
        <Capability label="Metadata" enabled={response.capabilities.metadata} />
        <Capability label="Thumbnails" enabled={response.capabilities.thumbnails} />
        <Capability label="Collections" enabled={response.capabilities.collections} />
        <Capability label="Resume" enabled={response.capabilities.resume} />
        <Capability label="Scheduling" enabled={response.capabilities.scheduling} />
      </div>

      <div className="space-y-4">
        {response.items.map((item) => {
          const formats = response.formats.filter((format) => format.media_item_id === item.id);
          return (
            <article key={item.id} className="rounded-xl border border-border bg-card p-5 shadow-sm">
              <div className="flex flex-col justify-between gap-4 sm:flex-row">
                <div>
                  <h3 className="text-lg font-semibold">{item.title}</h3>
                  <dl className="mt-3 grid gap-2 text-sm text-muted-foreground sm:grid-cols-3">
                    <div>
                      <dt className="font-medium text-foreground">Creator</dt>
                      <dd>{item.creator_name ?? "Unknown creator"}</dd>
                    </div>
                    <div>
                      <dt className="font-medium text-foreground">Duration</dt>
                      <dd>{formatDuration(item.duration_ms)}</dd>
                    </div>
                    <div>
                      <dt className="font-medium text-foreground">Thumbnail</dt>
                      <dd>{item.thumbnail_url ? "Available" : "Not available"}</dd>
                    </div>
                  </dl>
                </div>
                <span className="self-start rounded-md bg-secondary px-3 py-2 text-sm text-secondary-foreground">
                  {formats.length} format{formats.length === 1 ? "" : "s"}
                </span>
              </div>

              <div className="mt-5 overflow-x-auto rounded-lg border border-border">
                <table className="w-full min-w-[680px] text-left text-sm">
                  <caption className="sr-only">Available formats for {item.title}</caption>
                  <thead className="bg-muted text-xs uppercase tracking-wide text-muted-foreground">
                    <tr>
                      <th scope="col" className="px-4 py-3">Container</th>
                      <th scope="col" className="px-4 py-3">Dimensions</th>
                      <th scope="col" className="px-4 py-3">Codec</th>
                      <th scope="col" className="px-4 py-3">Size</th>
                      <th scope="col" className="px-4 py-3">Delivery</th>
                    </tr>
                  </thead>
                  <tbody>
                    {formats.length > 0 ? (
                      formats.map((format) => <FormatRow key={format.id} format={format} />)
                    ) : (
                      <tr>
                        <td className="px-4 py-4 text-muted-foreground" colSpan={5}>
                          This platform was detected, but no official public downloadable format is available. The app will not use credentials, cookies, or access-control workarounds.
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}
