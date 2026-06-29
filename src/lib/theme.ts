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
}
