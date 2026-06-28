import { useState } from "react";
import { Download, Loader2 } from "lucide-react";
import type { Update } from "../lib/updater";
import { installUpdate } from "../lib/updater";
import { useT } from "../lib/i18n";

interface Props {
  update: Update;
  onError: (msg: string) => void;
}

/** One-click update bar: download + install + relaunch into the new version. */
export default function UpdateBanner({ update, onError }: Props) {
  const { t } = useT();
  const [busy, setBusy] = useState(false);
  const [pct, setPct] = useState(0);

  async function run() {
    setBusy(true);
    try {
      await installUpdate(update, (d, total) => {
        if (total) setPct(Math.min(100, Math.floor((d / total) * 100)));
      });
      // installUpdate relaunches the app; nothing after this runs.
    } catch (e) {
      setBusy(false);
      onError(String(e));
    }
  }

  return (
    <div className="flex items-center gap-3 border-b border-accent/30 bg-accent/10 px-5 py-2 text-sm text-accent-soft">
      <Download className="h-4 w-4 shrink-0" />
      <span className="flex-1">
        {t("update_available")} <span className="font-semibold">v{update.version}</span>
      </span>
      {busy ? (
        <span className="flex items-center gap-2 text-xs text-ink-300">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t("updating")}
          {pct > 0 ? ` ${pct}%` : ""}
        </span>
      ) : (
        <button
          onClick={run}
          className="rounded-md bg-accent px-3 py-1 text-xs font-medium text-white hover:bg-accent-deep"
        >
          {t("update_now")}
        </button>
      )}
    </div>
  );
}
