import { expect, test, type Page } from "@playwright/test";

type MockJob = Record<string, unknown> & {
  id: string;
  status: string;
  downloaded_bytes: number;
  total_bytes: number | null;
};

type MockRequest = Record<string, unknown> & {
  media_item_id: string;
  format_id: string;
  destination_path: string;
  filename: string;
};

type MockInvokeArgs = Record<string, unknown> & {
  event?: string;
  handler?: number;
  request?: MockRequest;
  jobId?: string;
};

type MockWindow = Window & {
  __UMD_PLAYWRIGHT__: {
    seedJob(job: MockJob): void;
    markRecoveryPending(): void;
    crashNextBridgeCall(): void;
    emitProgress(payload: unknown): void;
  };
  electronAPI: {
    invoke(command: string, args?: MockInvokeArgs): Promise<unknown>;
    onDownloadProgress(callback: (payload: unknown) => void): () => void;
    onBridgeError(callback: (payload: { message: string }) => void): () => void;
  };
};

const analysisResponse = {
  platform_id: "reddit",
  capabilities: {
    metadata: true,
    thumbnails: false,
    collections: false,
    resume: true,
    scheduling: false,
  },
  source: {
    id: "source-1",
    canonical_url: "https://www.reddit.com/r/videos/comments/demo/",
    title: "Public demo source",
  },
  items: [
    {
      id: "item-1",
      source_id: "source-1",
      title: "Public demo video",
      creator_name: "Demo creator",
      duration_ms: 10_000,
      thumbnail_url: null,
    },
  ],
  formats: [
    {
      id: "format-1",
      media_item_id: "item-1",
      container: "mp4",
      video_codec: "h264",
      audio_codec: "aac",
      width: 1280,
      height: 720,
      file_size_bytes: 100,
      is_video: true,
      is_audio: true,
      is_progressive: true,
    },
  ],
};

async function installMockTauri(page: Page) {
  await page.addInitScript(({ response }: { response: unknown }) => {
    const storageKey = "umd-playwright-lifecycle-jobs";
    const recoveryKey = "umd-playwright-recovery-pending";
    const crashKey = "umd-playwright-bridge-crashed";
    const progressCallbacks: Array<(payload: unknown) => void> = [];

    function readJobs(): MockJob[] {
      const raw = window.localStorage.getItem(storageKey);
      return raw ? (JSON.parse(raw) as MockJob[]) : [];
    }

    function writeJobs(jobs: MockJob[]) {
      window.localStorage.setItem(storageKey, JSON.stringify(jobs));
    }

    function now() {
      return new Date().toISOString();
    }

    function recoverJobs(jobs: MockJob[]) {
      if (window.localStorage.getItem(recoveryKey) !== "1") return jobs;
      window.localStorage.removeItem(recoveryKey);
      const recovered = jobs.map((job) => {
        if (!["resolving", "downloading", "processing"].includes(job.status)) return job;
        return {
          ...job,
          status: "queued",
          retry_count: job.retry_count + 1,
          speed_bytes_per_sec: null,
          eta_seconds: null,
          error_code: null,
          error_message: null,
          updated_at: now(),
        };
      });
      writeJobs(recovered);
      return recovered;
    }

    function createJob(request: MockRequest): MockJob {
      const created = now();
      return {
        id: "job-playwright-1",
        media_item_id: request.media_item_id,
        format_id: request.format_id,
        status: "queued",
        priority: 0,
        destination_path: request.destination_path,
        temp_path: `${request.destination_path}/${request.filename}.part`,
        filename: request.filename,
        total_bytes: 100,
        downloaded_bytes: 0,
        speed_bytes_per_sec: null,
        eta_seconds: null,
        retry_count: 0,
        max_retries: 3,
        processing_json: null,
        etag: null,
        last_modified: null,
        error_code: null,
        error_message: null,
        started_at: null,
        completed_at: null,
        created_at: created,
        updated_at: created,
      };
    }

    window.electronAPI = {
      onDownloadProgress(callback: (payload: unknown) => void) {
        progressCallbacks.push(callback);
        return () => {
          const index = progressCallbacks.indexOf(callback);
          if (index >= 0) progressCallbacks.splice(index, 1);
        };
      },
      onBridgeError() {
        return () => undefined;
      },
      invoke(command: string, args: MockInvokeArgs = {}) {
        if (command === "get_download_jobs" && window.localStorage.getItem(crashKey) === "1") {
          window.localStorage.removeItem(crashKey);
          return Promise.reject({
            code: "UNKNOWN_ERROR",
            message: "The local desktop bridge stopped unexpectedly.",
            retryable: true,
          });
        }
        if (command === "subscribe_download_progress") return Promise.resolve(true);
        if (command === "get_foundation_status") {
          return Promise.resolve({
            appName: "Universal Media Downloader",
            phase: "hardening",
            electron: true,
            message: "Playwright Electron IPC harness",
          });
        }
        if (command === "analyze_url") return Promise.resolve(response);
        if (command === "create_download") {
          const job = createJob(args.request);
          writeJobs([job]);
          return Promise.resolve(job);
        }
        if (command === "get_download_jobs") return Promise.resolve(recoverJobs(readJobs()));
        if (command === "cancel_download") {
          const jobs = readJobs().map((job: MockJob) =>
            job.id === args.jobId ? { ...job, status: "cancelled", updated_at: now() } : job,
          );
          writeJobs(jobs);
          return Promise.resolve(true);
        }
        return Promise.reject({
          code: "UNKNOWN_ERROR",
          message: `Unsupported test command: ${command}`,
          retryable: false,
        });
      },
    };

    (window as MockWindow).__UMD_PLAYWRIGHT__ = {
      seedJob(job: MockJob) {
        writeJobs([job]);
      },
      markRecoveryPending() {
        window.localStorage.setItem(recoveryKey, "1");
      },
      crashNextBridgeCall() {
        window.localStorage.setItem(crashKey, "1");
      },
      emitProgress(payload: unknown) {
        progressCallbacks.forEach((callback) => callback(payload));
      },
      clear() {
        window.localStorage.removeItem(storageKey);
        window.localStorage.removeItem(recoveryKey);
        window.localStorage.removeItem(crashKey);
      },
    };
  }, { response: analysisResponse });
}

