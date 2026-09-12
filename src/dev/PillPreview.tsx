import { Fragment, useEffect } from "react";
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import Overlay from "../Overlay";

const params = new URLSearchParams(window.location.search);
const caseName = params.get("case");
const lang = params.get("lang") === "ar" ? "ar" : "en";

async function finishTake(result: Record<string, unknown>) {
  await emit("rec-started");
  await emit("rec-stopped");
  await emit("dictation-result", result);
}

const CASES: Record<string, () => Promise<void>> = {
  recording: () => emit("rec-started"),
  done: () => finishTake({}),
  clipboard: () => finishTake({ paste_blocked: true }),
  translate: () => finishTake({ translate_warning: true }),
  rejected: () => finishTake({ enhance_rejected: true }),
  ai_failed: () => finishTake({ enhance_failed: true }),
  no_speech: () => emit("rec-canceled", "no-speech"),
  canceled: () => emit("rec-canceled", "user"),
  error: () => emit("rec-error", "no microphone found"),
};

if (caseName) {
  mockWindows("overlay", "main");
  mockIPC(
    (cmd) => {
      if (cmd === "get_settings") return { accent: "iris", pill_style: "wave", ui_lang: lang };
      if (cmd === "get_level") return 0.15 + Math.random() * 0.5;
      return null;
    },
    { shouldMockEvents: true },
  );
  document.documentElement.classList.add("overlay");
}

/**
 * Dev-only harness (`npm run dev` + `/?preview=pill`): every pill state at the
 * real 150 x 40 window, in English and Arabic. Each tile is an iframe running
 * the real Overlay with the Tauri backend mocked and replays its event every
 * 6 s; `&case=<name>&lang=ar` renders one tile alone.
 */
export default function PillPreview() {
  useEffect(() => {
    const run = caseName ? CASES[caseName] : undefined;
    if (!run) return;
    const first = window.setTimeout(run, 400);
    // Faster than the shortest notice, so a screenshot always catches it.
    const again = window.setInterval(run, 1500);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(again);
    };
  }, []);

  if (caseName) return <Overlay />;

  return (
    <div className="min-h-screen bg-[#3b4150] p-6 text-xs text-white/80">
      <p className="mb-4">Pill states at the real 150 x 40 window. Each tile replays its event every 1.5 s.</p>
      <div className="grid w-max grid-cols-[6rem_150px_150px] items-center gap-x-6 gap-y-3">
        <span />
        <span>EN</span>
        <span>AR</span>
        {Object.keys(CASES).map((c) => (
          <Fragment key={c}>
            <span>{c}</span>
            {(["en", "ar"] as const).map((l) => (
              <iframe
                key={l}
                title={`${c}-${l}`}
                src={`?preview=pill&case=${c}&lang=${l}`}
                width={150}
                height={40}
                className="border-0"
              />
            ))}
          </Fragment>
        ))}
      </div>
    </div>
  );
}
