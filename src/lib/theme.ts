// Accent palettes. The app's aurora signal (orb, glows, primary buttons, focus
// rings) is driven entirely by CSS variables, so switching a palette recolors
// the whole UI live without a reload. Each palette is a harmonious trio (a
// bright core, a mid, an outer) plus a soft tint, given as "R G B" triplets so
// they slot into `rgb(var(--x) / <alpha>)`.

export interface Palette {
  key: string;
  label: string; // i18n key
  teal: string; // bright core (--aurora-teal / --accent-cyan)
  iris: string; // mid / primary (--aurora-iris / --accent)
  violet: string; // outer / deep (--aurora-violet / --accent-deep)
  soft: string; // light tint (--accent-soft)
  swatch: string; // representative hex for the picker dot
}

export const PALETTES: Palette[] = [
  { key: "iris", label: "accent_iris", teal: "46 230 198", iris: "123 107 248", violet: "168 85 247", soft: "154 140 255", swatch: "#7b6bf8" },
  { key: "teal", label: "accent_teal", teal: "45 212 191", iris: "20 184 166", violet: "6 182 212", soft: "94 234 212", swatch: "#14b8a6" },
  { key: "amber", label: "accent_amber", teal: "251 191 36", iris: "245 158 11", violet: "249 115 22", soft: "253 224 71", swatch: "#f59e0b" },
  { key: "rose", label: "accent_rose", teal: "251 113 133", iris: "244 63 94", violet: "217 70 239", soft: "253 164 175", swatch: "#f43f5e" },
  { key: "emerald", label: "accent_emerald", teal: "52 211 153", iris: "16 185 129", violet: "5 150 105", soft: "110 231 183", swatch: "#10b981" },
  { key: "sky", label: "accent_sky", teal: "56 189 248", iris: "59 130 246", violet: "99 102 241", soft: "125 211 252", swatch: "#3b82f6" },
  { key: "crimson", label: "accent_crimson", teal: "251 146 146", iris: "239 68 68", violet: "190 18 60", soft: "252 165 165", swatch: "#ef4444" },
  { key: "gold", label: "accent_gold", teal: "250 204 21", iris: "234 179 8", violet: "202 138 4", soft: "253 230 138", swatch: "#eab308" },
  { key: "fuchsia", label: "accent_fuchsia", teal: "240 171 252", iris: "217 70 239", violet: "168 85 247", soft: "245 208 254", swatch: "#d946ef" },
  { key: "lime", label: "accent_lime", teal: "190 242 100", iris: "132 204 22", violet: "77 124 15", soft: "217 249 157", swatch: "#84cc16" },
  { key: "cobalt", label: "accent_cobalt", teal: "96 165 250", iris: "37 99 235", violet: "67 56 202", soft: "147 197 253", swatch: "#2563eb" },
  { key: "violet", label: "accent_violet", teal: "196 181 253", iris: "139 92 246", violet: "109 40 217", soft: "216 205 255", swatch: "#8b5cf6" },

  // Gradient palettes: the trio deliberately spans two hues so the orb, pill,
  // glows and primary buttons render a full gradient (not a single tint). The
  // swatch is itself a gradient so the picker dot previews it.
  { key: "sunset", label: "accent_sunset", teal: "234 179 8", iris: "249 115 22", violet: "239 68 68", soft: "253 224 130", swatch: "linear-gradient(135deg, #eab308, #f97316, #ef4444)" },
  { key: "ember", label: "accent_ember", teal: "251 146 60", iris: "244 63 94", violet: "217 70 239", soft: "254 202 162", swatch: "linear-gradient(135deg, #fb923c, #f43f5e, #d946ef)" },
  { key: "ocean", label: "accent_ocean", teal: "34 211 238", iris: "59 130 246", violet: "79 70 229", soft: "165 243 252", swatch: "linear-gradient(135deg, #22d3ee, #3b82f6, #4f46e5)" },
  { key: "candy", label: "accent_candy", teal: "236 72 153", iris: "168 85 247", violet: "139 92 246", soft: "249 168 212", swatch: "linear-gradient(135deg, #ec4899, #a855f7, #8b5cf6)" },
  { key: "lagoon", label: "accent_lagoon", teal: "45 212 191", iris: "56 189 248", violet: "59 130 246", soft: "153 246 228", swatch: "linear-gradient(135deg, #2dd4bf, #38bdf8, #3b82f6)" },
];

export function paletteOf(key: string): Palette {
  return PALETTES.find((p) => p.key === key) ?? PALETTES[0];
}

/** Apply a palette by setting the accent/aurora CSS variables on the document. */
export function applyAccent(key: string) {
  const p = paletteOf(key);
  const root = document.documentElement.style;
  root.setProperty("--aurora-teal", p.teal);
  root.setProperty("--aurora-iris", p.iris);
  root.setProperty("--aurora-violet", p.violet);
  root.setProperty("--accent", p.iris);
  root.setProperty("--accent-soft", p.soft);
  root.setProperty("--accent-deep", p.violet);
  root.setProperty("--accent-cyan", p.teal);
  // Full gradient across the trio, used to paint primary buttons + selected
  // chips so a gradient palette actually reads as a gradient (not one tint).
  root.setProperty(
    "--accent-grad",
    `linear-gradient(120deg, rgb(${p.teal}) 0%, rgb(${p.iris}) 50%, rgb(${p.violet}) 100%)`,
  );
}
