import { Globe } from "lucide-react";
import { useT } from "../lib/i18n";

// Recognition languages. Value is the Whisper language code; label is the native
// name. Auto detects. European languages carry their own diacritics natively.
const LANGS: { code: string; label: string }[] = [
  { code: "en", label: "English" },
  { code: "ar", label: "العربية" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
  { code: "es", label: "Español" },
  { code: "it", label: "Italiano" },
  { code: "pt", label: "Português" },
  { code: "nl", label: "Nederlands" },
];

interface Props {
  value: string;
  onChange: (code: string) => void;
}

/** Compact header control to force the spoken language (or auto-detect). */
export default function LanguageSwitcher({ value, onChange }: Props) {
  const { t } = useT();
  const label =
    value === "auto"
      ? t("auto_detect")
      : LANGS.find((l) => l.code === value)?.label ?? value.toUpperCase();

  return (
    <label
      className="relative flex items-center gap-2 rounded-xl border border-white/[0.06] bg-white/[0.04] px-3 py-1.5 text-xs text-ink-300 transition hover:border-white/10 hover:text-white"
      title={t("language")}
    >
      <Globe className="h-3.5 w-3.5 text-aurora-teal" />
      <span>{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="absolute inset-0 cursor-pointer opacity-0"
        aria-label={t("language")}
      >
        <option value="auto">{t("auto_detect")}</option>
        {LANGS.map((l) => (
          <option key={l.code} value={l.code}>
            {l.label}
          </option>
        ))}
      </select>
    </label>
  );
}
