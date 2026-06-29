import { useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Globe,
  History,
  Keyboard,
  Mic,
  SlidersHorizontal,
  Wand2,
  X,
} from "lucide-react";
import { useT, type StringKey } from "../lib/i18n";

interface Props {
  /** Called when the guide is finished or skipped (persists the onboarded flag). */
  onClose: () => void;
}

const STEPS: { icon: typeof Mic; title: StringKey; body: StringKey }[] = [
  { icon: Mic, title: "ob_welcome_t", body: "ob_welcome_b" },
  { icon: Keyboard, title: "ob_ptt_t", body: "ob_ptt_b" },
  { icon: Wand2, title: "ob_modes_t", body: "ob_modes_b" },
  { icon: Globe, title: "ob_lang_t", body: "ob_lang_b" },
  { icon: History, title: "ob_hist_t", body: "ob_hist_b" },
  { icon: SlidersHorizontal, title: "ob_setup_t", body: "ob_setup_b" },
];

/**
 * First-run walkthrough: a short, skippable, step-by-step tour of what EchoFlow
 * does and how to drive it. Shown once (gated on the `onboarded` setting) after
 * the speech model is present. Matches the app's own dark design system.
 */
export default function Onboarding({ onClose }: Props) {
  const { t } = useT();
  const [i, setI] = useState(0);
  const step = STEPS[i];
  const Icon = step.icon;
  const last = i === STEPS.length - 1;

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/70 p-6 backdrop-blur-sm">
      <div className="relative w-full max-w-md rounded-2xl border border-white/[0.08] bg-ink-900/85 p-7 shadow-2xl backdrop-blur-2xl">
        <button
          onClick={onClose}
          className="tool-btn absolute end-3 top-3"
          title={t("ob_skip")}
          aria-label={t("ob_skip")}
        >
          <X className="h-4 w-4" />
        </button>

        <div className="grid h-14 w-14 place-items-center rounded-xl bg-accent/15 text-accent">
          <Icon className="h-7 w-7" strokeWidth={1.8} aria-hidden="true" />
        </div>
        <h2 className="mt-4 text-xl font-semibold">{t(step.title)}</h2>
        <p className="mt-2 text-sm leading-relaxed text-ink-400">{t(step.body)}</p>

        <div className="mt-6 flex items-center justify-between">
          <div className="flex gap-1.5" aria-hidden="true">
            {STEPS.map((_, idx) => (
              <span
                key={idx}
                className={[
                  "h-1.5 rounded-full transition-all",
                  idx === i ? "w-5 bg-accent" : "w-1.5 bg-white/15",
                ].join(" ")}
              />
            ))}
          </div>
          <div className="flex items-center gap-2">
            {i > 0 && (
              <button
                onClick={() => setI(i - 1)}
                className="inline-flex items-center gap-1 rounded-lg px-3 py-2 text-sm text-ink-400 hover:text-white"
              >
                <ArrowLeft className="h-4 w-4 rtl:rotate-180" aria-hidden="true" />
                {t("ob_back")}
              </button>
            )}
            <button
              onClick={() => (last ? onClose() : setI(i + 1))}
              className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent-deep"
            >
              {last ? t("ob_done") : t("ob_next")}
              {!last && <ArrowRight className="h-4 w-4 rtl:rotate-180" aria-hidden="true" />}
            </button>
          </div>
        </div>

        {!last && (
          <button
            onClick={onClose}
            className="mt-3 w-full text-center text-xs text-ink-500 hover:text-ink-300"
          >
            {t("ob_skip")}
          </button>
        )}
      </div>
    </div>
  );
}
