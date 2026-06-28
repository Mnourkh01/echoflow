// Thin wrappers over the Tauri updater + process plugins so components don't
// touch the plugin APIs directly.

import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type { Update };

/**
 * Returns an available update, or null if already on the latest version.
 * Races against a timeout so a stalled network never leaves the UI "checking…"
 * forever — it rejects with a clear message instead.
 */
export async function checkForUpdate(timeoutMs = 15000): Promise<Update | null> {
  // `timeout` is passed into the Rust HTTP request so a stalled connection is
  // actually aborted at the network layer (not just abandoned by the UI). The
  // Promise.race is a belt-and-suspenders guard in case the plugin call itself
  // never settles — either way the UI stops "checking…" after timeoutMs.
  return await Promise.race([
    check({ timeout: timeoutMs }),
    new Promise<never>((_, reject) =>
      setTimeout(
        () =>
          reject(
            new Error(
              "Update check timed out. Check your connection, or download the latest version manually.",
            ),
          ),
        timeoutMs + 2000,
      ),
    ),
  ]);
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
