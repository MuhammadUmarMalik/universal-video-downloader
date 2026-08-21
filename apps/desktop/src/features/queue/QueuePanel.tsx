import { useEffect, useMemo } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { cancelDownload, getDownloadJobs, subscribeDownloadProgress } from "@/lib/tauri";
import {
  formatBytes,
  formatEta,
  formatSpeed,
  isCancellable,
  progressPercent,
  selectVisibleJobs,
  statusLabel,
  useQueueStore,
} from "./queueState";
import type { DownloadJob } from "@umd/shared-types";

const queueQueryKey = ["download-jobs"];

function statusTone(status: DownloadJob["status"]): string {
  if (status === "completed") return "border-emerald-200 bg-emerald-50 text-emerald-700";
  if (status === "failed") return "border-red-200 bg-red-50 text-red-700";
  if (status === "processing") return "border-violet-200 bg-violet-50 text-violet-700";
  if (status === "downloading") return "border-blue-200 bg-blue-50 text-blue-700";
  if (status === "cancelled") return "border-slate-200 bg-slate-50 text-slate-600";
  return "border-amber-200 bg-amber-50 text-amber-700";
}

function QueueRow({ job }: { job: DownloadJob }) {
  const selected = useQueueStore((state) => state.selectedIds.includes(job.id));
  const toggleSelected = useQueueStore((state) => state.toggleSelected);
  const cancelMutation = useMutation({
    mutationFn: () => cancelDownload(job.id),
  });
  const percent = progressPercent(job);
  const processing = job.status === "processing";

  return (
    <article className="grid gap-4 border-b border-border px-5 py-5 last:border-b-0 lg:grid-cols-[auto_minmax(0,1fr)_180px_160px_auto] lg:items-center">
      <input
        aria-label={`Select ${job.filename}`}
        checked={selected}
        className="h-4 w-4 rounded border-border accent-primary"
        onChange={() => toggleSelected(job.id)}
        type="checkbox"
      />
      <div className="min-w-0">
        <div className="flex items-start gap-3">
          <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-lg bg-slate-100 text-xs font-semibold text-slate-500">
            {processing ? "FX" : job.filename.split(".").pop()?.toUpperCase().slice(0, 3) ?? "MED"}
          </div>
          <div className="min-w-0">
            <h3 className="truncate font-medium text-foreground" title={job.filename}>{job.filename}</h3>
            <p className="mt-1 truncate text-xs text-muted-foreground">{job.media_item_id} · {job.format_id ?? "Format pending"}</p>
            <p className="mt-1 text-xs text-muted-foreground">Public source · local destination</p>
          </div>
        </div>
      </div>
      <div className="min-w-0">
        <div className="flex items-center justify-between gap-3 text-xs">
          <span className={`rounded-full border px-2.5 py-1 font-medium ${statusTone(job.status)}`}>
            {statusLabel(job)}
          </span>
          <span className="text-muted-foreground">{percent === null ? "—" : `${Math.round(percent)}%`}</span>
        </div>
        <div className="mt-3 h-2 overflow-hidden rounded-full bg-slate-100" aria-label={`${job.filename} progress`}>
          <div
            className={`h-full rounded-full transition-all ${processing ? "bg-violet-500" : "bg-primary"}`}
            style={{ width: `${percent ?? (processing ? 100 : 4)}%` }}
          />
        </div>
      </div>
      <div className="grid grid-cols-3 gap-3 text-xs text-muted-foreground lg:block lg:space-y-1">
        <span className="block">{formatBytes(job.downloaded_bytes)}{job.total_bytes === null ? "" : ` / ${formatBytes(job.total_bytes)}`}</span>
        <span className="block">{processing ? "FFmpeg" : formatSpeed(job.speed_bytes_per_sec)}</span>
        <span className="block">ETA {processing ? "—" : formatEta(job.eta_seconds)}</span>
      </div>
      <div className="flex justify-start lg:justify-end">
        {isCancellable(job.status) ? (
          <Button
            disabled={cancelMutation.isPending}
            onClick={() => cancelMutation.mutate()}
            size="sm"
            variant="outline"
          >
            {cancelMutation.isPending ? "Stopping…" : "Cancel"}
          </Button>
        ) : (
          <span className="text-xs text-muted-foreground">{job.status === "failed" ? job.error_code ?? "Needs review" : "No action"}</span>
        )}
      </div>
    </article>
  );
}

