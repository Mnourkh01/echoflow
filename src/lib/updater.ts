// Thin wrappers over the Tauri updater + process plugins so components don't
// touch the plugin APIs directly.

import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type { Update };

/** Returns an available update, or null if already on the latest version. */
export async function checkForUpdate(): Promise<Update | null> {
  return await check();
}

/**
 * Download + install an update, reporting progress, then relaunch into the new
 * version. Nothing after `relaunch()` runs (the app restarts).
 */
export async function installUpdate(
  update: Update,
  onProgress?: (downloaded: number, total: number | null) => void,
): Promise<void> {
  let downloaded = 0;
  let total: number | null = null;
  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? null;
        onProgress?.(0, total);
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress?.(downloaded, total);
        break;
      case "Finished":
        onProgress?.(total ?? downloaded, total);
        break;
    }
  });
  await relaunch();
}
