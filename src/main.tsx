import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import Overlay from "./Overlay";
// Self-hosted variable fonts (bundled into the build, so the app stays offline).
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./index.css";

// One bundle serves both windows; render by window label.
const isOverlay = getCurrentWindow().label === "overlay";
if (isOverlay) document.documentElement.classList.add("overlay");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isOverlay ? <Overlay /> : <App />}</React.StrictMode>
);
