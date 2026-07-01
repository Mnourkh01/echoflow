import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Download, Loader2 } from "lucide-react";
import { api, type DownloadEnd, type DownloadProgress } from "../lib/api";
import { useT } from "../lib/i18n";

interface Props {
  onReady: () => void;
}

/**
 * Shown on a fresh install when no speech model is present. Downloads the
 * default `small` model (reusing the backend download command + events) so the
 * app is usable. The model lands in user app-data and survives app updates.
 */
export default function FirstRunModel({ onReady }: Props) {
  const { t } = useT();
  const [downloading, setDownloading] = useState(false);
  const [pct, setPct] = useState(0);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    const offProg = listen<DownloadProgress>("model-download-progress", (e) => {
      if (e.payload.total > 0) {
        setPct(Math.min(100, Math.floor((e.payload.downloaded / e.payload.total) * 100)));
      }
    });
    const offDone = listen<DownloadEnd>("model-download-done", () => onReady());
    const offErr = listen<DownloadEnd>("model-download-error", (e) => {
      setDownloading(false);
      setErr(e.payload.message);
    });
    return () => {
      offProg.then((f) => f());
      offDone.then((f) => f());
      offErr.then((f) => f());
    };
  }, [onReady]);

  async function start() {
    setErr(null);
    setDownloading(true);
    setPct(0);
    try {
      await api.downloadModel("small");
    } catch (e) {
      setDownloading(false);
      setErr(String(e));
    }
  }

  return (
    <div className="fixed inset-0 z-30 grid place-items-center bg-black/60 p-6 backdrop-blur-sm">
      <div className="w-full max-w-sm rounded-2xl border border-white/[0.08] bg-ink-900/85 p-6 text-center shadow-2xl backdrop-blur-2xl">
        <Download className="mx-auto h-8 w-8 text-accent" />
        <h2 className="mt-3 text-lg font-semibold">{t("first_run_title")}</h2>
        <p className="mt-1 text-sm text-ink-400">{t("first_run_desc")}</p>
        {err && <p className="mt-2 text-xs text-amber-400">{err}</p>}
        {downloading ? (
          <div className="mt-4">
            <div className="h-2 w-full overflow-hidden rounded-full bg-white/10">
              <div
                className="h-full rounded-full accent-grad transition-[width]"
                style={{ width: `${pct}%` }}
              />
            </div>
            <p className="mt-2 flex items-center justify-center gap-2 text-xs text-ink-400">
              <Loader2 className="h-3.5 w-3.5 animate-spin" /> {t("downloading")} {pct}%
            </p>
          </div>
        ) : (
          <button
            onClick={start}
            className="mt-4 w-full rounded-lg bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent-deep"
          >
            {t("first_run_button")}
          </button>
        )}
      </div>
    </div>
  );
}
