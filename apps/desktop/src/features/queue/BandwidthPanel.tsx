import { useEffect, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { getBandwidthStatus, setBandwidthLimit } from "@/lib/desktopBridge";
import type { BandwidthSnapshot } from "@umd/shared-types";
import { formatBytes } from "./queueState";

export function BandwidthPanel({ snapshot }: { snapshot: BandwidthSnapshot }) {
  const [limit, setLimit] = useState("");
  const statusQuery = useQuery({
    queryKey: ["bandwidth-status"],
    queryFn: getBandwidthStatus,
    staleTime: 5_000,
  });
  const mutation = useMutation({
    mutationFn: (limitKbps: number) => setBandwidthLimit(limitKbps),
    onSuccess: (status) => {
      setLimit(status.limit_kbps === null ? "" : String(status.limit_kbps));
      void statusQuery.refetch();
    },
  });

  useEffect(() => {
    if (statusQuery.data) {
      setLimit(statusQuery.data.limit_kbps === null ? "" : String(statusQuery.data.limit_kbps));
    }
  }, [statusQuery.data]);

  function saveLimit() {
    const parsed = limit.trim() === "" ? 0 : Number(limit);
    if (!Number.isInteger(parsed) || parsed < 0 || parsed > 1_000_000) return;
    mutation.mutate(parsed);
  }

  const effectiveLimit = snapshot.limit_bytes_per_sec === null
    ? "Unlimited"
    : `${formatBytes(snapshot.limit_bytes_per_sec)}/s`;

  return (
    <section className="rounded-xl border border-border bg-card p-4" aria-labelledby="bandwidth-heading">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-medium uppercase tracking-wide text-primary">Bandwidth</p>
          <h3 id="bandwidth-heading" className="mt-1 font-semibold">Aggregate network monitor</h3>
        </div>
        <div className="text-right">
          <p className="text-2xl font-semibold">{formatBytes(snapshot.current_bytes_per_sec)}/s</p>
          <p className="text-xs text-muted-foreground">Current throughput</p>
        </div>
      </div>
      <div className="mt-3 grid gap-3 text-xs text-muted-foreground sm:grid-cols-3">
        <span>Limit: <strong className="text-foreground">{effectiveLimit}</strong></span>
        <span>Session total: <strong className="text-foreground">{formatBytes(snapshot.total_bytes)}</strong></span>
        <span>Workers share one global cap.</span>
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <label className="text-xs font-medium" htmlFor="bandwidth-limit-kbps">Limit (KB/s)</label>
        <input
          aria-label="Bandwidth limit in kilobytes per second"
          className="h-9 w-32 rounded-md border border-input bg-background px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
          id="bandwidth-limit-kbps"
          inputMode="numeric"
          min="0"
          onChange={(event) => setLimit(event.target.value)}
          placeholder="0 = unlimited"
          type="number"
          value={limit}
        />
        <Button disabled={mutation.isPending} onClick={saveLimit} size="sm" variant="outline">
          {mutation.isPending ? "Saving…" : "Apply limit"}
        </Button>
        {statusQuery.isError ? <span className="text-red-700">Tauri bandwidth controls are unavailable in this preview.</span> : null}
      </div>
    </section>
  );
}
