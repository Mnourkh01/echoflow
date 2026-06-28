import { FileText, Languages, Sparkles, Wand2 } from "lucide-react";
import type { OutputMode } from "../lib/api";
import { useT, type StringKey } from "../lib/i18n";

const MODES: { value: OutputMode; icon: typeof FileText; key: StringKey }[] = [
  { value: "raw", icon: FileText, key: "mode_raw" },
  { value: "translate", icon: Languages, key: "mode_translate" },
  { value: "polish", icon: Sparkles, key: "mode_polish" },
  { value: "prompt", icon: Wand2, key: "mode_prompt" },
];

interface Props {
  value: OutputMode;
  onChange: (m: OutputMode) => void;
}

/** Compact header control to switch what the app does with recognized speech. */
export default function ModeSwitcher({ value, onChange }: Props) {
  const { t } = useT();
  const active = MODES.find((m) => m.value === value) ?? MODES[0];
  const Icon = active.icon;

  return (
    <label className="relative flex items-center gap-2 rounded-lg bg-ink-800 px-2.5 py-1.5 text-xs text-ink-300 hover:text-white">
      <Icon className="h-3.5 w-3.5 text-accent" />
      <span className="hidden sm:inline">{t(active.key)}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value as OutputMode)}
        className="absolute inset-0 cursor-pointer opacity-0"
        aria-label={t("output")}
      >
        {MODES.map((m) => (
          <option key={m.value} value={m.value}>
            {t(m.key)}
          </option>
        ))}
      </select>
    </label>
  );
}
