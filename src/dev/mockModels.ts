import { emit } from "@tauri-apps/api/event";
import type { ModelInfo } from "../lib/api";
import { MODEL_CATALOG } from "../lib/models";

/** The backend's list_models answer, with `onDisk` ids marked present. */
export function mockModels(onDisk: string[]): ModelInfo[] {
  return MODEL_CATALOG.map(({ id, mb }) => ({
    name: id,
    present: onDisk.includes(id),
    size_mb: onDisk.includes(id) ? mb : 0,
  }));
}

/** Emits the backend's download events on a fast timer, for the dev harnesses. */
export function fakeDownload(name: string, list: ModelInfo[], fail = false) {
  const total = 100;
  let downloaded = 0;
  const id = window.setInterval(() => {
    downloaded += 25;
    if (fail && downloaded >= 50) {
      window.clearInterval(id);
      emit("model-download-error", { name, message: "download failed (curl exit 6)" });
      return;
    }
    emit("model-download-progress", { name, downloaded, total });
    if (downloaded >= total) {
      window.clearInterval(id);
      const m = list.find((x) => x.name === name);
      if (m) m.present = true;
      emit("model-download-done", { name, message: "" });
    }
  }, 400);
}
