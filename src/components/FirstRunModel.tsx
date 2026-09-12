import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Download, Loader2 } from "lucide-react";
import { api, type DownloadEnd, type DownloadProgress } from "../lib/api";
import { useT } from "../lib/i18n";
import { MODEL_CATALOG, RECOMMENDED_MODEL, fmtSize, modelEntry } from "../lib/models";

interface Props {
  /** Makes `model` the active one; rejects when that fails so the user can retry. */
  onReady: (model: string) => Promise<void>;
}

/**
 * Shown when the model named in settings is not on disk (fresh install, or its
 * file was removed). The user picks which model to start with; it downloads
 * into user app-data and survives app updates.
 */
export default function FirstRunModel({ onReady }: Props) {
  const { t } = useT();
  const [choice, setChoice] = useState(RECOMMENDED_MODEL);
  const [present, setPresent] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [pct, setPct] = useState<number | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const choiceRef = useRef(choice);
  choiceRef.current = choice;
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;

  // A model already on disk (settings were reset, or the chosen file went
  // missing) is offered first so nobody downloads what they already have.
  useEffect(() => {
    api
      .listModels()
      .then((list) => {
        const onDisk = list.filter((m) => m.present).map((m) => m.name);
        setPresent(onDisk);
        if (!onDisk.includes(RECOMMENDED_MODEL)) {
          const first = MODEL_CATALOG.find((m) => onDisk.includes(m.id));
          if (first) setChoice(first.id);
        }
      })
      .catch(() => {});
  }, []);

  async function finish(model: string) {
    setBusy(true);
    setErr(null);
    try {
      await onReadyRef.current(model);
    } catch (e) {
      setErr(String(e));
      setBusy(false);
      setPct(null);
    }
  }

  useEffect(() => {
    const offProg = listen<DownloadProgress>("model-download-progress", (e) => {
      if (e.payload.name !== choiceRef.current || e.payload.total <= 0) return;
      setPct(Math.min(100, Math.floor((e.payload.downloaded / e.payload.total) * 100)));
    });
    const offDone = listen<DownloadEnd>("model-download-done", (e) => {
      if (e.payload.name !== choiceRef.current) return;
      setPresent((p) => [...p, e.payload.name]);
      finish(e.payload.name);
    });
    const offErr = listen<DownloadEnd>("model-download-error", (e) => {
      if (e.payload.name !== choiceRef.current) return;
      setBusy(false);
      setPct(null);
      setErr(e.payload.message);
    });
    return () => {
      offProg.then((f) => f());
      offDone.then((f) => f());
      offErr.then((f) => f());
    };
  }, []);

  async function start() {
    if (present.includes(choice)) return finish(choice);
    setErr(null);
    setBusy(true);
    setPct(0);
    try {
      await api.downloadModel(choice);
    } catch (e) {
      setBusy(false);
      setPct(null);
      setErr(String(e));
    }
  }

  const entry = modelEntry(choice) ?? MODEL_CATALOG[0];
  const onDisk = present.includes(choice);

  return (
    <div className="fixed inset-0 z-30 grid place-items-center bg-black/60 p-6 backdrop-blur-sm">
      <div className="w-full max-w-md rounded-2xl border border-white/[0.08] bg-ink-900/85 p-6 text-center shadow-2xl backdrop-blur-2xl">
        <Download className="mx-auto h-8 w-8 text-accent" />
        <h2 className="mt-3 text-lg font-semibold">{t("first_run_title")}</h2>
        <p className="mt-1 text-sm text-ink-400">{t("first_run_desc")}</p>
        <label className="mt-4 block text-start">
          <span className="mb-1 block text-xs text-ink-500">{t("model")}</span>
          <select
            value={choice}
            onChange={(e) => {
              setChoice(e.target.value);
              setErr(null);
            }}
            disabled={busy}
            className="field disabled:opacity-60"
          >
            {MODEL_CATALOG.map((m) => (
              <option key={m.id} value={m.id}>
                {[
                  t(m.name),
                  fmtSize(m.mb),
                  m.id === RECOMMENDED_MODEL ? t("recommended") : null,
                  present.includes(m.id) ? t("downloaded") : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </option>
            ))}
          </select>
        </label>
        <p className="mt-2 text-start text-xs text-ink-400" aria-live="polite">
          {t(entry.desc)}
        </p>
        {err && (
          <p className="mt-2 text-xs text-amber-400" role="alert">
            {err}
          </p>
        )}
        {pct !== null ? (
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
            disabled={busy}
            className="mt-4 w-full rounded-lg bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent-deep disabled:opacity-60"
          >
            {onDisk ? t("use_this_model") : `${t("download")} (${fmtSize(entry.mb)})`}
          </button>
        )}
      </div>
    </div>
  );
}
