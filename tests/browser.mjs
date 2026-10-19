import { _electron as electron, expect } from "@playwright/test";
import { mkdtempSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";

const dir = mkdtempSync(join(tmpdir(), "frame-desktop-test-"));
const source = join(dir, "flight-test.mp4");
execFileSync("ffmpeg", [
  "-v",
  "error",
  "-y",
  "-f",
  "lavfi",
  "-i",
  "testsrc2=s=640x360:r=30:d=4",
  "-f",
  "lavfi",
  "-i",
  "sine=frequency=440:duration=4",
  "-c:v",
  "libx264",
  "-preset",
  "ultrafast",
  "-threads",
  "2",
  "-c:a",
  "aac",
  "-shortest",
  source,
]);
let app;
const errors = [];
try {
  app = await electron.launch({
    args: [
      ...(process.env.FRAME_TEST_HEADLESS === "1"
        ? ["--no-sandbox", "--disable-gpu", "--ozone-platform=headless"]
        : []),
      ".",
    ],
    cwd: resolve("."),
    env: {
      ...process.env,
      XDG_CONFIG_HOME: join(dir, "config"),
      XDG_CACHE_HOME: join(dir, "cache"),
      FRAME_DATA_DIR: join(dir, "data"),
      FRAME_PYTHON: process.env.FRAME_PYTHON || "python",
    },
    timeout: 30000,
  });
  const page = await app.firstWindow();
  page.on("pageerror", (e) => errors.push(e.message));
  await expect(page.locator(".save-state")).toHaveText("All changes saved", {
    timeout: 15000,
  });
  expect(await page.evaluate(() => !!window.frameDesktop)).toBe(true);
  await expect(page.getByRole("button", { name: /Timeline clip/ })).toHaveCount(
    6,
  );
  const colors = await page.locator("canvas").evaluate((canvas) => {
    const pixels = canvas
      .getContext("2d")
      .getImageData(0, 0, canvas.width, canvas.height).data;
    let total = 0;
    for (let i = 0; i < pixels.length; i += 400) total += pixels[i];
    return total;
  });
  expect(colors).toBeGreaterThan(10000);
  // Split at the playhead, then undo and redo without losing the source offset.
  await page
    .getByRole("button", { name: "Split at playhead (S)", exact: true })
    .click();
  await expect(page.getByRole("button", { name: /Timeline clip/ })).toHaveCount(
    7,
  );
  await page.getByRole("button", { name: "Undo (⌘Z)", exact: true }).click();
  await expect(page.getByRole("button", { name: /Timeline clip/ })).toHaveCount(
    6,
  );
  await page.getByRole("button", { name: "Redo (⌘⇧Z)", exact: true }).click();
  await expect(page.getByRole("button", { name: /Timeline clip/ })).toHaveCount(
    7,
  );
  await page.getByRole("button", { name: "Undo (⌘Z)", exact: true }).click();
  // Exercise the real native IPC and backend linker, stubbing only the OS file dialog.
  await app.evaluate(({ dialog }, source) => {
    dialog.showOpenDialog = async () => ({
      canceled: false,
      filePaths: [source],
    });
  }, source);
  await page
    .getByRole("button", { name: "Import media", exact: false })
    .click();
  await expect(
    page.locator(".asset-name").filter({ hasText: "flight-test.mp4" }),
  ).toBeVisible({ timeout: 15000 });
  await expect
    .poll(
      () =>
        page.evaluate(async () => {
          const a = await (await fetch("/api/media")).json();
          return a.find((x) => x.name === "flight-test.mp4")?.status;
        }),
      { timeout: 30000 },
    )
    .toBe("ready");
  const linked = await page.evaluate(async () => {
    const a = await (await fetch("/api/media")).json();
    return a.find((x) => x.name === "flight-test.mp4");
  });
  const meta = JSON.parse(
    readFileSync(join(dir, "data", "media", linked.id, "meta.json")),
  );
  expect(meta.path).toBe(source);
  expect(linked.linked).toBe(true);
  await expect(
    page.getByRole("button", {
      name: "Add flight-test.mp4 to timeline",
      exact: true,
    }),
  ).toBeEnabled({ timeout: 15000 });
  await page
    .getByRole("button", {
      name: "Add flight-test.mp4 to timeline",
      exact: true,
    })
    .click();
  await expect(page.getByLabel("Speed multiplier")).toBeVisible();
  await page.getByLabel("Target duration seconds").fill("0.1");
  await expect(page.getByLabel("Speed multiplier")).toHaveValue("40");
  // Clean timelapse controls are under Color and apply to actual export settings.
  await page
    .locator(".inspector-tabs")
    .getByRole("button", { name: "Color", exact: true })
    .click();
  await expect(
    page.getByText("Clean Timelapse", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "8× clean", exact: true }).click();
  await expect(page.getByRole("checkbox").last()).toBeChecked();
  // Add and edit a title, ensuring it survives save and reload.
  await page
    .locator(".rail")
    .getByRole("button", { name: "Text", exact: true })
    .click();
  await page.getByRole("button", { name: /YOUR NEXT/ }).click();
  await page.getByLabel("Text content").fill("Liftoff — native editor test");
  await expect(page.locator(".save-state")).toHaveText("All changes saved", {
    timeout: 10000,
  });
  const name = await page.evaluate(async () => {
    const p = await (await fetch("/api/project")).json();
    return p.clips.find((c) => c.text === "Liftoff — native editor test")?.text;
  });
  expect(name).toBe("Liftoff — native editor test");
  await page.reload();
  await expect(page.locator(".save-state")).toHaveText("All changes saved", {
    timeout: 10000,
  });
  await expect(
    page.getByRole("button", {
      name: "Timeline clip Liftoff — native editor test",
      exact: true,
    }),
  ).toBeVisible();
  // Export in the native window and verify actual download availability.
  await page.getByRole("button", { name: "Export video", exact: true }).click();
  await page.getByLabel("Export resolution").selectOption("640x360");
  await page
    .locator(".modal")
    .getByRole("button", { name: "Export video", exact: true })
    .click();
  await expect(
    page.getByRole("link", { name: "Download video", exact: true }),
  ).toBeVisible({ timeout: 90000 });
  const response = await page.request.get(
    new URL(
      await page
        .getByRole("link", { name: "Download video", exact: true })
        .getAttribute("href"),
      page.url(),
    ).href,
  );
  expect(response.status()).toBe(200);
  expect((await response.body()).length).toBeGreaterThan(10000);
  expect(errors).toEqual([]);
  await page.getByRole("button", { name: "Close dialog", exact: true }).click();
  await page.getByTitle("Project settings", { exact: true }).click();
  await page.getByLabel("Project name").fill("Edits saved safely at close");
  // Close immediately, before the autosave debounce can fire.
  await app.close();
  app = null;
  const finalProject = JSON.parse(
    readFileSync(join(dir, "data", "projects", "current.json"), "utf8"),
  );
  expect(finalProject.name).toBe("Edits saved safely at close");
  console.log(
    "PASS: Native desktop launch, rendered preview, split/undo/redo, zero-copy file linking, retiming, clean timelapse, title editing, autosave/reload, MP4 export, and saving pending edits before app shutdown.",
  );
} finally {
  if (app) await app.close();
  rmSync(dir, { recursive: true, force: true });
}
