import type { FoundationStatus } from "@umd/shared-types";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { AnalyzerPanel } from "@/features/analyzer/AnalyzerPanel";
import { HistoryPanel } from "@/features/history/HistoryPanel";
import { QueuePanel } from "@/features/queue/QueuePanel";
import { SchedulerPanel } from "@/features/scheduler/SchedulerPanel";
import { getFoundationStatus } from "@/lib/desktopBridge";

const fallbackStatus: FoundationStatus = {
  appName: "Universal Media Downloader",
  phase: "foundation",
  electron: false,
  message: "The web preview is running outside the Electron shell.",
};

export default function App() {
  const [status, setStatus] = useState<FoundationStatus>(fallbackStatus);
  const [view, setView] = useState<"analyzer" | "queue" | "history" | "scheduler">("analyzer");

  useEffect(() => {
    void getFoundationStatus().then(setStatus);
  }, []);

  return (
    <main className="min-h-screen bg-background text-foreground">
      <div className="mx-auto flex min-h-screen max-w-6xl flex-col px-6 py-8 lg:px-10">
        <header className="flex items-center justify-between border-b border-border pb-6">
          <div>
            <p className="text-sm font-medium uppercase tracking-[0.18em] text-primary">UMD</p>
            <h1 className="mt-2 text-3xl font-semibold tracking-tight">Universal Media Downloader</h1>
          </div>
          <nav aria-label="Primary navigation" className="flex items-center gap-2">
            <Button onClick={() => setView("analyzer")} variant={view === "analyzer" ? "default" : "outline"} size="sm">Analyzer</Button>
            <Button onClick={() => setView("queue")} variant={view === "queue" ? "default" : "outline"} size="sm">Queue</Button>
            <Button onClick={() => setView("history")} variant={view === "history" ? "default" : "outline"} size="sm">History</Button>
            <Button onClick={() => setView("scheduler")} variant={view === "scheduler" ? "default" : "outline"} size="sm">Scheduler</Button>
          </nav>
        </header>

        <section className="grid gap-10 py-10 lg:grid-cols-[1fr_280px] lg:items-end">
          <div>
            <p className="mb-4 text-sm font-medium text-muted-foreground">Local-first desktop media management</p>
            <h2 className="max-w-3xl text-4xl font-semibold leading-tight tracking-tight sm:text-5xl">
              Understand a public media source before you queue it.
            </h2>
            <p className="mt-5 max-w-2xl text-lg leading-8 text-muted-foreground">
              Analyze supported public URLs, review the available media formats, and keep the workflow inside the local Rust application core.
            </p>
          </div>

          <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
            <p className="text-sm font-medium text-muted-foreground">Runtime status</p>
            <div className="mt-4 space-y-3 text-sm">
              <div className="flex items-center justify-between gap-4 border-b border-border pb-3">
                <span>Electron bridge</span>
                <span className="font-medium text-primary">{status.electron ? "Connected" : "Preview"}</span>
              </div>
              <div className="flex items-center justify-between gap-4 border-b border-border pb-3">
                <span>Analyzer boundary</span>
                <span className="font-medium">Ready</span>
              </div>
              <div className="flex items-center justify-between gap-4">
                <span>Registered adapters</span>
                <span className="font-medium">Reddit · Direct media · Social detection</span>
              </div>
            </div>
            <p className="mt-5 text-sm leading-6 text-muted-foreground">{status.message}</p>
          </div>
        </section>

        <section className="flex-1 pb-12">
          {view === "analyzer" ? <AnalyzerPanel /> : view === "queue" ? <QueuePanel /> : view === "history" ? <HistoryPanel /> : <SchedulerPanel />}
        </section>

        <footer className="border-t border-border pt-5 text-sm text-muted-foreground">
          Public-only analysis boundary. No credentials, cookies, private-content access, or security-control bypasses are supported.
        </footer>
      </div>
    </main>
  );
}