export function QueuePanel() {
  const queryClient = useQueryClient();
  const bulkCancelMutation = useMutation({
    mutationFn: async (jobIds: string[]) => Promise.all(jobIds.map((jobId) => cancelDownload(jobId))),
    onSuccess: () => {
      clearSelection();
      void queryClient.invalidateQueries({ queryKey: queueQueryKey });
    },
  });
  const { data, error, isLoading, refetch } = useQuery({
    queryKey: queueQueryKey,
    queryFn: getDownloadJobs,
    refetchInterval: 5_000,
  });
  const jobs = useQueueStore((state) => state.jobs);
  const setJobs = useQueueStore((state) => state.setJobs);
  const applyProgress = useQueueStore((state) => state.applyProgress);
  const selectedIds = useQueueStore((state) => state.selectedIds);
  const filter = useQueueStore((state) => state.filter);
  const sort = useQueueStore((state) => state.sort);
  const setFilter = useQueueStore((state) => state.setFilter);
  const setSort = useQueueStore((state) => state.setSort);
  const selectVisible = useQueueStore((state) => state.selectVisible);
  const clearSelection = useQueueStore((state) => state.clearSelection);

  useEffect(() => {
    if (data) setJobs(data);
  }, [data, setJobs]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let active = true;
    void subscribeDownloadProgress(applyProgress)
      .then((cleanup) => {
        if (active) unlisten = cleanup;
        else cleanup();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, [applyProgress]);

  const visibleJobs = useMemo(() => selectVisibleJobs(jobs, filter, sort), [jobs, filter, sort]);
  const activeCount = jobs.filter((job) => ["queued", "resolving", "downloading", "processing"].includes(job.status)).length;
  const processingCount = jobs.filter((job) => job.status === "processing").length;

  function selectAll() {
    selectVisible(visibleJobs.map((job) => job.id));
  }

  function refresh() {
    void queryClient.invalidateQueries({ queryKey: queueQueryKey });
    void refetch();
  }

  return (
    <section aria-labelledby="queue-heading" className="space-y-6">
      <div className="flex flex-col gap-5 border-b border-border pb-6 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="text-sm font-medium uppercase tracking-[0.18em] text-primary">Managed queue</p>
          <h2 id="queue-heading" className="mt-2 text-3xl font-semibold tracking-tight">Downloads and processing</h2>
          <p className="mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">
            Monitor Rust-managed transfers and the FFmpeg processing stage without exposing network or filesystem work to the browser.
          </p>
        </div>
        <Button onClick={refresh} size="sm" variant="outline">Refresh queue</Button>
      </div>

      <div className="grid gap-3 sm:grid-cols-3">
        <div className="rounded-xl border border-border bg-card p-4"><p className="text-xs uppercase tracking-wide text-muted-foreground">Total jobs</p><p className="mt-2 text-2xl font-semibold">{jobs.length}</p></div>
        <div className="rounded-xl border border-border bg-card p-4"><p className="text-xs uppercase tracking-wide text-muted-foreground">Active</p><p className="mt-2 text-2xl font-semibold">{activeCount}</p></div>
        <div className="rounded-xl border border-violet-200 bg-violet-50 p-4"><p className="text-xs uppercase tracking-wide text-violet-700">FFmpeg processing</p><p className="mt-2 text-2xl font-semibold text-violet-900">{processingCount}</p></div>
      </div>

      <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex flex-wrap gap-2" role="group" aria-label="Queue filters">
          {(["all", "active", "processing", "completed", "failed"] as const).map((value) => (
            <button
              className={`rounded-full px-3 py-1.5 text-xs font-medium transition ${filter === value ? "bg-primary text-primary-foreground" : "bg-slate-100 text-slate-600 hover:bg-slate-200"}`}
              key={value}
              onClick={() => setFilter(value)}
              type="button"
            >
              {value[0].toUpperCase() + value.slice(1)}
            </button>
          ))}
        </div>
        <div className="flex items-center gap-3">
          <label className="text-xs text-muted-foreground" htmlFor="queue-sort">Sort</label>
          <select className="rounded-md border border-border bg-background px-2 py-1.5 text-xs" id="queue-sort" onChange={(event) => setSort(event.target.value as "priority" | "created_desc" | "status")} value={sort}>
            <option value="priority">Priority</option>
            <option value="created_desc">Newest</option>
            <option value="status">Status</option>
          </select>
          <Button onClick={selectedIds.length === visibleJobs.length ? clearSelection : selectAll} size="sm" variant="outline">
            {selectedIds.length === visibleJobs.length && visibleJobs.length > 0 ? "Clear selection" : "Select visible"}
          </Button>
          {selectedIds.length > 0 ? (
            <Button
              disabled={bulkCancelMutation.isPending}
              onClick={() => bulkCancelMutation.mutate(selectedIds.filter((id) => {
                const selectedJob = jobs.find((job) => job.id === id);
                return selectedJob ? isCancellable(selectedJob.status) : false;
              }))}
              size="sm"
              variant="outline"
            >
              {bulkCancelMutation.isPending ? "Stopping…" : "Cancel selected"}
            </Button>
          ) : null}
        </div>
      </div>

      <div className="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
        {isLoading ? <div className="p-10 text-center text-sm text-muted-foreground">Loading the local queue…</div> : null}
        {error ? <div className="flex items-center justify-between gap-4 p-6 text-sm text-red-700"><span>Queue unavailable. The Tauri bridge may be disconnected.</span><Button onClick={refresh} size="sm" variant="outline">Retry</Button></div> : null}
        {!isLoading && !error && visibleJobs.length === 0 ? <div className="p-10 text-center"><p className="font-medium">No jobs in this view</p><p className="mt-2 text-sm text-muted-foreground">Analyze a public source, choose a format, and add it to the managed queue.</p></div> : null}
        {visibleJobs.map((job) => <QueueRow job={job} key={job.id} />)}
      </div>
    </section>
  );
}
