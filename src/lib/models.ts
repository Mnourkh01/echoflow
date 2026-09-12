import type { StringKey } from "./i18n";

export interface ModelEntry {
  id: string;
  /** Approximate download size, shown until the file is on disk. */
  mb: number;
  name: StringKey;
  desc: StringKey;
}

// Smallest to largest. Ids are the ggml-<id>.bin file names and the backend
// allowlist (KNOWN_MODELS in commands.rs), so they never change; only labels do.
export const MODEL_CATALOG: readonly ModelEntry[] = [
  { id: "tiny", mb: 74, name: "model_tiny", desc: "model_tiny_desc" },
  { id: "base", mb: 141, name: "model_base", desc: "model_base_desc" },
  { id: "small", mb: 465, name: "model_small", desc: "model_small_desc" },
  { id: "medium", mb: 1462, name: "model_medium", desc: "model_medium_desc" },
  { id: "large-v3-turbo-q5_0", mb: 547, name: "model_turbo_q5", desc: "model_turbo_q5_desc" },
  { id: "large-v3-turbo", mb: 1549, name: "model_turbo", desc: "model_turbo_desc" },
  { id: "large-v3", mb: 2952, name: "model_large", desc: "model_large_desc" },
];

export const RECOMMENDED_MODEL = "large-v3-turbo-q5_0";

export const modelEntry = (id: string) => MODEL_CATALOG.find((m) => m.id === id);

/** Friendly name for a model id; ids outside the catalog (old recordings) show as-is. */
export function modelLabel(id: string, t: (key: StringKey) => string): string {
  const entry = modelEntry(id);
  return entry ? t(entry.name) : id;
}

export const fmtSize = (mb: number) =>
  mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
