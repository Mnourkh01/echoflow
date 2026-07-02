import React, { Suspense } from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import Overlay from "./Overlay";
// Self-hosted variable fonts (bundled into the build, so the app stays offline).
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./index.css";

// One bundle serves both windows; render by window label. Outside Tauri (a
// plain browser hitting the Vite dev server) there is no window metadata —
// fall back to the main app so dev previews don't white-screen.
let isOverlay = false;
try {
  isOverlay = getCurrentWindow().label === "overlay";
} catch {
  /* not running inside Tauri */
}
if (isOverlay) document.documentElement.classList.add("overlay");

// Dev-only visual harness: /?preview=mascot shows the record control with fake
// state/level controls. Lazy so the chunk is never fetched in production.
const MascotPreview = React.lazy(() => import("./dev/MascotPreview"));
const preview = import.meta.env.DEV
  ? new URLSearchParams(window.location.search).get("preview")
  : null;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {preview === "mascot" ? (
      <Suspense fallback={null}>
        <MascotPreview />
      </Suspense>
    ) : isOverlay ? (
      <Overlay />
    ) : (
      <App />
    )}
  </React.StrictMode>
);
