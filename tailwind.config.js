/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        sans: ["Inter", "system-ui", "Segoe UI", "sans-serif"],
        arabic: ["Noto Naskh Arabic", "Segoe UI", "Tahoma", "sans-serif"],
        mono: ["ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
      },
      colors: {
        ink: {
          950: "#0c0d10",
          900: "#14161b",
          800: "#1c1f26",
          700: "#2a2e38",
          600: "#3a404d",
          500: "#5b6373",
          400: "#8b93a3",
          300: "#aab2c0",
          200: "#cbd0da",
          100: "#e7e9ee",
        },
        // EchoFlow brand: cyan -> blue -> purple from the logo gradient.
        accent: {
          DEFAULT: "#3d7bf7",
          soft: "#6f9cff",
          deep: "#5b32f1",
          cyan: "#2bb7f7",
        },
      },
    },
  },
  plugins: [],
};
