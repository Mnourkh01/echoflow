/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        // Native variable faces first (Segoe UI Variable on Win11, SF Pro on
        // macOS) — modern and crisp without bundling a webfont into an offline
        // app. `display` is the heavier UI face for headings/brand.
        sans: [
          '"Geist Variable"',
          '"Segoe UI Variable Text"',
          '"SF Pro Text"',
          "system-ui",
          "sans-serif",
        ],
        display: [
          '"Geist Variable"',
          '"Segoe UI Variable Display"',
          '"SF Pro Display"',
          "system-ui",
          "sans-serif",
        ],
        arabic: ['"Noto Naskh Arabic"', '"Segoe UI"', "Tahoma", "sans-serif"],
        mono: [
          '"Geist Mono Variable"',
          "ui-monospace",
          "SFMono-Regular",
          "Menlo",
          "monospace",
        ],
      },
      colors: {
        // Deep cool slate base for the dark-luminous glass shell.
        ink: {
          950: "#080a0f",
          900: "#0e1117",
          800: "#161a22",
          700: "#222834",
          600: "#333a49",
          500: "#525c6f",
          400: "#838da0",
          300: "#a7afc0",
          200: "#cbd1dc",
          100: "#e8ebf1",
        },
        // Aurora signal — the brand's one expressive move. Used ONLY on the live
        // voice element + primary actions; everything else stays quiet glass.
        // Driven by CSS variables (see index.css :root + lib/theme.ts) so the
        // user's chosen accent palette recolors every utility live.
        aurora: {
          teal: "rgb(var(--aurora-teal) / <alpha-value>)",
          iris: "rgb(var(--aurora-iris) / <alpha-value>)",
          violet: "rgb(var(--aurora-violet) / <alpha-value>)",
        },
        accent: {
          DEFAULT: "rgb(var(--accent) / <alpha-value>)", // iris — primary signal
          soft: "rgb(var(--accent-soft) / <alpha-value>)",
          deep: "rgb(var(--accent-deep) / <alpha-value>)", // violet
          cyan: "rgb(var(--accent-cyan) / <alpha-value>)", // teal/mint
        },
      },
      borderRadius: {
        "4xl": "2rem",
      },
      boxShadow: {
        glass:
          "inset 0 1px 0 0 rgba(255,255,255,0.07), 0 12px 36px -12px rgba(0,0,0,0.7)",
        orb: "inset 0 1px 0 0 rgba(255,255,255,0.18), 0 18px 50px -8px rgb(var(--accent) / 0.55)",
        signal: "0 0 0 1px rgb(var(--aurora-teal) / 0.35), 0 0 24px 2px rgb(var(--aurora-teal) / 0.45)",
      },
      backdropBlur: {
        xs: "2px",
      },
    },
  },
  plugins: [],
};