function seededJob(status: "downloading" | "processing") {
  return {
    id: "job-playwright-1",
    media_item_id: "item-1",
    format_id: "format-1",
    status,
    priority: 0,
    destination_path: "/tmp/umd-playwright",
    temp_path: "/tmp/umd-playwright/Public demo video.mp4.part",
    filename: "Public demo video.mp4",
    total_bytes: 100,
    downloaded_bytes: 40,
    speed_bytes_per_sec: 10,
    eta_seconds: 6,
    retry_count: 0,
    max_retries: 3,
    processing_json: null,
    etag: null,
    last_modified: null,
    error_code: null,
    error_message: null,
    started_at: new Date().toISOString(),
    completed_at: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
}

test.describe("download lifecycle IPC contract", () => {
  test.beforeEach(async ({ page }) => {
    await installMockTauri(page);
  });

  test("analyzes a public source, observes queue progress, and reaches completion", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("textbox", { name: "Public media URL" }).fill("https://www.reddit.com/r/videos/comments/demo/");
    await page.getByRole("button", { name: "Analyze", exact: true }).click();
    await expect(page.getByRole("heading", { name: "1 media item found" })).toBeVisible();

    await page.evaluate(() => (window as MockWindow).__UMD_PLAYWRIGHT__.seedJob({
      id: "job-playwright-1",
      media_item_id: "item-1",
      format_id: "format-1",
      status: "downloading",
      priority: 0,
      destination_path: "/tmp/umd-playwright",
      temp_path: "/tmp/umd-playwright/Public demo video.mp4.part",
      filename: "Public demo video.mp4",
      total_bytes: 100,
      downloaded_bytes: 40,
      speed_bytes_per_sec: 10,
      eta_seconds: 6,
      retry_count: 0,
      max_retries: 3,
      processing_json: null,
      etag: null,
      last_modified: null,
      error_code: null,
      error_message: null,
      started_at: new Date().toISOString(),
      completed_at: null,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    }));
    await page.getByRole("button", { name: "Queue" }).click();
    await expect(page.getByText("Downloading", { exact: true })).toBeVisible();
    await page.evaluate(() => (window as MockWindow).__UMD_PLAYWRIGHT__.emitProgress({
      job_id: "job-playwright-1",
      downloaded_bytes: 80,
      total_bytes: 100,
      speed_bytes_per_sec: 20,
      eta_seconds: 1,
    }));
    await expect(page.getByText("80%", { exact: true })).toBeVisible();

    await page.evaluate(() => (window as MockWindow).__UMD_PLAYWRIGHT__.seedJob({
      id: "job-playwright-1",
      media_item_id: "item-1",
      format_id: "format-1",
      status: "completed",
      priority: 0,
      destination_path: "/tmp/umd-playwright",
      temp_path: null,
      filename: "Public demo video.mp4",
      total_bytes: 100,
      downloaded_bytes: 100,
      speed_bytes_per_sec: null,
      eta_seconds: null,
      retry_count: 0,
      max_retries: 3,
      processing_json: null,
      etag: null,
      last_modified: null,
      error_code: null,
      error_message: null,
      started_at: null,
      completed_at: new Date().toISOString(),
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    }));
    await page.getByRole("button", { name: "Refresh queue" }).click();
    await expect(page.getByRole("article").getByText("Completed", { exact: true })).toBeVisible();
  });

  test("simulates restart recovery for an interrupted download and preserves the offset", async ({ page }) => {
    await page.goto("/");
    await page.evaluate((job) => (window as MockWindow).__UMD_PLAYWRIGHT__.seedJob(job), seededJob("downloading"));
    await page.evaluate(() => (window as MockWindow).__UMD_PLAYWRIGHT__.markRecoveryPending());
    await page.reload();
    await page.getByRole("button", { name: "Queue" }).click();
    await expect(page.getByText("Queued", { exact: true })).toBeVisible();
    await expect(page.getByText("40 B / 100 B", { exact: true })).toBeVisible();
  });

  test("surfaces a safe bridge rejection during a processing restart scenario", async ({ page }) => {
    await page.goto("/");
    await page.evaluate((job) => (window as MockWindow).__UMD_PLAYWRIGHT__.seedJob(job), seededJob("processing"));
    const rejected = await page.evaluate(async () => {
      (window as MockWindow).__UMD_PLAYWRIGHT__.crashNextBridgeCall();
      try {
        await (window as MockWindow).electronAPI.invoke("get_download_jobs");
        return false;
      } catch {
        return true;
      }
    });
    expect(rejected).toBe(true);
    await page.getByRole("button", { name: "Queue" }).click();
    await expect(page.getByText("Processing · FFmpeg", { exact: true })).toBeVisible();
  });
});
