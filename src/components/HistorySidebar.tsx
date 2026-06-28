import { useState } from "react";
import { Search, Trash2, Pin } from "lucide-react";
import type { RecordingSummary } from "../lib/api";
import { dirFor, fontFor, langName, fmtDuration, fmtTime, dayBucket } from "../lib/format";
import { useT } from "../lib/i18n";
import logo from "../assets/echoflow.png";

interface Props {
  items: RecordingSummary[];
  activeId: number | null;
  onSelect: (id: number) => void;
  onDelete: (id: number) => void;
  onPin: (id: number, pinned: boolean) => void;
  onSearch: (q: string) => void;
}

export default function HistorySidebar({
  items,
  activeId,
  onSelect,
  onDelete,
  onPin,
  onSearch,
}: Props) {
  const [q, setQ] = useState("");
  const { t, lang } = useT();
  const locale = lang === "ar" ? "ar" : undefined;

  // Headers for "Today" / "Yesterday" are translated; older buckets are dates.
  const headerLabel = (bucket: string) =>
    bucket === "today" ? t("today") : bucket === "yesterday" ? t("yesterday") : bucket;

  return (
    <aside className="flex w-72 shrink-0 flex-col border-r border-ink-800 bg-ink-950">
      <div className="flex items-center gap-2 px-4 py-4">
        <img src={logo} alt="" className="h-6 w-6" />
        <span className="font-semibold tracking-tight">{t("app_title")}</span>
      </div>

      <div className="px-3 pb-3">
        <div className="flex items-center gap-2 rounded-lg bg-ink-900 px-3 py-2">
          <Search className="h-4 w-4 text-ink-500" />
          <input
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              onSearch(e.target.value);
            }}
            placeholder={t("search_placeholder")}
            className="w-full bg-transparent text-sm text-ink-100 outline-none placeholder:text-ink-500"
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2 pb-3">
        {items.length === 0 ? (
          <p className="px-3 py-6 text-center text-sm text-ink-500">
            {q.trim() ? t("no_results") : t("nothing_saved")}
          </p>
        ) : (
          items.map((it, i) => {
            const active = it.id === activeId;
            const bucket = dayBucket(it.created_at, locale);
            const showHeader = i === 0 || bucket !== dayBucket(items[i - 1].created_at, locale);
            return (
              <div key={it.id}>
                {showHeader && (
                  <div className="sticky top-0 z-10 bg-ink-950/95 px-3 pb-1 pt-3 text-[11px] font-medium uppercase tracking-wide text-ink-500 backdrop-blur">
                    {headerLabel(bucket)}
                  </div>
                )}
                <div
                  onClick={() => onSelect(it.id)}
                  className={[
                    "group mb-1 cursor-pointer rounded-lg px-3 py-2.5 transition",
                    active ? "bg-ink-800" : "hover:bg-ink-900",
                  ].join(" ")}
                >
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-ink-500">
                      {fmtTime(it.created_at, locale)}
                    </span>
                    <div className="flex items-center gap-2">
                      <span className="rounded bg-ink-800 px-1.5 py-0.5 text-[10px] uppercase text-ink-400 group-hover:bg-ink-700">
                        {langName(it.language)}
                      </span>
                      <span className="text-[10px] text-ink-500">
                        {fmtDuration(it.duration_ms)}
                      </span>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          onPin(it.id, !it.pinned);
                        }}
                        className={[
                          "transition hover:text-accent",
                          it.pinned
                            ? "text-accent"
                            : "text-ink-600 opacity-0 group-hover:opacity-100",
                        ].join(" ")}
                        title={it.pinned ? t("unpin") : t("pin_keep")}
                      >
                        <Pin className="h-3.5 w-3.5" fill={it.pinned ? "currentColor" : "none"} />
                      </button>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          onDelete(it.id);
                        }}
                        className="text-ink-600 opacity-0 transition hover:text-accent group-hover:opacity-100"
                        title={t("delete")}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </div>
                  <p
                    dir={dirFor(it.language)}
                    className={`mt-1 line-clamp-2 text-sm text-ink-300 ${fontFor(it.language)}`}
                  >
                    {it.preview || t("no_speech_preview")}
                  </p>
                </div>
              </div>
            );
          })
        )}
      </div>
    </aside>
  );
}
