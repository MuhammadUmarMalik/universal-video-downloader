import { useRef, useState } from "react";
import { analyzeUrl, createDownload } from "@/lib/tauri";
import { Button } from "@/components/ui/button";

const MAX_FILE_BYTES = 2 * 1024 * 1024;
const MAX_URLS = 500;

function safeFilename(title: string, extension: string, index: number): string {
  const normalized = title
    .replace(/[\\/:*?"<>|]/g, " ")
    .split("")
    .filter((character) => character.charCodeAt(0) >= 32)
    .join("")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[. ]+$/g, "")
    .slice(0, 180);
  return `${String(index + 1).padStart(3, "0")} - ${normalized || "direct-media"}.${extension}`;
}

function parseDirectUrls(text: string): string[] {
  return [...new Set(
    text
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line.length > 0 && !line.startsWith("#")),
  )].slice(0, MAX_URLS);
}

export function BatchDirectImport({ onImported }: { onImported: () => void }) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [destination, setDestination] = useState("");
  const [isImporting, setIsImporting] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  async function handleFile(file: File) {
    setStatus(null);
    if (file.size > MAX_FILE_BYTES) {
      setStatus("The text file is too large. Maximum size is 2 MB.");
      return;
    }
    if (!destination.trim()) {
      setStatus("Enter an absolute destination directory first.");
      return;
    }

    const urls = parseDirectUrls(await file.text());
    if (urls.length === 0) {
      setStatus("No non-empty direct media URLs were found.");
      return;
    }

    setIsImporting(true);
    let queued = 0;
    let rejected = 0;
    try {
      for (const [index, url] of urls.entries()) {
        try {
          const analysis = await analyzeUrl({ url, platform_id: "direct" });
          const item = analysis.items[0];
          const format = analysis.formats.find(
            (candidate) => candidate.media_item_id === item?.id && candidate.is_progressive,
          );
          if (!item || !format) throw new Error("No direct progressive format was exposed.");
          const extension = (format.container || "bin").replace(/[^a-z0-9]/gi, "").toLowerCase() || "bin";
          await createDownload({
            media_item_id: item.id,
            format_id: format.id,
            destination_path: destination.trim(),
            filename: safeFilename(item.title, extension, index),
          });
          queued += 1;
        } catch {
          rejected += 1;
        }
      }
      setStatus(`Queued ${queued} of ${urls.length} URLs${rejected ? `; rejected ${rejected}` : ""}.`);
      if (queued > 0) onImported();
    } finally {
      setIsImporting(false);
      if (inputRef.current) inputRef.current.value = "";
    }
  }

  return (
    <section className="rounded-xl border border-border bg-card p-4" aria-labelledby="batch-import-heading">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-medium uppercase tracking-wide text-primary">Batch import</p>
          <h3 id="batch-import-heading" className="mt-1 font-semibold">Queue direct media URLs from a text file</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            One HTTPS .mp4, .webm, .mov, .mp3, or similar direct media URL per line. Social-page URLs are rejected.
          </p>
        </div>
        <Button disabled={isImporting} onClick={() => inputRef.current?.click()} size="sm" variant="outline">
          {isImporting ? "Importing…" : "Choose .txt file"}
        </Button>
      </div>
      <div className="mt-3 grid gap-2 sm:grid-cols-[1fr_auto]">
        <input
          aria-label="Batch destination directory"
          className="h-9 rounded-md border border-input bg-background px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
          onChange={(event) => setDestination(event.target.value)}
          placeholder="Absolute destination, e.g. /home/user/Downloads"
          value={destination}
        />
        <input
          ref={inputRef}
          accept=".txt,text/plain"
          className="sr-only"
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) void handleFile(file);
          }}
          type="file"
        />
      </div>
      {status ? <p className="mt-2 text-xs text-muted-foreground" role="status">{status}</p> : null}
    </section>
  );
}
