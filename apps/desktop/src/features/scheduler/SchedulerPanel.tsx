import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { CreateScheduleRequest, Schedule, ScheduleType, UpdateScheduleRequest } from "@umd/shared-types";
import { Button } from "@/components/ui/button";
import { createSchedule, deleteSchedule, getSchedulerEnabled, getSchedules, runSchedulerNow, setSchedulerEnabled, updateSchedule } from "@/lib/tauri";

function nextRunDefault(): string {
  return new Date(Date.now() + 60 * 60 * 1000).toISOString();
}

function scheduleLabel(schedule: Schedule): string {
  if (schedule.schedule_type === "once") return "One-time";
  if (schedule.schedule_type === "daily") return "Daily";
  if (schedule.schedule_type === "weekly") return "Weekly";
  return `Every ${schedule.interval_seconds ?? 0} seconds`;
}

function schedulerFormRequest(form: HTMLFormElement): CreateScheduleRequest {
  const data = new FormData(form);
  const scheduleType = data.get("schedule_type") as ScheduleType;
  const interval = Number(data.get("interval_seconds") ?? 0);
  return {
    source_id: String(data.get("source_id") ?? "").trim(),
    schedule_type: scheduleType,
    interval_seconds: scheduleType === "interval" ? interval : null,
    next_run_at: String(data.get("next_run_at") ?? "").trim(),
    enabled: true,
    format_id: String(data.get("format_id") ?? "").trim() || null,
    destination_path: String(data.get("destination_path") ?? "").trim(),
    filename_template: String(data.get("filename_template") ?? "").trim(),
    auto_download_new_items: true,
  };
}

