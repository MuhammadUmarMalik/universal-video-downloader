import { expect, test } from "@playwright/test";

test("analyzer workspace renders", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Universal Media Downloader" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Analyzer" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Inspect an authorized public media URL" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Public media URL" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Analyze", exact: true })).toBeVisible();
});

test("queue workspace renders processing-aware controls", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Queue" }).click();
  await expect(page.getByRole("heading", { name: "Downloads and processing" })).toBeVisible();
  await expect(page.getByText("FFmpeg processing", { exact: true })).toBeVisible();
  await expect(page.getByRole("group", { name: "Queue filters" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Processing" })).toBeVisible();
  await expect(page.getByText("Queue unavailable. The Tauri bridge may be disconnected.")).toBeVisible();
});

test("history workspace renders persistent-local controls", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "History" }).click();
  await expect(page.getByRole("heading", { name: "Download history" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Search" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Clear all" })).toBeVisible();
  await expect(page.getByText("History unavailable. The Tauri bridge may be disconnected.")).toBeVisible();
});

test("scheduler workspace renders opt-in controls", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Scheduler" }).click();
  await expect(page.getByRole("heading", { name: "Scheduler", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Enable scheduler" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Add recurring public-source monitor" })).toBeVisible();
  await expect(page.getByText("Schedules unavailable. The Tauri bridge may be disconnected.")).toBeVisible();
});
