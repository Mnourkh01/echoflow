// Small presentation helpers shared across components.

export const RTL_LANGS = new Set(["ar", "he", "fa", "ur"]);

export function dirFor(lang: string): "rtl" | "ltr" {
  return RTL_LANGS.has(lang) ? "rtl" : "ltr";
}

export function fontFor(lang: string): string {
  return RTL_LANGS.has(lang) ? "font-arabic" : "font-sans";
}

const LANG_NAMES: Record<string, string> = {
  ar: "Arabic",
  en: "English",
  auto: "Auto",
};

export function langName(code: string): string {
  return LANG_NAMES[code] ?? code.toUpperCase();
}

export function fmtDuration(ms: number): string {
  const total = Math.round(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export function fmtClock(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Just the time of day, for rows already grouped under a date header. */
export function fmtTime(iso: string, locale?: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
}

/** A full date label for a day-group header, e.g. "Jun 12, 2026". */
export function fmtDay(iso: string, locale?: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(locale, { month: "short", day: "numeric", year: "numeric" });
}

/**
 * Bucket a timestamp into "today" / "yesterday" / a date label, by calendar day
 * in local time. Used to group the history list so past dictations are easy to find.
 */
export function dayBucket(iso: string, locale?: string): "today" | "yesterday" | string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const startOf = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round((startOf(new Date()) - startOf(d)) / 86_400_000);
  if (diffDays <= 0) return "today";
  if (diffDays === 1) return "yesterday";
  return fmtDay(iso, locale);
}