export function SchedulerPanel() {
  const queryClient = useQueryClient();
  const [schedulerEnabled, setSchedulerEnabledState] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const enabledQuery = useQuery({ queryKey: ["scheduler-enabled"], queryFn: getSchedulerEnabled });
  const schedulesQuery = useQuery({ queryKey: ["schedules"], queryFn: getSchedules });
  useEffect(() => {
    if (enabledQuery.data !== undefined) setSchedulerEnabledState(enabledQuery.data);
  }, [enabledQuery.data]);
  const createMutation = useMutation({
    mutationFn: createSchedule,
    onSuccess: () => {
      setMessage("Schedule saved locally.");
      void queryClient.invalidateQueries({ queryKey: ["schedules"] });
    },
    onError: () => setMessage("The scheduler bridge is unavailable or the request was rejected."),
  });
  const deleteMutation = useMutation({
    mutationFn: deleteSchedule,
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["schedules"] }),
  });
  const updateMutation = useMutation({
    mutationFn: updateSchedule,
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["schedules"] }),
  });
  const enabledMutation = useMutation({
    mutationFn: setSchedulerEnabled,
    onSuccess: (enabled) => {
      setSchedulerEnabledState(enabled);
      void queryClient.invalidateQueries({ queryKey: ["scheduler-enabled"] });
    },
    onError: () => setMessage("The scheduler setting could not be saved locally."),
  });
  const runMutation = useMutation({
    mutationFn: runSchedulerNow,
    onSuccess: (report) => {
      setMessage(`Scheduler checked ${report.schedules_checked} due schedule(s) and queued ${report.jobs_enqueued} job(s).`);
      void queryClient.invalidateQueries({ queryKey: ["schedules"] });
    },
    onError: () => setMessage("The scheduler run could not be completed."),
  });
  const schedules = schedulesQuery.data ?? [];

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const request = schedulerFormRequest(event.currentTarget);
    createMutation.mutate(request);
  }

  function toggleSchedule(schedule: Schedule) {
    const configuration = schedule.configuration_json;
    if (!configuration) return;
    const request: UpdateScheduleRequest = {
      id: schedule.id,
      source_id: schedule.source_id,
      schedule_type: schedule.schedule_type,
      interval_seconds: schedule.interval_seconds,
      next_run_at: schedule.enabled ? null : schedule.next_run_at ?? nextRunDefault(),
      enabled: !schedule.enabled,
      format_id: configuration.format_id,
      destination_path: configuration.destination_path,
      filename_template: configuration.filename_template,
      auto_download_new_items: configuration.auto_download_new_items,
    };
    updateMutation.mutate(request);
  }

  return (
    <section aria-labelledby="scheduler-heading" className="space-y-6">
      <div className="flex flex-col gap-5 border-b border-border pb-6 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="text-sm font-medium uppercase tracking-[0.18em] text-primary">Local automation</p>
          <h2 id="scheduler-heading" className="mt-2 text-3xl font-semibold tracking-tight">Scheduler</h2>
          <p className="mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">Recurring monitoring runs inside this desktop app only. It is disabled by default, uses public adapter capabilities, and places validated new items into the existing download queue.</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button disabled={runMutation.isPending} onClick={() => runMutation.mutate()} size="sm" variant="outline">{runMutation.isPending ? "Running…" : "Run due now"}</Button>
          <Button disabled={enabledMutation.isPending} onClick={() => enabledMutation.mutate(!schedulerEnabled)} size="sm">{schedulerEnabled ? "Disable scheduler" : "Enable scheduler"}</Button>
        </div>
      </div>

      <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
        <div className="flex items-center justify-between gap-4">
          <div><h3 className="font-medium">Scheduler status</h3><p className="mt-1 text-sm text-muted-foreground">{schedulerEnabled ? "The embedded loop is enabled while the app is open." : "Opt-in is off. No recurring work will run."}</p></div>
          <span className={`rounded-full border px-3 py-1 text-xs font-medium ${schedulerEnabled ? "border-emerald-200 bg-emerald-50 text-emerald-700" : "border-slate-200 bg-slate-50 text-slate-600"}`}>{schedulerEnabled ? "Enabled" : "Disabled"}</span>
        </div>
      </div>

      <form className="grid gap-4 rounded-xl border border-border bg-card p-5 shadow-sm md:grid-cols-2" onSubmit={submit}>
        <div className="md:col-span-2"><h3 className="font-medium">Add recurring public-source monitor</h3><p className="mt-1 text-sm text-muted-foreground">Analyze a supported source first, then use its persisted source ID. The selected adapter must explicitly advertise scheduling support.</p></div>
        <label className="text-sm">Source ID<input className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 outline-none focus:ring-2 focus:ring-primary" name="source_id" placeholder="reddit:source:…" required /></label>
        <label className="text-sm">Schedule type<select className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 outline-none focus:ring-2 focus:ring-primary" defaultValue="interval" name="schedule_type"><option value="once">One-time</option><option value="daily">Daily</option><option value="weekly">Weekly</option><option value="interval">Interval</option></select></label>
        <label className="text-sm">Interval seconds<input className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 outline-none focus:ring-2 focus:ring-primary" defaultValue="3600" min="60" name="interval_seconds" type="number" /></label>
        <label className="text-sm">First run (RFC3339)<input className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 outline-none focus:ring-2 focus:ring-primary" defaultValue={nextRunDefault()} name="next_run_at" required /></label>
        <label className="text-sm">Destination directory<input className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 outline-none focus:ring-2 focus:ring-primary" name="destination_path" placeholder="/Users/name/Downloads" required /></label>
        <label className="text-sm">Format ID (optional)<input className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 outline-none focus:ring-2 focus:ring-primary" name="format_id" placeholder="Use first progressive format" /></label>
        <label className="text-sm md:col-span-2">Filename template<input className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 outline-none focus:ring-2 focus:ring-primary" defaultValue="{creator} - {title}.mp4" name="filename_template" required /></label>
        <div className="md:col-span-2"><Button disabled={createMutation.isPending} type="submit">{createMutation.isPending ? "Saving…" : "Save schedule"}</Button></div>
      </form>

      {message ? <p className="rounded-lg border border-border bg-muted/30 px-4 py-3 text-sm text-muted-foreground">{message}</p> : null}
      {schedulesQuery.error ? <div className="rounded-lg border border-red-200 bg-red-50 p-5 text-sm text-red-700">Schedules unavailable. The Tauri bridge may be disconnected.</div> : null}
      {!schedulesQuery.error && schedules.length === 0 ? <div className="rounded-xl border border-dashed border-border p-10 text-center"><p className="font-medium">No schedules configured</p><p className="mt-2 text-sm text-muted-foreground">Create an opt-in monitor above. It will not run while the scheduler setting is disabled.</p></div> : null}
      <div className="space-y-3">
        {schedules.map((schedule) => <article className="flex flex-col gap-4 rounded-xl border border-border bg-card p-5 shadow-sm sm:flex-row sm:items-center sm:justify-between" key={schedule.id}><div><div className="flex items-center gap-2"><span className={`rounded-full border px-2 py-1 text-xs font-medium ${schedule.enabled ? "border-emerald-200 bg-emerald-50 text-emerald-700" : "border-slate-200 bg-slate-50 text-slate-600"}`}>{schedule.enabled ? "Enabled" : "Paused"}</span><span className="text-sm font-medium">{scheduleLabel(schedule)}</span></div><p className="mt-2 text-xs text-muted-foreground">Source {schedule.source_id} · Next run {schedule.next_run_at ?? "not scheduled"}</p><p className="mt-1 text-xs text-muted-foreground">{schedule.configuration_json?.destination_path ?? "No destination"} · {schedule.configuration_json?.filename_template ?? "No template"}</p></div><div className="flex gap-2"><Button onClick={() => toggleSchedule(schedule)} size="sm" variant="outline">{schedule.enabled ? "Pause" : "Resume"}</Button><Button disabled={deleteMutation.isPending} onClick={() => deleteMutation.mutate(schedule.id)} size="sm" variant="outline">Delete</Button></div></article>)}
      </div>
    </section>
  );
}
