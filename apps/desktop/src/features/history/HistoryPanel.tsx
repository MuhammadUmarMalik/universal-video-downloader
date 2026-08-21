import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { clearHistory, deleteHistoryEntry, getHistory } from "@/lib/desktopBridge";
import type { HistoryEntry } from "@umd/shared-types";

function formatBytes(value: number | null): string {
  if (value === null || value < 0) return "Size unavailable";
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let amount = value;
  let unit = -1;
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024;
    unit += 1;
  }
  return `${amount.toFixed(1)} ${units[unit]}`;
}

function statusTone(status: HistoryEntry["status"]): string {
  if (status === "completed") return "border-emerald-200 bg-emerald-50 text-emerald-700";
  if (status === "failed") return "border-red-200 bg-red-50 text-red-700";
  return "border-slate-200 bg-slate-50 text-slate-600";
}

function statusLabel(entry: HistoryEntry): string {
  if (entry.status === "completed") return "Completed";
  if (entry.status === "failed") return entry.error_code === "FFMPEG_FAILED" ? "Processing failed" : "Failed";
  return "Cancelled";
}

export function HistoryPanel() {
  const queryClient = useQueryClient();
  const [search, setSearch] = useState("");
  const { data, error, isLoading, refetch } = useQuery({
    queryKey: ["history", search],
    queryFn: () => getHistory(search || undefined),
  });
  const deleteMutation = useMutation({
    mutationFn: deleteHistoryEntry,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["history"] }),
  });
  const clearMutation = useMutation({
    mutationFn: clearHistory,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["history"] }),
  });
  const entries = data ?? [];

  function clearAll() {
    if (entries.length > 0 && window.confirm("Clear all local history entries?")) {
      clearMutation.mutate();
    }
  }

  return (
    <section aria-labelledby="history-heading" className="space-y-6">
      <div className="flex flex-col gap-5 border-b border-border pb-6 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="text-sm font-medium uppercase tracking-[0.18em] text-primary">Local record</p>
          <h2 id="history-heading" className="mt-2 text-3xl font-semibold tracking-tight">Download history</h2>
          <p className="mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">Completed, failed, and cancelled jobs are kept locally with source metadata, destination, size, and terminal error information.</p>
        </div>
        <div className="flex gap-2">
          <Button onClick={() => void refetch()} size="sm" variant="outline">Refresh</Button>
          <Button disabled={entries.length === 0 || clearMutation.isPending} onClick={clearAll} size="sm" variant="outline">{clearMutation.isPending ? "Clearing…" : "Clear all"}</Button>
        </div>
      </div>

      <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4 sm:flex-row sm:items-center sm:justify-between">
        <label className="flex flex-1 items-center gap-3 text-sm" htmlFor="history-search">
          <span className="text-muted-foreground">Search</span>
          <input className="min-w-0 flex-1 rounded-md border border-border bg-background px-3 py-2 text-sm outline-none ring-primary focus:ring-2" id="history-search" onChange={(event) => setSearch(event.target.value)} placeholder="Title, filename, platform, or source URL" value={search} />
        </label>
        <span className="text-xs text-muted-foreground">{entries.length} {entries.length === 1 ? "entry" : "entries"}</span>
      </div>

      <div className="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
        {isLoading ? <div className="p-10 text-center text-sm text-muted-foreground">Loading local history…</div> : null}
        {error ? <div className="flex items-center justify-between gap-4 p-6 text-sm text-red-700"><span>History unavailable. The Electron bridge may be disconnected.</span><Button onClick={() => void refetch()} size="sm" variant="outline">Retry</Button></div> : null}
        {!isLoading && !error && entries.length === 0 ? <div className="p-10 text-center"><p className="font-medium">No history entries</p><p className="mt-2 text-sm text-muted-foreground">Terminal download results will appear here after the local worker records them.</p></div> : null}
        {entries.map((entry) => (
          <article className="grid gap-4 border-b border-border px-5 py-5 last:border-b-0 lg:grid-cols-[minmax(0,1fr)_160px_220px_auto] lg:items-center" key={entry.id}>
            <div className="min-w-0">
              <div className="flex items-start gap-3">
                <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-lg bg-slate-100 text-xs font-semibold text-slate-500">{entry.platform_name.slice(0, 3).toUpperCase()}</div>
                <div className="min-w-0">
                  <h3 className="truncate font-medium" title={entry.title}>{entry.title}</h3>
                  <p className="mt-1 truncate text-xs text-muted-foreground">{entry.filename} · {entry.platform_name}{entry.creator_name ? ` · ${entry.creator_name}` : ""}</p>
                  <p className="mt-1 truncate text-xs text-muted-foreground" title={entry.destination_path}>{entry.destination_path}</p>
                </div>
              </div>
            </div>
            <div><span className={`rounded-full border px-2.5 py-1 text-xs font-medium ${statusTone(entry.status)}`}>{statusLabel(entry)}</span>{entry.error_message ? <p className="mt-2 text-xs text-muted-foreground">{entry.error_message}</p> : null}</div>
            <div className="text-xs text-muted-foreground"><p>{formatBytes(entry.size_bytes)}</p><p className="mt-1">{new Date(entry.finished_at).toLocaleString()}</p><a className="mt-1 block truncate text-primary hover:underline" href={entry.source_url} rel="noreferrer" target="_blank">Open public source</a></div>
            <div className="flex justify-start lg:justify-end"><Button disabled={deleteMutation.isPending} onClick={() => deleteMutation.mutate(entry.id)} size="sm" variant="outline">Delete</Button></div>
          </article>
        ))}
      </div>
    </section>
  );
}
